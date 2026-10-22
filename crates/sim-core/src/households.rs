//! Household month-end settlement: interest, income/wealth/head taxes,
//! consumption (with sales tax) and saving. Also net-worth bookkeeping.

use crate::types::*;
use crate::world::World;

pub fn begin_month(w: &mut World) {
    for h in w.hh.iter_mut().filter(|h| h.alive) {
        h.income_labor = 0.0;
        h.income_capital = 0.0;
        h.income_transfer = 0.0;
        h.taxes_paid = 0.0;
    }
}

pub fn settle(w: &mut World) {
    let e = w.econ.clone();
    let sales = w.taxes.sales_rate();
    let n = w.mesh.len();
    let mut pool = vec![0f32; n];
    let center = w.center;
    let mut imports = 0.0;
    let mut interest_in = 0.0;
    for id in 0..w.hh.len() {
        if !w.hh[id].alive {
            continue;
        }
        // Interest on deposits (paid by the outside world).
        let interest = w.hh[id].cash.max(0.0) * e.deposit_return as f64 / 12.0;
        interest_in += interest;
        {
            let h = &mut w.hh[id];
            h.cash += interest;
            h.income_capital += interest as f32;
        }
        let (labor, capital, wealth) = {
            let h = &w.hh[id];
            (h.income_labor as f64, h.income_capital as f64, h.wealth)
        };
        let income_tax = w.taxes.levy(|m| m.income(labor * 12.0, capital * 12.0) / 12.0);
        let wealth_tax = w.taxes.levy(|m| m.wealth(wealth) / 12.0);
        let head = w.taxes.levy(|m| m.head());
        let tax = income_tax + wealth_tax + head;
        w.gov.cash += tax;

        let rent = crate::housing::rent_paid(w, id as u32) as f64;
        let h = &mut w.hh[id];
        h.cash -= tax;
        h.taxes_paid += tax as f32;

        // Consumption: buffer-stock rule out of disposable income and cash.
        let disposable = (labor + capital + h.income_transfer as f64 - h.taxes_paid as f64 - rent).max(0.0);
        let mut c = e.marginal_propensity_consume as f64 * disposable + e.wealth_consumption as f64 * h.cash.max(0.0);
        c = c.max((e.subsistence as f64).min(h.cash.max(0.0) * 0.5));
        c = c.min(h.cash.max(0.0));
        h.cash -= c;
        h.consumption = c as f32;
        let net_of_tax = c / (1.0 + sales);
        let local = net_of_tax * e.local_spend_share as f64;
        imports += net_of_tax - local;
        let cell = if h.home != NONE { h.home as usize } else { center };
        pool[cell] += local as f32;
        let vat = c - net_of_tax;
        if vat > 0.0 {
            w.taxes.levy(|m| m.sales_rate() / (1.0 + sales) * c);
            w.gov.cash += vat;
            w.hh[id].taxes_paid += vat as f32;
        }
    }
    w.retail_pool = pool;
    w.flows.imports += imports;
    w.flows.interest_in = interest_in;
}

/// Recompute net worth: cash − mortgages + assessed property + firm equity.
pub fn update_wealth(w: &mut World) {
    let r = w.econ.outside_return as f64 + 0.055;
    let mut wealth: Vec<f64> = w.hh.iter().map(|h| h.cash).collect();
    let mut debt = vec![0f64; w.hh.len()];
    for i in 0..w.p.len() {
        if let Owner::Hh(id) = w.p.owner[i] {
            let v = w.parcel_value(i);
            wealth[id as usize] += v - w.p.mortgage[i] as f64;
            debt[id as usize] += w.p.mortgage[i] as f64;
        }
    }
    for f in w.firms.iter().filter(|f| f.alive) {
        if let Owner::Hh(id) = f.owner {
            wealth[id as usize] += f.cash.max(0.0) + (f.profit.max(0.0) as f64 * 12.0) / r;
        }
    }
    for (id, h) in w.hh.iter().enumerate() {
        if h.alive && h.owns_home {
            let i = h.home as usize;
            wealth[id] += w.flat_assessment(i) - h.flat_mortgage;
            debt[id] += h.flat_mortgage;
        }
    }
    for (id, h) in w.hh.iter_mut().enumerate() {
        if h.alive {
            h.wealth = wealth[id];
            h.debt = debt[id];
        }
    }
}
