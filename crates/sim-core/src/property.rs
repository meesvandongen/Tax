//! Owners' monthly obligations: property-type taxes, maintenance and
//! mortgage service. Maintenance is a choice: owners keep a building up only
//! if the extra rent is worth more than the upkeep *after* the extra
//! building tax that a better-kept building attracts.

use crate::econ;
use crate::types::*;
use crate::world::{Event, World};

pub fn idle_share(w: &World, i: usize) -> f64 {
    let p = &w.p;
    match p.land_use[i] {
        LandUse::Residential => {
            if p.units[i] == 0 {
                1.0
            } else {
                1.0 - p.occupied[i] as f64 / p.units[i] as f64
            }
        }
        LandUse::Commercial | LandUse::Industrial => (p.firm[i] == NONE) as u8 as f64,
        LandUse::Vacant => 1.0,
        _ => 0.0,
    }
}

/// In the property tax base.
pub fn taxable(w: &World, i: usize) -> bool {
    w.p.in_city[i]
        && !matches!(w.p.owner[i], Owner::Gov)
        && !matches!(w.p.land_use[i], LandUse::Water | LandUse::Road | LandUse::Park | LandUse::Civic)
}

pub fn owner_costs(w: &mut World) {
    let e = w.econ.clone();
    let mut deferred = 0u32;
    // Upkeep per dwelling this month, charged to flat owners below.
    let mut flat_upkeep = vec![0f64; w.p.len()];
    for i in 0..w.p.len() {
        let owner = w.p.owner[i];
        // The parcel owner pays the freehold's share; flat owners pay theirs.
        let share = w.p.freehold_share(i);
        // ---- property-type taxes
        if taxable(w, i) {
            let land = w.p.assessed_land[i] as f64;
            let bldg = w.p.assessed_bldg[i] as f64;
            let declared = w.p.declared[i] as f64;
            let idle = idle_share(w, i);
            let bill = |m: &dyn crate::tax::TaxModule| {
                (m.property(land, bldg) + m.vacancy(land + bldg, idle) + m.harberger_rate() * declared) / 12.0
            };
            let due = w.taxes.estimate(bill);
            // Outside owners have no other stake in the city: once a parcel
            // is worth nothing net of its future taxes they stop paying, and
            // after a year of arrears the city takes the title.
            if owner == Owner::Absentee && due > 0.0 && (i as u64 + w.tick) % 6 == 0 {
                let inv = w.absentee_investor();
                let worthless = econ::valuation(w, i, &inv) < due * share;
                w.p.arrears[i] = if worthless { w.p.arrears[i].max(1) } else { 0 };
            }
            let tax = if w.p.arrears[i] > 0 {
                w.p.arrears[i] += 1;
                if w.p.arrears[i] > 12 {
                    foreclose(w, i);
                }
                0.0
            } else {
                w.taxes.levy(bill)
            };
            w.p.tax_last[i] = tax as f32;
            if tax > 0.0 {
                w.gov.cash += tax;
                let tax = tax * share;
                match owner {
                    Owner::Hh(id) => {
                        let h = &mut w.hh[id as usize];
                        h.cash -= tax;
                        h.taxes_paid += tax as f32;
                    }
                    // Outside owners pay out of their rental income.
                    _ => w.flows.absentee_income -= tax,
                }
            }
        } else {
            w.p.tax_last[i] = 0.0;
        }

        // ---- maintenance (private buildings)
        let u = w.p.land_use[i];
        if u.is_private_use() {
            w.p.age_months[i] += 1;
            let upkeep = econ::upkeep(w, i);
            let inv = w.investor_for(owner);
            // Value gained by keeping the building up this month rather than
            // letting it slip: the owner's own after-tax valuation at both
            // conditions (so rent quality, taxes on the better building and
            // the point where nobody will rent it any more all count).
            let c = w.p.condition[i];
            let kept = (c + 0.25 * e.depreciation).min(1.0);
            let slipped = (c - e.depreciation).max(0.0);
            let pv_gain = econ::building_in_condition_value(w, i, &inv, kept)
                - econ::building_in_condition_value(w, i, &inv, slipped);
            let can_pay = w.owner_cash(owner) > upkeep * share;
            if pv_gain >= upkeep && can_pay {
                match owner {
                    Owner::Hh(id) => w.hh[id as usize].cash -= upkeep * share,
                    Owner::Gov => w.gov.cash -= upkeep * share,
                    Owner::Absentee => w.flows.absentee_income -= upkeep * share,
                }
                if w.p.owned_flats[i] > 0 {
                    flat_upkeep[i] = upkeep / w.p.units[i] as f64;
                }
                w.flows.construction_spend += upkeep;
                w.p.condition[i] = (w.p.condition[i] + 0.25 * e.depreciation).min(1.0);
            } else {
                deferred += 1;
                w.p.condition[i] = (w.p.condition[i] - e.depreciation).max(0.0);
            }
        }

        // ---- mortgage service
        let m = w.p.mortgage[i] as f64;
        if m > 0.0 {
            if let Owner::Hh(id) = owner {
                let pay = m * (e.mortgage_rate as f64 / 12.0 + 0.003);
                w.hh[id as usize].cash -= pay;
                w.p.mortgage[i] = (m * 0.997) as f32;
            } else {
                w.p.mortgage[i] = 0.0;
            }
        }
    }
    w.flows.deferred_maintenance = deferred;

    // ---- flat owners: their share of the building's taxes and upkeep, and
    // their mortgage
    for id in 0..w.hh.len() {
        let h = &w.hh[id];
        if !h.alive || !h.owns_home {
            continue;
        }
        let i = h.home as usize;
        let tax = w.p.tax_last[i] as f64 / w.p.units[i].max(1) as f64;
        let mortgage = h.flat_mortgage * (e.mortgage_rate as f64 / 12.0 + 0.003);
        let h = &mut w.hh[id];
        h.cash -= tax + flat_upkeep[i] + mortgage;
        h.taxes_paid += tax as f32;
        h.flat_mortgage *= 0.997;
    }
}

/// Charge every owner-occupier of a flat in building `i` `amount` (their
/// share of a renovation).
pub fn charge_flat_owners(w: &mut World, i: usize, amount: f64) {
    for h in w.hh.iter_mut().filter(|h| h.alive && h.owns_home && h.home == i as u32) {
        h.cash -= amount;
    }
}

/// Tax foreclosure: title passes to the city, which auctions the parcel
/// (see `land::market`). The defaulting owner's mortgage is written off.
pub fn foreclose(w: &mut World, i: usize) {
    w.p.owner[i] = Owner::Gov;
    w.p.mortgage[i] = 0.0;
    w.p.arrears[i] = 0;
    w.p.declared[i] = 0.0;
    w.flows.foreclosures += 1;
    w.push_event(Event::Foreclosure { cell: i as u32 });
}
