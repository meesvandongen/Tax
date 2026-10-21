//! Derived spatial fields (accessibility, amenities, pollution, market rent
//! surfaces). Recomputed every tick from the agent state; they are the
//! "information" agents use when they evaluate locations.

use crate::grid::{exp_blur, smoothed_mean};
use crate::types::*;
use crate::world::World;

#[derive(Clone, Debug, Default)]
pub struct Fields {
    /// Workers by job location.
    pub jobs: Vec<f32>,
    /// Soft-min distance (cells) to the city's jobs.
    pub commute_eff: Vec<f32>,
    /// Households by home cell.
    pub pop: Vec<f32>,
    /// Local consumer spending by home cell (last month).
    pub spend: Vec<f32>,
    /// Retail revenue a unit of shop capacity would capture here (Huff model).
    pub retail_draw: Vec<f32>,
    /// Staffed retail capacity by cell.
    pub retail_cap: Vec<f32>,
    /// Competing shop capacity within reach (blurred `retail_cap`).
    pub retail_supply: Vec<f32>,
    /// Emissions by source cell (last month).
    pub emissions: Vec<f32>,
    /// Ambient pollution (neighbourhood mean of emissions).
    pub pollution: Vec<f32>,
    /// Industry jobs nearby (agglomeration).
    pub industry_access: Vec<f32>,
    /// Money-equivalent amenity value per month (parks, water, nature).
    pub amenity: Vec<f32>,
    /// Public-service quality 0..1.
    pub service: Vec<f32>,
    /// Residential location premium, money per dwelling per month.
    pub loc_res: Vec<f32>,
    /// Estimated market rent per unit per month for each use.
    pub rent_res: Vec<f32>,
    pub rent_com: Vec<f32>,
    pub rent_ind: Vec<f32>,
    /// Business rents per job slot that tenants nearby actually pay, and how
    /// much evidence there is for them (0 = none, → 1 = plenty).
    pub obs_com: Vec<f32>,
    pub obs_ind: Vec<f32>,
    pub evid_com: Vec<f32>,
    pub evid_ind: Vec<f32>,
    /// Smoothed occupancy rates per use.
    pub occ_res: Vec<f32>,
    pub occ_com: Vec<f32>,
    pub occ_ind: Vec<f32>,
    /// Smoothed unit counts per use (for absorption estimates).
    pub units_res: Vec<f32>,
    pub units_com: Vec<f32>,
    pub units_ind: Vec<f32>,
    /// Units under construction nearby, per use (the development pipeline).
    pub pipeline_res: Vec<f32>,
    pub pipeline_com: Vec<f32>,
    pub pipeline_ind: Vec<f32>,
    /// City-wide rent level (rent minus location premium), the fallback.
    pub level_res: f32,
}

impl Fields {
    pub fn new(n: usize) -> Fields {
        let z = || vec![0f32; n];
        Fields {
            jobs: z(),
            commute_eff: z(),
            pop: z(),
            spend: z(),
            retail_draw: z(),
            retail_cap: z(),
            retail_supply: z(),
            emissions: z(),
            pollution: z(),
            industry_access: z(),
            amenity: z(),
            service: z(),
            loc_res: z(),
            rent_res: z(),
            rent_com: z(),
            rent_ind: z(),
            obs_com: z(),
            obs_ind: z(),
            evid_com: z(),
            evid_ind: z(),
            occ_res: z(),
            occ_com: z(),
            occ_ind: z(),
            units_res: z(),
            units_com: z(),
            units_ind: z(),
            pipeline_res: z(),
            pipeline_com: z(),
            pipeline_ind: z(),
            level_res: 700.0,
        }
    }
}

/// Total mass of the kernel a^(|dx|+|dy|) on an unbounded grid.
fn kernel_mass(lambda: f32) -> f32 {
    let a = (-1.0 / lambda).exp();
    let s = (1.0 + a) / (1.0 - a);
    s * s
}

