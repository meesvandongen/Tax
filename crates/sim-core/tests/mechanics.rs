//! Mechanism tests: properties the model must have *by construction*, so
//! that differences between tax regimes come from behaviour rather than
//! accidents of implementation. (See docs/research/economics.md §7.)

use sim_core::econ::{best_development, Investor};
use sim_core::roads;
use sim_core::tax::{TaxSpec, TaxSystem};
use sim_core::types::{LandUse, Owner};
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
    let mut sim = grown_city(120);
    // (Check right after production: workers who leave later in the month
    // would make last month's revenue look like more per worker.)
    sim_core::production::run(&mut sim.world);
    let w = &sim.world;
    for f in w.firms.iter().filter(|f| f.alive && f.sector == sim_core::types::Sector::Retail && f.workers > 0) {
        let cap = w.econ.retail_output_per_worker * f.productivity * f.workers as f32;
        assert!(f.revenue <= cap * 1.001, "revenue {} above capacity {cap}", f.revenue);
    }
}

/// The government buys the land for public works at the owner's valuation
/// (or the assessment, if higher) plus the purchase premium.
#[test]
fn public_works_buy_land_at_a_premium() {
    let mut sim = grown_city(48);
    let w = &mut sim.world;
    let premium = 1.0 + w.cfg.policy.spending.purchase_premium as f64;
    assert!(premium > 1.0);
    // (A resident-owned home; if the grown city has none, hand one over.)
    let i = match (0..w.p.len())
        .find(|&i| matches!(w.p.owner[i], Owner::Hh(_)) && w.p.land_use[i] == LandUse::Residential)
    {
        Some(i) => i,
        None => {
            let i = (0..w.p.len()).find(|&i| w.p.land_use[i] == LandUse::Residential).expect("a home");
            let id = (0..w.hh.len()).find(|&id| w.hh[id].alive).expect("a household");
            w.p.owner[i] = Owner::Hh(id as u32);
            i
        }
    };
    let Owner::Hh(id) = w.p.owner[i] else { unreachable!() };
    let value = sim_core::econ::valuation(w, i, &w.household_investor(id)).max(w.parcel_value(i));
    let (cash, debt) = (w.hh[id as usize].cash, w.p.mortgage[i] as f64);
    let paid = sim_core::land::compulsory_purchase(w, i);
    assert!((paid - premium * value).abs() < 1.0, "paid {paid:.0}, owner values it at {value:.0}");
    assert!(
        (w.hh[id as usize].cash - cash - (paid - debt)).abs() < 1.0,
        "the owner receives the price net of the mortgage"
    );
    assert_eq!(w.p.owner[i], Owner::Gov);
}

/// Roads come in projects that pay for themselves: the planner builds only
/// when serviced land is short, and only projects whose unlocked land value
/// exceeds their cost.
#[test]
fn roads_are_built_when_land_is_short_and_they_pay() {
    let mut sim = grown_city(60);
    let w = &mut sim.world;
    w.tick -= w.tick % 3; // a planning month
    w.gov.infra_budget = 1e9;
    // Plenty of serviced land: nothing to do.
    w.gov.starts_ema = -10.0;
    assert!(roads::land_shortfall(w) <= 0.0);
    let before = w.gov.roads_built;
    roads::plan(w, 1e6);
    assert_eq!(w.gov.roads_built, before, "built roads although serviced land was plentiful");
    // Land is short: the best projects that pay are built.
    w.gov.starts_ema = 100.0;
    let best = roads::candidates(w, 1e12);
    assert!(best.iter().any(|p| p.ratio() >= roads::MIN_RETURN), "no project pays in a growing city");
    assert!(best.windows(2).all(|p| p[0].ratio() >= p[1].ratio()));
    let projects = w.gov.road_projects;
    roads::plan(w, 1e6);
    assert!(w.gov.road_projects > projects);
    let first = &best[0].path;
    assert!(first.iter().all(|&i| w.p.land_use[i] == LandUse::Road), "the best project was not built");
    assert_eq!(roads::disconnected(w), 0);
}

