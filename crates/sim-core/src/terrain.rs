//! Procedural terrain: height, a meandering river, optional coast, soil
//! fertility and initial vegetation.

use crate::grid::Dims;
use crate::rng::Rng;

#[derive(Clone, Debug)]
pub struct Terrain {
    /// Metres above sea level.
    pub height: Vec<f32>,
    pub water: Vec<bool>,
    /// Max height difference to 4-neighbours (metres).
    pub slope: Vec<f32>,
    pub fertility: Vec<f32>,
    /// Initial vegetation density 0..1.
    pub vegetation: Vec<f32>,
    /// Distance (cells, Manhattan-ish) to nearest water.
    pub water_dist: Vec<f32>,
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let tx = smooth(x - xi as f32);
    let ty = smooth(y - yi as f32);
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    let ab = a + (b - a) * tx;
    let cd = c + (d - c) * tx;
    ab + (cd - ab) * ty
}

/// Fractal Brownian motion in [0, 1).
pub fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += amp * value_noise(x * freq, y * freq, seed.wrapping_add(o * 7919));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

impl Terrain {
    pub fn generate(dims: Dims, rng: &mut Rng) -> Terrain {
        let n = dims.len();
        let seed = rng.next_u64() as u32;
        let (w, h) = (dims.w as f32, dims.h as f32);
        let scale = 1.0 / 28.0;
        let mut height = vec![0f32; n];
        for i in 0..n {
            let (x, y) = dims.xy(i);
            let (xf, yf) = (x as f32, y as f32);
            let base = fbm(xf * scale, yf * scale, 5, seed);
            // gentle hills, higher towards the edges
            let edge = ((xf / w - 0.5).abs().max((yf / h - 0.5).abs()) * 2.0).powf(2.0);
            height[i] = 4.0 + base * 34.0 + edge * 22.0;
        }

        // River: a meandering path from top to bottom.
        let mut water = vec![false; n];
        let mut rx = w * rng.range_f64(0.35, 0.6) as f32;
        let phase = rng.range_f64(0.0, 6.28) as f32;
        let amp = rng.range_f64(6.0, 14.0) as f32;
        let river_seed = seed ^ 0xA5A5;
        for y in 0..dims.h {
            let yf = y as f32;
            let meander = (yf / h * 6.28 * 1.3 + phase).sin() * amp
                + (fbm(yf * 0.05, 3.3, 3, river_seed) - 0.5) * 18.0;
            let cx = rx + meander;
            let half = 1.4 + fbm(yf * 0.08, 9.1, 2, river_seed) * 1.6;
            for x in 0..dims.w {
                let d = (x as f32 - cx).abs();
                let i = dims.idx(x, y);
                if d <= half {
                    water[i] = true;
                }
                // carve a valley
                let valley = (1.0 - (d / 22.0).min(1.0)).powf(1.5);
                height[i] = height[i] * (1.0 - 0.75 * valley) + 1.5 * valley;
            }
            rx += (rng.f32() - 0.5) * 0.3;
        }

        // Optional coast on one side.
        if rng.chance(0.5) {
            let side_east = rng.chance(0.5);
            for i in 0..n {
                let (x, y) = dims.xy(i);
                let t = if side_east { x as f32 / w } else { 1.0 - x as f32 / w };
                let wobble = (fbm(y as f32 * 0.06, 1.7, 3, seed ^ 0x51) - 0.5) * 0.12;
                if t + wobble > 0.9 {
                    water[i] = true;
                    height[i] = height[i].min(1.0);
                } else if t + wobble > 0.82 {
                    height[i] = height[i].min(2.0 + (0.9 - t - wobble) * 60.0);
                }
            }
        }
        for i in 0..n {
            if water[i] {
                height[i] = height[i].min(0.5);
            }
        }

        let mut slope = vec![0f32; n];
        for i in 0..n {
            let mut m = 0f32;
            for j in dims.neighbors4(i) {
                m = m.max((height[i] - height[j]).abs());
            }
            slope[i] = m;
        }

        // Distance to water via two-pass chamfer.
        let mut water_dist = vec![1e6f32; n];
        for i in 0..n {
            if water[i] {
                water_dist[i] = 0.0;
            }
        }
        for _ in 0..2 {
            for i in 0..n {
                for j in dims.neighbors4(i) {
                    water_dist[i] = water_dist[i].min(water_dist[j] + 1.0);
                }
            }
            for i in (0..n).rev() {
                for j in dims.neighbors4(i) {
                    water_dist[i] = water_dist[i].min(water_dist[j] + 1.0);
                }
            }
        }

        let mut fertility = vec![0f32; n];
        let mut vegetation = vec![0f32; n];
        for i in 0..n {
            let (x, y) = dims.xy(i);
            let near_water = (1.0 - water_dist[i] / 25.0).max(0.0);
            let flat = (1.0 - slope[i] / 3.0).max(0.0);
            fertility[i] = (0.3 + 0.5 * near_water * flat + 0.2 * fbm(x as f32 * 0.07, y as f32 * 0.07, 3, seed ^ 0x77)).min(1.0);
            let forest = fbm(x as f32 * 0.045, y as f32 * 0.045, 4, seed ^ 0xF0F0);
            vegetation[i] = if water[i] {
                0.0
            } else {
                ((forest - 0.38) * 2.2 + height[i] / 120.0).clamp(0.12, 1.0)
            };
        }

        Terrain { height, water, slope, fertility, vegetation, water_dist }
    }
}
