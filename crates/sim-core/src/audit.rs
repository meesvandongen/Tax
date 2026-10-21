//! Sanity audit of a running city: a detailed state report and a list of
//! plausibility checks. Used by `taxsim audit` to validate long runs every
//! few years, so that implausible states (money created from nothing,
//! buildings paying tax they cannot earn, collapsed markets) are caught
//! instead of silently feeding the indicators.

use crate::econ;
use crate::types::*;
use crate::world::World;
use std::fmt::Write;

/// Aggregates of one land use.
#[derive(Clone, Debug, Default)]
pub struct UseStats {
    pub cells: u32,
    pub units: u64,
    pub occupied: u64,
    pub floors: u64,
    pub derelict: u32,
    pub with_firm: u32,
    pub absentee: u32,
    pub resident: u32,
    pub gov: u32,
    pub land_value: f64,
    pub tax: f64,
    /// Rent actually received by owners per month (tenants' leases, firms' rent).
    pub rent_income: f64,
}

pub fn use_stats(w: &World, u: LandUse) -> UseStats {
    let p = &w.p;
    let mut s = UseStats::default();
    for i in 0..p.len() {
        if p.land_use[i] != u {
            continue;
        }
        s.cells += 1;
        s.units += p.units[i] as u64;
        s.occupied += p.occupied[i] as u64;
        s.floors += p.floors[i] as u64;
        s.derelict += (p.condition[i] < 0.25 && u.is_building()) as u32;
        s.with_firm += (p.firm[i] != NONE) as u32;
        match p.owner[i] {
            Owner::Absentee => s.absentee += 1,
            Owner::Hh(_) => s.resident += 1,
            Owner::Gov => s.gov += 1,
        }
        s.land_value += p.assessed_land[i] as f64;
        s.tax += p.tax_last[i] as f64;
        if matches!(u, LandUse::Commercial | LandUse::Industrial) && p.firm[i] != NONE {
            let f = &w.firms[p.firm[i] as usize];
            s.rent_income += p.asking_rent[i] as f64 * f.workers.max(p.units[i] / 4) as f64;
        }
    }
    if u == LandUse::Residential {
        for h in w.hh.iter().filter(|h| h.alive && h.home != NONE) {
            if p.land_use[h.home as usize] == LandUse::Residential {
                s.rent_income += h.rent as f64;
            }
        }
    }
    s
}

fn pct(a: f64, b: f64) -> f64 {
    if b > 0.0 {
        100.0 * a / b
    } else {
        0.0
    }
}

