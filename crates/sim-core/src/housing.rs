//! Rental housing market: tenants pay rent, landlords adapt asking rents to
//! vacancy, and searching households pick among a few sampled vacancies with
//! a logit (Gumbel-noise) choice over money-metric utility.

use crate::econ;
use crate::fields::{commute_cost, residential_quality};
use crate::types::*;
use crate::world::World;

/// Below this condition a dwelling is unfit to let: nobody new moves in.
pub const LETTABLE_CONDITION: f32 = 0.12;

/// Whether parcel `i` has dwellings that can be let to a newcomer.
pub fn lettable(w: &World, i: usize) -> bool {
    w.p.land_use[i] == LandUse::Residential && w.p.condition[i] > LETTABLE_CONDITION
}

/// Rent the household actually pays this month: nothing if it owns its
/// flat or the building it lives in.
pub fn rent_paid(w: &World, id: u32) -> f32 {
    let h = &w.hh[id as usize];
    if h.home == NONE || h.owns_home || w.p.owner[h.home as usize] == Owner::Hh(id) {
        0.0
    } else {
        h.rent
    }
}

/// Tenants pay rent; those who cannot are evicted.
pub fn pay_rents(w: &mut World) {
    for id in 0..w.hh.len() {
        let h = &w.hh[id];
        if !h.alive || h.home == NONE {
            continue;
        }
        let home = h.home as usize;
        let rent = h.rent as f64;
        let owner = w.p.owner[home];
        if owner == Owner::Hh(id as u32) || h.owns_home {
            continue; // owner-occupier: no cash changes hands
        }
        if h.cash < rent {
            evict(w, id as u32);
            continue;
        }
        w.hh[id].cash -= rent;
        w.pay_owner(owner, rent);
    }
}

/// The household leaves its home. A flat owner first sells the flat.
pub fn evict(w: &mut World, id: u32) {
    sell_flat(w, id);
    let h = &mut w.hh[id as usize];
    if h.home != NONE {
        let o = &mut w.p.occupied[h.home as usize];
        *o = o.saturating_sub(1);
        h.home = NONE;
        h.rent = 0.0;
    }
}

/// A flat owner sells the flat back to the building's freeholder at market
/// value and stays on as a tenant at the asking rent. (Selling to the next
/// occupier is the same thing in two steps: they can buy it back from the
/// freeholder, see [`tenure`].) The buyer pays the transfer tax; a household
/// freeholder borrows against the building what it cannot pay in cash.
pub fn sell_flat(w: &mut World, id: u32) {
    let h = &w.hh[id as usize];
    if !h.alive || !h.owns_home {
        return;
    }
    let i = h.home as usize;
    let freeholder = w.p.owner[i];
    let price = if freeholder == Owner::Hh(id) { 0.0 } else { econ::flat_value(w, i) };
    let transfer = if matches!(freeholder, Owner::Hh(b) if b != id) || freeholder == Owner::Absentee {
        w.taxes.levy(|m| m.transfer(price))
    } else {
        0.0
    };
    w.gov.cash += transfer;
    match freeholder {
        // (Its own freehold: the flat merges back, and so does its debt.)
        Owner::Hh(b) if b == id => w.p.mortgage[i] += w.hh[id as usize].flat_mortgage as f32,
        Owner::Hh(b) => {
            let o = &mut w.hh[b as usize];
            let cash = o.cash.clamp(0.0, price + transfer);
            o.cash -= cash;
            o.taxes_paid += transfer as f32;
            w.p.mortgage[i] += (price + transfer - cash) as f32;
        }
        Owner::Gov => w.gov.cash -= price,
        Owner::Absentee => w.flows.absentee_income -= price + transfer,
    }
    let h = &mut w.hh[id as usize];
    if freeholder != Owner::Hh(id) {
        h.cash += price - h.flat_mortgage;
    }
    h.flat_mortgage = 0.0;
    h.owns_home = false;
    h.rent = w.p.asking_rent[i];
    w.p.owned_flats[i] -= 1;
    w.flows.flat_sales += 1;
}