/// A road may have to go through a building: the government buys it at its
/// valuation plus the premium, compensates the tenants and demolishes it.
#[test]
fn roads_through_buildings_pay_full_compensation() {
    let mut sim = grown_city(60);
    let w = &mut sim.world;
    // Wall the whole network in with houses.
    let frontage: Vec<usize> = (0..w.p.len())
        .filter(|&i| matches!(w.p.land_use[i], LandUse::Wild | LandUse::Farm | LandUse::Vacant))
        .filter(|&i| w.mesh.neighbors(i).any(|j| w.p.land_use[j] == LandUse::Road))
        .collect();
    assert!(!frontage.is_empty());
    for &i in &frontage {
        w.p.land_use[i] = LandUse::Residential;
        w.p.floors[i] = 1;
        w.p.units[i] = sim_core::econ::units_for(w, LandUse::Residential, 1);
        w.p.condition[i] = 1.0;
        // (Foreclosed land is already the city's: it pays itself nothing.)
        if w.p.owner[i] == Owner::Gov {
            w.p.owner[i] = Owner::Absentee;
        }
    }
    sim_core::land::assess(w, true);
    let best = roads::candidates(w, 1e12);
    // (Whether this project would pass the planner's return test depends on
    // the grown city; what is tested here is what building it pays.)
    let p = best.first().expect("no road project");
    let in_way: Vec<usize> = p.path.iter().copied().filter(|&i| w.p.land_use[i].is_building()).collect();
    assert!(!in_way.is_empty(), "a new road must cross the wall");
    for &i in &in_way {
        if w.p.owner[i] == Owner::Gov {
            w.p.owner[i] = Owner::Absentee;
        }
    }
    let premium = 1.0 + w.cfg.policy.spending.purchase_premium as f64;
    let owed: f64 = in_way
        .iter()
        .map(|&i| premium * sim_core::econ::valuation(w, i, &w.investor_for(w.p.owner[i])).max(w.parcel_value(i)))
        .sum();
    let (expropriated, compensation) = (w.gov.roads_expropriated, w.gov.spend_compensation);
    let path = p.path.clone();
    roads::build(w, &path);
    assert_eq!(w.gov.roads_expropriated - expropriated, in_way.len() as u32);
    assert!(w.gov.spend_compensation - compensation >= owed * 0.999);
    // Owners were paid at least the replacement value of their houses.
    assert!(owed > 400_000.0 * in_way.len() as f64);
}

