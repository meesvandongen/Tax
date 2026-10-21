//! `taxsim` — headless runner for the city simulation.
//!
//! ```text
//! taxsim presets
//! taxsim run     [--preset NAME | --scenario FILE] [--years N] [--seed S] [--size N] [--csv FILE] [--profile]
//! taxsim compare [--presets a,b,c] [--years N] [--seeds K] [--size N] [--out DIR]
//! ```
//! `compare` runs every (preset, seed) pair in parallel on all cores and
//! prints end-of-run indicators averaged over seeds (paired by seed).

use sim_core::presets::{preset, PRESETS};
use sim_core::{Metrics, SimConfig, Simulation};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::thread;

fn args_map(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(k) = args[i].strip_prefix("--") {
            let v = args.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
            if v.is_some() {
                i += 1;
            }
            m.insert(k.to_string(), v.unwrap_or_else(|| "true".into()));
        }
        i += 1;
    }
    m
}

fn config_from(a: &HashMap<String, String>) -> SimConfig {
    let mut cfg = match a.get("scenario") {
        Some(path) => {
            let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {path}: {e}"))
        }
        None => SimConfig::default(),
    };
    if let Some(p) = a.get("preset") {
        cfg.policy = preset(p).unwrap_or_else(|| panic!("unknown preset {p}"));
    }
    if let Some(s) = a.get("seed") {
        cfg.seed = s.parse().expect("seed");
    }
    if let Some(s) = a.get("size") {
        let n: usize = s.parse().expect("size");
        cfg.width = n;
        cfg.height = n;
    }
    cfg
}

const CSV_COLUMNS: &[&str] = &[
    "year",
    "households",
    "unhoused",
    "unemployment",
    "mean_wage",
    "median_income",
    "gdp",
    "gini_income",
    "gini_wealth",
    "top10_wealth_share",
    "median_rent",
    "rent_burden",
    "housing_vacancy",
    "land_price_index",
    "avg_floors",
    "urban_cells",
    "idle_urban_lots",
    "speculative_holds",
    "derelict",
    "green_cover",
    "pollution_exposure",
    "avg_commute",
    "revenue",
    "gov_cash",
    "absentee_land_share",
    "emigrants",
    "wealthy_emigrants",
    "capital_flight",
    "deterred_workers",
    "blocked_sales",
    "firms",
];

