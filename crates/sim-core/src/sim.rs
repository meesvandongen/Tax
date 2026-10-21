//! The monthly tick: an explicit, ordered pipeline of systems. Each system is
//! a plain function over `&mut World`, so the order is visible in one place
//! and systems can be added, removed or reordered easily.

use crate::config::{Policy, SimConfig};
use crate::metrics::{self, Metrics};
use crate::tax::TaxSystem;
use crate::world::{Flows, World};
use crate::*;

pub type System = fn(&mut World);

/// Named pipeline (names are used for profiling).
pub const PIPELINE: &[(&str, System)] = &[
    ("exogenous", exogenous),
    ("fields", fields::update),
    ("firm_decisions", labour::firm_decisions),
    ("labour_matching", labour::matching),
    ("production", production::run),
    ("transfers", government::transfers),
    ("rents", housing::pay_rents),
    ("owner_costs", property::owner_costs),
    ("settle", households::settle),
    ("housing_market", housing::market),
    ("rent_adjust", housing::adjust_rents),
    ("commercial_rents", production::adjust_commercial_rents),
    ("assess", |w| land::assess(w, false)),
    ("declare", land::declare),
    ("land_market", land::market),
    ("development", development::run),
    ("government", government::budget),
    ("firm_exit", firms::exit),
    ("firm_entry", firms::entry),
    ("emigration", migration::emigration),
    ("immigration", migration::immigration),
    ("demographics", migration::demographics),
    ("environment", environment::run),
    ("wealth", households::update_wealth),
    ("indices", indices),
];

pub struct Simulation {
    pub world: World,
    /// Accumulated time per system (ms), when profiling is enabled.
    pub profile: Option<Vec<f64>>,
}

impl Simulation {
    pub fn new(cfg: SimConfig) -> Simulation {
        Simulation { world: World::new(cfg), profile: None }
    }

    pub fn step(&mut self) -> &Metrics {
        begin_tick(&mut self.world);
        for (k, (_, sys)) in PIPELINE.iter().enumerate() {
            match &mut self.profile {
                Some(prof) => {
                    let t0 = crate::clock::now_ms();
                    sys(&mut self.world);
                    prof[k] += crate::clock::now_ms() - t0;
                }
                None => sys(&mut self.world),
            }
        }
        let m = metrics::compute(&self.world);
        self.world.history.push(m);
        self.world.tick += 1;
        self.world.history.last().unwrap()
    }

    pub fn run(&mut self, months: u32) {
        for _ in 0..months {
            self.step();
        }
    }

    pub fn set_policy(&mut self, policy: Policy) {
        self.world.taxes = TaxSystem::new(&policy.taxes);
        self.world.cfg.policy = policy;
    }

    pub fn enable_profiling(&mut self) {
        self.profile = Some(vec![0.0; PIPELINE.len()]);
    }

    pub fn profile_report(&self) -> Vec<(&'static str, f64)> {
        match &self.profile {
            Some(p) => PIPELINE.iter().map(|(n, _)| *n).zip(p.iter().copied()).collect(),
            None => vec![],
        }
    }
}

fn begin_tick(w: &mut World) {
    w.flows = Flows::default();
    w.events.clear();
    w.taxes.reset_month();
    let g = &mut w.gov;
    g.spend_services = 0.0;
    g.spend_infrastructure = 0.0;
    g.spend_parks = 0.0;
    g.spend_dividend = 0.0;
    g.spend_welfare = 0.0;
    g.spend_interest = 0.0;
    g.spend_compensation = 0.0;
    g.spend_rebate = 0.0;
    households::begin_month(w);
    w.update_pressure();
    w.update_wage_cache();
    w.update_labour_slack();
}

/// Outside-world shocks: an AR(1) business cycle on export prices.
fn exogenous(w: &mut World) {
    let shock = 0.025 * w.rng.normal() as f32;
    w.cycle = (1.0 + 0.94 * (w.cycle - 1.0) + shock).clamp(0.7, 1.3);
}

/// Price and rent indices with smoothed growth (feeds expectations).
fn indices(w: &mut World) {
    let mut rent = 0.0f64;
    let mut n = 0.0f64;
    let mut land = 0.0f64;
    let mut ln = 0.0f64;
    for i in 0..w.p.len() {
        if w.p.land_use[i] == types::LandUse::Residential && w.p.occupied[i] > 0 {
            rent += w.f.rent_res[i] as f64 * w.p.occupied[i] as f64;
            n += w.p.occupied[i] as f64;
        }
        if w.p.land_use[i].is_urban() && property::taxable(w, i) {
            land += w.p.assessed_land[i] as f64;
            ln += 1.0;
        }
    }
    if n > 0.0 {
        let idx = rent / n;
        if w.rent_index > 1.0 && w.tick > 6 {
            let g = (idx / w.rent_index).powf(12.0) - 1.0;
            w.rent_growth = 0.95 * w.rent_growth + 0.05 * g.clamp(-0.5, 0.5);
        }
        w.rent_index = idx;
    }
    if ln > 0.0 {
        let idx = land / ln;
        if w.price_index > 1.0 && w.tick > 6 {
            let g = (idx / w.price_index).powf(12.0) - 1.0;
            w.price_growth = 0.95 * w.price_growth + 0.05 * g.clamp(-0.5, 0.5);
        }
        w.price_index = idx;
    }
}
