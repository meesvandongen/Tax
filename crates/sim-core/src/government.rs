//! Government: pays transfers, budgets revenue across spending categories,
//! builds roads, parks and civic buildings. Public works buy land at market
//! prices, so a high land-value city pays more for its parks.

use crate::econ;
use crate::land::compulsory_purchase;
use crate::types::*;
use crate::world::{Event, World};

/// Dividend and welfare, paid at the start of the month from last month's budget.
pub fn transfers(w: &mut World) {
    let div = w.gov.dividend_per_hh + w.gov.rebate_per_hh;
    let rebate_share = if div > 0.0 { w.gov.rebate_per_hh / div } else { 0.0 };
    let level = w.gov.welfare_level;
    let mut paid_div = 0.0;
    let mut paid_welfare = 0.0;
    for h in w.hh.iter_mut().filter(|h| h.alive) {
        let mut t = div;
        // Means-tested top-up with 50% withdrawal; extra for the unhoused.
        let own = (h.income_labor + h.income_capital) as f64;
        let mut benefit = (level - 0.5 * own).max(0.0);
        if h.home == NONE {
            benefit += 0.5 * level;
        }
        t += benefit;
        paid_div += div;
        paid_welfare += benefit;
        h.cash += t;
        h.income_transfer = t as f32;
    }
    w.gov.cash -= paid_div + paid_welfare;
    w.gov.spend_rebate += paid_div * rebate_share;
    w.gov.spend_dividend += paid_div * (1.0 - rebate_share);
    w.gov.spend_welfare += paid_welfare;
}

/// End of month: set next month's budgets and carry out public works.
pub fn budget(w: &mut World) {
    let revenue: f64 = w.taxes.month.iter().sum();
    w.gov.revenue_month = revenue;
    w.gov.revenue_ema = if w.tick < 3 { revenue } else { 0.92 * w.gov.revenue_ema + 0.08 * revenue };
    let s = w.cfg.policy.spending.clone();
    let total_share = (s.services + s.infrastructure + s.parks + s.dividend + s.welfare).max(1e-6) as f64;
    // Spend revenue, plus a slow draw-down of reserves above six months of revenue.
    let reserve = (w.gov.cash - 6.0 * w.gov.revenue_ema).max(0.0);
    let mut envelope = w.gov.revenue_ema.max(0.0) + reserve / 36.0;
    // Debt brake: borrowing beyond the limit forces austerity.
    let debt = (-w.gov.cash).max(0.0);
    if debt > s.max_debt_to_revenue as f64 * 12.0 * w.gov.revenue_ema.max(1.0) {
        envelope *= 0.85;
    }
    if debt > 0.0 {
        let interest = debt * w.cfg.econ.outside_return as f64 / 12.0;
        w.gov.cash -= interest;
        w.gov.spend_interest += interest;
    }
    let share = |x: f32| envelope * x as f64 / total_share;

    // Dividend for next month, plus a rebate of reserves beyond a year of
    // revenue that no programme could absorb (otherwise taxes would simply
    // destroy money).
    let n = w.n_alive_hh.max(1) as f64;
    w.gov.dividend_per_hh = share(s.dividend) / n;
    let surplus = (w.gov.cash - 12.0 * w.gov.revenue_ema.max(0.0)).max(0.0);
    w.gov.rebate_per_hh = surplus / 36.0 / n;
    w.gov.starts_ema = 0.95 * w.gov.starts_ema + 0.05 * w.flows.buildings_started as f64;

    // Welfare level: budget spread over likely recipients.
    let poor = w.hh.iter().filter(|h| h.alive && (h.income_labor + h.income_capital) < 1500.0).count().max(1) as f64;
    w.gov.welfare_budget = share(s.welfare);
    w.gov.welfare_level = (w.gov.welfare_budget / poor).min(1800.0);

    // Public services: staff civic buildings up to the budget.
    w.gov.service_budget = share(s.services);
    let public_capacity: f64 = w
        .firms
        .iter()
        .filter(|f| f.alive && f.sector == Sector::Public)
        .map(|f| w.p.units[f.parcel as usize] as f64)
        .sum();
    let wage = w.market_wage_rate() as f64 * 1.1;
    let affordable = w.gov.service_budget / wage.max(1.0);
    w.gov.public_staffing = if public_capacity > 0.0 { (affordable / public_capacity).min(1.0) } else { 0.0 };
    if affordable > public_capacity * 1.15 {
        build_civic(w);
    }

    // Infrastructure and parks are funded from accumulating funds.
    w.gov.infra_budget += share(s.infrastructure);
    w.gov.park_budget += share(s.parks);
    build_roads(w);
    maintain_roads(w);
    build_park(w);
}

