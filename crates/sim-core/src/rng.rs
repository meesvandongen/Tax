//! Small, fast, deterministic PRNG (xoshiro256++), seeded through SplitMix64.
//!
//! The simulation must produce bit-identical results natively and in
//! WebAssembly for a given seed, so we avoid platform RNGs and keep all
//! randomness flowing through this type.

#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut sm = seed;
        let s = [splitmix64(&mut sm), splitmix64(&mut sm), splitmix64(&mut sm), splitmix64(&mut sm)];
        Rng { s }
    }

    /// Derive an independent stream (e.g. per subsystem) from this one.
    pub fn fork(&mut self, salt: u64) -> Rng {
        Rng::new(self.next_u64() ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03))
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = (self.s[0].wrapping_add(self.s[3])).rotate_left(23).wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1).
    #[inline]
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    #[inline]
    pub fn f32(&mut self) -> f32 {
        self.f64() as f32
    }

    /// Uniform integer in [0, n). Returns 0 when n == 0.
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }

    #[inline]
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    #[inline]
    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    /// Standard normal via Box-Muller.
    pub fn normal(&mut self) -> f64 {
        let u1 = self.f64().max(1e-12);
        let u2 = self.f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    pub fn lognormal(&mut self, mu: f64, sigma: f64) -> f64 {
        (mu + sigma * self.normal()).exp()
    }

    /// Gumbel noise, used for logit (multinomial) choice: argmax(u_i + g_i).
    pub fn gumbel(&mut self) -> f64 {
        -(-(self.f64().max(1e-12)).ln()).ln()
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn unit_interval() {
        let mut r = Rng::new(1);
        for _ in 0..10_000 {
            let x = r.f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn below_in_range() {
        let mut r = Rng::new(7);
        for n in 1..50 {
            for _ in 0..100 {
                assert!(r.below(n) < n);
            }
        }
    }
}