fn csv_row(m: &Metrics) -> String {
    let v = serde_json::to_value(m).unwrap();
    CSV_COLUMNS
        .iter()
        .map(|c| match &v[*c] {
            serde_json::Value::Number(n) => format!("{}", n.as_f64().unwrap_or(0.0)),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn summary_line(m: &Metrics) -> String {
    format!(
        "y{:>5.1} hh {:>6} unh {:>4} unemp {:>5.1}% wage {:>6.0} rent {:>5.0} burden {:>4.0}% vac {:>4.1}% \
         floors {:>4.1} urban {:>5} idle {:>4} gini(i/w) {:.2}/{:.2} green {:>4.1}% rev {:>8.0} gov {:>10.0} firms {:>4}",
        m.year,
        m.households,
        m.unhoused,
        m.unemployment * 100.0,
        m.mean_wage,
        m.median_rent,
        m.rent_burden * 100.0,
        m.housing_vacancy * 100.0,
        m.avg_floors,
        m.urban_cells,
        m.idle_urban_lots,
        m.gini_income,
        m.gini_wealth,
        m.green_cover * 100.0,
        m.revenue,
        m.gov_cash,
        m.firms
    )
}

fn cmd_run(a: &HashMap<String, String>) {
    let cfg = config_from(a);
    let years: u32 = a.get("years").map(|s| s.parse().unwrap()).unwrap_or(30);
    let mut sim = Simulation::new(cfg);
    if a.contains_key("profile") {
        sim.enable_profiling();
    }
    let mut csv = a.get("csv").map(|p| {
        let mut f = fs::File::create(p).expect("csv");
        writeln!(f, "{}", CSV_COLUMNS.join(",")).unwrap();
        f
    });
    let t0 = std::time::Instant::now();
    for month in 0..years * 12 {
        let m = sim.step().clone();
        if let Some(f) = csv.as_mut() {
            writeln!(f, "{}", csv_row(&m)).unwrap();
        }
        if month % 12 == 11 {
            println!("{}", summary_line(&m));
        }
    }
    let dt = t0.elapsed().as_secs_f64();
    eprintln!("{} months in {:.2}s ({:.1} ms/month)", years * 12, dt, dt * 1000.0 / (years * 12) as f64);
    if a.contains_key("profile") {
        let mut rep = sim.profile_report();
        rep.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (name, ms) in rep {
            eprintln!("  {:<18} {:>9.1} ms", name, ms);
        }
    }
    if a.contains_key("json") {
        println!("{}", serde_json::to_string_pretty(sim.world.history.last().unwrap()).unwrap());
    }
}

/// Indicators reported by `compare`, with display scaling.
const COMPARE: &[(&str, f64, &str)] = &[
    ("households", 1.0, "households"),
    ("unhoused", 1.0, "unhoused"),
    ("unemployment", 100.0, "unemployment %"),
    ("mean_wage", 1.0, "mean wage"),
    ("median_income", 1.0, "median income"),
    ("gdp", 1e-6, "GDP (M/month)"),
    ("median_rent", 1.0, "median rent"),
    ("rent_burden", 100.0, "rent burden %"),
    ("housing_vacancy", 100.0, "housing vacancy %"),
    ("land_price_index", 1.0, "land price index"),
    ("avg_floors", 1.0, "avg floors"),
    ("urban_cells", 1.0, "urban footprint"),
    ("idle_urban_lots", 1.0, "idle urban lots"),
    ("speculative_holds", 1.0, "speculative holds"),
    ("derelict", 1.0, "derelict buildings"),
    ("gini_income", 1.0, "Gini income"),
    ("gini_wealth", 1.0, "Gini wealth"),
    ("top10_wealth_share", 100.0, "top 10% wealth %"),
    ("absentee_land_share", 100.0, "absentee land %"),
    ("green_cover", 100.0, "green cover %"),
    ("pollution_exposure", 1.0, "pollution exposure"),
    ("avg_commute", 1.0, "avg commute (cells)"),
    ("revenue", 1e-3, "revenue (k/month)"),
    ("gov_cash", 1e-6, "gov cash (M)"),
    ("wealthy_emigrants", 1.0, "wealthy emigrants/mo"),
    ("deterred_workers", 1.0, "deterred workers"),
    ("blocked_sales", 1.0, "blocked sales/mo"),
    ("mean_utility", 1.0, "mean utility"),
];

fn cmd_compare(a: &HashMap<String, String>) {
    let names: Vec<String> = a
        .get("presets")
        .map(|s| s.split(',').map(|x| x.to_string()).collect())
        .unwrap_or_else(|| PRESETS.iter().map(|(n, _)| n.to_string()).collect());
    let years: u32 = a.get("years").map(|s| s.parse().unwrap()).unwrap_or(30);
    let seeds: u64 = a.get("seeds").map(|s| s.parse().unwrap()).unwrap_or(3);
    let base = config_from(a);
    let jobs: Vec<(String, u64)> = names.iter().flat_map(|n| (0..seeds).map(move |s| (n.clone(), s + 1))).collect();
    let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    eprintln!("running {} simulations on {} threads…", jobs.len(), threads);
    let results: Vec<(String, u64, Metrics)> = thread::scope(|scope| {
        let chunks: Vec<Vec<(String, u64)>> =
            (0..threads).map(|t| jobs.iter().skip(t).step_by(threads).cloned().collect()).collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let base = base.clone();
                scope.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|(name, seed)| {
                            let mut cfg = base.clone();
                            cfg.policy = preset(&name).expect("preset");
                            cfg.seed = seed;
                            let mut sim = Simulation::new(cfg);
                            sim.run(years * 12);
                            // Average the final year to smooth monthly noise.
                            let tail = &sim.world.history[sim.world.history.len().saturating_sub(12)..];
                            (name, seed, average(tail))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    // table
    print!("{:<22}", "indicator (mean ± se)");
    for n in &names {
        print!("{:>20}", n);
    }
    println!();
    for (key, scale, label) in COMPARE {
        print!("{:<22}", label);
        for n in &names {
            let vals: Vec<f64> = results
                .iter()
                .filter(|(rn, _, _)| rn == n)
                .map(|(_, _, m)| serde_json::to_value(m).unwrap()[*key].as_f64().unwrap_or(0.0) * scale)
                .collect();
            let k = vals.len().max(1) as f64;
            let mean = vals.iter().sum::<f64>() / k;
            let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (k - 1.0).max(1.0);
            let se = (var / k).sqrt();
            print!("{:>11.2} ±{:<7.2}", mean, se);
        }
        println!();
    }
    if let Some(dir) = a.get("out") {
        fs::create_dir_all(dir).unwrap();
        let path = format!("{dir}/compare.json");
        let rows: Vec<_> =
            results.iter().map(|(n, s, m)| serde_json::json!({"preset": n, "seed": s, "metrics": m})).collect();
        fs::write(&path, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
        eprintln!("wrote {path}");
    }
}

/// Field-wise mean of numeric metrics over a window.
fn average(ms: &[Metrics]) -> Metrics {
    let mut acc: HashMap<String, f64> = HashMap::new();
    for m in ms {
        if let serde_json::Value::Object(o) = serde_json::to_value(m).unwrap() {
            for (k, v) in o {
                if let Some(x) = v.as_f64() {
                    *acc.entry(k).or_default() += x / ms.len() as f64;
                }
            }
        }
    }
    let mut base = serde_json::to_value(ms.last().unwrap()).unwrap();
    for (k, v) in acc {
        let target = &mut base[&k];
        *target = if target.is_u64() || target.is_i64() {
            serde_json::Value::from(v.round() as i64)
        } else {
            serde_json::Value::from(v)
        };
    }
    deserialize_metrics(base).unwrap_or_else(|| ms.last().unwrap().clone())
}

fn deserialize_metrics(v: serde_json::Value) -> Option<Metrics> {
    // Metrics is Serialize-only; rebuild through a lenient path.
    let mut m = Metrics::default();
    let get = |k: &str| v[k].as_f64().unwrap_or(0.0);
    macro_rules! set {
        ($($f:ident),*) => { $( m.$f = get(stringify!($f)) as _; )* };
    }
    set!(
        households,
        unhoused,
        unemployment,
        mean_wage,
        median_income,
        gdp,
        median_rent,
        rent_burden,
        housing_vacancy,
        land_price_index,
        avg_floors,
        urban_cells,
        idle_urban_lots,
        speculative_holds,
        derelict,
        gini_income,
        gini_wealth,
        top10_wealth_share,
        absentee_land_share,
        green_cover,
        pollution_exposure,
        avg_commute,
        revenue,
        gov_cash,
        wealthy_emigrants,
        deterred_workers,
        blocked_sales,
        mean_utility,
        year
    );
    Some(m)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    let a = args_map(&args[1.min(args.len())..]);
    match cmd {
        "run" => cmd_run(&a),
        "compare" => cmd_compare(&a),
        "presets" => {
            for (n, d) in PRESETS {
                println!("{n:<14} {d}");
            }
        }
        _ => {
            eprintln!("usage: taxsim <run|compare|presets> [options]   (see crates/sim-cli/src/main.rs)");
        }
    }
}
