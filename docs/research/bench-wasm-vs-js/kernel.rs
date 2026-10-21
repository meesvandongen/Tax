// ABM-like micro-kernel shared by the native binary and the wasm cdylib.
// SoA layout; per-agent xorshift32 RNG; f64 only (+,-,*,/) so results are bit-identical with JS.

pub struct Sim {
    n: usize,
    w: usize,
    wealth: Vec<f64>,
    income: Vec<f64>,
    parcel: Vec<u32>,
    rng: Vec<u32>,
    lv: Vec<f64>,
    lv2: Vec<f64>,
    occ: Vec<i32>,
    pub tax_total: f64,
}

#[inline(always)]
fn xs(s: &mut u32) -> u32 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = x;
    x
}

impl Sim {
    pub fn new(n: usize, w: usize, seed: u32) -> Sim {
        let mut g = seed | 1;
        let cells = w * w;
        let mut s = Sim {
            n,
            w,
            wealth: vec![0.0; n],
            income: vec![0.0; n],
            parcel: vec![0; n],
            rng: vec![0; n],
            lv: vec![0.0; cells],
            lv2: vec![0.0; cells],
            occ: vec![0; cells],
            tax_total: 0.0,
        };
        for c in 0..cells {
            s.lv[c] = (xs(&mut g) as f64) * (1.0 / 4294967296.0);
        }
        for i in 0..n {
            s.rng[i] = xs(&mut g) | 1;
            s.income[i] = 10.0 + (xs(&mut g) as f64) * (90.0 / 4294967296.0);
            s.wealth[i] = 100.0;
            let p = xs(&mut g) % (cells as u32);
            s.parcel[i] = p;
            s.occ[p as usize] += 1;
        }
        s
    }

    pub fn step(&mut self) {
        let w = self.w;
        // 1. land-value diffusion (5-point stencil) + occupancy pressure
        for y in 1..w - 1 {
            let row = y * w;
            for x in 1..w - 1 {
                let i = row + x;
                let nb = self.lv[i - 1] + self.lv[i + 1] + self.lv[i - w] + self.lv[i + w];
                self.lv2[i] = self.lv[i] * 0.6 + nb * 0.1 + (self.occ[i] as f64) * 0.001 - 0.0005;
            }
        }
        std::mem::swap(&mut self.lv, &mut self.lv2);
        // 2. agents: sample 4 candidate parcels, relocate if better, pay taxes
        let cells = (w * w) as u32;
        let wi = w as i32;
        let mut tax = 0.0f64;
        for i in 0..self.n {
            let mut r = self.rng[i];
            let cur = self.parcel[i];
            let cx = (cur % w as u32) as i32;
            let cy = (cur / w as u32) as i32;
            let inc = self.income[i];
            let mut best = cur;
            let mut best_u = self.lv[cur as usize] * inc / (1.0 + self.occ[cur as usize] as f64);
            let cur_u = best_u;
            for _ in 0..4 {
                let c = xs(&mut r) % cells;
                let px = (c % w as u32) as i32;
                let py = (c / w as u32) as i32;
                let d = ((px - cx).abs() + (py - cy).abs()) as f64;
                let u = self.lv[c as usize] * inc / (1.0 + self.occ[c as usize] as f64) - d * 0.01;
                if u > best_u {
                    best_u = u;
                    best = c;
                }
            }
            if best != cur && best_u > cur_u * 1.05 {
                self.occ[cur as usize] -= 1;
                self.occ[best as usize] += 1;
                self.parcel[i] = best;
            }
            let lvp = self.lv[self.parcel[i] as usize];
            let t = inc * 0.2 + lvp * 0.5; // income tax + land value tax
            let rent = lvp * 2.0;
            self.wealth[i] += inc - t - rent;
            tax += t;
            self.rng[i] = r;
            let _ = wi;
        }
        self.tax_total += tax;
    }

    pub fn hash(&self) -> u32 {
        let mut h: u32 = 2166136261;
        let mut mix = |v: u32| {
            h ^= v;
            h = h.wrapping_mul(16777619);
        };
        for &x in &self.wealth {
            let b = x.to_bits();
            mix(b as u32);
            mix((b >> 32) as u32);
        }
        for &x in &self.lv {
            let b = x.to_bits();
            mix(b as u32);
            mix((b >> 32) as u32);
        }
        for &p in &self.parcel {
            mix(p);
        }
        h
    }
}
