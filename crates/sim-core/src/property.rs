//! Owners' monthly obligations: property-type taxes, maintenance and
//! mortgage service. Maintenance is a choice: owners keep a building up only
//! if the extra rent is worth more than the upkeep *after* the extra
//! building tax that a better-kept building attracts.

use crate::econ::{building_value, construction_cost, market_rent};
use crate::types::*;
use crate::world::World;

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

pub fn taxable(w: &World, i: usize) -> bool {
    w.p.in_city[i]
        && !matches!(w.p.owner[i], Owner::Gov)
        && !matches!(w.p.land_use[i], LandUse::Water | LandUse::Road | LandUse::Park | LandUse::Civic)
}

pub fn owner_costs(w: &mut World) {
    let e = w.cfg.econ.clone();
    let tau_b = w.taxes.estimate(|m| m.property(0.0, 1.0)) + w.taxes.harberger_rate();
    let tau_l = w.taxes.estimate(|m| m.property(1.0, 0.0)) + w.taxes.harberger_rate();
    let mut deferred = 0u32;
    for i in 0..w.p.len() {
        let owner = w.p.owner[i];
        // ---- property-type taxes
        if taxable(w, i) {
            let land = w.p.assessed_land[i] as f64;
            let bldg = w.p.assessed_bldg[i] as f64;
            let declared = w.p.declared[i] as f64;
            let idle = idle_share(w, i);
            let tax = w.taxes.levy(|m| {
                (m.property(land, bldg) + m.vacancy(land + bldg, idle) + m.harberger_rate() * declared) / 12.0
            });
            w.p.tax_last[i] = tax as f32;
            if tax > 0.0 {
                w.gov.cash += tax;
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
            let cost = construction_cost(w, i, u, w.p.floors[i]);
            let upkeep = e.maintenance_rate as f64 * cost;
            let inv = w.investor_for(owner);
            let gross = market_rent(w, i, u, w.p.units[i]) as f64 * w.p.occupied[i].max(w.p.units[i] / 2) as f64;
            // PV gain from preventing one month of depreciation.
            let annual_benefit = 12.0 * 0.4 * gross * (1.0 - inv.t_inc) - tau_b * 0.7 * cost;
            let pv_gain = e.depreciation as f64 * annual_benefit / (inv.r - inv.g + tau_l + inv.w).max(0.01);
            let can_pay = w.owner_cash(owner) > upkeep;
            if pv_gain >= upkeep && can_pay {
                match owner {
                    Owner::Hh(id) => w.hh[id as usize].cash -= upkeep,
                    _ => w.flows.absentee_income -= upkeep,
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
    let _ = building_value;
}
