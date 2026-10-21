//! Rental housing market: tenants pay rent, landlords adapt asking rents to
//! vacancy, and searching households pick among a few sampled vacancies with
//! a logit (Gumbel-noise) choice over money-metric utility.

use crate::fields::{commute_cost, residential_quality};
use crate::types::*;
use crate::world::World;

/// Below this condition a dwelling is unfit to let: nobody new moves in.
pub const LETTABLE_CONDITION: f32 = 0.12;

/// Whether parcel `i` has dwellings that can be let to a newcomer.
pub fn lettable(w: &World, i: usize) -> bool {
    w.p.land_use[i] == LandUse::Residential && w.p.condition[i] > LETTABLE_CONDITION
}

/// Rent the household actually pays this month: nothing if it owns the
/// building it lives in.
pub fn rent_paid(w: &World, id: u32) -> f32 {
    let h = &w.hh[id as usize];
    if h.home == NONE || w.p.owner[h.home as usize] == Owner::Hh(id) {
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
        if owner == Owner::Hh(id as u32) {
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

pub fn evict(w: &mut World, id: u32) {
    let h = &mut w.hh[id as usize];
    if h.home != NONE {
        let o = &mut w.p.occupied[h.home as usize];
        *o = o.saturating_sub(1);
        h.home = NONE;
        h.rent = 0.0;
    }
}

/// Landlords adjust asking rents to the state of the market: rents rise
/// when vacancy (in the building and around it) is below the natural rate,
/// faster when people are queueing for housing, and fall when units stand
/// empty. Existing leases move to market on renewal.
pub const NATURAL_VACANCY: f32 = 0.05;

pub fn adjust_rents(w: &mut World) {
    let adj = w.econ.rent_adjust;
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
            let queue = if own_vac <= 0.0 { 3.0 * pressure } else { 0.0 };
            *r *= 1.0 + drift + adj * (tightness + queue);
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

/// Utility (money per month) of living at `cell` paying `rent`.
/// Monthly money-equivalent cost of living in a run-down building.
pub const DISREPAIR_COST: f32 = 450.0;

fn location_utility(w: &World, h: &Household, cell: usize, rent: f32) -> f32 {
    let commute = if h.job != NONE {
        commute_cost(w, cell, w.firms[h.job as usize].parcel as usize)
    } else {
        w.econ.commute_cost_per_cell * w.f.commute_eff[cell]
    };
    residential_quality(w, cell) - commute - rent - w.index * DISREPAIR_COST * (1.0 - w.p.condition[cell])
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
        if h.home == NONE || slum || burden > 0.55 || w.rng.chance(e.relocation_rate as f64) {
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
