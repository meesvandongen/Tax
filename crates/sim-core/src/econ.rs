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
    /// What each owner-occupier is paid for their flat when the site is
    /// cleared (0: the standard buyout, [`flat_buyout`]).
    pub flat_price: f64,
}

pub fn units_for(w: &World, u: LandUse, floors: u8) -> u16 {
    let e = &w.econ;
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
    let e = &w.econ;
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

/// Years after which a building's upkeep has doubled: systems wear out and
/// the layout dates, so old buildings cost more to keep up and are
/// eventually worth replacing. Renovation halves the effective age.
pub const OBSOLESCENCE_YEARS: f64 = 60.0;

/// Upkeep multiplier for a building of this age.
pub fn ageing(age_months: u32) -> f64 {
    1.0 + age_months as f64 / 12.0 / OBSOLESCENCE_YEARS
}

/// Monthly cost of keeping the current building in shape.
pub fn upkeep(w: &World, i: usize) -> f64 {
    let p = &w.p;
    w.econ.maintenance_rate as f64 * construction_cost(w, i, p.land_use[i], p.floors[i]) * ageing(p.age_months[i])
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
        LandUse::Commercial => comparable_rent(firm_bid_rent(w, i, u, units), w.f.obs_com[i], w.f.evid_com[i]),
        LandUse::Industrial => comparable_rent(firm_bid_rent(w, i, u, units), w.f.obs_ind[i], w.f.evid_ind[i]),
        _ => 0.0,
    }
}

/// Business rent: the bid-rent of a typical firm for premises of this size,
/// but never above what tenants nearby actually pay (allowing more headroom
/// where the evidence is thin). A bid-rent nobody pays is not a market rent.
fn comparable_rent(bid: f32, observed: f32, evidence: f32) -> f32 {
    if evidence < 0.02 {
        return bid;
    }
    bid.min(observed * (1.0 + 1.5 * (1.0 - evidence)))
}

/// The most a typical firm could pay per job slot for premises with
/// `slots` jobs here: revenue per worker at that scale, minus labour cost,
/// emission tax and a normal margin. Shops' revenue per worker falls as
/// they add capacity (they share local spending with competitors, Huff
/// model); factories' average product falls with scale.
pub fn firm_bid_rent(w: &World, i: usize, u: LandUse, slots: u16) -> f32 {
    match u {
        // Commercial space goes to shops or offices, whichever bids more.
        LandUse::Commercial => {
            sector_bid_rent(w, i, Sector::Retail, slots).max(sector_bid_rent(w, i, Sector::Office, slots))
        }
        LandUse::Industrial => sector_bid_rent(w, i, Sector::Industry, slots),
        _ => 0.0,
    }
}

/// Whether offices outbid shops for commercial premises of this size here.
pub fn offices_lead(w: &World, i: usize, slots: u16) -> bool {
    sector_bid_rent(w, i, Sector::Office, slots) > sector_bid_rent(w, i, Sector::Retail, slots)
}