/// Amenity weights (money per month at full exposure).
pub const PARK_VALUE: f32 = 320.0;
pub const WATER_VALUE: f32 = 160.0;
pub const NATURE_VALUE: f32 = 140.0;
pub const SERVICE_VALUE: f32 = 380.0;
pub const POLLUTION_COST: f32 = 30.0;
/// Shopping elsewhere (outside the city / online) as competing capacity.
pub const OUTSIDE_SHOP_CAPACITY: f32 = 2.0;
/// Decay length (cells) of shopping trips.
pub const SHOP_LAMBDA: f32 = 3.0;
/// Fraction of a shop's capacity "seen" by its typical customer: customers
/// are spread over the catchment, so a new shop competes with itself less
/// than if they all lived next door.
pub const SHOP_REACH_SHARE: f32 = 0.25;
/// Public workers per household for full service quality.
pub const SERVICE_RATIO: f32 = 0.06;

/// Municipal boundary: land within `CITY_MARGIN` cells of urban use.
pub const CITY_MARGIN: f32 = 4.0;

fn update_city_limits(w: &mut World) {
    let dims = w.dims;
    let n = dims.len();
    let mut d = vec![1e6f32; n];
    for i in 0..n {
        let u = w.p.land_use[i];
        if u.is_urban() && u != LandUse::Water {
            d[i] = 0.0;
        }
    }
    for _ in 0..2 {
        for i in 0..n {
            for j in dims.neighbors4(i) {
                d[i] = d[i].min(d[j] + 1.0);
            }
        }
        for i in (0..n).rev() {
            for j in dims.neighbors4(i) {
                d[i] = d[i].min(d[j] + 1.0);
            }
        }
    }
    for i in 0..n {
        w.p.in_city[i] = d[i] <= CITY_MARGIN;
    }
}

