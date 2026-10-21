//! Labour market: firms adapt hiring and wages from simple signals
//! (Lengnick-style), workers search a few vacancies and decide whether
//! working is worth it after taxes, commute and lost benefits.

use crate::fields::commute_cost;
use crate::types::*;
use crate::world::World;

/// Workers per firm, rebuilt each tick (CSR layout).
pub struct Roster {
    pub offsets: Vec<u32>,
    pub members: Vec<u32>,
}

pub fn roster(w: &World) -> Roster {
    let nf = w.firms.len();
    let mut counts = vec![0u32; nf + 1];
    for h in w.hh.iter().filter(|h| h.alive && h.job != NONE) {
        counts[h.job as usize + 1] += 1;
    }
    for k in 1..=nf {
        counts[k] += counts[k - 1];
    }
    let mut fill = counts.clone();
    let mut members = vec![0u32; counts[nf] as usize];
    for (id, h) in w.hh.iter().enumerate() {
        if h.alive && h.job != NONE {
            let f = h.job as usize;
            members[fill[f] as usize] = id as u32;
            fill[f] += 1;
        }
    }
    Roster { offsets: counts, members }
}

/// Sales a unit of shop capacity at `i` can make per month: its share of
/// local spending (Huff), capped by what one worker can serve.
pub fn retail_sales_per_unit(w: &World, i: usize) -> f32 {
    w.f.retail_draw[i].min(w.econ.retail_output_per_worker)
}

/// Revenue a worker of unit skill would add at the margin, per month.
pub fn industry_marginal_product(w: &World, f: &Firm) -> f32 {
    let e = &w.econ;
    let i = f.parcel as usize;
    let a = industry_tfp(w, f, i);
    let s = f.skill_sum.max(1.0);
    e.labour_elasticity * a * s.powf(e.labour_elasticity - 1.0)
}

pub fn industry_tfp(w: &World, f: &Firm, i: usize) -> f32 {
    let e = &w.econ;
    e.industry_output_per_worker
        * e.industry_price
        * w.cycle
        * f.productivity
        * (1.0 + e.agglomeration * (1.0 + w.f.industry_access[i]).ln())
}

pub fn firm_decisions(w: &mut World) {
    let unemployment = w.last_unemployment();
    let payroll = w.taxes.payroll_rate() as f32;
    let public_wage = w.market_wage_rate();
    let public_capacity_frac = w.public_staffing();
    for id in 0..w.firms.len() {
        if !w.firms[id].alive {
            continue;
        }
        let f = &w.firms[id];
        let i = f.parcel as usize;
        let capacity = w.p.units[i];
        let workers = f.workers;
        let avg_skill = if workers > 0 { f.skill_sum / workers as f32 } else { 1.0 };
        let wage_cost = f.wage_rate * avg_skill * (1.0 + payroll);
        let desired: u16 = match f.sector {
            Sector::Industry => {
                let mp = industry_marginal_product(w, f) * avg_skill;
                // Emission tax per worker at the chosen intensity also counts.
                let em_cost =
                    w.taxes.estimate(|m| m.emission(w.econ.pollution_per_worker as f64 * f.emission_intensity as f64))
                        as f32;
                let margin = mp - wage_cost - em_cost;
                step_toward(workers, margin / wage_cost.max(1.0))
            }
            Sector::Retail => {
                let rev_per_worker =
                    // (A shop without a month of trading yet goes by the local draw.)
                    if workers > 1 && f.age_months > 0 {
                        f.revenue / workers as f32
                    } else {
                        f.productivity * retail_sales_per_unit(w, i)
                    };
                let margin = rev_per_worker * 0.92 - wage_cost;
                step_toward(workers, margin / wage_cost.max(1.0))
            }
            Sector::Public => (capacity as f32 * public_capacity_frac) as u16,
        };
        let desired = desired.min(capacity).max(if f.sector == Sector::Public { 0 } else { 1 });
        let profitable_margin = match f.sector {
            Sector::Industry => industry_marginal_product(w, f) * avg_skill > wage_cost * 1.03,
            Sector::Retail => workers == 0 || f.age_months == 0 || f.revenue / workers.max(1) as f32 > wage_cost * 1.03,
            Sector::Public => false,
        };
        let f = &mut w.firms[id];
        f.desired_workers = desired;
        if f.sector == Sector::Public {
            f.wage_rate = public_wage;
            continue;
        }
        if desired > f.workers {
            f.months_unfilled += 1;
            if f.months_unfilled >= 2 && profitable_margin {
                f.wage_rate *= 1.01;
            }
        } else {
            f.months_unfilled = 0;
            if unemployment > 0.06 {
                f.wage_rate *= 0.996;
            }
        }
        if !profitable_margin && f.workers > 0 && f.months_loss > 0 {
            f.wage_rate *= 0.995;
        }
        f.wage_rate = f.wage_rate.clamp(0.35 * w.econ.base_wage, 6.0 * w.econ.base_wage);
    }

    // Layoffs where workforce exceeds desired.
    let r = roster(w);
    for id in 0..w.firms.len() {
        let f = &w.firms[id];
        if !f.alive || f.workers <= f.desired_workers {
            continue;
        }
        let excess = (f.workers - f.desired_workers) as usize;
        let lo = r.offsets[id] as usize;
        let hi = r.offsets[id + 1] as usize;
        let mut staff: Vec<u32> = r.members[lo..hi].to_vec();
        w.rng.shuffle(&mut staff);
        for &hid in staff.iter().take(excess) {
            fire(w, hid);
        }
    }
}

