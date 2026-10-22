//! Procedural terrain: height, a meandering river, optional coast, soil
//! fertility and initial vegetation, sampled on the parcel mesh.

use crate::grid::Dims;
use crate::mesh::Mesh;
use crate::rng::Rng;

#[derive(Clone, Debug)]
pub struct Terrain {
    /// Metres above sea level.
    pub height: Vec<f32>,
    pub water: Vec<bool>,
    /// Max height difference to edge neighbours (metres).
    pub slope: Vec<f32>,
    pub fertility: Vec<f32>,
    /// Initial vegetation density 0..1.
    pub vegetation: Vec<f32>,
    /// Distance (cells) to nearest water.
    pub water_dist: Vec<f32>,
    /// Height at each mesh corner (for rendering).
    pub vert_height: Vec<f32>,
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
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

/// The river's course: centre line and half-width at each raster row,
/// interpolated in between.
struct River {
    centre: Vec<f32>,
    half: Vec<f32>,
}

impl River {
    fn at(&self, y: f32) -> (f32, f32) {
        let n = self.centre.len();
        let fy = y.clamp(0.0, (n - 1) as f32);
        let (a, b) = (fy as usize, (fy as usize + 1).min(n - 1));
        let t = fy - a as f32;
        (self.centre[a] + (self.centre[b] - self.centre[a]) * t, self.half[a] + (self.half[b] - self.half[a]) * t)
    }
}

/// The landscape as continuous functions of map position, so it can be
/// sampled at parcel centroids and at mesh corners alike.
struct Landscape {
    seed: u32,
    w: f32,
    h: f32,
    river: River,
    /// Coast on the east (true) or west side, if any.
    coast: Option<bool>,
}

impl Landscape {
    fn new(dims: Dims, rng: &mut Rng) -> Landscape {
        let seed = rng.next_u64() as u32;
        let (w, h) = (dims.w as f32, dims.h as f32);
        // River: a meandering path from top to bottom.
        let mut rx = w * rng.range_f64(0.35, 0.6) as f32;
        let phase = rng.range_f64(0.0, std::f64::consts::TAU) as f32;
        let amp = rng.range_f64(6.0, 14.0) as f32;
        let river_seed = seed ^ 0xA5A5;
        let mut centre = Vec::with_capacity(dims.h + 1);
        let mut half = Vec::with_capacity(dims.h + 1);
        for y in 0..=dims.h {
            let yf = y as f32;
            let meander = (yf / h * std::f32::consts::TAU * 1.3 + phase).sin() * amp
                + (fbm(yf * 0.05, 3.3, 3, river_seed) - 0.5) * 18.0;
            centre.push(rx + meander);
            half.push(1.4 + fbm(yf * 0.08, 9.1, 2, river_seed) * 1.6);
            rx += (rng.f32() - 0.5) * 0.3;
        }
        // Optional coast on one side.
        let coast = if rng.chance(0.5) { Some(rng.chance(0.5)) } else { None };
        Landscape { seed, w, h, river: River { centre, half }, coast }
    }

    /// (height in metres, water)
    fn at(&self, x: f32, y: f32) -> (f32, bool) {
        let (w, h, seed) = (self.w, self.h, self.seed);
        let scale = 1.0 / 28.0;
        let base = fbm(x * scale, y * scale, 5, seed);
        // gentle hills, higher towards the edges
        let edge = ((x / w - 0.5).abs().max((y / h - 0.5).abs()) * 2.0).min(1.0).powf(2.0);
        let mut height = 4.0 + base * 34.0 + edge * 22.0;
        let (cx, half) = self.river.at(y);
        let d = (x - cx).abs();
        let mut water = d <= half;
        // carve a valley
        let valley = (1.0 - (d / 22.0).min(1.0)).powf(1.5);
        height = height * (1.0 - 0.75 * valley) + 1.5 * valley;
        if let Some(side_east) = self.coast {
            let t = if side_east { x / w } else { 1.0 - x / w };
            let wobble = (fbm(y * 0.06, 1.7, 3, seed ^ 0x51) - 0.5) * 0.12;
            if t + wobble > 0.9 {
                water = true;
                height = height.min(1.0);
            } else if t + wobble > 0.82 {
                height = height.min(2.0 + (0.9 - t - wobble) * 60.0);
            }
        }
        if water {
            height = height.min(0.5);
        }
        (height, water)
    }
}

impl Terrain {
    pub fn generate(mesh: &Mesh, rng: &mut Rng) -> Terrain {
        let n = mesh.len();
        let land = Landscape::new(mesh.raster, rng);
        let seed = land.seed;
        let mut height = vec![0f32; n];
        let mut water = vec![false; n];
        for i in 0..n {
            let [x, y] = mesh.centroid[i];
            (height[i], water[i]) = land.at(x, y);
        }
        let vert_height = mesh.verts.iter().map(|v| land.at(v[0], v[1]).0).collect();

        let mut slope = vec![0f32; n];
        for i in 0..n {
            slope[i] = mesh.neighbors(i).map(|j| (height[i] - height[j]).abs()).fold(0.0, f32::max);
        }

        let sources: Vec<usize> = (0..n).filter(|&i| water[i]).collect();
        let water_dist = mesh.distances(&sources, f32::INFINITY, |_| true);
        let water_dist: Vec<f32> = water_dist.into_iter().map(|d| d.min(1e6)).collect();

        let mut fertility = vec![0f32; n];
        let mut vegetation = vec![0f32; n];
        for i in 0..n {
            let [x, y] = mesh.centroid[i];
            let near_water = (1.0 - water_dist[i] / 25.0).max(0.0);
            let flat = (1.0 - slope[i] / 3.0).max(0.0);
            fertility[i] = (0.3 + 0.5 * near_water * flat + 0.2 * fbm(x * 0.07, y * 0.07, 3, seed ^ 0x77)).min(1.0);
            let forest = fbm(x * 0.045, y * 0.045, 4, seed ^ 0xF0F0);
            vegetation[i] = if water[i] { 0.0 } else { ((forest - 0.38) * 2.2 + height[i] / 120.0).clamp(0.12, 1.0) };
        }

        Terrain { height, water, slope, fertility, vegetation, water_dist, vert_height }
    }
}