/// [`firm_bid_rent`] of a typical firm of one sector.
pub fn sector_bid_rent(w: &World, i: usize, sector: Sector, slots: u16) -> f32 {
    let e = &w.econ;
    let wage = w.market_wage_rate() * (1.0 + w.taxes.payroll_rate() as f32);
    let k = slots.max(1) as f32;
    let revenue = match sector {
        Sector::Office => {
            if e.office_output_per_worker <= 0.0 {
                return 0.0;
            }
            let a = e.office_output_per_worker * (1.0 + e.office_agglomeration * (1.0 + w.f.office_access[i]).ln());
            a * k.powf(e.office_labour_elasticity - 1.0)
        }
        Sector::Retail => {
            let s = w.f.retail_supply[i] + crate::fields::OUTSIDE_SHOP_CAPACITY;
            let share = w.f.retail_draw[i] * s / (s + crate::fields::SHOP_REACH_SHARE * k);
            0.92 * share.min(e.retail_output_per_worker)
        }
        Sector::Industry => {
            let a = e.industry_output_per_worker
                * e.industry_price
                * cycle_outlook(w)
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

/// Expected occupancy, once let, of a building with `new_units` added here.
/// New space competes with what is already being built nearby; how long the
/// new units themselves take to let is [`lease_up_loss`].
pub fn expected_occupancy(w: &World, i: usize, u: LandUse, new_units: u16) -> f64 {
    let (occ, local_units, pipeline) = local_occ(w, i, u);
    // Business space is let whenever the bid-rent is positive (already
    // scale-adjusted), so only competition from the pipeline matters.
    // Tight labour markets make new business space harder (not impossible)
    // to fill: tenants can still bid workers away with higher wages.
    // Developers also look at how well existing business space is let,
    // nearby and city-wide: empty premises compete with new ones.
    // A business building houses one firm, and firms only grow so large:
    // floors beyond what a typical tenant staffs stand empty.
    let occ = if u == LandUse::Residential {
        occ
    } else {
        let labour = 0.92 * (0.35 + 0.65 * w.labour_slack.min(1.0));
        // (Offices let floor by floor, or one firm grows into the building:
        // their returns to scale fall slowly.)
        let office = u == LandUse::Commercial && offices_lead(w, i, new_units.max(1));
        let typical = if u == LandUse::Commercial { w.firm_size[0] } else { w.firm_size[1] };
        let fill = if new_units > 0 && !office { (typical / new_units as f32).min(1.0) } else { 1.0 };
        labour.min(occ).min(w.biz_occupancy + 0.05).min(fill)
    };
    // New space competes with what is already being built nearby.
    let absorption = 0.6 * pipeline / (local_units + 25.0);
    let demand_boost = if u == LandUse::Residential { w.housing_pressure() } else { 0.0 };
    ((occ + demand_boost) - absorption).clamp(0.02, 0.97) as f64
}

/// Rent lost while `new_units` of new space here are let. A new building
/// competes in the whole city's market, not only its block: space fills at a
/// pace that grows with the neighbourhood and the city (movers and
/// newcomers), so a tall building takes months, not forever, to let.
/// (Treating the first tenants' scarcity as permanent vacancy made every
/// tall building look a third empty forever.)
pub fn lease_up_loss(w: &World, i: usize, u: LandUse, new_units: u16, occ: f64) -> f64 {
    if new_units == 0 {
        return 0.0;
    }
    let (_, local_units, _) = local_occ(w, i, u);
    let per_month = 4.0 + 0.03 * local_units as f64 + 0.0005 * w.n_alive_hh as f64;
    let months = new_units as f64 / per_month;
    0.5 * months * new_units as f64 * occ * market_rent(w, i, u, new_units) as f64
}

/// Whether zoning / height rules permit this use here.
pub fn permitted(w: &World, i: usize, u: LandUse, floors: u8) -> bool {
    let pol = &w.cfg.policy;
    if pol.height_limit > 0 && floors > pol.height_limit {
        return false;
    }
    if floors > w.econ.max_floors {
        return false;
    }
    if u == LandUse::Industrial && floors > 3 {
        return false;
    }
    if pol.zoning {
        let ring = w.mesh.near(i, 2.5);
        match u {
            LandUse::Residential => !ring.iter().any(|&j| w.p.land_use[j as usize] == LandUse::Industrial),
            LandUse::Industrial => !ring.iter().any(|&j| w.p.land_use[j as usize] == LandUse::Residential),
            _ => true,
        }
    } else {
        true
    }
}

pub fn capitalize(w: &World, inv: &Investor, i: usize, noi_month: f64, bldg: f64, idle_share: f64) -> f64 {
    capitalize_at_land(w, inv, i, noi_month, bldg, idle_share, w.p.assessed_land[i] as f64)
}

/// [`capitalize`] with the parcel's land assessed at `land`.
pub fn capitalize_at_land(
    w: &World,
    inv: &Investor,
    i: usize,
    noi_month: f64,
    bldg: f64,
    idle_share: f64,
    land: f64,
) -> f64 {
    let t = &w.taxes;
    // Land in public use is exempt; land outside the boundary is valued as
    // if annexed, since developing it would bring it into the city. (Who
    // owns the parcel now does not matter: a buyer will owe the tax.)
    let exempt =
        inv.no_tax || matches!(w.p.land_use[i], LandUse::Water | LandUse::Road | LandUse::Park | LandUse::Civic);
    let (fixed_tax, harberger) = if exempt {
        (0.0, 0.0)
    } else {
        (t.estimate(|m| m.property(land, bldg) + m.vacancy(land + bldg, idle_share)), t.harberger_rate())
    };
    let denom = (inv.r - inv.g + harberger + inv.w).max(0.01);
    ((12.0 * noi_month * (1.0 - inv.t_inc) - fixed_tax) / denom).max(0.0)
}

/// Share of a new building's rent that a building in `condition` (0 =
/// ruin, 1 = new) can command. Steep near ruin: a slum dwelling lets for a
/// fraction of the rent of a sound one, so maintenance pays most where a
/// building is most run down, and conditions settle instead of collapsing.
pub fn rent_quality(condition: f32) -> f64 {
    (condition.clamp(0.0, 1.0) as f64).sqrt()
}

fn gross_rent(w: &World, i: usize, u: LandUse, units: u16, occ: f64) -> f64 {
    units as f64 * occ * market_rent(w, i, u, units) as f64
}

/// Net operating income per month before property taxes and income tax.
pub fn noi(w: &World, i: usize, u: LandUse, units: u16, occ: f64, bldg_cost: f64) -> f64 {
    let rent = market_rent(w, i, u, units) as f64;
    units as f64 * occ * rent - w.econ.maintenance_rate as f64 * bldg_cost
}

/// Value of the parcel in its current state (no redevelopment).
pub fn current_use_value(w: &World, i: usize, inv: &Investor) -> f64 {
    let p = &w.p;
    let u = p.land_use[i];
    match u {
        // The parcel owner holds only the dwellings not sold as flats.
        LandUse::Residential | LandUse::Commercial | LandUse::Industrial => {
            p.freehold_share(i) * building_in_condition_value(w, i, inv, p.condition[i])
        }
        LandUse::Farm | LandUse::Wild => {
            let farm = w.econ.farm_rent as f64 * (0.4 + w.terrain.fertility[i] as f64);
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

/// Value of the whole existing building (freehold and flats) as an income
/// property if it were in `condition` (0 = derelict, 1 = new).
pub fn building_in_condition_value(w: &World, i: usize, inv: &Investor, condition: f32) -> f64 {
    building_value_aged(w, i, inv, condition, w.p.age_months[i])
}

/// Value of the whole building just after a renovation: new condition, and
/// half its effective age (see [`OBSOLESCENCE_YEARS`]).
pub fn renovated_value(w: &World, i: usize, inv: &Investor) -> f64 {
    building_value_aged(w, i, inv, 1.0, w.p.age_months[i] / 2)
}

fn building_value_aged(w: &World, i: usize, inv: &Investor, condition: f32, age_months: u32) -> f64 {
    let p = &w.p;
    let u = p.land_use[i];
    let bldg = construction_cost(w, i, u, p.floors[i]) * (0.3 + 0.7 * condition as f64);
    let units = p.units[i];
    // Empty space is expected to let again, unless the building is too
    // run-down for anyone to move in.
    let unlettable = u == LandUse::Residential && condition <= crate::housing::LETTABLE_CONDITION;
    let occ = if units == 0 {
        0.0
    } else if unlettable {
        p.occupied[i] as f64 / units as f64
    } else {
        (p.occupied[i] as f64 / units as f64).max(expected_occupancy(w, i, u, 0) * 0.8)
    };
    // Rent actually achievable falls with the building's condition, and an
    // old building costs more to keep up.
    let ageing_cost =
        w.econ.maintenance_rate as f64 * construction_cost(w, i, u, p.floors[i]) * (ageing(age_months) - 1.0);
    let n = noi(w, i, u, units, occ, bldg) + (rent_quality(condition) - 1.0) * gross_rent(w, i, u, units, occ)
        - ageing_cost;
    let market = capitalize(w, inv, i, n, bldg, 1.0 - occ);
    // Let business premises are worth at least the lease the sitting tenant
    // pays, as an appraiser would value them, not only what a newcomer might
    // bid in today's labour market.
    let leased = leased_slots(w, i);
    if leased > 0 {
        // (A tenant in a crumbling building renegotiates its lease down.)
        let n =
            p.asking_rent[i] as f64 * leased as f64 * rent_quality(condition) - w.econ.maintenance_rate as f64 * bldg;
        let idle = 1.0 - leased as f64 / units.max(1) as f64;
        market.max(capitalize(w, inv, i, n, bldg, idle))
    } else {
        market
    }
}

/// Cost of restoring the building on `i` to new condition: renovation is
/// cheaper than rebuilding (half the construction cost for a ruin).
pub fn renovation_cost(w: &World, i: usize) -> f64 {
    let p = &w.p;
    0.5 * (1.0 - p.condition[i] as f64) * construction_cost(w, i, p.land_use[i], p.floors[i])
}

/// Cost of clearing the site: demolishing a building or felling woodland.
pub fn demolition_cost(w: &World, i: usize) -> f64 {
    let p = &w.p;
    if p.land_use[i].is_building() {
        w.econ.demolition_cost as f64 * (1.0 + p.floors[i] as f64 * 0.3)
    } else if p.land_use[i] == LandUse::Wild {
        w.econ.demolition_cost as f64 * 0.2 * p.nature[i] as f64
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
/// relocation allowances for households, lease buy-outs for firms, and the
/// market value of owner-occupied flats plus a premium.
pub fn tenant_compensation(w: &World, i: usize) -> f64 {
    let months = w.econ.relocation_months as f64;
    let rent = w.p.asking_rent[i] as f64;
    match w.p.land_use[i] {
        LandUse::Residential => {
            let owned = w.p.owned_flats[i] as f64;
            let flats = if owned > 0.0 { owned * flat_buyout(w, i) } else { 0.0 };
            months * rent * (w.p.occupied[i] as f64 - owned) + flats
        }
        LandUse::Commercial | LandUse::Industrial => 2.0 * months * rent * leased_slots(w, i) as f64,
        _ => 0.0,
    }
}

/// Premium over market value paid to flat owners whose building is
/// cleared (a collective sale or compulsory purchase).
pub const FLAT_BUYOUT_PREMIUM: f64 = 0.2;

/// Market value of one dwelling in residential building `i`: what outside
/// capital would pay for it as a let flat.
pub fn flat_value(w: &World, i: usize) -> f64 {
    let units = w.p.units[i].max(1) as f64;
    building_in_condition_value(w, i, &w.absentee_investor(), w.p.condition[i]) / units
}

/// What a developer pays an owner-occupier to clear their flat.
pub fn flat_buyout(w: &World, i: usize) -> f64 {
    (1.0 + FLAT_BUYOUT_PREMIUM) * flat_value(w, i)
}

/// Extra return a household wants for tying its savings up in its own
/// home: one undiversified, illiquid asset in a place it may have to leave.
pub const OWNER_OCCUPIER_PREMIUM: f64 = 0.01;

/// What the flat at `i` is worth to `inv` living in it: the rent it saves
/// (untaxed and never vacant), less its share of upkeep and property taxes.
pub fn owner_occupier_value(w: &World, i: usize, inv: &Investor) -> f64 {
    let p = &w.p;
    let units = p.units[i].max(1) as f64;
    let bldg = building_value(w, i);
    let upkeep = upkeep(w, i);
    let rent = market_rent(w, i, p.land_use[i], p.units[i]) as f64 * rent_quality(p.condition[i]);
    let n = units * rent - upkeep;
    let own = Investor { t_inc: 0.0, r: inv.r + OWNER_OCCUPIER_PREMIUM, ..*inv };
    capitalize(w, &own, i, n, bldg, crate::property::idle_share(w, i)) / units
}

/// Cost premium for building on top of an existing, occupied building
/// (strengthening, access, working around residents).
pub const EXTENSION_PREMIUM: f64 = 1.25;
/// Floors can be added up to this height (taller needs new foundations).
pub const EXTENSION_MAX_FLOORS: u8 = 8;

/// Adding floors: the best of putting 1 or 2 floors of the same use on top
/// of a sound building, renovating the rest at the same time. No
/// demolition, and the residents stay. Returns (floors added, cost, value of
/// the whole extended building to `inv`), if any extension adds value.
pub fn best_extension(w: &World, i: usize, inv: &Investor) -> Option<(u8, f64, f64)> {
    let p = &w.p;
    let u = p.land_use[i];
    if !matches!(u, LandUse::Residential | LandUse::Commercial) || p.condition[i] < 0.5 || !p.road_access[i] {
        return None;
    }
    let floors = p.floors[i];
    let now = building_in_condition_value(w, i, inv, p.condition[i]);
    let mut best: Option<(u8, f64, f64)> = None;
    for add in 1..=2u8 {
        let to = floors + add;
        if to > EXTENSION_MAX_FLOORS || !permitted(w, i, u, to) {
            break;
        }
        let bcost = construction_cost(w, i, u, to);
        let cost = EXTENSION_PREMIUM * (bcost - construction_cost(w, i, u, floors)) + renovation_cost(w, i);
        let units = units_for(w, u, to);
        let added = units.saturating_sub(p.units[i]);
        let occ = expected_occupancy(w, i, u, added);
        let n = noi(w, i, u, units, occ, bcost);
        let value = capitalize(w, inv, i, n, bcost, 1.0 - occ) - lease_up_loss(w, i, u, added, occ);
        if value - now - cost > best.map_or(0.0, |b| b.2 - now - b.1) {
            best = Some((add, cost, value));
        }
    }
    best
}

/// Best redevelopment option for this parcel from `inv`'s perspective.
pub fn best_development(w: &World, i: usize, inv: &Investor) -> Option<DevOption> {
    let p = &w.p;
    if !p.road_access[i] || !p.land_use[i].is_developable() {
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
            let value = capitalize(w, inv, i, n, cost, 1.0 - occ) - lease_up_loss(w, i, u, units, occ);
            let residual = value - cost - demolition;
            if best.map_or(true, |b| residual > b.residual) {
                best = Some(DevOption { land_use: u, floors, units, cost, value, residual, flat_price: 0.0 });
            }
        }
    }
    best
}

/// Value of the site as if vacant: the best project's value minus its
/// construction cost, with no building to demolish. This is what the land
/// alone is worth to `inv` (zero if no project pays), and what assessors
/// estimate for land value taxes: it does not depend on what the owner
/// has actually built.
pub fn vacant_site_value(w: &World, i: usize, inv: &Investor) -> f64 {
    vacant_site_value_at_land(w, i, inv, w.p.assessed_land[i] as f64)
}

/// [`vacant_site_value`] if the land were assessed at `land`.
pub fn vacant_site_value_at_land(w: &World, i: usize, inv: &Investor, land: f64) -> f64 {
    if !w.p.road_access[i] {
        return 0.0;
    }
    serviced_site_value(w, i, inv, land)
}

/// What the site would be worth as if vacant were it served by a street,
/// with its land assessed at `land` (the value a new road unlocks).
pub fn serviced_site_value(w: &World, i: usize, inv: &Investor, land: f64) -> f64 {
    if matches!(w.p.land_use[i], LandUse::Water | LandUse::Road | LandUse::Park | LandUse::Civic) {
        return 0.0;
    }
    let mut best = 0.0f64;
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
            let value = capitalize_at_land(w, inv, i, n, cost, 1.0 - occ, land) - lease_up_loss(w, i, u, units, occ);
            best = best.max(value - cost);
        }
    }
    best
}

/// The site's land value consistent with its own land tax: buyers deduct the
/// tax on the assessed land, and the assessment is the land value, so the
/// value solves `L = V(L)`. Property taxes are linear in the assessment, so
/// `V(L) = V(0) − c·L` and `L = V(0) / (1 + c)`. (Valuing land at the
/// current assessment instead makes a high assessment produce a low value
/// and vice versa, so assessments oscillate when land is taxed heavily.)
pub fn self_consistent_site_value(w: &World, i: usize, inv: &Investor) -> f64 {
    let untaxed = vacant_site_value_at_land(w, i, inv, 0.0);
    if untaxed <= 0.0 {
        return 0.0;
    }
    // Measure the slope well inside the range where the value stays above
    // its floor of zero (a heavy land tax can capitalise at more than 100%).
    let probe = 0.25 * untaxed;
    let taxed = vacant_site_value_at_land(w, i, inv, probe);
    let c = ((untaxed - taxed) / probe).max(0.0);
    untaxed / (1.0 + c)
}

/// What `inv` would pay for the land + building as is, including the option
/// to redevelop.
pub fn valuation(w: &World, i: usize, inv: &Investor) -> f64 {
    let mut cur = current_use_value(w, i, inv);
    // A run-down building is worth at least what it would be renovated.
    if w.p.land_use[i].is_private_use() && w.p.condition[i] < 0.6 {
        let share = w.p.freehold_share(i);
        cur = cur.max(share * (renovated_value(w, i, inv) - renovation_cost(w, i)));
    }
    match best_development(w, i, inv) {
        // Redevelopment destroys the current building's value.
        Some(d) if d.residual > cur => d.residual,
        _ => cur,
    }
}

/// Expected growth term shared by all investors (adaptive expectations).
pub fn expected_growth(w: &World, r: f64) -> f64 {
    let e = &w.econ;
    // Owners extrapolate the recent real growth of *rents*, the cash flows
    // they are buying. Not land prices: those are themselves valuations
    // built on this expectation, and feeding them back makes a self-fulfilling
    // boom-and-bust cycle in land values.
    let base = e.outside_growth as f64;
    let trend = w.rent_growth - base;
    // Extrapolation is bounded: nobody prices a perpetuity on a boom lasting
    // forever (values scale with 1/(r − g), so small moves in g swing land
    // values, and land taxes, enormously).
    (base + e.speculation as f64 * trend).clamp(base - 0.02, (base + 0.015).min(r - 0.035))
}

impl World {
    /// Outside capital: requires the outside return plus a premium for
    /// managing remote property.
    pub fn absentee_investor(&self) -> Investor {
        let r = self.econ.outside_return as f64 + ABSENTEE_PREMIUM;
        Investor { r, g: expected_growth(self, r), t_inc: 0.0, w: 0.0, no_tax: false }
    }

    pub fn household_investor(&self, id: u32) -> Investor {
        let h = &self.hh[id as usize];
        // Residents compare property with their deposit alternative plus a risk premium.
        let r = self.econ.deposit_return as f64 + RESIDENT_PREMIUM;
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
        let (mut units, mut occupied) = (0f32, 0f32);
        for i in 0..self.p.len() {
            if matches!(self.p.land_use[i], LandUse::Commercial | LandUse::Industrial) {
                units += self.p.units[i] as f32;
                occupied += self.p.occupied[i] as f32;
            }
        }
        self.biz_occupancy = if units > 50.0 { occupied / units } else { 0.9 };
        for (k, sector) in [Sector::Retail, Sector::Industry, Sector::Office].into_iter().enumerate() {
            let (n, workers) = self
                .firms
                .iter()
                .filter(|f| f.alive && f.sector == sector)
                .fold((0f32, 0f32), |(n, s), f| (n + 1.0, s + f.workers as f32));
            self.firm_size[k] = if n >= 5.0 { workers / n } else { 15.0 };
        }
    }

    pub fn update_pressure(&mut self) {
        let unhoused = self.hh.iter().filter(|h| h.alive && h.home == NONE).count() as f32;
        // Households stuck in dwellings too run-down to let are queueing too.
        let slum = self
            .hh
            .iter()
            .filter(|h| h.alive && h.home != NONE && !crate::housing::lettable(self, h.home as usize))
            .count() as f32;
        let waiting = unhoused + 0.5 * slum + self.latent_demand as f32;
        self.pressure = (waiting / (self.n_alive_hh as f32 + 50.0)).min(0.3);
    }
}
