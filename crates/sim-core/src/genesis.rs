//! The founding settlement: a river crossing with a few streets, homes,
//! shops, a workshop district and a town hall. Everything after this grows
//! from agent decisions.

use crate::econ;
use crate::government::set_road;
use crate::migration::new_household;
use crate::types::*;
use crate::world::World;

fn pick_center(w: &mut World) -> usize {
    let dims = w.dims;
    let (mx, my) = (dims.w as f32 / 2.0, dims.h as f32 / 2.0);
    let mut best = (0usize, f32::INFINITY);
    for i in 0..dims.len() {
        if w.terrain.water[i] {
            continue;
        }
        let (x, y) = dims.xy(i);
        let d_center = ((x as f32 - mx).powi(2) + (y as f32 - my).powi(2)).sqrt();
        let wd = w.terrain.water_dist[i];
        let score = d_center / 6.0 + (wd - 2.0).abs() * 2.0 + w.terrain.slope[i] * 3.0;
        if score < best.1 {
            best = (i, score);
        }
    }
    best.0
}

pub fn found_settlement(w: &mut World) {
    let dims = w.dims;
    let center = pick_center(w);
    w.center = center;
    let (cx, cy) = dims.xy(center);

    // Main roads crossing at the centre (bridging the river), plus a small grid.
    let reach = 18usize;
    for x in cx.saturating_sub(reach)..(cx + reach).min(dims.w) {
        set_road(w, dims.idx(x, cy));
    }
    for y in cy.saturating_sub(reach)..(cy + reach).min(dims.h) {
        set_road(w, dims.idx(cx, y));
    }
    let r = 7usize;
    for k in [cx.saturating_sub(5), cx + 5] {
        if k >= dims.w {
            continue;
        }
        for y in cy.saturating_sub(r)..(cy + r).min(dims.h) {
            let i = dims.idx(k, y);
            if !w.terrain.water[i] {
                set_road(w, i);
            }
        }
    }
    for k in [cy.saturating_sub(5), cy + 5] {
        if k >= dims.h {
            continue;
        }
        for x in cx.saturating_sub(r)..(cx + r).min(dims.w) {
            let i = dims.idx(x, k);
            if !w.terrain.water[i] {
                set_road(w, i);
            }
        }
    }

    // Farms on fertile land around the village.
    for i in 0..dims.len() {
        if w.p.land_use[i] != LandUse::Wild {
            continue;
        }
        let d = dims.manhattan(i, center);
        if d < 34.0 && w.terrain.fertility[i] > 0.5 && w.terrain.vegetation[i] < 0.6 && w.rng.chance(0.85) {
            w.p.land_use[i] = LandUse::Farm;
            w.p.nature[i] = 0.12;
        }
    }

    // Candidate building plots: road-accessible land near the centre.
    let mut plots: Vec<usize> = (0..dims.len())
        .filter(|&i| w.p.road_access[i] && w.p.land_use[i].is_developable() && !w.terrain.water[i])
        .filter(|&i| dims.manhattan(i, center) <= 14.0)
        .collect();
    plots.sort_by(|&a, &b| dims.manhattan(a, center).total_cmp(&dims.manhattan(b, center)).then(a.cmp(&b)));

    let n_hh = w.cfg.initial_households as usize;
    let units_per_floor = w.cfg.econ.res_units_per_floor as usize;
    let res_needed = (n_hh as f32 * 1.08 / (2 * units_per_floor) as f32).ceil() as usize;
    // Enough jobs for the founders: half industrial, a third in shops.
    let e = &w.cfg.econ;
    let n_ind = ((0.5 * n_hh as f32) / (2 * e.ind_jobs_per_floor) as f32).ceil() as usize;
    let n_com = ((0.33 * n_hh as f32) / e.com_jobs_per_floor as f32).ceil() as usize;
    let mut res_cells = Vec::new();
    let mut com_cells = Vec::new();
    let mut ind_cells = Vec::new();
    let mut civic = None;
    for (k, &i) in plots.iter().enumerate() {
        if civic.is_none() {
            civic = Some(i);
            continue;
        }
        let (x, _) = dims.xy(i);
        // A workshop district on the far side of the river from the centre.
        if ind_cells.len() < n_ind && k > plots.len() / 3 && (x as isize - cx as isize).abs() > 4 {
            ind_cells.push(i);
        } else if com_cells.len() < n_com && k % 4 == 1 {
            com_cells.push(i);
        } else if res_cells.len() < res_needed {
            res_cells.push(i);
        }
    }
    let place = |w: &mut World, i: usize, u: LandUse, floors: u8| {
        w.p.land_use[i] = u;
        w.p.floors[i] = floors;
        w.p.units[i] = econ::units_for(w, u, floors);
        w.p.condition[i] = 0.8 + 0.2 * w.rng.f32();
        w.p.age_months[i] = w.rng.below(240) as u32;
        w.p.nature[i] = if u == LandUse::Residential { 0.25 } else { 0.03 };
    };
    for &i in &res_cells {
        place(w, i, LandUse::Residential, 2);
        w.p.asking_rent[i] = 900.0;
    }
    for &i in &com_cells {
        place(w, i, LandUse::Commercial, 1);
        w.p.asking_rent[i] = 500.0;
    }
    for &i in &ind_cells {
        place(w, i, LandUse::Industrial, 2);
        w.p.asking_rent[i] = 400.0;
    }
    if let Some(i) = civic {
        place(w, i, LandUse::Civic, 2);
        w.p.owner[i] = Owner::Gov;
    }

    // Founding households.
    let sigma = w.cfg.econ.skill_sigma as f64;
    for _ in 0..n_hh {
        let skill = w.rng.lognormal(0.0, sigma) as f32;
        let mut h = new_household(w, skill);
        h.age_months = (25.0 + w.rng.f64() * 40.0) as u32 * 12;
        w.add_household(h);
    }
    // Homes.
    let mut slots: Vec<usize> = Vec::new();
    for &i in &res_cells {
        for _ in 0..w.p.units[i] {
            slots.push(i);
        }
    }
    w.rng.shuffle(&mut slots);
    for id in 0..w.hh.len() {
        if let Some(i) = slots.pop() {
            w.hh[id].home = i as u32;
            w.hh[id].rent = w.p.asking_rent[i];
            w.p.occupied[i] += 1;
        }
    }
    // Some founders own land in the village; the rest belongs to outside landholders.
    let n = w.hh.len();
    let mut by_cash: Vec<u32> = (0..n as u32).collect();
    by_cash.sort_by(|&a, &b| w.hh[b as usize].cash.total_cmp(&w.hh[a as usize].cash));
    let top = (n / 6).max(1);
    for &i in res_cells.iter().chain(&com_cells).chain(&ind_cells) {
        if w.rng.chance(0.6) {
            let id = by_cash[w.rng.below(top)];
            w.p.owner[i] = Owner::Hh(id);
        }
    }

    // Firms with founders' or outside capital.
    let base = w.cfg.econ.base_wage;
    for (cells, sector) in [(&com_cells, Sector::Retail), (&ind_cells, Sector::Industry)] {
        for &i in cells.iter() {
            let owner = if w.rng.chance(0.5) { Owner::Hh(by_cash[w.rng.below(top)]) } else { Owner::Absentee };
            let productivity = w.rng.lognormal(0.0, 0.15) as f32;
            w.add_firm(Firm {
                alive: true,
                sector,
                parcel: i as u32,
                owner,
                cash: 50_000.0,
                productivity,
                wage_rate: base,
                workers: 0,
                skill_sum: 0.0,
                desired_workers: w.p.units[i] / 2,
                months_unfilled: 0,
                months_loss: 0,
                emission_intensity: 1.0,
                revenue: 0.0,
                profit: 0.0,
                age_months: 0,
            });
        }
    }
    if let Some(i) = civic {
        crate::firms::open_public(w, i);
    }

    // Prime last month's shopping so shops have customers on day one.
    for id in 0..w.hh.len() {
        w.hh[id].consumption = 1200.0;
        if w.hh[id].home != NONE {
            w.retail_pool[w.hh[id].home as usize] += 1200.0 * w.cfg.econ.local_spend_share;
        }
    }
    // Founders start out employed.
    let mut job_slots: Vec<u32> = Vec::new();
    for (fid, f) in w.firms.iter().enumerate() {
        let take = (w.p.units[f.parcel as usize] as f32 * 0.9) as usize;
        job_slots.extend(std::iter::repeat(fid as u32).take(take));
    }
    w.rng.shuffle(&mut job_slots);
    for id in 0..w.hh.len() as u32 {
        if w.rng.chance(0.93) {
            if let Some(fid) = job_slots.pop() {
                crate::labour::hire(w, id, fid);
            }
        }
    }
    for f in w.firms.iter_mut() {
        f.desired_workers = f.workers;
    }
    w.gov.cash = 2_000_000.0;
    w.gov.public_staffing = 1.0;
    w.gov.welfare_level = 300.0;
    crate::fields::update(w);
    crate::land::assess(w, true);
    crate::households::update_wealth(w);
}
