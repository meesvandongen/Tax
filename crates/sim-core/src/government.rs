//! Government: pays transfers, budgets revenue across spending categories,
//! builds roads (see [`crate::roads`]), parks and civic buildings. Public
//! works buy land at market prices plus a premium, so a high land-value city
//! pays more for its parks.

use crate::econ;
use crate::land::compulsory_purchase;
use crate::types::*;
use crate::world::{Event, World};

/// Dividend and welfare, paid at the start of the month from last month's budget.
pub fn transfers(w: &mut World) {
    let div = w.gov.dividend_per_hh + w.gov.rebate_per_hh;
    let rebate_share = if div > 0.0 { w.gov.rebate_per_hh / div } else { 0.0 };
    let level = w.gov.welfare_level;
    // Means-tested top-up with 50% withdrawal; extra for the unhoused. The
    // programme never pays out more than its budget.
    let benefit = |h: &Household| {
        let own = (h.income_labor + h.income_capital) as f64;
        (level - 0.5 * own).max(0.0) + if h.home == NONE { 0.5 * level } else { 0.0 }
    };
    let claims: f64 = w.hh.iter().filter(|h| h.alive).map(benefit).sum();
    w.gov.welfare_paid_budget = w.gov.welfare_budget;
    let scale = if claims > w.gov.welfare_budget { w.gov.welfare_budget.max(0.0) / claims } else { 1.0 };
    let mut paid_div = 0.0;
    let mut paid_welfare = 0.0;
    for h in w.hh.iter_mut().filter(|h| h.alive) {
        let b = benefit(h) * scale;
        let t = div + b;
        paid_div += div;
        paid_welfare += b;
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
        let interest = debt * w.econ.outside_return as f64 / 12.0;
        w.gov.cash -= interest;
        w.gov.spend_interest += interest;
    } else {
        // Reserves are deposited and earn the deposit rate.
        let interest = w.gov.cash * w.econ.deposit_return as f64 / 12.0;
        w.gov.cash += interest;
        w.flows.interest_in += interest;
    }
    let share = |x: f32| envelope * x as f64 / total_share;

    // Dividend for next month, plus a rebate of reserves beyond a year of
    // revenue that no programme could absorb (otherwise taxes would simply
    // destroy money).
    let n = w.n_alive_hh.max(1) as f64;
    w.gov.dividend_per_hh = share(s.dividend) / n;
    let surplus = (w.gov.cash - 12.0 * w.gov.revenue_ema.max(0.0)).max(0.0);
    w.gov.rebate_per_hh = surplus / 12.0 / n;
    w.gov.starts_ema = 0.95 * w.gov.starts_ema + 0.05 * w.flows.buildings_started as f64;

    // Welfare level: the highest top-up (with its 50% withdrawal and the
    // extra for the unhoused) that the welfare budget can pay for.
    let ix = w.index as f64;
    w.gov.welfare_budget = share(s.welfare);
    let incomes: Vec<(f64, bool)> =
        w.hh.iter().filter(|h| h.alive).map(|h| ((h.income_labor + h.income_capital) as f64, h.home == NONE)).collect();
    let payout = |level: f64| -> f64 {
        incomes
            .iter()
            .map(|&(own, unhoused)| (level - 0.5 * own).max(0.0) + if unhoused { 0.5 * level } else { 0.0 })
            .sum()
    };
    let (mut lo, mut hi) = (0.0, 1800.0 * ix);
    if payout(hi) <= w.gov.welfare_budget {
        lo = hi;
    } else {
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if payout(mid) <= w.gov.welfare_budget {
                lo = mid;
            } else {
                hi = mid;
            }
        }
    }
    w.gov.welfare_level = lo;

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
    // Build a civic building when the budget could staff more than the
    // existing and planned buildings hold *and* residents need more staff
    // than that (`SERVICE_RATIO` per household), and only if it can be paid
    // for without breaking the debt limit.
    let planned: f64 = (0..w.p.len())
        .filter(|&i| w.p.land_use[i] == LandUse::Construction && w.p.target_use[i] == LandUse::Civic)
        .map(|i| econ::units_for(w, LandUse::Civic, w.p.floors[i]) as f64)
        .sum();
    let needed = crate::fields::SERVICE_RATIO as f64 * n;
    let capacity = public_capacity + planned;
    let debt_room = s.max_debt_to_revenue as f64 * 12.0 * w.gov.revenue_ema.max(0.0) + w.gov.cash;
    if affordable > capacity * 1.15
        && needed > capacity
        && debt_room > 4.0 * econ::construction_cost(w, w.center, LandUse::Civic, 2)
    {
        build_civic(w);
    }

    // Infrastructure and parks are funded from accumulating funds. What the
    // month's works leave unspent is capped at a few years of allocations:
    // money no project needs stays in the reserves (and is eventually rebated).
    w.gov.infra_budget += share(s.infrastructure);
    w.gov.park_budget += share(s.parks);
    crate::roads::plan(w, share(s.infrastructure));
    crate::roads::maintain(w);
    build_park(w);
    w.gov.infra_budget = w.gov.infra_budget.min(36.0 * share(s.infrastructure));
    w.gov.park_budget = w.gov.park_budget.min(36.0 * share(s.parks));
}

fn build_park(w: &mut World) {
    if w.gov.park_budget <= 0.0 {
        return;
    }
    let n = w.p.len();
    let mut best: Option<(usize, f64)> = None;
    for i in 0..n {
        let u = w.p.land_use[i];
        if !matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant) || !w.p.road_access[i] {
            continue;
        }
        // Residents who would gain park access, per unit of cost.
        let pop: f32 = w.mesh.near(i, 3.5).iter().map(|&j| w.f.pop[j as usize]).sum();
        let lacking = 1.0 - (w.f.amenity[i] / (w.index * crate::fields::PARK_VALUE)).min(1.0);
        let price = w.parcel_value(i).max(1.0);
        let score = (pop * lacking) as f64 / price;
        if pop > 20.0 && best.map_or(true, |b| score > b.1) {
            best = Some((i, score));
        }
    }
    let Some((i, _)) = best else { return };
    let premium = 1.0 + w.cfg.policy.spending.purchase_premium as f64;
    let price = premium * w.parcel_value(i).max(econ::valuation(w, i, &w.investor_for(w.p.owner[i])));
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
    let n = w.p.len();
    let mut best: Option<(usize, f32)> = None;
    for i in 0..n {
        let u = w.p.land_use[i];
        if !matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant) || !w.p.road_access[i] {
            continue;
        }
        let need = w.f.pop[i] + w.mesh.near(i, 3.5).iter().map(|&j| w.f.pop[j as usize]).sum::<f32>();
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
    let d = econ::DevOption {
        land_use: LandUse::Civic,
        floors: 2,
        units: 24,
        cost,
        value: cost,
        residual: 0.0,
        flat_price: 0.0,
    };
    crate::development::start_construction(w, i, &d);
}
