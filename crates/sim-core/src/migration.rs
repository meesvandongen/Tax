//! Open-city closure: people arrive when the city looks better than the
//! outside option for someone of their skill, and leave when it stops being
//! so. This is the channel through which tax burdens capitalise into rents
//! and land prices, and through which mobile capital can flee.

use crate::fields::residential_quality;
use crate::types::*;
use crate::world::{Event, World};

/// A household's monthly money-metric utility here, including taxes.
pub fn utility_here(w: &World, h: &Household) -> f32 {
    let commute = if h.job != NONE && h.home != NONE {
        crate::fields::commute_cost(w, h.home as usize, w.firms[h.job as usize].parcel as usize)
    } else {
        0.0
    };
    let quality = if h.home != NONE { residential_quality(w, h.home as usize) } else { -600.0 };
    let net = h.income_labor + h.income_capital + h.income_transfer - h.taxes_paid;
    let effort = if h.job != NONE { h.effort_cost } else { 0.0 };
    net - h.rent - commute + quality - effort
}

/// The same household's utility elsewhere: outside wages for its skill plus
/// its (portable) capital income taxed at outside rates.
pub fn utility_outside(w: &World, h: &Household) -> f32 {
    let labour = w.outside_utility(h.skill);
    let capital = h.income_capital.max(0.0) * (1.0 - w.cfg.econ.outside_income_tax);
    labour + capital
}

pub fn new_household(w: &mut World, skill: f32) -> Household {
    let e = &w.cfg.econ;
    let wealth_draw = w.rng.lognormal(0.0, 1.1);
    let cash = (e.base_wage as f64 * skill as f64 * 4.0 * wealth_draw).max(500.0);
    Household {
        alive: true,
        skill,
        effort_cost: (w.rng.lognormal(5.6, 0.7) as f32).min(3000.0),
        cash,
        debt: 0.0,
        home: NONE,
        rent: 0.0,
        job: NONE,
        wage: 0.0,
        age_months: (25.0 + w.rng.f64() * 20.0) as u32 * 12,
        months_unhoused: 0,
        months_unhappy: 0,
        months_unemployed: 0,
        income_labor: 0.0,
        income_capital: 0.0,
        income_transfer: 0.0,
        taxes_paid: 0.0,
        consumption: 0.0,
        wealth: cash,
        utility: 0.0,
        declare_markup: 1.1,
        generation: 0,
    }
}

pub fn immigration(w: &mut World) {
    let e = w.cfg.econ.clone();
    let vacant_units: u32 =
        (0..w.p.len()).filter(|&i| w.p.land_use[i] == LandUse::Residential).map(|i| w.p.vacant_units(i) as u32).sum();
    let openings: u32 =
        w.firms.iter().filter(|f| f.alive).map(|f| f.desired_workers.saturating_sub(f.workers) as u32).sum();
    // City conditions as an outsider perceives them.
    let unemployment = w.last_unemployment();
    let wage_rate = w.market_wage_rate();
    let rent = median_vacant_rent(w);
    let avg_quality = {
        let mut s = 0.0;
        let mut c = 0.0;
        for i in 0..w.p.len() {
            if w.p.land_use[i] == LandUse::Residential && w.p.occupied[i] > 0 {
                s += residential_quality(w, i) * w.p.occupied[i] as f32;
                c += w.p.occupied[i] as f32;
            }
        }
        if c > 0.0 {
            s / c
        } else {
            0.0
        }
    };
    let commute = e.commute_cost_per_cell * 8.0;
    let div = w.gov.dividend_per_hh as f32;
    let attempts = (w.n_alive_hh as f32 * e.immigration_rate).max(4.0) as usize;
    // Newcomers need somewhere to live; the rest form latent demand that
    // landlords and developers can see.
    let cap = vacant_units as usize + (openings as usize / 4).min(8);
    let mut arrived = 0usize;
    let mut latent = 0u32;
    for _ in 0..attempts {
        let skill = w.rng.lognormal(0.0, e.skill_sigma as f64) as f32;
        let gross = wage_rate * skill;
        let tax = w.taxes.income_monthly(gross as f64, 0.0) as f32 + w.taxes.head_monthly() as f32;
        let expected = (1.0 - unemployment) * (gross - tax) + div - rent - commute + avg_quality;
        let outside = w.outside_utility(skill);
        let moving = e.moving_cost_months * outside / 24.0;
        let noise = e.location_logit_scale * 2.0 * w.rng.gumbel() as f32;
        if expected + noise > outside + moving {
            if arrived >= cap {
                latent += 1;
                continue;
            }
            let h = new_household(w, skill);
            w.add_household(h);
            arrived += 1;
            w.flows.immigrants += 1;
            let cell = edge_cell(w);
            w.push_event(Event::Immigrate { cell });
        }
    }
    w.latent_demand = latent;
}

