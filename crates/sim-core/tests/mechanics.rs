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
    let value = |w: &mut World, specs: &[TaxSpec]| -> f64 {
        w.taxes = TaxSystem::new(specs);
        let inv = w.absentee_investor();
        parcels.iter().map(|&i| sim_core::land::farm_value(w, i, &inv) + sim_core::econ::valuation(w, i, &inv)).sum()
    };
    let v0 = value(w, &[]);
    let v1 = value(w, &[TaxSpec::LandValue { rate: 0.05 }]);
    assert!(v1 < v0 * 0.8, "LVT should lower land values: {v0} -> {v1}");
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
