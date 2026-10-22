//! City-wide indicators, computed from agent and parcel state each tick.

use crate::types::*;
use crate::world::World;
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Metrics {
    pub tick: u64,
    pub year: f32,
    // --- people
    pub households: u32,
    pub unhoused: u32,
    pub immigrants: u32,
    pub emigrants: u32,
    pub wealthy_emigrants: u32,
    pub deaths: u32,
    pub mean_utility: f32,
    // --- economy
    pub employed: u32,
    pub unemployment: f32,
    pub mean_wage: f32,
    pub median_income: f32,
    pub gdp: f64,
    pub exports: f64,
    pub firms: u32,
    pub firms_opened: u32,
    pub firms_closed: u32,
    pub retail_sales: f64,
    // --- inequality
    pub gini_income: f32,
    pub gini_wealth: f32,
    pub top10_wealth_share: f32,
    pub top1_wealth_share: f32,
    pub poverty_rate: f32,
    pub absentee_land_share: f32,
    pub resident_owner_share: f32,
    /// Households owning the home they live in (a flat or a whole building).
    pub owner_occupier_share: f32,
    pub flat_purchases: u32,
    pub flat_sales: u32,
    // --- housing and land
    pub median_rent: f32,
    pub rent_burden: f32,
    pub housing_units: u32,
    pub housing_vacancy: f32,
    pub land_value_total: f64,
    pub land_price_index: f32,
    pub avg_floors: f32,
    pub max_floors: u8,
    pub urban_cells: u32,
    pub idle_urban_lots: u32,
    pub speculative_holds: u32,
    pub derelict: u32,
    pub sales: u32,
    pub forced_sales: u32,
    pub foreclosures: u32,
    pub harberger_buyouts: u32,
    pub buildings_started: u32,
    pub buildings_completed: u32,
    pub renovations: u32,
    // --- environment
    pub green_cover: f32,
    pub forest_cells: u32,
    pub pollution_exposure: f32,
    pub emissions: f64,
    pub avg_commute: f32,
    pub co2_index: f32,
    pub parks: u32,
    /// Road parcels, road projects built so far, and road parcels laid
    /// through buildings the government had to buy.
    pub road_cells: u32,
    pub road_projects: u32,
    pub roads_expropriated: u32,
    // --- government
    pub revenue: f64,
    pub revenue_by_tax: Vec<(String, f64)>,
    pub spend_services: f64,
    pub spend_infrastructure: f64,
    pub spend_parks: f64,
    pub spend_dividend: f64,
    pub spend_welfare: f64,
    pub spend_interest: f64,
    pub spend_rebate: f64,
    pub gov_cash: f64,
    pub dividend_per_hh: f64,
    // --- distortions and flows
    pub deterred_workers: u32,
    pub blocked_sales: u32,
    pub floors_built: u32,
    pub floors_no_tax: u32,
    pub deferred_maintenance: u32,
    pub capital_flight: f64,
    pub absentee_income: f64,
    /// Henry George test: annual differential land rent vs public goods.
    pub land_rent_annual: f64,
    pub public_goods_annual: f64,
}

/// Gini coefficient of non-negative values (negatives clamped to 0).
pub fn gini(values: &mut [f64]) -> f32 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    for v in values.iter_mut() {
        *v = v.max(0.0);
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let sum: f64 = values.iter().sum();
    if sum <= 0.0 {
        return 0.0;
    }
    let weighted: f64 = values.iter().enumerate().map(|(i, v)| (i as f64 + 1.0) * v).sum();
    ((2.0 * weighted) / (n as f64 * sum) - (n as f64 + 1.0) / n as f64) as f32
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    v[mid]
}