/// Multi-line human-readable state report.
pub fn report(w: &World) -> String {
    let mut out = String::new();
    let m = w.history.last().cloned().unwrap_or_default();
    let _ = writeln!(out, "=== year {:.1} (tick {}) ===", w.year(), w.tick);

    // ---- land uses
    let _ = writeln!(
        out,
        "{:<13}{:>6}{:>8}{:>8}{:>6}{:>6}{:>6}{:>9}{:>12}{:>11}{:>11}",
        "use", "cells", "units", "occ%", "flr", "derel", "firm", "abs/res", "land M", "tax k/mo", "rent k/mo"
    );
    for u in [
        LandUse::Residential,
        LandUse::Commercial,
        LandUse::Industrial,
        LandUse::Civic,
        LandUse::Construction,
        LandUse::Vacant,
        LandUse::Farm,
        LandUse::Wild,
        LandUse::Park,
        LandUse::Road,
    ] {
        let s = use_stats(w, u);
        if s.cells == 0 {
            continue;
        }
        let _ = writeln!(
            out,
            "{:<13}{:>6}{:>8}{:>7.0}%{:>6.1}{:>6}{:>6}{:>9}{:>12.1}{:>11.1}{:>11.1}",
            format!("{u:?}"),
            s.cells,
            s.units,
            pct(s.occupied as f64, s.units as f64),
            s.floors as f64 / s.cells as f64,
            s.derelict,
            s.with_firm,
            format!("{}/{}", s.absentee, s.resident),
            s.land_value / 1e6,
            s.tax / 1e3,
            s.rent_income / 1e3,
        );
    }

    // ---- households
    let alive: Vec<&Household> = w.hh.iter().filter(|h| h.alive).collect();
    let n = alive.len().max(1) as f64;
    let mut cash: Vec<f64> = alive.iter().map(|h| h.cash).collect();
    cash.sort_by(|a, b| a.total_cmp(b));
    let med_cash = cash.get(cash.len() / 2).copied().unwrap_or(0.0);
    let max_cash = cash.last().copied().unwrap_or(0.0);
    let income: f64 =
        alive.iter().map(|h| (h.income_labor + h.income_capital + h.income_transfer) as f64).sum::<f64>() / n;
    let labour: f64 = alive.iter().map(|h| h.income_labor as f64).sum::<f64>() / n;
    let transfer: f64 = alive.iter().map(|h| h.income_transfer as f64).sum::<f64>() / n;
    let capital: f64 = alive.iter().map(|h| h.income_capital as f64).sum::<f64>() / n;
    let cons: f64 = alive.iter().map(|h| h.consumption as f64).sum::<f64>() / n;
    let taxes: f64 = alive.iter().map(|h| h.taxes_paid as f64).sum::<f64>() / n;
    let rent: f64 = alive.iter().map(|h| h.rent as f64).sum::<f64>() / n;
    let owners_occ = alive
        .iter()
        .enumerate()
        .filter(|(_, h)| h.home != NONE)
        .filter(|(_, h)| matches!(w.p.owner[h.home as usize], Owner::Hh(_)))
        .count();
    let _ = writeln!(
        out,
        "households {} housed {} employed {} (unemp {:.1}%) | resident-landlord homes {}",
        alive.len(),
        alive.iter().filter(|h| h.home != NONE).count(),
        alive.iter().filter(|h| h.job != NONE).count(),
        m.unemployment * 100.0,
        owners_occ
    );
    let _ = writeln!(
        out,
        "per hh/mo: income {:.0} = labour {:.0} + capital {:.0} + transfer {:.0} | taxes {:.0} rent {:.0} consumption {:.0}",
        income, labour, capital, transfer, taxes, rent, cons
    );
    let _ = writeln!(
        out,
        "cash: median {:.0} max {:.3e} | median rent {:.0} burden {:.0}%",
        med_cash,
        max_cash,
        m.median_rent,
        m.rent_burden * 100.0
    );

    // ---- firms and jobs
    for sector in [Sector::Retail, Sector::Industry, Sector::Public] {
        let fs: Vec<&Firm> = w.firms.iter().filter(|f| f.alive && f.sector == sector).collect();
        let slots: u64 = fs.iter().map(|f| w.p.units[f.parcel as usize] as u64).sum();
        let workers: u64 = fs.iter().map(|f| f.workers as u64).sum();
        let desired: u64 = fs.iter().map(|f| f.desired_workers as u64).sum();
        let revenue: f64 = fs.iter().map(|f| f.revenue as f64).sum();
        let profit: f64 = fs.iter().map(|f| f.profit as f64).sum();
        let wage: f64 = if workers > 0 {
            fs.iter().map(|f| f.wage_rate as f64 * f.workers as f64).sum::<f64>() / workers as f64
        } else {
            0.0
        };
        let losing = fs.iter().filter(|f| f.months_loss > 0).count();
        let cash: f64 = fs.iter().map(|f| f.cash).sum();
        let _ = writeln!(
            out,
            "{:<9} firms {:>5} slots {:>7} workers {:>7} desired {:>7} wage/skill {:>6.0} revenue {:>10.0}k profit {:>9.0}k cash {:>9.0}k losing {}",
            format!("{sector:?}"),
            fs.len(),
            slots,
            workers,
            desired,
            wage,
            revenue / 1e3,
            profit / 1e3,
            cash / 1e3,
            losing
        );
    }

    // ---- government and flows
    let g = &w.gov;
    let by_tax: Vec<String> = m.revenue_by_tax.iter().map(|(k, v)| format!("{k} {:.0}k", v / 1e3)).collect::<Vec<_>>();
    let _ = writeln!(
        out,
        "gov cash {:.1}M rev {:.0}k/mo [{}] ema {:.0}k | spend k: svc {:.0} infra {:.0} parks {:.0} div {:.0} welfare {:.0} rebate {:.0} interest {:.0}",
        g.cash / 1e6,
        m.revenue / 1e3,
        by_tax.join(", "),
        g.revenue_ema / 1e3,
        g.spend_services / 1e3,
        g.spend_infrastructure / 1e3,
        g.spend_parks / 1e3,
        g.spend_dividend / 1e3,
        g.spend_welfare / 1e3,
        g.spend_rebate / 1e3,
        g.spend_interest / 1e3,
    );
    let f = &w.flows;
    let _ = writeln!(
        out,
        "flows k/mo: gdp {:.0} wages {:.0} exports {:.0} imports {:.0} retail {:.0} absentee {:.0} interest {:.0} | immig {} emig {} sales {} forced {} starts {} done {}",
        f.value_added / 1e3,
        f.wages / 1e3,
        f.exports / 1e3,
        f.imports / 1e3,
        f.retail_sales / 1e3,
        f.absentee_income / 1e3,
        f.interest_in / 1e3,
        f.immigrants,
        f.emigrants,
        f.sales,
        f.forced_sales,
        f.buildings_started,
        f.buildings_completed
    );
    let fl = &w.f;
    let max_com = fl.rent_com.iter().cloned().fold(0.0f32, f32::max);
    let max_ind = fl.rent_ind.iter().cloned().fold(0.0f32, f32::max);
    let max_res = fl.rent_res.iter().cloned().fold(0.0f32, f32::max);
    let inv = w.absentee_investor();
    let _ = writeln!(
        out,
        "fields: level_res {:.0} max rent res/com/ind {:.0}/{:.0}/{:.0} | rent_growth {:.3} price_growth {:.3} g {:.3} | pressure {:.2} latent {} slack {:.2} wage_cache {:.0}",
        fl.level_res, max_res, max_com, max_ind, w.rent_growth, w.price_growth, inv.g, w.pressure, w.latent_demand, w.labour_slack, w.wage_cache
    );
    let best_com = (0..w.p.len()).max_by(|&a, &b| w.p.assessed_land[a].total_cmp(&w.p.assessed_land[b]));
    if let Some(i) = best_com {
        let _ = writeln!(
            out,
            "priciest parcel: {:?} floors {} units {} occ {} ask {:.0} land {:.0}k tax {:.0}/mo value(abs) {:.0}k",
            w.p.land_use[i],
            w.p.floors[i],
            w.p.units[i],
            w.p.occupied[i],
            w.p.asking_rent[i],
            w.p.assessed_land[i] / 1e3,
            w.p.tax_last[i],
            econ::current_use_value(w, i, &inv) / 1e3
        );
    }
    out
}

