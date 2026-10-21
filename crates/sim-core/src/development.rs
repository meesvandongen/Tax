//! Construction and redevelopment. Owners periodically compare
//! * keeping the parcel as is,
//! * building the best (use, height) option now,
//! * waiting a year for rents to grow (real-option / speculation rule).
//!
//! The tax schedule enters only through the valuation functions.

use crate::econ::{self, DevOption};
use crate::housing::evict;
use crate::property::idle_share;
use crate::types::*;
use crate::world::{Event, World};

pub fn progress(w: &mut World) {
    for i in 0..w.p.len() {
        if w.p.land_use[i] != LandUse::Construction {
            continue;
        }
        if w.p.build_left[i] > 0 {
            w.p.build_left[i] -= 1;
            continue;
        }
        let u = w.p.target_use[i];
        w.p.land_use[i] = u;
        w.p.condition[i] = 1.0;
        w.p.age_months[i] = 0;
        w.p.units[i] = econ::units_for(w, u, w.p.floors[i]);
        w.p.occupied[i] = 0;
        w.p.nature[i] = if u == LandUse::Residential && w.p.floors[i] <= 2 { 0.25 } else { 0.03 };
        let units = w.p.units[i];
        w.p.asking_rent[i] = econ::market_rent(w, i, u, units).max(50.0);
        w.p.assessed_bldg[i] = econ::building_value(w, i) as f32;
        w.flows.buildings_completed += 1;
        w.push_event(Event::BuildDone { cell: i as u32 });
        if u == LandUse::Civic {
            crate::firms::open_public(w, i);
        }
    }
}

/// Clear a parcel for construction: tenants evicted, firm closed.
pub fn clear_site(w: &mut World, i: usize) {
    if w.p.land_use[i] == LandUse::Residential && w.p.occupied[i] > 0 {
        let tenants: Vec<u32> =
            w.hh.iter().enumerate().filter(|(_, h)| h.alive && h.home == i as u32).map(|(id, _)| id as u32).collect();
        for id in tenants {
            evict(w, id);
        }
    }
    if w.p.firm[i] != NONE {
        let f = w.p.firm[i];
        w.close_firm(f);
    }
    if w.p.land_use[i].is_building() {
        w.flows.demolitions += 1;
        w.push_event(Event::Demolish { cell: i as u32 });
    }
    w.p.occupied[i] = 0;
    w.p.units[i] = 0;
}

pub fn start_construction(w: &mut World, i: usize, d: &DevOption) {
    clear_site(w, i);
    let months = w.cfg.econ.construction_months as u32 + d.floors as u32 / 4;
    w.p.land_use[i] = LandUse::Construction;
    w.p.target_use[i] = d.land_use;
    w.p.floors[i] = d.floors;
    w.p.build_left[i] = months.min(u16::MAX as u32) as u16;
    w.p.nature[i] = 0.0;
    w.p.months_held_empty[i] = 0;
    w.flows.construction_spend += d.cost;
    w.flows.buildings_started += 1;
    w.push_event(Event::BuildStart { cell: i as u32 });
}

/// Annual after-tax income of the parcel while waiting (may be negative:
/// idle land still owes land tax).
fn interim_income(w: &World, i: usize, inv: &econ::Investor) -> f64 {
    let u = w.p.land_use[i];
    let noi = match u {
        LandUse::Farm => w.cfg.econ.farm_rent as f64 * (0.4 + w.terrain.fertility[i] as f64),
        LandUse::Wild => 0.5 * w.cfg.econ.farm_rent as f64 * (0.4 + w.terrain.fertility[i] as f64),
        _ if u.is_private_use() => {
            let b = econ::building_value(w, i);
            econ::noi(w, i, u, w.p.occupied[i], 1.0, b)
        }
        _ => 0.0,
    };
    let land = w.p.assessed_land[i] as f64;
    let bldg = w.p.assessed_bldg[i] as f64;
    let tax = if inv.no_tax {
        0.0
    } else {
        w.taxes.estimate(|m| m.property(land, bldg) + m.vacancy(land + bldg, idle_share(w, i)))
            + w.taxes.harberger_rate() * w.p.declared[i] as f64
    };
    12.0 * noi * (1.0 - inv.t_inc) - tax
}

pub enum Decision {
    Keep,
    Wait,
    Build(DevOption),
}

pub fn decide(w: &World, i: usize, inv: &econ::Investor) -> Decision {
    let Some(best) = econ::best_development(w, i, inv) else { return Decision::Keep };
    let cur = econ::current_use_value(w, i, inv);
    if best.residual <= 0.0 || best.residual <= cur * 1.05 {
        return Decision::Keep;
    }
    // Waiting: the project is worth (1+g) more next year, costs are fixed,
    // meanwhile the parcel earns its interim income (net of holding taxes).
    let demolition = best.value - best.cost - best.residual;
    let next = best.value * (1.0 + inv.g) - best.cost - demolition;
    let wait = (next + interim_income(w, i, inv)) / (1.0 + inv.r);
    if wait > best.residual {
        Decision::Wait
    } else {
        Decision::Build(best)
    }
}

pub fn run(w: &mut World) {
    progress(w);
    let e = w.cfg.econ.clone();
    // Owners of serviced parcels review their options now and then.
    let candidates: Vec<usize> = (0..w.p.len())
        .filter(|&i| w.p.road_access[i] && w.p.land_use[i].is_developable() && crate::property::taxable(w, i))
        .collect();
    for i in candidates {
        if !w.rng.chance(e.review_rate as f64 * 2.5) {
            continue;
        }
        let owner = w.p.owner[i];
        let inv = w.investor_for(owner);
        match decide(w, i, &inv) {
            Decision::Keep => {
                if w.p.land_use[i] == LandUse::Vacant {
                    w.p.months_held_empty[i] = w.p.months_held_empty[i].saturating_add(1);
                }
            }
            Decision::Wait => {
                // Profitable to build, but more profitable to hold: speculation.
                w.p.months_held_empty[i] = w.p.months_held_empty[i].saturating_add(1);
            }
            Decision::Build(d) => {
                // Financing: households need a down payment, else they sell.
                let ok = match owner {
                    Owner::Hh(id) => {
                        let ltv = e.loan_to_value as f64;
                        let h = &mut w.hh[id as usize];
                        if h.cash >= d.cost * (1.0 - ltv) {
                            h.cash -= d.cost * (1.0 - ltv);
                            w.p.mortgage[i] += (d.cost * ltv) as f32;
                            true
                        } else {
                            false
                        }
                    }
                    _ => true,
                };
                if !ok {
                    continue;
                }
                // Distortion metric: what would have been built without taxes.
                let free = econ::best_development(w, i, &inv.untaxed());
                w.flows.floors_built += d.floors as u32;
                w.flows.floors_no_tax += free.map(|f| f.floors as u32).unwrap_or(0);
                start_construction(w, i, &d);
            }
        }
    }
}
