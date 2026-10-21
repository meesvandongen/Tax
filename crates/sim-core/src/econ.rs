//! Property valuation and development feasibility — the heart of the land
//! market. Every owner, buyer and developer values parcels with these
//! functions, using *their own* discount rate and the *actual* tax schedule.
//!
//! Value of an income property (growing perpetuity of after-tax cash flow):
//!
//! ```text
//!   V (r − g) = 12·NOI·(1 − t_inc) − τ_L·L_a − τ_B·B − (h + w)·V
//!   ⇒ V = (12·NOI·(1 − t_inc) − τ_L·L_a − τ_B·B) / (r − g + h + w)
//! ```
//!
//! * `L_a` is the parcel's *assessed* land value. The assessor values land
//!   by comparables ("as if vacant"), so nothing the owner builds or
//!   maintains changes it: a land tax is a fixed charge per parcel. It
//!   still capitalises into lower prices — buyers deduct its present value,
//!   sales feed the comparables, assessments fall — but through the market
//!   loop rather than a formula.
//! * `B` is the building value; building taxes scale with every floor.
//! * `h` is a Harberger (self-assessed) rate: declared ≈ own value.
//! * `t_inc` is the owner's tax rate on rental income, `w` their marginal
//!   wealth-tax rate.

use crate::types::*;
use crate::world::World;

pub const ABSENTEE_PREMIUM: f64 = 0.015;
pub const RESIDENT_PREMIUM: f64 = 0.035;

pub const FLOOR_OPTIONS: &[u8] = &[1, 2, 3, 4, 5, 6, 8, 10, 12, 15, 20, 25, 30, 40];

/// The financial perspective of a (prospective) owner.
#[derive(Clone, Copy, Debug)]
pub struct Investor {
    /// Annual discount rate.
    pub r: f64,
    /// Expected annual growth of rents.
    pub g: f64,
    /// Tax rate on net rental income.
    pub t_inc: f64,
    /// Marginal annual wealth-tax rate.
    pub w: f64,
    /// Ignore property-type taxes (counterfactual "no-tax world").
    pub no_tax: bool,
}