fn maintain_roads(w: &mut World) {
    let roads = w.p.land_use.iter().filter(|&&u| u == LandUse::Road).count() as f64;
    let cost = roads * 60.0;
    let paid = cost.min(w.gov.infra_budget.max(0.0));
    w.gov.infra_budget -= paid;
    w.gov.cash -= paid;
    w.gov.spend_infrastructure += paid;
    w.flows.construction_spend += paid;
}

const ROAD_COST: f64 = 40_000.0;
const BRIDGE_COST: f64 = 400_000.0;
const BLOCK: usize = 5;

/// The mapped street plan: every fifth row and column through the founding
/// crossing. Like an official map (or Manhattan's 1811 grid), it reserves
/// the right of way ahead of the road: owners may farm it, but not build on
/// it, so the network can always be extended. The government buys the strip
/// when it lays the road.
pub fn on_street_plan(w: &World, i: usize) -> bool {
    let (x, y) = w.dims.xy(i);
    let (cx, cy) = w.dims.xy(w.center);
    (x + BLOCK - cx % BLOCK) % BLOCK == 0 || (y + BLOCK - cy % BLOCK) % BLOCK == 0
}

/// Right of way held back for a future road (not yet built on).
pub fn reserved_for_street(w: &World, i: usize) -> bool {
    matches!(w.p.land_use[i], LandUse::Wild | LandUse::Farm | LandUse::Vacant) && on_street_plan(w, i)
}

/// Land acquisition for a road cell, before the road itself. A building in
/// the way (one that predates the plan) can be expropriated, but only with
/// full compensation: the property at market value, demolition, and
/// relocation payments to its tenants. That makes it a last resort.
fn acquisition_cost(w: &World, i: usize) -> f64 {
    let u = w.p.land_use[i];
    if u == LandUse::Water {
        return 0.0;
    }
    let mut cost = w.parcel_value(i);
    if u.is_building() {
        cost += econ::demolition_cost(w, i) + econ::tenant_compensation(w, i);
    }
    cost
}

fn build_roads(w: &mut World) {
    if w.gov.infra_budget < ROAD_COST {
        return;
    }
    // Plan land supply: extend the network only while serviced land that
    // can actually be built on is short of about two years of construction.
    let serviced_idle = (0..w.p.len())
        .filter(|&i| {
            w.p.road_access[i]
                && matches!(w.p.land_use[i], LandUse::Wild | LandUse::Farm | LandUse::Vacant)
                && !on_street_plan(w, i)
        })
        .count() as f64;
    if serviced_idle > 40.0 + 24.0 * w.gov.starts_ema {
        return;
    }
    let dims = w.dims;
    // Frontier: cells on the street plan next to an existing road.
    let mut frontier: Vec<(usize, f64)> = Vec::new();
    for i in 0..dims.len() {
        let u = w.p.land_use[i];
        let open = matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant | LandUse::Water);
        let expropriable = matches!(u, LandUse::Residential | LandUse::Commercial | LandUse::Industrial);
        if !(open || expropriable) || !on_street_plan(w, i) {
            continue;
        }
        if !dims.neighbors4(i).any(|j| w.p.land_use[j] == LandUse::Road) {
            continue;
        }
        // Demand: the rent potential of land the road would open up.
        let unlock: f64 = dims
            .ring(i, 1)
            .iter()
            .filter(|&&j| !w.p.road_access[j] && w.p.land_use[j].is_developable() && !w.terrain.water[j])
            .map(|&j| w.f.rent_res[j].max(w.f.rent_com[j] * 0.3) as f64)
            .sum();
        if unlock <= 0.0 {
            continue;
        }
        let build = if u == LandUse::Water { BRIDGE_COST } else { ROAD_COST };
        frontier.push((i, unlock / (build + acquisition_cost(w, i))));
    }
    frontier.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    for (i, _) in frontier.into_iter().take(6) {
        let water = w.p.land_use[i] == LandUse::Water;
        let build = if water { BRIDGE_COST } else { ROAD_COST };
        if w.gov.infra_budget < build + acquisition_cost(w, i) {
            continue;
        }
        if !water {
            let paid = compulsory_purchase(w, i);
            w.gov.infra_budget -= paid;
            if w.p.land_use[i].is_building() {
                // The government now owns the building: it pays its
                // tenants' relocation and the demolition.
                let relocation = crate::development::clear_site(w, i);
                let demolition = econ::demolition_cost(w, i);
                w.gov.cash -= relocation + demolition;
                w.gov.infra_budget -= relocation + demolition;
                w.gov.spend_compensation += relocation;
                w.gov.spend_infrastructure += demolition;
                w.flows.construction_spend += demolition;
                w.gov.roads_expropriated += 1;
            }
        }
        w.gov.infra_budget -= build;
        w.gov.cash -= build;
        w.gov.spend_infrastructure += build;
        w.flows.construction_spend += build;
        set_road(w, i);
        w.gov.roads_built += 1;
        w.push_event(Event::Road { cell: i as u32 });
    }
}

