#[path = "kernel.rs"]
mod kernel;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(200_000);
    let w: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(512);
    let ticks: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(200);
    let mut s = kernel::Sim::new(n, w, 12345);
    for _ in 0..10 {
        s.step();
    }
    let t0 = Instant::now();
    for _ in 0..ticks {
        s.step();
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
    println!("native n={} w={} ms/tick={:.3} hash={:08x} tax={}", n, w, ms, s.hash(), s.tax_total);
}