/// Tenure choice. Once a year, at lease renewal, a tenant may buy the flat
/// it rents: if owning it (no rent, no vacancy, no tax on the rent it saves)
/// is worth more to the household than the freeholder's asking price plus
/// transfer tax, and the household can pay the deposit and the mortgage.
pub fn tenure(w: &mut World) {
    let tick = w.tick;
    let ltv = w.econ.loan_to_value as f64;
    let payment_rate = w.econ.mortgage_rate as f64 / 12.0 + 0.003;
    let reserve = 3.0 * w.econ.subsistence as f64;
    let t_rate = w.taxes.transfer_cost(1.0);
    for id in 0..w.hh.len() {
        let h = &w.hh[id];
        if !h.alive || h.home == NONE || h.owns_home || (id as u64 + tick) % 12 != 0 {
            continue;
        }
        let i = h.home as usize;
        let freeholder = w.p.owner[i];
        if !lettable(w, i) || w.p.condition[i] < 0.5 || freeholder == Owner::Hh(id as u32) {
            continue;
        }
        // Cheap screen first: most tenants cannot raise a deposit at all.
        if h.cash < 0.5 * (1.0 - ltv) * w.flat_assessment(i) {
            continue;
        }
        let units = w.p.units[i].max(1) as f64;
        let ask = 1.03 * econ::building_in_condition_value(w, i, &w.investor_for(freeholder), w.p.condition[i]) / units;
        let transfer = ask * t_rate;
        let deposit = ask * (1.0 - ltv);
        let h = &w.hh[id];
        if ask <= 0.0 || h.cash < deposit + transfer + reserve || ask * ltv * payment_rate > 0.35 * budget(w, h) as f64
        {
            continue;
        }
        if econ::owner_occupier_value(w, i, &w.household_investor(id as u32)) < ask + transfer {
            continue;
        }
        let transfer = w.taxes.levy(|m| m.transfer(ask));
        w.gov.cash += transfer;
        match freeholder {
            Owner::Hh(b) => w.hh[b as usize].cash += ask,
            Owner::Gov => w.gov.cash += ask,
            Owner::Absentee => w.flows.absentee_income += ask,
        }
        let h = &mut w.hh[id];
        h.cash -= deposit + transfer;
        h.taxes_paid += transfer as f32;
        h.flat_mortgage = ask * ltv;
        h.owns_home = true;
        w.p.owned_flats[i] += 1;
        w.flows.flat_purchases += 1;
    }
}

/// Landlords adjust asking rents to the state of the market: rents rise
/// when vacancy (in the building and around it) is below the natural rate,
/// faster when people are queueing for housing, and fall when units stand
/// empty. Existing leases move to market on renewal.
pub const NATURAL_VACANCY: f32 = 0.05;

pub fn adjust_rents(w: &mut World) {
    let adj = w.econ.housing_rent_adjust;
    let pressure = w.housing_pressure();
    let drift = w.econ.outside_growth / 12.0;
    for i in 0..w.p.len() {
        if w.p.land_use[i] != LandUse::Residential || w.p.units[i] == 0 {
            continue;
        }
        let units = w.p.units[i] as f32;
        let own_vac = (units - w.p.occupied[i] as f32) / units;
        let local_vac = 1.0 - w.f.occ_res[i];
        // (A single empty flat in a small building is normal turnover, so
        // the building's own vacancy is blended with its neighbourhood's.)
        let vac = 0.3 * own_vac + 0.7 * local_vac;
        // Excess demand: tight market > 0, slack market < 0.
        let tightness = ((NATURAL_VACANCY - vac) / NATURAL_VACANCY).clamp(-1.0, 1.0);
        let r = &mut w.p.asking_rent[i];
        if tightness >= 0.0 {
            // A full building with people queueing for homes is a shortage
            // landlords can see directly, so they raise rents fast.
            let queue = if own_vac <= 0.0 { pressure } else { 0.0 };
            *r *= 1.0 + drift + adj * tightness + w.econ.housing_queue_response * queue;
        } else {
            // Rents are sticky downward: landlords cut slowly.
            *r *= 1.0 + 0.6 * adj * tightness;
        }
        if own_vac > NATURAL_VACANCY {
            w.p.months_vacant[i] = w.p.months_vacant[i].saturating_add(1);
        } else {
            w.p.months_vacant[i] = 0;
        }
        *r = r.max(40.0 * w.index);
    }
    // Lease renewals: roughly yearly.
    let tick = w.tick;
    for (id, h) in w.hh.iter_mut().enumerate() {
        if h.alive && h.home != NONE && (id as u64 + tick) % 12 == 0 {
            h.rent = w.p.asking_rent[h.home as usize];
        }
    }
}

