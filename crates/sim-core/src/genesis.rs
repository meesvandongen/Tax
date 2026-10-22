//! The founding settlement: a river crossing with a few streets, homes,
//! shops, a workshop district and a town hall. Everything after this grows
//! from agent decisions.

use crate::econ;
use crate::migration::new_household;
use crate::roads::{set_road, Routes};
use crate::types::*;
use crate::world::World;

fn pick_center(w: &mut World) -> usize {
    let m = &w.mesh;
    let (mx, my) = (w.dims.w as f32 / 2.0, w.dims.h as f32 / 2.0);
    let mut best = (0usize, f32::INFINITY);
    for i in 0..m.len() {
        if w.terrain.water[i] {
            continue;
        }
        let [x, y] = m.centroid[i];
        let d_center = ((x - mx).powi(2) + (y - my).powi(2)).sqrt();
        let wd = w.terrain.water_dist[i];
        let score = d_center / 6.0 + (wd - 2.0).abs() * 2.0 + w.terrain.slope[i] * 3.0;
        if score < best.1 {
            best = (i, score);
        }
    }
    best.0
}

/// Lay a road along the cheapest route from `from` to `to`: following rows
/// of parcels, avoiding slopes, bridging water only if `bridges`.
fn lay_route(w: &mut World, from: usize, to: usize, bridges: bool) {
    let enter = |j: usize, _: usize| {
        if w.p.land_use[j] == LandUse::Road {
            0.3
        } else if w.terrain.water[j] {
            if bridges {
                6.0
            } else {
                f64::INFINITY
            }
        } else {
            1.0 + w.terrain.slope[j] as f64 / 4.0
        }
    };
    let starts: Vec<(usize, usize, f64)> = (0..4).map(|e| (from, e, 0.0)).collect();
    let routes = Routes::search(w, &starts, enter, |_| 0.0, 0.8, f64::INFINITY, usize::MAX, Some(to));
    let (cost, s) = routes.best(to);
    if cost.is_finite() {
        for i in routes.path(s) {
            set_road(w, i);
        }
    }
}

pub fn found_settlement(w: &mut World) {
    let center = pick_center(w);
    w.center = center;
    let [cx, cy] = w.mesh.centroid[center];
    let (wf, hf) = (w.dims.w as f32, w.dims.h as f32);
    let at =
        |w: &World, dx: f32, dy: f32| w.mesh.cell_at((cx + dx).clamp(1.0, wf - 2.0), (cy + dy).clamp(1.0, hf - 2.0));

    // Main roads out of the centre (bridging the river), and a ring street.
    set_road(w, center);
    let reach = 18.0;
    for (dx, dy) in [(reach, 0.0), (-reach, 0.0), (0.0, reach), (0.0, -reach)] {
        let to = at(w, dx, dy);
        lay_route(w, center, to, true);
    }
    let r = 5.0;
    let ring: Vec<usize> = [(r, r), (r, -r), (-r, -r), (-r, r)].iter().map(|&(dx, dy)| at(w, dx, dy)).collect();
    for k in 0..4 {
        if !w.terrain.water[ring[k]] && !w.terrain.water[ring[(k + 1) % 4]] {
            lay_route(w, ring[k], ring[(k + 1) % 4], false);
        }
    }

    // Farms on fertile land around the village.
    for i in 0..w.p.len() {
        if w.p.land_use[i] != LandUse::Wild {
            continue;
        }
        let d = w.mesh.dist(i, center);
        if d < 27.0 && w.terrain.fertility[i] > 0.5 && w.terrain.vegetation[i] < 0.6 && w.rng.chance(0.85) {
            w.p.land_use[i] = LandUse::Farm;
            w.p.nature[i] = 0.12;
        }
    }

    // Candidate building plots: road-accessible land near the centre.
    let m = &w.mesh;
    let mut plots: Vec<usize> = (0..w.p.len())
        .filter(|&i| w.p.road_access[i] && w.p.land_use[i].is_developable() && !w.terrain.water[i])
        .filter(|&i| m.dist(i, center) <= 12.0)
        .collect();
    plots.sort_by(|&a, &b| m.dist(a, center).total_cmp(&m.dist(b, center)).then(a.cmp(&b)));

    let n_hh = w.cfg.initial_households as usize;
    let units_per_floor = w.econ.res_units_per_floor as usize;
    let res_needed = (n_hh as f32 * 1.08 / (2 * units_per_floor) as f32).ceil() as usize;
    // Enough jobs for the founders: most in the export workshops, a fifth
    // in shops (what local spending can support).
    let e = &w.econ;
    let n_ind = ((0.85 * n_hh as f32) / (2 * e.ind_jobs_per_floor) as f32).ceil() as usize;
    let n_com = ((0.2 * n_hh as f32) / e.com_jobs_per_floor as f32).ceil() as usize;
    let mut res_cells = Vec::new();
    let mut com_cells = Vec::new();
    let mut ind_cells = Vec::new();
    let mut civic = None;
    for (k, &i) in plots.iter().enumerate() {
        if civic.is_none() {
            civic = Some(i);
            continue;
        }
        let x = m.centroid[i][0];
        // A workshop district on the far side of the river from the centre.
        if ind_cells.len() < n_ind && k > plots.len() / 3 && (x - cx).abs() > 4.0 {
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
    let sigma = w.econ.skill_sigma as f64;
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
    let base = w.econ.base_wage;
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
            w.retail_pool[w.hh[id].home as usize] += 1200.0 * w.econ.local_spend_share;
        }
    }
    // Founders start out employed.
    let mut job_slots: Vec<u32> = Vec::new();
    for (fid, f) in w.firms.iter().enumerate() {
        let take = (w.p.units[f.parcel as usize] as f32 * 0.9) as usize;
        job_slots.extend(std::iter::repeat_n(fid as u32, take));
    }
    w.rng.shuffle(&mut job_slots);
    for id in 0..w.hh.len() as u32 {
        if w.rng.chance(0.97) {
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