pub fn compute(w: &World) -> Metrics {
    let mut m = Metrics { tick: w.tick, year: w.year(), ..Default::default() };
    let p = &w.p;

    // ---- households
    let mut incomes = Vec::new();
    let mut wealths = Vec::new();
    let mut burdens = Vec::new();
    let mut owner_occupiers = 0u32;
    let mut util = 0.0f64;
    let mut wage_sum = 0.0f64;
    let mut commute_sum = 0.0f64;
    let mut commuters = 0u32;
    let mut exposure = 0.0f64;
    for (id, h) in w.hh.iter().enumerate().filter(|(_, h)| h.alive) {
        m.households += 1;
        if h.home == NONE {
            m.unhoused += 1;
        } else {
            exposure += w.f.pollution[h.home as usize] as f64;
        }
        if h.job != NONE {
            m.employed += 1;
            wage_sum += h.wage as f64;
            if h.home != NONE {
                commute_sum += w.mesh.route_dist(h.home as usize, w.firms[h.job as usize].parcel as usize) as f64;
                commuters += 1;
            }
        }
        let inc = h.income_labor + h.income_capital + h.income_transfer;
        incomes.push(inc);
        wealths.push(h.wealth);
        if h.home != NONE && (h.owns_home || p.owner[h.home as usize] == Owner::Hh(id as u32)) {
            owner_occupiers += 1;
        } else if h.home != NONE {
            burdens.push(h.rent / inc.max(1.0));
        }
        util += h.utility as f64;
    }
    let nh = m.households.max(1) as f32;
    m.unemployment = 1.0 - m.employed as f32 / nh;
    m.mean_wage = if m.employed > 0 { (wage_sum / m.employed as f64) as f32 } else { 0.0 };
    m.mean_utility = (util / nh as f64) as f32;
    m.owner_occupier_share = owner_occupiers as f32 / nh;
    m.avg_commute = if commuters > 0 { (commute_sum / commuters as f64) as f32 } else { 0.0 };
    m.pollution_exposure = (exposure / nh as f64) as f32;
    let med_income = median(&mut incomes.clone());
    m.median_income = med_income;
    m.poverty_rate = incomes.iter().filter(|&&x| x < 0.5 * med_income).count() as f32 / nh;
    m.rent_burden = median(&mut burdens);
    let mut inc64: Vec<f64> = incomes.iter().map(|&x| x as f64).collect();
    m.gini_income = gini(&mut inc64);
    let mut w64 = wealths.clone();
    m.gini_wealth = gini(&mut w64);
    // w64 is now sorted ascending
    let total_w: f64 = w64.iter().sum::<f64>().max(1.0);
    let top = |frac: f64| -> f32 {
        let k = ((w64.len() as f64) * frac).ceil() as usize;
        (w64.iter().rev().take(k).sum::<f64>() / total_w) as f32
    };
    m.top10_wealth_share = top(0.10);
    m.top1_wealth_share = top(0.01);

    // ---- parcels
    let mut rents = Vec::new();
    let mut floors_w = 0.0f64;
    let mut bldg_count = 0u32;
    let mut land_total = 0.0f64;
    let mut land_absentee = 0.0f64;
    let mut land_resident = 0.0f64;
    let mut nature_sum = 0.0f64;
    let mut land_cells = 0u32;
    let mut vacant_units = 0u32;
    let mut farm_rent_total = 0.0f64;
    let r = w.econ.outside_return as f64;
    for i in 0..p.len() {
        let u = p.land_use[i];
        if u != LandUse::Water {
            land_cells += 1;
            nature_sum += p.nature[i] as f64;
            if p.nature[i] > 0.6 {
                m.forest_cells += 1;
            }
        }
        if u.is_urban() {
            m.urban_cells += 1;
        }
        if u == LandUse::Park {
            m.parks += 1;
        }
        if u == LandUse::Road {
            m.road_cells += 1;
        }
        if u.is_building() {
            bldg_count += 1;
            floors_w += p.floors[i] as f64;
            m.max_floors = m.max_floors.max(p.floors[i]);
            if p.condition[i] < 0.25 {
                m.derelict += 1;
            }
        }
        if u == LandUse::Residential && crate::housing::lettable(w, i) {
            m.housing_units += p.units[i] as u32;
            vacant_units += p.vacant_units(i) as u32;
            if p.occupied[i] > 0 {
                for _ in 0..p.occupied[i].min(8) {
                    rents.push(p.asking_rent[i]);
                }
            }
        }
        if crate::property::taxable(w, i) {
            let lv = p.assessed_land[i] as f64;
            land_total += lv;
            // Owner-occupied flats carry their share of the land.
            let freehold = p.freehold_share(i);
            land_resident += lv * (1.0 - freehold);
            match p.owner[i] {
                Owner::Absentee => land_absentee += lv * freehold,
                Owner::Hh(_) => land_resident += lv * freehold,
                _ => {}
            }
            farm_rent_total += 12.0 * w.econ.farm_rent as f64 * (0.4 + w.terrain.fertility[i] as f64);
        }
        if matches!(u, LandUse::Wild | LandUse::Farm | LandUse::Vacant) && p.road_access[i] {
            let urban_nb = w.mesh.near(i, 1.5).iter().filter(|&&j| p.land_use[j as usize].is_building()).count();
            if urban_nb >= 3 {
                m.idle_urban_lots += 1;
            }
        }
        if p.months_held_empty[i] > 12 && !u.is_building() {
            m.speculative_holds += 1;
        }
    }
    m.housing_vacancy = if m.housing_units > 0 { vacant_units as f32 / m.housing_units as f32 } else { 0.0 };
    m.median_rent = median(&mut rents);
    m.avg_floors = if bldg_count > 0 { (floors_w / bldg_count as f64) as f32 } else { 0.0 };
    m.land_value_total = land_total;
    m.land_price_index = if m.urban_cells > 0 { (land_total / m.urban_cells as f64 / 1000.0) as f32 } else { 0.0 };
    m.absentee_land_share = if land_total > 0.0 { (land_absentee / land_total) as f32 } else { 0.0 };
    m.resident_owner_share = if land_total > 0.0 { (land_resident / land_total) as f32 } else { 0.0 };
    m.green_cover = if land_cells > 0 { (nature_sum / land_cells as f64) as f32 } else { 0.0 };
    // Differential land rent: annual rent of land above its agricultural use.
    m.land_rent_annual = (land_total * r - farm_rent_total).max(0.0);

    // ---- flows
    let f = &w.flows;
    m.immigrants = f.immigrants;
    m.emigrants = f.emigrants;
    m.wealthy_emigrants = f.wealthy_emigrants;
    m.deaths = f.deaths;
    m.gdp = f.value_added;
    m.exports = f.exports;
    m.retail_sales = f.retail_sales;
    m.firms = w.n_alive_firms;
    m.firms_opened = f.firms_opened;
    m.firms_closed = f.firms_closed;
    m.sales = f.sales;
    m.forced_sales = f.forced_sales;
    m.foreclosures = f.foreclosures;
    m.harberger_buyouts = f.harberger_buyouts;
    m.buildings_started = f.buildings_started;
    m.buildings_completed = f.buildings_completed;
    m.renovations = f.renovations;
    m.flat_purchases = f.flat_purchases;
    m.flat_sales = f.flat_sales;
    m.emissions = f.emissions;
    m.co2_index = (f.emissions + commute_sum * 0.05) as f32;
    m.deterred_workers = f.deterred_workers;
    m.blocked_sales = f.blocked_sales;
    m.floors_built = f.floors_built;
    m.floors_no_tax = f.floors_no_tax;
    m.deferred_maintenance = f.deferred_maintenance;
    m.capital_flight = f.capital_flight;
    m.absentee_income = f.absentee_income;

    // ---- government
    let g = &w.gov;
    m.revenue = w.taxes.month.iter().sum();
    m.revenue_by_tax = w.taxes.specs().iter().zip(&w.taxes.month).map(|(s, v)| (s.kind().to_string(), *v)).collect();
    m.spend_services = g.spend_services;
    m.spend_infrastructure = g.spend_infrastructure;
    m.spend_parks = g.spend_parks;
    m.road_projects = g.road_projects;
    m.roads_expropriated = g.roads_expropriated;
    m.spend_dividend = g.spend_dividend;
    m.spend_welfare = g.spend_welfare;
    m.spend_interest = g.spend_interest;
    m.spend_rebate = g.spend_rebate;
    m.gov_cash = g.cash;
    m.dividend_per_hh = g.dividend_per_hh;
    m.public_goods_annual = 12.0 * (g.spend_services + g.spend_infrastructure + g.spend_parks);
    m
}