fn step_toward(workers: u16, margin_ratio: f32) -> u16 {
    let wf = workers as f32;
    // Expand faster the larger the margin; contract cautiously.
    let step = (wf * (0.05 + 0.5 * margin_ratio.clamp(0.0, 0.5))).max(1.0);
    if margin_ratio > 0.08 {
        (wf + step) as u16
    } else if margin_ratio < -0.03 {
        (wf - (wf * 0.06).max(1.0)).max(0.0) as u16
    } else {
        workers
    }
}

pub fn fire(w: &mut World, hid: u32) {
    let h = &mut w.hh[hid as usize];
    if h.job == NONE {
        return;
    }
    let f = &mut w.firms[h.job as usize];
    f.workers = f.workers.saturating_sub(1);
    f.skill_sum = (f.skill_sum - h.skill).max(0.0);
    let parcel = f.parcel as usize;
    w.p.occupied[parcel] = f.workers;
    h.job = NONE;
    h.wage = 0.0;
}

pub fn hire(w: &mut World, hid: u32, fid: u32) {
    let skill = w.hh[hid as usize].skill;
    let wage = w.firms[fid as usize].wage_rate * skill;
    {
        let h = &mut w.hh[hid as usize];
        h.job = fid;
        h.wage = wage;
        h.months_unemployed = 0;
    }
    let f = &mut w.firms[fid as usize];
    f.workers += 1;
    f.skill_sum += skill;
    w.p.occupied[f.parcel as usize] = f.workers;
}

/// Monthly net gain from taking a job paying `gross`, given other income,
/// commute and benefit withdrawal. Returns (with taxes, without taxes).
pub fn work_gain(w: &World, h: &Household, gross: f32, commute: f32) -> (f32, f32) {
    let other = h.income_capital as f64;
    let tax = w.taxes.income_monthly(gross as f64, other) - w.taxes.income_monthly(0.0, other);
    let lost_welfare = (w.gov_welfare_level() as f64).min(0.5 * gross as f64);
    let with_tax = gross as f64 - tax - lost_welfare - commute as f64;
    let no_tax = gross as f64 - lost_welfare - commute as f64;
    (with_tax as f32, no_tax as f32)
}

pub fn matching(w: &mut World) {
    let k = w.econ.search_samples.max(1);
    let mut openings: Vec<u32> = Vec::new();
    for (id, f) in w.firms.iter().enumerate() {
        if f.alive && f.desired_workers > f.workers {
            openings.push(id as u32);
        }
    }
    let mut seekers: Vec<u32> = Vec::new();
    for (id, h) in w.hh.iter().enumerate() {
        if !h.alive {
            continue;
        }
        if h.job == NONE || w.rng.chance(0.04) {
            seekers.push(id as u32);
        }
    }
    w.rng.shuffle(&mut seekers);
    let mut deterred = 0u32;
    for hid in seekers {
        if openings.is_empty() {
            break;
        }
        let h = w.hh[hid as usize].clone();
        let home = if h.home != NONE { Some(h.home as usize) } else { None };
        let commute_to = |w: &World, parcel: usize| match home {
            Some(hm) => commute_cost(w, hm, parcel),
            None => w.econ.commute_cost_per_cell * 10.0,
        };
        // value of current situation
        let current = if h.job != NONE {
            let f = &w.firms[h.job as usize];
            work_gain(w, &h, f.wage_rate * h.skill, commute_to(w, f.parcel as usize)).0 - h.effort_cost
        } else {
            0.0
        };
        let mut best: Option<(u32, f32, f32)> = None;
        for _ in 0..k {
            let slot = w.rng.below(openings.len());
            let fid = openings[slot];
            let f = &w.firms[fid as usize];
            if !f.alive || f.desired_workers <= f.workers || fid == h.job {
                continue;
            }
            let (g, g0) = work_gain(w, &h, f.wage_rate * h.skill, commute_to(w, f.parcel as usize));
            let v = g - h.effort_cost;
            if best.map_or(true, |b| v > b.1) {
                best = Some((fid, v, g0 - h.effort_cost));
            }
        }
        if let Some((fid, v, v_no_tax)) = best {
            let threshold = if h.job == NONE { 0.0 } else { current + 80.0 * w.index };
            if v > threshold {
                if h.job != NONE {
                    fire(w, hid);
                }
                hire(w, hid, fid);
                let f = &w.firms[fid as usize];
                if f.workers >= f.desired_workers {
                    openings.retain(|&o| o != fid);
                }
            } else if h.job == NONE && v_no_tax > 0.0 {
                // Would have worked in a world without taxes: a labour-supply distortion.
                deterred += 1;
            }
        }
    }
    w.flows.deterred_workers = deterred;
}

impl World {
    pub fn last_unemployment(&self) -> f32 {
        self.history.last().map(|m| m.unemployment).unwrap_or(0.05)
    }
    /// Average private wage per unit skill, cached once per tick.
    pub fn market_wage_rate(&self) -> f32 {
        self.wage_cache
    }

    pub fn update_wage_cache(&mut self) {
        let mut s = 0.0;
        let mut n = 0.0;
        for f in self.firms.iter().filter(|f| f.alive && f.sector != Sector::Public && f.workers > 0) {
            s += f.wage_rate * f.workers as f32;
            n += f.workers as f32;
        }
        self.wage_cache = if n > 0.0 { s / n } else { self.econ.base_wage };
    }
    pub fn gov_welfare_level(&self) -> f32 {
        self.gov.welfare_level as f32
    }
    /// Fraction of public job capacity the service budget can pay for.
    pub fn public_staffing(&self) -> f32 {
        self.gov.public_staffing as f32
    }
}
