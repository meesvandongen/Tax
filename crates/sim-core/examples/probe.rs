//! Debug probe: prints per-household utility components for early months.
use sim_core::migration::{utility_here, utility_outside};
use sim_core::types::NONE;
use sim_core::*;

fn main() {
    let mut sim = Simulation::new(SimConfig::default());
    for month in 0..6 {
        sim.step();
        let w = &sim.world;
        let mut unhappy = 0;
        let mut n = 0;
        let mut sums = [0f64; 8];
        for h in w.hh.iter().filter(|h| h.alive) {
            n += 1;
            let here = utility_here(w, h);
            let out = utility_outside(w, h);
            if out > here { unhappy += 1; }
            sums[0] += h.income_labor as f64;
            sums[1] += h.income_capital as f64;
            sums[2] += h.income_transfer as f64;
            sums[3] += h.taxes_paid as f64;
            sums[4] += h.rent as f64;
            sums[5] += here as f64;
            sums[6] += out as f64;
            sums[7] += if h.job == NONE {0.0} else {h.effort_cost as f64};
        }
        let a = |k: usize| sums[k] / n as f64;
        println!("m{month} n={n} unhappy={unhappy} labor={:.0} cap={:.0} transfer={:.0} taxes={:.0} rent={:.0} effort={:.0} here={:.0} out={:.0}",
            a(0), a(1), a(2), a(3), a(4), a(7), a(5), a(6));
    }
}