pub fn update(w: &mut World) {
    update_city_limits(w);
    let dims = w.dims;
    let n = dims.len();
    let e = w.econ.clone();
    let p = &w.p;

    // ---- sources
    let mut jobs = vec![0f32; n];
    let mut ind_jobs = vec![0f32; n];
    let mut retail_cap = vec![0f32; n];
    let mut public_jobs = vec![0f32; n];
    for f in w.firms.iter().filter(|f| f.alive) {
        let i = f.parcel as usize;
        jobs[i] += f.workers as f32;
        match f.sector {
            Sector::Industry => ind_jobs[i] += f.workers as f32,
            Sector::Retail => retail_cap[i] += f.workers as f32 * f.productivity,
            Sector::Public => public_jobs[i] += f.workers as f32,
        }
    }
    let mut pop = vec![0f32; n];
    let mut spend = vec![0f32; n];
    for h in w.hh.iter().filter(|h| h.alive) {
        if h.home != NONE {
            pop[h.home as usize] += 1.0;
            spend[h.home as usize] += h.consumption * e.local_spend_share;
        }
    }

    // ---- accessibility: soft-min distance to jobs
    let la = e.access_decay_cells;
    let job_access = exp_blur(&jobs, dims, la);
    let total_jobs: f32 = jobs.iter().sum::<f32>().max(1.0);
    let commute_eff: Vec<f32> =
        job_access.iter().map(|&j| (-la * (j / total_jobs).max(1e-9).ln()).clamp(0.0, 4.0 * dims.w as f32)).collect();

    // ---- retail: Huff gravity model
    let shop_lambda = SHOP_LAMBDA;
    let s_field = exp_blur(&retail_cap, dims, shop_lambda);
    let ratio: Vec<f32> = spend.iter().zip(&s_field).map(|(c, s)| c / (s + OUTSIDE_SHOP_CAPACITY)).collect();
    let retail_draw = exp_blur(&ratio, dims, shop_lambda);

    // ---- pollution and amenities
    let lp = e.pollution_decay_cells;
    let mp = kernel_mass(lp);
    let pollution: Vec<f32> = exp_blur(&w.f.emissions, dims, lp).iter().map(|v| v / mp).collect();
    let lam = e.amenity_decay_cells;
    let ma = kernel_mass(lam);
    let parks: Vec<f32> = (0..n).map(|i| (p.land_use[i] == LandUse::Park) as u8 as f32).collect();
    let park_frac = exp_blur(&parks, dims, lam);
    let nature_src: Vec<f32> = (0..n).map(|i| if p.land_use[i] == LandUse::Park { 0.0 } else { p.nature[i] }).collect();
    let nature_frac = exp_blur(&nature_src, dims, lam);
    let pub_access = exp_blur(&public_jobs, dims, 5.0);
    let pop_access = exp_blur(&pop, dims, 5.0);
    let industry_access = exp_blur(&ind_jobs, dims, 4.0);

    let mut amenity = vec![0f32; n];
    let mut service = vec![0f32; n];
    let mut loc_res = vec![0f32; n];
    let ix = w.index;
    for i in 0..n {
        let water = (-w.terrain.water_dist[i] / 3.0).exp() * (!w.terrain.water[i]) as u8 as f32;
        amenity[i] = ix
            * (PARK_VALUE * (park_frac[i] / ma * 4.0).min(1.0)
                + WATER_VALUE * water
                + NATURE_VALUE * (nature_frac[i] / ma).min(1.0));
        service[i] = (pub_access[i] / (pop_access[i].max(1.0) * SERVICE_RATIO)).min(1.0);
        loc_res[i] = -e.commute_cost_per_cell * commute_eff[i] + amenity[i] + ix * SERVICE_VALUE * service[i]
            - ix * POLLUTION_COST * pollution[i];
    }

    // ---- residential rent surface: hedonic level + location premium.
    // (Business space is priced by firms' bid-rents, see econ::firm_bid_rent.)
    let mut d_res = vec![0f32; n];
    let mut wt_res = vec![0f32; n];
    let mut occ_src = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    let mut biz_rent = [vec![0f32; n], vec![0f32; n]];
    let mut biz_wt = [vec![0f32; n], vec![0f32; n]];
    let mut unit_src = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    for i in 0..n {
        let units = p.units[i] as f32;
        if units <= 0.0 {
            continue;
        }
        let occ = p.occupied[i] as f32;
        let k = match p.land_use[i] {
            // (Dwellings too run-down to let are not part of the market.)
            LandUse::Residential if p.condition[i] <= crate::housing::LETTABLE_CONDITION => continue,
            LandUse::Residential => 0,
            LandUse::Commercial => 1,
            LandUse::Industrial => 2,
            _ => continue,
        };
        occ_src[k][i] = occ;
        unit_src[k][i] = units;
        if k == 0 {
            // Observations weighted by occupied units: rents that actually clear.
            d_res[i] = p.asking_rent[i] - loc_res[i];
            wt_res[i] = occ.max(0.25 * units);
        } else if p.firm[i] != NONE {
            // Business rents that tenants actually pay.
            biz_rent[k - 1][i] = p.asking_rent[i];
            biz_wt[k - 1][i] = occ.max(1.0);
        }
    }
    // Developers and assessors cap business rents at what tenants nearby
    // actually pay (see `econ::market_rent`).
    let mut biz_obs = [vec![0f32; n], vec![0f32; n]];
    let mut biz_evid = [vec![0f32; n], vec![0f32; n]];
    for k in 0..2 {
        biz_obs[k] = smoothed_mean(&biz_rent[k], &biz_wt[k], dims, 4.0, 0.0);
        let thin = exp_blur(&biz_wt[k], dims, 4.0);
        for i in 0..n {
            biz_evid[k][i] = thin[i] / (thin[i] + 4.0);
        }
    }
    let global = |d: &[f32], wt: &[f32], prev: f32| {
        let sw: f32 = wt.iter().sum();
        if sw > 1.0 {
            d.iter().zip(wt).map(|(a, b)| a * b).sum::<f32>() / sw
        } else {
            prev
        }
    };
    let f = &mut w.f;
    f.level_res = global(&d_res, &wt_res, f.level_res);
    let lr = 6.0;
    let lvl_res = smoothed_mean(&d_res, &wt_res, dims, lr, f.level_res);
    // Blend local level with the global one where observations are thin.
    let thin = exp_blur(&wt_res, dims, lr);
    let lo = 4.0;
    for i in 0..n {
        let a = (thin[i] / (thin[i] + lo)).min(1.0);
        let level = a * lvl_res[i] + (1.0 - a) * f.level_res;
        f.rent_res[i] = (level + loc_res[i]).max(50.0);
    }
    let mut pipe = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    for i in 0..n {
        if p.land_use[i] == LandUse::Construction {
            let u = p.target_use[i];
            let k = match u {
                LandUse::Residential => 0,
                LandUse::Commercial => 1,
                LandUse::Industrial => 2,
                _ => continue,
            };
            pipe[k][i] = crate::econ::units_for(w, u, p.floors[i]) as f32;
        }
    }
    let [p0, p1, p2] = pipe;
    let f = &mut w.f;
    f.pipeline_res = exp_blur(&p0, dims, 3.0);
    f.pipeline_com = exp_blur(&p1, dims, 3.0);
    f.pipeline_ind = exp_blur(&p2, dims, 3.0);
    let lo_occ = 3.0;
    for k in 0..3 {
        let u = exp_blur(&unit_src[k], dims, lo_occ);
        let o = exp_blur(&occ_src[k], dims, lo_occ);
        let (occ_out, units_out) = match k {
            0 => (&mut f.occ_res, &mut f.units_res),
            1 => (&mut f.occ_com, &mut f.units_com),
            _ => (&mut f.occ_ind, &mut f.units_ind),
        };
        for i in 0..n {
            occ_out[i] = if u[i] > 0.5 { (o[i] / u[i]).clamp(0.0, 1.0) } else { 0.9 };
            units_out[i] = u[i];
        }
    }

    f.jobs = jobs;
    f.commute_eff = commute_eff;
    f.pop = pop;
    f.spend = spend;
    f.retail_draw = retail_draw;
    f.retail_cap = retail_cap;
    f.retail_supply = s_field;
    f.pollution = pollution;
    f.industry_access = industry_access;
    f.amenity = amenity;
    f.service = service;
    f.loc_res = loc_res;
    let [obs_com, obs_ind] = biz_obs;
    let [evid_com, evid_ind] = biz_evid;
    f.obs_com = obs_com;
    f.obs_ind = obs_ind;
    f.evid_com = evid_com;
    f.evid_ind = evid_ind;
    // Market rents for business space at a typical premises size.
    // (Only where business could locate: serviced land inside the city.)
    for i in 0..n {
        let (rc, ri) = if w.p.in_city[i] || w.p.road_access[i] {
            (
                crate::econ::market_rent(w, i, LandUse::Commercial, 18),
                crate::econ::market_rent(w, i, LandUse::Industrial, 28),
            )
        } else {
            (0.0, 0.0)
        };
        w.f.rent_com[i] = rc;
        w.f.rent_ind[i] = ri;
    }
}

/// Money value per month of living at `i`, excluding commute (which is
/// person-specific once employed).
pub fn residential_quality(w: &World, i: usize) -> f32 {
    w.f.amenity[i] + w.index * (SERVICE_VALUE * w.f.service[i] - POLLUTION_COST * w.f.pollution[i])
}

pub fn commute_cost(w: &World, home: usize, job_parcel: usize) -> f32 {
    w.econ.commute_cost_per_cell * w.dims.manhattan(home, job_parcel)
}
