//! Firms produce, pay wages, rent and taxes, and distribute profits.
//!
//! Money enters the city through industrial exports, transfers and absentee
//! investment, circulates through wages, rents and local shopping, and leaves
//! through imports, absentee income and emigrants' wealth.

use crate::fields::OUTSIDE_SHOP_CAPACITY;
use crate::grid::exp_blur;
use crate::labour::industry_tfp;
use crate::types::*;
use crate::world::World;

pub fn run(w: &mut World) {
    let e = w.econ.clone();
    let n = w.dims.len();
    let nf = w.firms.len();

    // ---- wages
    let mut wage_bill = vec![0f64; nf];
    let mut public_wages = 0.0;
    for h in w.hh.iter_mut().filter(|h| h.alive && h.job != NONE) {
        let f = &w.firms[h.job as usize];
        let wage = f.wage_rate * h.skill;
        h.wage = wage;
        h.cash += wage as f64;
        h.income_labor += wage;
        wage_bill[h.job as usize] += wage as f64;
        if f.sector == Sector::Public {
            public_wages += wage as f64;
        }
    }
    w.gov.cash -= public_wages;
    w.gov.spend_services += public_wages;
    w.flows.wages = wage_bill.iter().sum();

    // ---- retail demand from last month's local spending (Huff model)
    let mut cap = vec![0f32; n];
    for f in w.firms.iter().filter(|f| f.alive && f.sector == Sector::Retail) {
        cap[f.parcel as usize] += f.workers as f32 * f.productivity;
    }
    let s_field = exp_blur(&cap, w.dims, crate::fields::SHOP_LAMBDA);
    let ratio: Vec<f32> = w.retail_pool.iter().zip(&s_field).map(|(c, s)| c / (s + OUTSIDE_SHOP_CAPACITY)).collect();
    let draw = exp_blur(&ratio, w.dims, crate::fields::SHOP_LAMBDA);
    let pool_total: f64 = w.retail_pool.iter().map(|&v| v as f64).sum();

    // ---- per-firm accounts
    let mut retail_revenue = 0.0;
    let mut emissions = vec![0f32; n];
    let mut abatement_total = 0.0;
    let mut value_added = public_wages;
    for id in 0..nf {
        if !w.firms[id].alive {
            continue;
        }
        let f = w.firms[id].clone();
        let i = f.parcel as usize;
        let wages = wage_bill[id];
        let (revenue, intensity, abatement, emitted) = match f.sector {
            Sector::Industry => {
                let a = industry_tfp(w, &f, i) as f64;
                let y = a * (f.skill_sum as f64).powf(e.labour_elasticity as f64);
                // Choose emission intensity to minimise tax + abatement.
                let tax_per_unit = w.taxes.estimate(|m| m.emission(1.0));
                let intensity = (1.0 - tax_per_unit * e.pollution_per_worker as f64 / (2.0 * e.abatement_cost as f64))
                    .clamp(0.05, 1.0);
                let abatement = e.abatement_cost as f64 * f.workers as f64 * (1.0 - intensity).powi(2);
                let units = f.workers as f64 * e.pollution_per_worker as f64 * intensity;
                (y, intensity as f32, abatement, units)
            }
            Sector::Retail => {
                // Each worker can serve a limited volume of customers; demand
                // beyond that goes to shops elsewhere (imports).
                let per_unit = draw[i].min(e.retail_output_per_worker) as f64;
                let r = f.workers as f64 * f.productivity as f64 * per_unit;
                retail_revenue += r;
                (r, 0.0, 0.0, 0.0)
            }
            Sector::Public => (0.0, 0.0, 0.0, 0.0),
        };
        if f.sector == Sector::Public {
            continue;
        }
        if f.sector == Sector::Industry {
            w.flows.exports += revenue;
        }
        value_added += revenue;
        emissions[i] += emitted as f32;
        abatement_total += abatement;

        let payroll_tax = w.taxes.levy(|m| m.payroll_rate() * wages);
        let emission_tax = w.taxes.levy(|m| m.emission(emitted));
        // Rent for the space in use (with a minimum lease of a quarter of the premises).
        let rent = w.p.asking_rent[i] as f64 * (f.workers.max(w.p.units[i] / 4)) as f64;
        let profit = revenue - wages - payroll_tax - emission_tax - abatement - rent;
        let corp = if profit > 0.0 { w.taxes.levy(|m| m.profit(profit * 12.0) / 12.0) } else { 0.0 };
        w.gov.cash += payroll_tax + emission_tax + corp;

        // Rent to the landlord.
        let landlord = w.p.owner[i];
        w.pay_owner(landlord, rent);

        let after_tax = profit - corp;
        // Firms keep 30% of profits and pay out cash beyond a six-month
        // buffer of costs, so retained earnings do not pile up forever.
        let buffer = 6.0 * (revenue - profit).max(0.0);
        let excess = (f.cash + 0.3 * after_tax.max(0.0) - buffer).max(0.0) / 12.0;
        let dividend = if after_tax > 0.0 { after_tax * 0.7 } else { 0.0 } + excess;
        {
            let fm = &mut w.firms[id];
            fm.revenue = revenue as f32;
            fm.profit = after_tax as f32;
            fm.emission_intensity = intensity;
            fm.cash += after_tax - dividend;
            fm.age_months += 1;
            if after_tax < 0.0 {
                fm.months_loss += 1;
            } else {
                fm.months_loss = fm.months_loss.saturating_sub(1);
            }
        }
        if dividend > 0.0 {
            w.pay_owner(f.owner, dividend);
        }
    }
    w.flows.retail_sales = retail_revenue;
    w.flows.imports += (pool_total - retail_revenue).max(0.0);
    w.flows.emissions = emissions.iter().map(|&v| v as f64).sum();
    w.flows.abatement = abatement_total;
    w.flows.value_added = value_added;
    w.f.emissions = emissions;
    w.f.retail_draw = draw;
}

/// Commercial landlords renegotiate with their tenant firm: they raise the
/// rent while the tenant earns a healthy margin (capturing the location's
/// rent) and cut it when the tenant loses money. Empty premises are offered
/// at falling rents, toward what space nearby lets for.
pub fn adjust_commercial_rents(w: &mut World) {
    let adj = w.econ.rent_adjust;
    for i in 0..w.p.len() {
        let u = w.p.land_use[i];
        if u != LandUse::Commercial && u != LandUse::Industrial {
            continue;
        }
        let fid = w.p.firm[i];
        let mut r = w.p.asking_rent[i];
        if fid == NONE {
            let market = crate::econ::market_rent(w, i, u, w.p.units[i]);
            r *= 1.0 - adj;
            r += (market - r).min(0.0) * 0.02;
        } else {
            let f = &w.firms[fid as usize];
            if f.age_months >= 6 {
                let margin = f.profit / f.revenue.max(1.0);
                if margin > 0.15 {
                    r *= 1.0 + 0.5 * adj;
                } else if margin < 0.03 {
                    r *= 1.0 - adj;
                }
            }
            // A tenant moves out rather than pay far above what comparable
            // premises cost a typical firm: the landlord can capture the
            // location's rent, not the tenant's own productivity.
            let observed = if u == LandUse::Commercial { w.f.obs_com[i] } else { w.f.obs_ind[i] };
            let typical = crate::econ::firm_bid_rent(w, i, u, w.p.units[i]).max(0.5 * observed);
            r = r.min(1.3 * typical);
        }
        w.p.asking_rent[i] = r.max(w.index);
    }
}
