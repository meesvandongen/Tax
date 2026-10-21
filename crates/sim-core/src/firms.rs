//! Firm entry and exit. Empty commercial/industrial premises attract a new
//! firm when expected after-tax profit is positive; founders are resident
//! households with enough savings, otherwise outside capital.

use crate::labour::industry_tfp;
use crate::types::*;
use crate::world::{Event, World};

pub fn exit(w: &mut World) {
    for id in 0..w.firms.len() {
        let f = &w.firms[id];
        if !f.alive || f.sector == Sector::Public {
            continue;
        }
        let monthly_cost = (f.revenue - f.profit).max(1000.0) as f64;
        if f.months_loss >= 6 || f.cash < -2.0 * monthly_cost {
            let cell = f.parcel;
            w.close_firm(id as u32);
            w.flows.firms_closed += 1;
            w.push_event(Event::FirmClose { cell });
        }
    }
}

fn new_firm(w: &mut World, i: usize, sector: Sector, owner: Owner, capital: f64) -> u32 {
    let productivity = w.rng.lognormal(0.0, 0.18) as f32;
    let wage_rate = w.market_wage_rate();
    w.add_firm(Firm {
        alive: true,
        sector,
        parcel: i as u32,
        owner,
        cash: capital,
        productivity,
        wage_rate,
        workers: 0,
        skill_sum: 0.0,
        desired_workers: 1,
        months_unfilled: 0,
        months_loss: 0,
        emission_intensity: 1.0,
        revenue: 0.0,
        profit: 0.0,
        age_months: 0,
    })
}

pub fn open_public(w: &mut World, i: usize) {
    if w.p.firm[i] == NONE {
        new_firm(w, i, Sector::Public, Owner::Gov, 0.0);
    }
}

/// Expected monthly profit per job slot for a new firm at `i`.
fn expected_profit_per_slot(w: &World, i: usize, sector: Sector) -> f32 {
    let wage = w.market_wage_rate() * (1.0 + w.taxes.payroll_rate() as f32);
    let rent = w.p.asking_rent[i];
    let gross = match sector {
        Sector::Retail => w.f.retail_draw[i] * 0.92,
        Sector::Industry => {
            let probe = Firm {
                alive: true,
                sector,
                parcel: i as u32,
                owner: Owner::Absentee,
                cash: 0.0,
                productivity: 1.0,
                wage_rate: 0.0,
                workers: 0,
                skill_sum: 0.0,
                desired_workers: 0,
                months_unfilled: 0,
                months_loss: 0,
                emission_intensity: 1.0,
                revenue: 0.0,
                profit: 0.0,
                age_months: 0,
            };
            let a = industry_tfp(w, &probe, i);
            let units = w.p.units[i].max(1) as f32;
            let em = w.taxes.estimate(|m| m.emission(w.cfg.econ.pollution_per_worker as f64)) as f32;
            a * units.powf(w.cfg.econ.labour_elasticity) / units - em
        }
        Sector::Public => 0.0,
    };
    let pre_tax = gross - wage - rent;
    pre_tax - w.taxes.profit_monthly(pre_tax as f64) as f32
}

pub fn entry(w: &mut World) {
    let rate = w.cfg.econ.firm_entry_rate as f64;
    let required_margin = 0.04 * w.market_wage_rate();
    let founders: Vec<u32> =
        w.hh.iter().enumerate().filter(|(_, h)| h.alive && h.cash > 150_000.0).map(|(id, _)| id as u32).collect();
    for i in 0..w.p.len() {
        let u = w.p.land_use[i];
        let sector = match u {
            LandUse::Commercial => Sector::Retail,
            LandUse::Industrial => Sector::Industry,
            LandUse::Civic => {
                open_public(w, i);
                continue;
            }
            _ => continue,
        };
        if w.p.firm[i] != NONE || w.p.condition[i] < 0.15 || !w.rng.chance(rate) {
            continue;
        }
        let pi = expected_profit_per_slot(w, i, sector);
        if pi <= required_margin {
            continue;
        }
        let capital = 3.0 * w.market_wage_rate() as f64 * w.p.units[i] as f64 * 0.5;
        let mut owner = Owner::Absentee;
        for _ in 0..3 {
            if let Some(&id) = w.rng.pick(&founders) {
                if w.hh[id as usize].cash > capital * 1.5 {
                    owner = Owner::Hh(id);
                    break;
                }
            }
        }
        match owner {
            Owner::Hh(id) => w.hh[id as usize].cash -= capital,
            _ => w.flows.absentee_income -= capital,
        }
        new_firm(w, i, sector, owner, capital);
        w.flows.firms_opened += 1;
        w.push_event(Event::FirmOpen { cell: i as u32 });
    }
}
