//! Mechanism tests: properties the model must have *by construction*, so
//! that differences between tax regimes come from behaviour rather than
//! accidents of implementation. (See docs/research/economics.md §7.)

use sim_core::econ::{best_development, Investor};
use sim_core::tax::{TaxSpec, TaxSystem};
use sim_core::types::LandUse;
use sim_core::world::World;
use sim_core::{SimConfig, Simulation};

fn grown_city(months: u32) -> Simulation {
    let cfg = SimConfig { seed: 7, width: 96, height: 96, ..Default::default() };
    let mut sim = Simulation::new(cfg);
    sim.run(months);
    sim
}

/// Developable, serviced parcels in the grown city.
fn candidate_parcels(w: &World) -> Vec<usize> {
    (0..w.p.len()).filter(|&i| w.p.road_access[i] && w.p.land_use[i].is_developable()).collect()
}

/// Best (use, floors) per parcel, only where that project is viable.
fn options_under(w: &mut World, taxes: &[TaxSpec], parcels: &[usize]) -> Vec<Option<(LandUse, u8)>> {
    w.taxes = TaxSystem::new(taxes);
    let inv = Investor { r: 0.065, g: 0.01, t_inc: 0.0, w: 0.0, no_tax: false };
    parcels
        .iter()
        .map(|&i| best_development(w, i, &inv).filter(|d| d.residual > 0.0).map(|d| (d.land_use, d.floors)))
        .collect()
}

#[test]
fn deterministic_replay() {
    let a = grown_city(36);
    let b = grown_city(36);
    let ja = serde_json::to_string(&a.world.history).unwrap();
    let jb = serde_json::to_string(&b.world.history).unwrap();
    assert_eq!(ja, jb, "same seed and policy must give identical histories");
}

#[test]
fn lump_sum_tax_does_not_change_development_choices() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    let parcels = candidate_parcels(w);
    assert!(!parcels.is_empty());
    let none = options_under(w, &[], &parcels);
    let head = options_under(w, &[TaxSpec::Head { amount: 500.0 }], &parcels);
    assert_eq!(none, head);
}

#[test]
fn land_value_tax_does_not_change_building_height() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    let parcels = candidate_parcels(w);
    let none = options_under(w, &[], &parcels);
    let lvt = options_under(w, &[TaxSpec::LandValue { rate: 0.03 }], &parcels);
    // An LVT can make a project unviable (None), but never changes *which*
    // project is best when one exists.
    let mut compared = 0;
    for (a, b) in none.iter().zip(&lvt) {
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!(a, b);
            compared += 1;
        }
    }
    assert!(compared > 0);
}

#[test]
fn building_tax_never_raises_height() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    let parcels = candidate_parcels(w);
    let none = options_under(w, &[], &parcels);
    let btax = options_under(w, &[TaxSpec::Property { land_rate: 0.0, building_rate: 0.03 }], &parcels);
    let mut lower = 0;
    for (a, b) in none.iter().zip(&btax) {
        if let (Some(a), Some(b)) = (a, b) {
            if a.0 == b.0 {
                assert!(b.1 <= a.1, "building tax raised height {} -> {}", a.1, b.1);
                if b.1 < a.1 {
                    lower += 1;
                }
            }
        }
    }
    let _ = lower;
}

#[test]
fn land_taxes_capitalise_into_lower_land_values() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    let parcels = candidate_parcels(w);
    // Land as the assessor values it: as if vacant, net of its own land tax.
    let value = |w: &mut World, specs: &[TaxSpec]| -> f64 {
        w.taxes = TaxSystem::new(specs);
        let inv = w.absentee_investor();
        parcels
            .iter()
            .map(|&i| sim_core::land::farm_value(w, i, &inv) + sim_core::econ::self_consistent_site_value(w, i, &inv))
            .sum()
    };
    let v0 = value(w, &[]);
    let v1 = value(w, &[TaxSpec::LandValue { rate: 0.05 }]);
    // A 5% tax capitalises at 1 / (1 + 0.05 / (r − g)) with r − g ≈ 6%.
    assert!(v1 < v0 * 0.7, "LVT should lower land values: {v0} -> {v1}");
}

#[test]
fn long_run_stays_finite() {
    let sim = grown_city(240);
    for m in &sim.world.history {
        let j = serde_json::to_value(m).unwrap();
        for (k, v) in j.as_object().unwrap() {
            if let Some(x) = v.as_f64() {
                assert!(x.is_finite(), "{k} not finite at tick {}", m.tick);
            }
        }
    }
    let last = sim.world.history.last().unwrap();
    assert!(last.households > 50, "city collapsed: {} households", last.households);
}

#[test]
fn every_preset_runs() {
    for (name, _) in sim_core::presets::PRESETS {
        let mut cfg = SimConfig { seed: 3, width: 64, height: 64, ..Default::default() };
        cfg.policy = sim_core::presets::preset(name).unwrap();
        let mut sim = Simulation::new(cfg);
        sim.run(60);
        assert!(sim.world.history.last().unwrap().households > 0, "{name}");
    }
}