fn median_vacant_rent(w: &World) -> f32 {
    let mut rents: Vec<f32> = (0..w.p.len())
        .filter(|&i| w.p.land_use[i] == LandUse::Residential && w.p.vacant_units(i) > 0)
        .map(|i| w.p.asking_rent[i])
        .collect();
    if rents.is_empty() {
        return (w.rent_index as f32).max(300.0);
    }
    let mid = rents.len() / 2;
    rents.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    rents[mid]
}

/// A road cell on the map edge (for arrival/departure effects).
pub fn edge_cell(w: &World) -> u32 {
    let (cx, cy) = w.dims.xy(w.center);
    let i = if cx < w.dims.w / 2 { w.dims.idx(0, cy) } else { w.dims.idx(w.dims.w - 1, cy) };
    i as u32
}

pub fn emigration(w: &mut World) {
    let e = w.cfg.econ.clone();
    let mut leaving: Vec<u32> = Vec::new();
    for id in 0..w.hh.len() {
        if !w.hh[id].alive {
            continue;
        }
        let here = utility_here(w, &w.hh[id]);
        let out = utility_outside(w, &w.hh[id]);
        let h = &mut w.hh[id];
        h.utility = here;
        if h.home == NONE {
            h.months_unhoused += 1;
        }
        if h.job == NONE {
            h.months_unemployed = h.months_unemployed.saturating_add(1);
        }
        let moving = e.moving_cost_months * out.abs() / 12.0;
        if out > here + moving {
            h.months_unhappy += 1;
        } else {
            h.months_unhappy = h.months_unhappy.saturating_sub(1);
        }
        let give_up_unhoused = h.months_unhoused >= 4 && w.rng.chance(0.3);
        if h.months_unhappy >= 8 || give_up_unhoused {
            leaving.push(id as u32);
        }
    }
    for id in leaving {
        let (cash, wealthy, cell) = {
            let h = &w.hh[id as usize];
            let cell = if h.home != NONE { h.home } else { w.center as u32 };
            (h.cash.max(0.0), h.wealth > 1_000_000.0, cell)
        };
        w.flows.emigrants += 1;
        w.flows.capital_flight += cash;
        if wealthy {
            w.flows.wealthy_emigrants += 1;
        }
        w.push_event(Event::Emigrate { cell, wealthy });
        w.remove_household(id);
    }
}

/// Ageing, death and inheritance. The heir continues the household (same
/// id) with the estate net of estate tax, a fresh job search and a skill that
/// regresses toward the mean.
pub fn demographics(w: &mut World) {
    let p_death = 1.0 / (w.cfg.econ.mean_lifetime_years as f64 * 12.0);
    let sigma = w.cfg.econ.skill_sigma as f64;
    for id in 0..w.hh.len() {
        if !w.hh[id].alive {
            continue;
        }
        w.hh[id].age_months += 1;
        let age_factor = if w.hh[id].age_months > 12 * 60 { 3.0 } else { 0.6 };
        if !w.rng.chance(p_death * age_factor) {
            continue;
        }
        let estate = w.hh[id].wealth;
        let tax = w.taxes.levy(|m| m.estate(estate));
        w.gov.cash += tax;
        let child_skill = (0.5 * (w.hh[id].skill as f64).ln() + w.rng.normal() * sigma * 0.85).exp() as f32;
        let h = &mut w.hh[id];
        h.cash -= tax;
        h.taxes_paid += tax as f32;
        h.skill = child_skill;
        h.age_months = 25 * 12;
        h.generation = h.generation.saturating_add(1);
        h.effort_cost = (w.rng.lognormal(5.6, 0.7) as f32).min(3000.0);
        h.months_unhappy = 0;
        let cell = if h.home != NONE { h.home } else { w.center as u32 };
        crate::labour::fire(w, id as u32);
        w.flows.deaths += 1;
        w.push_event(Event::Death { cell });
    }
}
