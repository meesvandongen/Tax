//! Debug: building stock composition after N years.
use sim_core::types::*;
use sim_core::*;

fn main() {
    let years: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let preset = std::env::args().nth(2).unwrap_or("status_quo".into());
    let mut cfg = SimConfig::default();
    cfg.policy = presets::preset(&preset).unwrap();
    let mut sim = Simulation::new(cfg);
    sim.run(years * 12);
    let w = &sim.world;
    for u in [LandUse::Residential, LandUse::Commercial, LandUse::Industrial, LandUse::Civic, LandUse::Construction, LandUse::Road, LandUse::Park, LandUse::Vacant] {
        let cells: Vec<usize> = (0..w.p.len()).filter(|&i| w.p.land_use[i] == u).collect();
        let units: u32 = cells.iter().map(|&i| w.p.units[i] as u32).sum();
        let occ: u32 = cells.iter().map(|&i| w.p.occupied[i] as u32).sum();
        let floors: f32 = cells.iter().map(|&i| w.p.floors[i] as f32).sum::<f32>() / cells.len().max(1) as f32;
        let rent: f32 = cells.iter().map(|&i| w.p.asking_rent[i]).sum::<f32>() / cells.len().max(1) as f32;
        let bv: f64 = cells.iter().map(|&i| w.p.assessed_bldg[i] as f64).sum();
        let lv: f64 = cells.iter().map(|&i| w.p.assessed_land[i] as f64).sum();
        let firms = cells.iter().filter(|&&i| w.p.firm[i] != NONE).count();
        println!("{:?}: cells {} units {} occ {} avg_floors {:.1} avg_ask {:.0} bldg {:.0}M land {:.0}M firms {}", u, cells.len(), units, occ, floors, rent, bv/1e6, lv/1e6, firms);
    }
    let i_max = (0..w.p.len()).max_by(|&a,&b| w.f.rent_com[a].total_cmp(&w.f.rent_com[b])).unwrap();
    println!("max rent_com {:.0} draw {:.0} | level_com {:.0} level_ind {:.0} level_res {:.0}", w.f.rent_com[i_max], w.f.retail_draw[i_max], w.f.level_com, w.f.level_ind, w.f.level_res);
    let m = w.history.last().unwrap();
    println!("hh {} wage {:.0} rent {:.0}", m.households, m.mean_wage, m.median_rent);
    let cons: f64 = w.hh.iter().filter(|h| h.alive).map(|h| h.consumption as f64).sum();
    let pool: f64 = w.retail_pool.iter().map(|&v| v as f64).sum();
    let retail_workers: u32 = w.firms.iter().filter(|f| f.alive && f.sector == Sector::Retail).map(|f| f.workers as u32).sum();
    println!("consumption total {:.0} (avg {:.0}) pool {:.0} retail_sales {:.0} retail_workers {}", cons, cons / m.households as f64, pool, m.retail_sales, retail_workers);
    let imax = (0..w.p.len()).max_by(|&a,&b| w.f.retail_draw[a].total_cmp(&w.f.retail_draw[b])).unwrap();
    println!("max draw {:.0} supply there {:.1} spend field max {:.0}", w.f.retail_draw[imax], w.f.retail_supply[imax], w.f.spend.iter().cloned().fold(0.0, f32::max));
    for f in w.firms.iter().filter(|f| f.alive && f.sector == Sector::Retail) {
        println!("  shop workers {} desired {} revenue {:.0} profit {:.0} wage_rate {:.0} cap {}", f.workers, f.desired_workers, f.revenue, f.profit, f.wage_rate, w.p.units[f.parcel as usize]);
    }
}

#[allow(dead_code)]
fn unused() {}