/// The founding streets and every road built since form one network.
#[test]
fn road_network_stays_connected() {
    let sim = grown_city(120);
    let w = &sim.world;
    assert!(w.gov.road_projects > 0);
    assert_eq!(roads::disconnected(w), 0);
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

/// Individual parcels' land values follow their location's fortunes, not a
/// boom/bust cycle: few parcels' assessments rise or fall more than
/// threefold and back within five years once the city has matured.
#[test]
fn parcel_land_values_do_not_cycle() {
    let mut cfg = SimConfig { seed: 7, width: 96, height: 96, ..Default::default() };
    cfg.policy = sim_core::presets::preset("georgist").unwrap();
    let mut sim = Simulation::new(cfg);
    sim.run(30 * 12);
    // Half-yearly snapshots of assessed land, in constant money.
    let mut series: Vec<Vec<f32>> = Vec::new();
    for _ in 0..50 * 2 {
        sim.run(6);
        let ix = sim.world.index;
        series.push(sim.world.p.assessed_land.iter().map(|v| v / ix).collect());
    }
    let w = &sim.world;
    let mut worst = Vec::new();
    for i in 0..w.p.len() {
        if !sim_core::property::taxable(w, i) || !w.p.land_use[i].is_developable() {
            continue;
        }
        let mut swing = 1f32;
        for win in series.windows(10) {
            let v: Vec<f32> = win.iter().map(|s| s[i]).collect();
            let hi = v.iter().cloned().fold(0f32, f32::max);
            let lo = v.iter().cloned().fold(f32::INFINITY, f32::min);
            if hi < 200_000.0 {
                continue;
            }
            // A round trip: a peak above both ends of the window, or a trough below both.
            let (a, b) = (v[0], v[9]);
            swing = swing.max(hi / a.max(b).max(1.0)).max(a.min(b) / lo.max(1.0));
        }
        if swing > 1.0 {
            worst.push(swing);
        }
    }
    assert!(worst.len() > 100, "too few valuable parcels: {}", worst.len());
    worst.sort_by(|a, b| a.total_cmp(b));
    let p90 = worst[worst.len() * 9 / 10];
    assert!(p90 < 3.0, "land values cycle: 90th percentile parcel swings {p90:.1}x and back within 5 years");
}

/// Tenants buy the flats they live in, and a developer who clears their
/// building must buy them out at market value plus the premium.
#[test]
fn flat_owners_are_bought_out_when_their_building_is_cleared() {
    let mut sim = grown_city(120);
    let w = &mut sim.world;
    let owners = w.hh.iter().filter(|h| h.alive && h.owns_home).count();
    assert!(owners > 0, "nobody bought a flat");
    let flats: u32 = w.p.owned_flats.iter().map(|&k| k as u32).sum();
    assert_eq!(flats as usize, owners);
    let i = (0..w.p.len()).find(|&i| w.p.owned_flats[i] > 0).unwrap();
    let ids: Vec<usize> =
        (0..w.hh.len()).filter(|&id| w.hh[id].alive && w.hh[id].owns_home && w.hh[id].home == i as u32).collect();
    let before: Vec<f64> = ids.iter().map(|&id| w.hh[id].cash - w.hh[id].flat_mortgage).collect();
    let buyout = sim_core::econ::flat_buyout(w, i);
    assert!(buyout > sim_core::econ::flat_value(w, i));
    sim_core::development::clear_site(w, i);
    assert_eq!(w.p.owned_flats[i], 0);
    for (k, &id) in ids.iter().enumerate() {
        let h = &w.hh[id];
        assert!(!h.owns_home && h.home == sim_core::types::NONE);
        assert!((h.cash - before[k] - buyout).abs() < 1.0, "owner not paid the buyout");
    }
}

/// A developer must pay owner-occupiers enough that the policy's share of
/// the building accepts: unanimity costs at least every owner's ask, and
/// a threshold the unsold dwellings already meet costs only the buyout.
#[test]
fn collective_sales_pay_what_enough_owners_accept() {
    let mut sim = grown_city(120);
    let w = &mut sim.world;
    let i = (0..w.p.len()).find(|&i| w.p.owned_flats[i] > 0).expect("no owned flats");
    let floor = sim_core::econ::flat_buyout(w, i);
    let moving = w.econ.relocation_months as f64 * w.p.asking_rent[i] as f64;
    let top_ask = (0..w.hh.len())
        .filter(|&id| w.hh[id].alive && w.hh[id].owns_home && w.hh[id].home == i as u32)
        .map(|id| sim_core::econ::owner_occupier_value(w, i, &w.household_investor(id as u32)) + moving)
        .fold(0.0, f64::max);
    w.cfg.policy.collective_sale_threshold = 1.0;
    let unanimous = sim_core::development::collective_sale_price(w, i);
    assert!(unanimous >= floor && unanimous >= top_ask - 1e-6);
    w.cfg.policy.collective_sale_threshold = (w.p.units[i] - w.p.owned_flats[i]) as f32 / w.p.units[i] as f32;
    assert_eq!(sim_core::development::collective_sale_price(w, i), floor);
}

/// Adding floors: at most two at a time, never above the cap, and dearer
/// than building those floors new.
#[test]
fn extensions_are_small_and_cost_a_premium() {
    let sim = grown_city(120);
    let w = &sim.world;
    let inv = w.absentee_investor();
    let mut seen = 0;
    for i in 0..w.p.len() {
        if let Some((add, cost, _)) = sim_core::econ::best_extension(w, i, &inv) {
            let (u, f) = (w.p.land_use[i], w.p.floors[i]);
            assert!(add <= 2 && f + add <= sim_core::econ::EXTENSION_MAX_FLOORS);
            let plain =
                sim_core::econ::construction_cost(w, i, u, f + add) - sim_core::econ::construction_cost(w, i, u, f);
            assert!(cost > plain);
            seen += 1;
        }
    }
    assert!(seen > 0, "no building could add floors");
}
