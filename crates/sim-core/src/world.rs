//! The complete simulation state and its initial generation.

use crate::config::{EconParams, SimConfig};
use crate::fields::Fields;
use crate::grid::Dims;
use crate::metrics::Metrics;
use crate::rng::Rng;
use crate::tax::TaxSystem;
use crate::terrain::Terrain;
use crate::types::*;
use serde::Serialize;

/// Discrete things that happened this tick, for visual effects and logs.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Event {
    Immigrate { cell: u32 },
    Emigrate { cell: u32, wealthy: bool },
    Sale { cell: u32, forced: bool },
    HarbergerBuyout { cell: u32 },
    Foreclosure { cell: u32 },
    BuildStart { cell: u32 },
    BuildDone { cell: u32 },
    Demolish { cell: u32 },
    FirmOpen { cell: u32 },
    FirmClose { cell: u32 },
    Park { cell: u32 },
    Road { cell: u32 },
    Death { cell: u32 },
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Government {
    pub cash: f64,
    /// Smoothed monthly revenue used for budgeting.
    pub revenue_ema: f64,
    pub revenue_month: f64,
    pub spend_services: f64,
    pub spend_infrastructure: f64,
    pub spend_parks: f64,
    pub spend_dividend: f64,
    pub spend_welfare: f64,
    pub spend_interest: f64,
    pub spend_compensation: f64,
    /// Reserves the programmes could not absorb, returned equally.
    pub spend_rebate: f64,
    pub rebate_per_hh: f64,
    /// Smoothed monthly construction starts (for land-supply planning).
    pub starts_ema: f64,
    /// Dividend paid per household (set at end of month, paid next month).
    pub dividend_per_hh: f64,
    /// Welfare budget available next month.
    pub welfare_budget: f64,
    /// The welfare budget this month's payments were made from.
    pub welfare_paid_budget: f64,
    pub service_budget: f64,
    pub infra_budget: f64,
    pub park_budget: f64,
    /// Welfare top-up level per eligible household (money / month).
    pub welfare_level: f64,
    /// Fraction of public job capacity funded.
    pub public_staffing: f64,
    pub roads_built: u32,
    /// Road cells laid through expropriated buildings.
    pub roads_expropriated: u32,
    pub parks_built: u32,
}

/// Monthly flow accumulators, reset each tick.
#[derive(Clone, Debug, Default)]
pub struct Flows {
    pub immigrants: u32,
    pub emigrants: u32,
    pub wealthy_emigrants: u32,
    pub capital_flight: f64,
    pub exports: f64,
    pub imports: f64,
    pub absentee_income: f64,
    pub construction_spend: f64,
    pub sales: u32,
    pub forced_sales: u32,
    pub foreclosures: u32,
    pub harberger_buyouts: u32,
    pub buildings_started: u32,
    pub buildings_completed: u32,
    pub renovations: u32,
    pub demolitions: u32,
    pub firms_opened: u32,
    pub firms_closed: u32,
    pub deaths: u32,
    pub retail_sales: f64,
    pub emissions: f64,
    pub abatement: f64,
    pub value_added: f64,
    pub wages: f64,
    /// Job seekers who would work without taxes but not with them.
    pub deterred_workers: u32,
    /// Land deals that would happen without transfer tax but did not.
    pub blocked_sales: u32,
    /// Floors built vs floors that would have been built without taxes.
    pub floors_built: u32,
    pub floors_no_tax: u32,
    /// Owners deferring maintenance.
    pub deferred_maintenance: u32,
    pub interest_in: f64,
}