/// Utility (money per month) of living at `cell` paying `rent`. Living in a
/// run-down building costs what its condition takes off the local rent.
fn location_utility(w: &World, h: &Household, cell: usize, rent: f32) -> f32 {
    let commute = if h.job != NONE {
        commute_cost(w, cell, w.firms[h.job as usize].parcel as usize)
    } else {
        w.econ.commute_cost_per_cell * w.f.commute_eff[cell]
    };
    let disrepair = (1.0 - crate::econ::rent_quality(w.p.condition[cell]) as f32) * w.f.rent_res[cell];
    residential_quality(w, cell) - commute - rent - disrepair
}

/// Monthly income a household can count on (for affordability).
pub fn budget(w: &World, h: &Household) -> f32 {
    let inc = h.income_labor + h.income_capital + h.income_transfer - h.taxes_paid;
    let expected = if h.job == NONE { 0.6 * w.market_wage_rate() * h.skill } else { 0.0 };
    inc.max(0.0) + expected + (h.cash as f32 / 36.0).max(0.0)
}

pub fn market(w: &mut World) {
    let e = w.econ.clone();
    let mut vacant: Vec<u32> = Vec::new();
    for i in 0..w.p.len() {
        if lettable(w, i) && w.p.occupied[i] < w.p.units[i] {
            vacant.push(i as u32);
        }
    }
    let mut searchers: Vec<u32> = Vec::new();
    for (id, h) in w.hh.iter().enumerate() {
        if !h.alive {
            continue;
        }
        let burden = if h.home != NONE { rent_paid(w, id as u32) / budget(w, h).max(1.0) } else { 1.0 };
        let slum = h.home != NONE && !lettable(w, h.home as usize);
        // Owners move less often: selling and buying again costs fees and taxes.
        let relocate = e.relocation_rate as f64 * if h.owns_home { 0.3 } else { 1.0 };
        if h.home == NONE || slum || burden > 0.55 || w.rng.chance(relocate) {
            searchers.push(id as u32);
        }
    }
    w.rng.shuffle(&mut searchers);
    let k = e.search_samples.max(2);
    let scale = e.location_logit_scale;
    for hid in searchers {
        if vacant.is_empty() {
            break;
        }
        let h = w.hh[hid as usize].clone();
        let max_rent = 0.5 * budget(w, &h);
        let mut best: Option<(usize, f32)> = None;
        let mut current_u = f32::NEG_INFINITY;
        if h.home != NONE {
            current_u = location_utility(w, &h, h.home as usize, h.rent) + 0.25 * e.moving_cost_months * 100.0;
        }
        for _ in 0..k {
            let slot = w.rng.below(vacant.len());
            let cell = vacant[slot] as usize;
            if w.p.occupied[cell] >= w.p.units[cell] {
                vacant.swap_remove(slot);
                if vacant.is_empty() {
                    break;
                }
                continue;
            }
            let rent = w.p.asking_rent[cell];
            if rent > max_rent {
                continue;
            }
            let u = location_utility(w, &h, cell, rent) + scale * w.rng.gumbel() as f32;
            if best.map_or(true, |b| u > b.1) {
                best = Some((cell, u));
            }
        }
        if let Some((cell, u)) = best {
            if u > current_u {
                evict(w, hid);
                let h = &mut w.hh[hid as usize];
                h.home = cell as u32;
                h.rent = w.p.asking_rent[cell];
                h.months_unhoused = 0;
                w.p.occupied[cell] += 1;
            }
        }
    }
}