/// A city grown for decades must pass every plausibility check in
/// `audit::check` (money is not created from nothing, markets clear,
/// bookkeeping is consistent), checked once a year.
#[test]
fn grown_city_passes_audit() {
    let cfg = SimConfig { seed: 7, width: 96, height: 96, ..Default::default() };
    let mut sim = Simulation::new(cfg);
    let mut failures = Vec::new();
    for year in 1..=60 {
        sim.run(12);
        for issue in sim_core::audit::check(&sim.world) {
            failures.push(format!("y{year}: {issue}"));
        }
    }
    assert!(failures.is_empty(), "audit failures:\n{}", failures.join("\n"));
    assert!(sim.world.history.last().unwrap().households > 1000, "the city should grow");
}

/// Land without street access can only be farmed, so it is assessed as
/// farmland: a land tax cannot exceed what its owner could earn on it.
#[test]
fn unserviced_land_is_assessed_as_farmland() {
    let sim = grown_city(120);
    let w = &sim.world;
    let inv = w.absentee_investor();
    let mut checked = 0;
    for i in 0..w.p.len() {
        if w.p.in_city[i] && !w.p.road_access[i] && w.p.land_use[i] == LandUse::Wild {
            let farm = sim_core::land::farm_value(w, i, &inv) as f32;
            assert!(w.p.assessed_land[i] <= 1.35 * farm, "cell {i}: {} vs farm {farm}", w.p.assessed_land[i]);
            checked += 1;
        }
    }
    assert!(checked > 0);
}

/// A shop worker can only serve so many customers: spending beyond that
/// goes elsewhere instead of becoming unbounded revenue per worker.
#[test]
fn retail_sales_per_worker_are_bounded() {
    let sim = grown_city(120);
    let w = &sim.world;
    for f in w.firms.iter().filter(|f| f.alive && f.sector == sim_core::types::Sector::Retail && f.workers > 0) {
        let cap = w.econ.retail_output_per_worker * f.productivity * f.workers as f32;
        assert!(f.revenue <= cap * 1.001, "revenue {} above capacity {cap}", f.revenue);
    }
}

#[test]
fn private_development_keeps_the_street_plan_free() {
    let cfg = SimConfig { seed: 7, width: 96, height: 96, ..Default::default() };
    let mut sim = Simulation::new(cfg);
    let on_plan = |w: &World| -> Vec<usize> {
        (0..w.p.len())
            .filter(|&i| sim_core::government::on_street_plan(w, i))
            .filter(|&i| w.p.land_use[i].is_building() || w.p.land_use[i] == LandUse::Construction)
            .collect()
    };
    let founding = on_plan(&sim.world);
    sim.run(120);
    let w = &sim.world;
    for i in on_plan(w) {
        assert!(
            founding.contains(&i) || w.p.owner[i] == sim_core::types::Owner::Gov,
            "private building on the street plan at {i}"
        );
    }
    assert!(w.gov.roads_built > 0);
}

#[test]
fn roads_expropriate_buildings_in_the_way_at_full_compensation() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    // Put a building on every open end of the street plan.
    let ends: Vec<usize> = (0..w.p.len())
        .filter(|&i| sim_core::government::reserved_for_street(w, i))
        .filter(|&i| w.dims.neighbors4(i).any(|j| w.p.land_use[j] == LandUse::Road))
        .collect();
    assert!(!ends.is_empty());
    for &i in &ends {
        w.p.land_use[i] = LandUse::Residential;
        w.p.floors[i] = 1;
        w.p.units[i] = sim_core::econ::units_for(w, LandUse::Residential, 1);
        w.p.condition[i] = 1.0;
    }
    sim_core::land::assess(w, true);
    // Plenty of money and demand for land.
    w.gov.infra_budget = 1e9;
    w.gov.starts_ema = 1e3;
    let before = w.gov.roads_built;
    sim_core::government::budget(w);
    assert!(w.gov.roads_built > before, "no road built");
    assert!(w.gov.roads_expropriated > 0, "no building expropriated");
    // The owners were paid at least the buildings' replacement value.
    assert!(w.gov.spend_compensation > 400_000.0 * w.gov.roads_expropriated as f64);
}

/// The assessor values land net of the land tax on that same assessment
/// (`L = V(L)`). Valuing it at last quarter's assessment instead made a
/// high assessment produce a low value and vice versa, so under a heavy land
/// tax assessments oscillated every quarter.
#[test]
fn land_assessment_is_consistent_with_its_own_tax() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    w.taxes = TaxSystem::new(&[TaxSpec::LandValue { rate: 0.06 }]);
    let inv = w.absentee_investor();
    let mut checked = 0;
    for i in candidate_parcels(w) {
        let l = sim_core::econ::self_consistent_site_value(w, i, &inv);
        if l < 50_000.0 {
            continue;
        }
        let v = sim_core::econ::vacant_site_value_at_land(w, i, &inv, l);
        assert!((v - l).abs() < 0.05 * l, "cell {i}: assessed {l:.0} but worth {v:.0} at that assessment");
        checked += 1;
    }
    assert!(checked > 10);
}