pub struct World {
    pub cfg: SimConfig,
    /// `cfg.econ` in money of the day (see [`EconParams::indexed`]).
    pub econ: EconParams,
    /// Level of outside productivity and prices, `(1 + outside_growth)^year`.
    pub index: f32,
    pub dims: Dims,
    pub tick: u64,
    pub rng: Rng,
    pub terrain: Terrain,
    pub p: Parcels,
    pub f: Fields,
    pub hh: Vec<Household>,
    free_hh: Vec<u32>,
    pub firms: Vec<Firm>,
    free_firms: Vec<u32>,
    pub gov: Government,
    pub taxes: TaxSystem,
    pub flows: Flows,
    pub events: Vec<Event>,
    pub history: Vec<Metrics>,
    /// Industry export price multiplier (business cycle, AR(1)).
    pub cycle: f32,
    /// Smoothed real export volume (exports / index) and the resulting
    /// export price (downward-sloping outside demand).
    pub export_volume: f64,
    pub export_price: f32,
    /// City-wide land price index and its smoothed growth (per year).
    pub price_index: f64,
    pub price_growth: f64,
    /// Smoothed residential rent index and growth (per year).
    pub rent_index: f64,
    pub rent_growth: f64,
    pub n_alive_hh: u32,
    pub n_alive_firms: u32,
    /// Share of households without a home (excess housing demand).
    pub pressure: f32,
    /// Would-be immigrants turned away last month for lack of housing.
    pub latent_demand: u32,
    /// Available labour (unemployed + latent immigrants) per open job slot.
    pub labour_slack: f32,
    /// Share of business job slots (shops, factories) that are filled.
    pub biz_occupancy: f32,
    /// Average workers per shop and per factory.
    pub firm_size: [f32; 2],
    /// Last month's local consumer spending by home cell (net of sales tax),
    /// to be captured by shops this month.
    pub retail_pool: Vec<f32>,
    /// The founding crossing.
    pub center: usize,
    /// Average private wage per unit skill (updated each tick).
    pub wage_cache: f32,
}

impl World {
    pub fn new(cfg: SimConfig) -> World {
        let dims = Dims { w: cfg.width.max(16), h: cfg.height.max(16) };
        let mut rng = Rng::new(cfg.seed);
        let terrain = Terrain::generate(dims, &mut rng);
        let n = dims.len();
        let mut p = Parcels::new(n);
        for i in 0..n {
            if terrain.water[i] {
                p.land_use[i] = LandUse::Water;
                p.owner[i] = Owner::Gov;
            } else {
                p.nature[i] = terrain.vegetation[i];
                p.land_use[i] = LandUse::Wild;
            }
        }
        let taxes = TaxSystem::new(&cfg.policy.taxes);
        let mut w = World {
            dims,
            tick: 0,
            rng,
            terrain,
            p,
            f: Fields::new(n),
            hh: Vec::new(),
            free_hh: Vec::new(),
            firms: Vec::new(),
            free_firms: Vec::new(),
            gov: Government::default(),
            taxes,
            flows: Flows::default(),
            events: Vec::new(),
            history: Vec::new(),
            cycle: 1.0,
            export_volume: 0.0,
            export_price: 1.0,
            price_index: 1.0,
            price_growth: 0.0,
            rent_index: 1.0,
            rent_growth: 0.0,
            n_alive_hh: 0,
            n_alive_firms: 0,
            pressure: 0.0,
            latent_demand: 0,
            labour_slack: 1.0,
            biz_occupancy: 0.9,
            firm_size: [15.0, 15.0],
            retail_pool: vec![0.0; n],
            center: 0,
            wage_cache: 0.0,
            econ: cfg.econ.clone(),
            index: 1.0,
            cfg,
        };
        w.wage_cache = w.econ.base_wage;
        crate::genesis::found_settlement(&mut w);
        w
    }

    // ---------------------------------------------------------------- agents

    pub fn add_household(&mut self, h: Household) -> u32 {
        self.n_alive_hh += 1;
        if let Some(id) = self.free_hh.pop() {
            self.hh[id as usize] = h;
            id
        } else {
            self.hh.push(h);
            (self.hh.len() - 1) as u32
        }
    }