/// Plausibility checks. Each returned string describes one violation.
pub fn check(w: &World) -> Vec<String> {
    let mut v = Vec::new();
    let Some(m) = w.history.last() else { return v };
    let json = serde_json::to_value(m).unwrap_or_default();
    if let Some(o) = json.as_object() {
        for (k, x) in o {
            if let Some(x) = x.as_f64() {
                if !x.is_finite() {
                    v.push(format!("{k} is not finite"));
                }
            }
        }
    }
    let n = m.households.max(1) as f64;
    let growth = w.index as f64;
    let base = w.econ.base_wage as f64;

    // People and markets.
    let peak = w.history.iter().map(|h| h.households).max().unwrap_or(0);
    if (m.households as f64) < 0.4 * peak as f64 && peak > 200 {
        v.push(format!("population collapsed: {} households vs peak {}", m.households, peak));
    }
    if m.unemployment > 0.25 && m.households > 50 {
        v.push(format!("unemployment {:.0}%", m.unemployment * 100.0));
    }
    if m.unhoused as f64 > 0.15 * n && m.households > 50 {
        v.push(format!("{} of {} households unhoused", m.unhoused, m.households));
    }
    if m.housing_vacancy > 0.3 {
        v.push(format!("housing vacancy {:.0}%", m.housing_vacancy * 100.0));
    }
    if m.rent_burden > 0.6 {
        v.push(format!("median rent burden {:.0}%", m.rent_burden * 100.0));
    }
    if (m.mean_wage as f64) > 4.0 * base || ((m.mean_wage as f64) < 0.4 * base && m.employed > 0) {
        v.push(format!("mean wage {:.0} vs base {:.0}", m.mean_wage, base));
    }
    if m.median_rent as f64 > 2.0 * base {
        v.push(format!("median rent {:.0} vs base wage {:.0}", m.median_rent, base));
    }

    // Business space. (Some spare floor space is normal: a building houses
    // one firm, and firms shrink when wages rise. Mostly empty premises,
    // or buildings without any tenant, are not.)
    let com = use_stats(w, LandUse::Commercial);
    let ind = use_stats(w, LandUse::Industrial);
    let res = use_stats(w, LandUse::Residential);
    let biz_units = (com.units + ind.units) as f64;
    let biz_occ = (com.occupied + ind.occupied) as f64;
    if biz_units > 500.0 && biz_occ < 0.25 * biz_units {
        v.push(format!(
            "business space {:.0}% occupied ({} of {} job slots)",
            pct(biz_occ, biz_units),
            biz_occ,
            biz_units
        ));
    }
    if com.cells + ind.cells > 20 && (com.with_firm + ind.with_firm) * 2 < com.cells + ind.cells {
        v.push(format!(
            "{} of {} business buildings have no firm",
            com.cells + ind.cells - com.with_firm - ind.with_firm,
            com.cells + ind.cells
        ));
    }
    let jobs = biz_units
        + w.firms
            .iter()
            .filter(|f| f.alive && f.sector == Sector::Public)
            .map(|f| w.p.units[f.parcel as usize] as f64)
            .sum::<f64>();
    if jobs > 3.0 * n && m.households > 50 {
        v.push(format!("{jobs:.0} job slots for {} households", m.households));
    }
    if (res.units as f64) < 0.5 * n && m.households > 50 {
        v.push(format!("only {} dwellings for {} households", res.units, m.households));
    }

    // Taxes that the property cannot earn: annual property-type tax above
    // the rent the buildings actually take in.
    for (name, s) in [("residential", &res), ("commercial", &com), ("industrial", &ind)] {
        if s.cells > 20 && s.tax > 1.5 * s.rent_income.max(1.0) {
            v.push(format!(
                "{name} property tax {:.0}k/mo exceeds rent received {:.0}k/mo",
                s.tax / 1e3,
                s.rent_income / 1e3
            ));
        }
    }

    // Public finances.
    let income_total: f64 = w.hh.iter().filter(|h| h.alive).map(|h| (h.income_labor + h.income_capital) as f64).sum();
    let output = m.gdp.max(1.0);
    if m.revenue > 0.8 * output && m.households > 50 {
        v.push(format!("tax revenue {:.0}k/mo is {:.0}% of GDP", m.revenue / 1e3, pct(m.revenue, output)));
    }
    if w.gov.cash > 60.0 * w.gov.revenue_ema.max(1.0) && w.gov.cash > 1e7 {
        v.push(format!("government hoards {:.1} years of revenue", w.gov.cash / w.gov.revenue_ema.max(1.0) / 12.0));
    }
    if w.gov.cash < -36.0 * w.gov.revenue_ema.max(1.0) && w.gov.cash < -1e7 {
        v.push(format!(
            "government debt {:.1}M is {:.1} years of revenue",
            -w.gov.cash / 1e6,
            -w.gov.cash / w.gov.revenue_ema.max(1.0) / 12.0
        ));
    }
    let transfers = m.spend_dividend + m.spend_welfare + m.spend_rebate;
    if transfers > 1.0 * income_total.max(1.0) && m.households > 50 {
        v.push(format!(
            "transfers {:.0}k/mo exceed residents' market income {:.0}k/mo",
            transfers / 1e3,
            income_total / 1e3
        ));
    }
    if m.spend_welfare > 1.001 * w.gov.welfare_paid_budget.max(0.0) + 1.0 {
        v.push(format!(
            "welfare paid {:.0}k exceeds its budget {:.0}k",
            m.spend_welfare / 1e3,
            w.gov.welfare_paid_budget / 1e3
        ));
    }

    // Bookkeeping invariants.
    let dead_owner = |o: Owner| matches!(o, Owner::Hh(id) if !w.hh[id as usize].alive);
    let dead_parcels = (0..w.p.len()).filter(|&i| dead_owner(w.p.owner[i])).count();
    if dead_parcels > 0 {
        v.push(format!("{dead_parcels} parcels owned by households that left"));
    }
    let dead_firms = w.firms.iter().filter(|f| f.alive && dead_owner(f.owner)).count();
    if dead_firms > 0 {
        v.push(format!("{dead_firms} firms owned by households that left"));
    }
    // Each firm's head count and skill total must match the households it
    // employs: output is computed from them.
    let mut staff = vec![(0u32, 0f64); w.firms.len()];
    for h in w.hh.iter().filter(|h| h.alive && h.job != NONE) {
        let s = &mut staff[h.job as usize];
        s.0 += 1;
        s.1 += h.skill as f64;
    }
    let miscounted = w
        .firms
        .iter()
        .zip(&staff)
        .filter(|(f, s)| f.alive && (f.workers as u32 != s.0 || (f.skill_sum as f64 - s.1).abs() > 0.01 * s.1 + 0.05))
        .count();
    if miscounted > 0 {
        v.push(format!("{miscounted} firms' head count or skill total does not match their staff"));
    }
    let on_streets =
        (0..w.p.len()).filter(|&i| w.p.land_use[i].is_private_use() && crate::government::on_street_plan(w, i)).count();
    if on_streets > 0 {
        v.push(format!("{on_streets} buildings block planned streets"));
    }

    let max_cash = w.hh.iter().filter(|h| h.alive).map(|h| h.cash).fold(0.0, f64::max);
    if max_cash > 2e8 * growth {
        v.push(format!("a household holds {max_cash:.3e} in cash"));
    }
    if m.land_price_index as f64 > 50_000.0 * growth {
        v.push(format!("land price index {:.0}k per urban cell", m.land_price_index));
    }
    // Land values may cycle with rents, but not boom and bust by multiples
    // within a few years (a symptom of self-referential expectations or
    // assessments).
    if w.year() >= 20.0 {
        let recent = &w.history[w.history.len() - 60..];
        let (lo, hi) = recent
            .iter()
            .fold((f32::INFINITY, 0f32), |(lo, hi), h| (lo.min(h.land_price_index), hi.max(h.land_price_index)));
        if lo > 0.0 && hi > 3.0 * lo {
            v.push(format!("land price index swung {:.1}x within five years ({lo:.0} to {hi:.0})", hi / lo));
        }
    }
    v
}