pub fn set_road(w: &mut World, i: usize) {
    w.p.land_use[i] = LandUse::Road;
    w.p.owner[i] = Owner::Gov;
    w.p.nature[i] = 0.0;
    w.p.units[i] = 0;
    w.p.floors[i] = 0;
    w.p.road_access[i] = true;
    for j in w.dims.neighbors4(i).collect::<Vec<_>>() {
        w.p.road_access[j] = true;
    }
}

fn build_park(w: &mut World) {
    if w.gov.park_budget <= 0.0 {
        return;
    }
    let n = w.dims.len();
    let mut best: Option<(usize, f64)> = None;
    for i in 0..n {
        let u = w.p.land_use[i];
        if !matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant) || !w.p.road_access[i] || on_street_plan(w, i)
        {
            continue;
        }
        // Residents who would gain park access, per unit of cost.
        let pop: f32 = w.dims.ring(i, 3).iter().map(|&j| w.f.pop[j]).sum();
        let lacking = 1.0 - (w.f.amenity[i] / crate::fields::PARK_VALUE).min(1.0);
        let price = w.parcel_value(i).max(1.0);
        let score = (pop * lacking) as f64 / price;
        if pop > 20.0 && best.map_or(true, |b| score > b.1) {
            best = Some((i, score));
        }
    }
    let Some((i, _)) = best else { return };
    let price = w.parcel_value(i).max(econ::valuation(w, i, &w.investor_for(w.p.owner[i])));
    if price > w.gov.park_budget {
        return;
    }
    let paid = compulsory_purchase(w, i);
    w.gov.park_budget -= paid;
    w.gov.spend_parks += paid;
    w.p.land_use[i] = LandUse::Park;
    w.p.nature[i] = w.p.nature[i].max(0.6);
    w.gov.parks_built += 1;
    w.push_event(Event::Park { cell: i as u32 });
}

fn build_civic(w: &mut World) {
    let n = w.dims.len();
    let mut best: Option<(usize, f32)> = None;
    for i in 0..n {
        let u = w.p.land_use[i];
        if !matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant) || !w.p.road_access[i] || on_street_plan(w, i)
        {
            continue;
        }
        let need = w.f.pop[i] + w.dims.ring(i, 3).iter().map(|&j| w.f.pop[j]).sum::<f32>();
        let score = need * (1.0 - w.f.service[i]);
        if best.map_or(true, |b| score > b.1) {
            best = Some((i, score));
        }
    }
    let Some((i, score)) = best else { return };
    if score < 10.0 {
        return;
    }
    let paid = compulsory_purchase(w, i);
    let cost = econ::construction_cost(w, i, LandUse::Civic, 2);
    w.gov.cash -= cost;
    w.gov.spend_services += cost + paid;
    let d = econ::DevOption { land_use: LandUse::Civic, floors: 2, units: 24, cost, value: cost, residual: 0.0 };
    crate::development::start_construction(w, i, &d);
}