    /// Remove a household that leaves. The caller must first dispose of its
    /// parcels and firms (emigrants sell them, see `migration::emigration`).
    pub fn remove_household(&mut self, id: u32) {
        let h = &mut self.hh[id as usize];
        if !h.alive {
            return;
        }
        h.alive = false;
        let home = h.home;
        let job = h.job;
        h.home = NONE;
        h.job = NONE;
        if home != NONE {
            let o = &mut self.p.occupied[home as usize];
            *o = o.saturating_sub(1);
        }
        if job != NONE {
            let f = &mut self.firms[job as usize];
            f.workers = f.workers.saturating_sub(1);
            f.skill_sum = (f.skill_sum - self.hh[id as usize].skill).max(0.0);
        }
        self.n_alive_hh -= 1;
        self.free_hh.push(id);
    }

    pub fn add_firm(&mut self, f: Firm) -> u32 {
        self.n_alive_firms += 1;
        let parcel = f.parcel as usize;
        let id = if let Some(id) = self.free_firms.pop() {
            self.firms[id as usize] = f;
            id
        } else {
            self.firms.push(f);
            (self.firms.len() - 1) as u32
        };
        self.p.firm[parcel] = id;
        id
    }

    /// Close a firm: lay off its workers and free its premises.
    pub fn close_firm(&mut self, id: u32) {
        self.close_firms(&[id]);
    }

    /// Close several firms with a single pass over the households.
    pub fn close_firms(&mut self, ids: &[u32]) {
        let mut closing = Vec::with_capacity(ids.len());
        for &id in ids {
            let f = &mut self.firms[id as usize];
            if !f.alive {
                continue;
            }
            f.alive = false;
            f.workers = 0;
            f.skill_sum = 0.0;
            let i = f.parcel as usize;
            if self.p.firm[i] == id {
                self.p.firm[i] = NONE;
                self.p.occupied[i] = 0;
            }
            self.n_alive_firms -= 1;
            self.free_firms.push(id);
            closing.push(id);
        }
        if closing.is_empty() {
            return;
        }
        let mut closed = vec![false; self.firms.len()];
        for &id in &closing {
            closed[id as usize] = true;
        }
        for h in self.hh.iter_mut() {
            if h.alive && h.job != NONE && closed[h.job as usize] {
                h.job = NONE;
                h.wage = 0.0;
            }
        }
    }

    pub fn year(&self) -> f32 {
        self.tick as f32 / 12.0
    }

    /// Advance the price/productivity index to the current tick.
    pub fn update_index(&mut self) {
        self.index = (1.0 + self.cfg.econ.outside_growth).powf(self.year());
        self.econ = self.cfg.econ.indexed(self.index);
        // Traffic: the more people commute, the slower (and dearer) every trip.
        let e = &mut self.econ;
        e.industry_price *= self.export_price;
        let crowd = (self.n_alive_hh as f32 / e.congestion_scale.max(1.0)).powf(e.congestion_elasticity);
        e.commute_cost_per_cell *= 1.0 + crowd;
        self.taxes.set_index(self.index as f64);
    }

    /// Outside option (monthly money-metric utility) for a given skill.
    pub fn outside_utility(&self, skill: f32) -> f32 {
        let e = &self.econ;
        e.outside_utility * skill.powf(0.85)
    }

    pub fn owner_cash(&self, o: Owner) -> f64 {
        match o {
            Owner::Hh(id) => self.hh[id as usize].cash,
            _ => f64::INFINITY,
        }
    }

    /// Credit `amount` to an owner. Absentee income leaves the city.
    pub fn pay_owner(&mut self, o: Owner, amount: f64) {
        match o {
            Owner::Hh(id) => {
                let h = &mut self.hh[id as usize];
                h.cash += amount;
                h.income_capital += amount as f32;
            }
            Owner::Gov => self.gov.cash += amount,
            Owner::Absentee => self.flows.absentee_income += amount,
        }
    }

    /// Market value estimate of a parcel (assessed land + building).
    pub fn parcel_value(&self, i: usize) -> f64 {
        (self.p.assessed_land[i] + self.p.assessed_bldg[i]) as f64
    }

    pub fn push_event(&mut self, e: Event) {
        if self.events.len() < 4096 {
            self.events.push(e);
        }
    }
}