impl Investor {
    /// The same investor in a world without taxes (for distortion metrics).
    pub fn untaxed(&self) -> Investor {
        Investor { t_inc: 0.0, w: 0.0, no_tax: true, ..*self }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DevOption {
    pub land_use: LandUse,
    pub floors: u8,
    pub units: u16,
    pub cost: f64,
    /// Value of the completed property.
    pub value: f64,
    /// Residual: value − construction − demolition.
    pub residual: f64,
}

pub fn units_for(w: &World, u: LandUse, floors: u8) -> u16 {
    let e = &w.cfg.econ;
    let per = match u {
        LandUse::Residential => e.res_units_per_floor,
        LandUse::Commercial => e.com_jobs_per_floor,
        LandUse::Industrial => e.ind_jobs_per_floor,
        LandUse::Civic => 12,
        _ => 0,
    };
    per.saturating_mul(floors as u16)
}

pub fn construction_cost(w: &World, i: usize, u: LandUse, floors: u8) -> f64 {
    let e = &w.cfg.econ;
    let use_factor = match u {
        LandUse::Residential => 1.0,
        LandUse::Commercial => 1.1,
        LandUse::Industrial => 0.75,
        LandUse::Civic => 1.0,
        _ => 0.0,
    };
    let slope = 1.0 + (w.terrain.slope[i] as f64 / 4.0);
    e.cost_per_floor as f64 * (floors as f64).powf(1.0 + e.height_premium as f64) * use_factor * slope
}

/// Depreciated replacement cost of the current building.
pub fn building_value(w: &World, i: usize) -> f64 {
    let u = w.p.land_use[i];
    if !u.is_building() {
        return 0.0;
    }
    construction_cost(w, i, u, w.p.floors[i]) * (0.3 + 0.7 * w.p.condition[i] as f64)
}

/// Expected rent per unit / job slot for a building of `units` here.
pub fn market_rent(w: &World, i: usize, u: LandUse, units: u16) -> f32 {
    match u {
        LandUse::Residential => w.f.rent_res[i],
        LandUse::Commercial | LandUse::Industrial => firm_bid_rent(w, i, u, units),
        _ => 0.0,
    }
}

/// The most a typical firm could pay per job slot for premises with
/// `slots` jobs here: revenue per worker at that scale, minus labour cost,
/// emission tax and a normal margin. Shops' revenue per worker falls as
/// they add capacity (they share local spending with competitors, Huff
/// model); factories' average product falls with scale.
pub fn firm_bid_rent(w: &World, i: usize, u: LandUse, slots: u16) -> f32 {
    let e = &w.cfg.econ;
    let wage = w.market_wage_rate() * (1.0 + w.taxes.payroll_rate() as f32);
    let k = slots.max(1) as f32;
    let revenue = match u {
        LandUse::Commercial => {
            let s = w.f.retail_supply[i] + crate::fields::OUTSIDE_SHOP_CAPACITY;
            0.92 * w.f.retail_draw[i] * s / (s + crate::fields::SHOP_REACH_SHARE * k)
        }
        LandUse::Industrial => {
            let growth = (1.0 + e.outside_growth).powf(w.year());
            let a = e.industry_output_per_worker
                * e.industry_price
                * cycle_outlook(w)
                * growth
                * (1.0 + e.agglomeration * (1.0 + w.f.industry_access[i]).ln());
            let em = w.taxes.estimate(|m| m.emission(e.pollution_per_worker as f64)) as f32;
            a * k.powf(e.labour_elasticity - 1.0) - em
        }
        _ => 0.0,
    };
    let pre_tax = (revenue - wage * 1.05).max(0.0);
    pre_tax - w.taxes.profit_monthly(pre_tax as f64) as f32
}

/// The export price expected over a typical ten-year occupancy. The cycle
/// is mean-reverting (AR(1)), so a boom or slump fades within a few years:
/// premises are priced through the cycle, not at today's price.
pub fn cycle_outlook(w: &World) -> f32 {
    let rho = crate::sim::CYCLE_PERSISTENCE;
    let months = 120.0;
    let mean_weight = (1.0 - rho.powf(months)) / (months * (1.0 - rho));
    1.0 + mean_weight * (w.cycle - 1.0)
}

/// (local occupancy, local units, local units under construction)
fn local_occ(w: &World, i: usize, u: LandUse) -> (f32, f32, f32) {
    match u {
        LandUse::Residential => (w.f.occ_res[i], w.f.units_res[i], w.f.pipeline_res[i]),
        LandUse::Commercial => (w.f.occ_com[i], w.f.units_com[i], w.f.pipeline_com[i]),
        LandUse::Industrial => (w.f.occ_ind[i], w.f.units_ind[i], w.f.pipeline_ind[i]),
        _ => (0.0, 0.0, 0.0),
    }
}

/// Expected occupancy if `new_units` are added here: developers anticipate
/// that flooding a neighbourhood with space lowers occupancy.
pub fn expected_occupancy(w: &World, i: usize, u: LandUse, new_units: u16) -> f64 {
    let (occ, local_units, pipeline) = local_occ(w, i, u);
    // Business space is let whenever the bid-rent is positive (already
    // scale-adjusted), so only competition from the pipeline matters.
    // Tight labour markets make new business space harder (not impossible)
    // to fill: tenants can still bid workers away with higher wages.
    let occ = if u == LandUse::Residential { occ } else { 0.92 * (0.35 + 0.65 * w.labour_slack.min(1.0)) };
    // New space competes with what is already being built nearby.
    let absorption = 0.6 * (new_units as f32 + pipeline) / (local_units + 25.0);
    let demand_boost = if u == LandUse::Residential { w.housing_pressure() } else { 0.0 };
    ((occ + demand_boost) - absorption).clamp(0.15, 0.97) as f64
}

/// Whether zoning / height rules permit this use here.
pub fn permitted(w: &World, i: usize, u: LandUse, floors: u8) -> bool {
    let pol = &w.cfg.policy;
    if pol.height_limit > 0 && floors > pol.height_limit {
        return false;
    }
    if floors > w.cfg.econ.max_floors {
        return false;
    }
    if u == LandUse::Industrial && floors > 3 {
        return false;
    }
    if pol.zoning {
        let ring = w.dims.ring(i, 2);
        match u {
            LandUse::Residential => !ring.iter().any(|&j| w.p.land_use[j] == LandUse::Industrial),
            LandUse::Industrial => !ring.iter().any(|&j| w.p.land_use[j] == LandUse::Residential),
            _ => true,
        }
    } else {
        true
    }
}

pub fn capitalize(w: &World, inv: &Investor, i: usize, noi_month: f64, bldg: f64, idle_share: f64) -> f64 {
    let t = &w.taxes;
    let land = w.p.assessed_land[i] as f64;
    // Public land in the city is exempt; land outside the boundary is valued
    // as if annexed, since developing it would bring it into the city.
    let exempt = inv.no_tax || (w.p.in_city[i] && !crate::property::taxable(w, i));
    let (fixed_tax, harberger) = if exempt {
        (0.0, 0.0)
    } else {
        (t.estimate(|m| m.property(land, bldg) + m.vacancy(land + bldg, idle_share)), t.harberger_rate())
    };
    let denom = (inv.r - inv.g + harberger + inv.w).max(0.01);
    ((12.0 * noi_month * (1.0 - inv.t_inc) - fixed_tax) / denom).max(0.0)
}

/// Net operating income per month before property taxes and income tax.
pub fn noi(w: &World, i: usize, u: LandUse, units: u16, occ: f64, bldg_cost: f64) -> f64 {
    let rent = market_rent(w, i, u, units) as f64;
    units as f64 * occ * rent - w.cfg.econ.maintenance_rate as f64 * bldg_cost
}

/// Value of the parcel in its current state (no redevelopment).
pub fn current_use_value(w: &World, i: usize, inv: &Investor) -> f64 {
    let p = &w.p;
    let u = p.land_use[i];
    match u {
        LandUse::Residential | LandUse::Commercial | LandUse::Industrial => {
            let bldg = building_value(w, i);
            let units = p.units[i];
            let occ = if units > 0 {
                (p.occupied[i] as f64 / units as f64).max(expected_occupancy(w, i, u, 0) * 0.8)
            } else {
                0.0
            };
            // Rent actually achievable is capped by building condition.
            let quality = 0.6 + 0.4 * p.condition[i] as f64;
            let n = noi(w, i, u, units, occ, bldg) * quality;
            let market = capitalize(w, inv, i, n, bldg, 1.0 - occ);
            // Let business premises are worth at least the lease the sitting
            // tenant pays, as an appraiser would value them, not only what a
            // newcomer might bid in today's labour market.
            let leased = leased_slots(w, i);
            if leased > 0 {
                let n = p.asking_rent[i] as f64 * leased as f64 - w.cfg.econ.maintenance_rate as f64 * bldg;
                let idle = 1.0 - leased as f64 / units.max(1) as f64;
                market.max(capitalize(w, inv, i, n, bldg, idle))
            } else {
                market
            }
        }
        LandUse::Farm | LandUse::Wild => {
            let farm = w.cfg.econ.farm_rent as f64 * (0.4 + w.terrain.fertility[i] as f64);
            let n = if u == LandUse::Farm { farm } else { farm * 0.5 };
            capitalize(w, inv, i, n, 0.0, 0.0)
        }
        LandUse::Vacant => capitalize(w, inv, i, 0.0, 0.0, 1.0),
        LandUse::Construction => {
            // Valued as the finished project.
            let tu = p.target_use[i];
            let units = units_for(w, tu, p.floors[i]);
            let occ = expected_occupancy(w, i, tu, 0);
            let cost = construction_cost(w, i, tu, p.floors[i]);
            capitalize(w, inv, i, noi(w, i, tu, units, occ, cost), cost, 1.0 - occ)
        }
        _ => 0.0,
    }
}

/// Cost of clearing the site: demolishing a building or felling woodland.
pub fn demolition_cost(w: &World, i: usize) -> f64 {
    let p = &w.p;
    if p.land_use[i].is_building() {
        w.cfg.econ.demolition_cost as f64 * (1.0 + p.floors[i] as f64 * 0.3)
    } else if p.land_use[i] == LandUse::Wild {
        w.cfg.econ.demolition_cost as f64 * 0.2 * p.nature[i] as f64
    } else {
        0.0
    }
}

/// Space a business tenant leases (its workers, but at least a quarter of
/// the premises; see `production::run`).
pub fn leased_slots(w: &World, i: usize) -> u16 {
    if w.p.firm[i] == NONE {
        0
    } else {
        w.p.occupied[i].max(w.p.units[i] / 4)
    }
}

/// What the owner must pay sitting tenants to clear the building:
/// relocation allowances for households, lease buy-outs for firms.
pub fn tenant_compensation(w: &World, i: usize) -> f64 {
    let months = w.cfg.econ.relocation_months as f64;
    let rent = w.p.asking_rent[i] as f64;
    match w.p.land_use[i] {
        LandUse::Residential => months * rent * w.p.occupied[i] as f64,
        LandUse::Commercial | LandUse::Industrial => 2.0 * months * rent * leased_slots(w, i) as f64,
        _ => 0.0,
    }
}

/// Best redevelopment option for this parcel from `inv`'s perspective.
pub fn best_development(w: &World, i: usize, inv: &Investor) -> Option<DevOption> {
    let p = &w.p;
    if !p.road_access[i] || !p.land_use[i].is_developable() || crate::government::on_street_plan(w, i) {
        return None;
    }
    let demolition = demolition_cost(w, i) + tenant_compensation(w, i);
    let mut best: Option<DevOption> = None;
    for &u in &[LandUse::Residential, LandUse::Commercial, LandUse::Industrial] {
        for &floors in FLOOR_OPTIONS {
            if !permitted(w, i, u, floors) {
                continue;
            }
            let units = units_for(w, u, floors);
            let occ = expected_occupancy(w, i, u, units);
            let cost = construction_cost(w, i, u, floors);
            let n = noi(w, i, u, units, occ, cost);
            if n <= 0.0 {
                continue;
            }
            let value = capitalize(w, inv, i, n, cost, 1.0 - occ);
            let residual = value - cost - demolition;
            if best.map_or(true, |b| residual > b.residual) {
                best = Some(DevOption { land_use: u, floors, units, cost, value, residual });
            }
        }
    }
    best
}

/// What `inv` would pay for the land + building as is, including the option
/// to redevelop.
pub fn valuation(w: &World, i: usize, inv: &Investor) -> f64 {
    let cur = current_use_value(w, i, inv);
    match best_development(w, i, inv) {
        // Redevelopment destroys the current building's value.
        Some(d) if d.residual > cur => d.residual,
        _ => cur,
    }
}

/// Expected growth term shared by all investors (adaptive expectations).
pub fn expected_growth(w: &World, r: f64) -> f64 {
    let e = &w.cfg.econ;
    let trend = w.rent_growth.max(w.price_growth * 0.5);
    (e.outside_growth as f64 + e.speculation as f64 * trend).clamp(-0.03, r - 0.035)
}

impl World {
    /// Outside capital: requires the outside return plus a premium for
    /// managing remote property.
    pub fn absentee_investor(&self) -> Investor {
        let r = self.cfg.econ.outside_return as f64 + ABSENTEE_PREMIUM;
        Investor { r, g: expected_growth(self, r), t_inc: 0.0, w: 0.0, no_tax: false }
    }

    pub fn household_investor(&self, id: u32) -> Investor {
        let h = &self.hh[id as usize];
        // Residents compare property with their deposit alternative plus a risk premium.
        let r = self.cfg.econ.deposit_return as f64 + RESIDENT_PREMIUM;
        let lab = h.income_labor as f64;
        let cap = h.income_capital as f64;
        // Marginal tax on an extra 1000/month of rental income.
        let t_inc = ((self.taxes.income_monthly(lab, cap + 1000.0) - self.taxes.income_monthly(lab, cap)) / 1000.0)
            .clamp(0.0, 0.95);
        Investor { r, g: expected_growth(self, r), t_inc, w: self.taxes.wealth_marginal(h.wealth), no_tax: false }
    }

    pub fn investor_for(&self, o: Owner) -> Investor {
        match o {
            Owner::Hh(id) => self.household_investor(id),
            _ => self.absentee_investor(),
        }
    }

    /// Excess housing demand signal: unhoused households relative to stock.
    /// Cached once per tick in `World::pressure`.
    pub fn housing_pressure(&self) -> f32 {
        self.pressure
    }

    pub fn update_labour_slack(&mut self) {
        let unemployed = self.hh.iter().filter(|h| h.alive && h.job == NONE).count() as f32;
        let mut open_slots = 0f32;
        for i in 0..self.p.len() {
            if matches!(self.p.land_use[i], LandUse::Commercial | LandUse::Industrial) {
                open_slots += self.p.vacant_units(i) as f32;
            }
            if self.p.land_use[i] == LandUse::Construction
                && matches!(self.p.target_use[i], LandUse::Commercial | LandUse::Industrial)
            {
                open_slots += units_for(self, self.p.target_use[i], self.p.floors[i]) as f32;
            }
        }
        let available = unemployed + 0.5 * self.latent_demand as f32 + 0.02 * self.n_alive_hh as f32;
        self.labour_slack = (available / (open_slots + 10.0)).clamp(0.05, 2.0);
    }

    pub fn update_pressure(&mut self) {
        let unhoused = self.hh.iter().filter(|h| h.alive && h.home == NONE).count() as f32;
        let waiting = unhoused + self.latent_demand as f32;
        self.pressure = (waiting / (self.n_alive_hh as f32 + 50.0)).min(0.3);
    }
}
