//! The parcel mesh: an irregular grid of quadrilaterals, built the way
//! Townscaper builds its grid. A triangular lattice is laid over the map,
//! random pairs of neighbouring triangles are merged into quads, every
//! polygon is split into quads (a triangle into three, a quad into four),
//! and the vertices are relaxed until each quad is as square as its
//! neighbours allow. Every parcel has four sides and at most four edge
//! neighbours, but rows of parcels meander and meet at three-, five- and
//! six-way corners, so streets that follow them curve like an old town's.
//!
//! Spatial interactions still use a square raster underneath: a field over
//! parcels is splatted onto the raster, blurred with the separable
//! exponential filter in [`crate::grid`], and sampled back at each parcel's
//! centroid. Parcels average about one raster cell in area, so decay
//! lengths and kernel masses keep their meaning ("cells" ≈ 40 m).

use crate::grid::{exp_blur_in_place, Dims};
use crate::rng::Rng;
use crate::types::NONE;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// Triangle side of the base lattice, sized so that a parcel averages about
/// one raster cell.
const SPACING: f32 = 2.2;
const RELAX_ITERS: usize = 48;
const RELAX_STEP: f32 = 0.6;
/// Radius up to which neighbourhood lists are precomputed.
pub const NEAR_MAX: f32 = 3.5;
/// Travel over a street network is longer than the straight line; for a
/// grid the mean ratio is 4/π.
pub const ROUTE_FACTOR: f32 = 1.27;

pub struct Mesh {
    pub verts: Vec<[f32; 2]>,
    /// Corner vertices, counter-clockwise (positive shoelace area). Edge `k`
    /// runs from corner `k` to corner `k + 1`.
    pub quads: Vec<[u32; 4]>,
    pub centroid: Vec<[f32; 2]>,
    pub area: Vec<f32>,
    /// The parcel across edge `k`, or `NONE` at the map border.
    pub adj: Vec<[u32; 4]>,
    /// The field raster.
    pub raster: Dims,
    /// Raster pixel holding each parcel's centroid.
    pub pixel: Vec<u32>,
    /// Parcel nearest to each raster pixel's centre.
    pub pixel_cell: Vec<u32>,
    near_start: Vec<u32>,
    near: Vec<u32>,
    near_d: Vec<f32>,
}

fn shoelace(p: &[[f32; 2]]) -> f32 {
    let m = p.len();
    (0..m).map(|k| p[k][0] * p[(k + 1) % m][1] - p[(k + 1) % m][0] * p[k][1]).sum::<f32>() * 0.5
}

/// Rotate by `q` quarter turns counter-clockwise.
fn rot(v: [f32; 2], q: usize) -> [f32; 2] {
    match q % 4 {
        0 => v,
        1 => [-v[1], v[0]],
        2 => [-v[0], -v[1]],
        _ => [v[1], -v[0]],
    }
}

fn key(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

#[derive(PartialEq)]
struct Item(f32, u32);
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Mesh {
    pub fn generate(w: usize, h: usize, rng: &mut Rng) -> Mesh {
        let s = SPACING;
        let row_h = s * 3f32.sqrt() / 2.0;
        let margin = s;
        let cols = ((w as f32 + 2.0 * margin) / s).ceil() as usize + 2;
        let rows = ((h as f32 + 2.0 * margin) / row_h).ceil() as usize + 2;

        // Triangular lattice.
        let mut verts: Vec<[f32; 2]> = Vec::with_capacity(cols * rows * 3);
        for j in 0..rows {
            for i in 0..cols {
                let shift = if j % 2 == 1 { s / 2.0 } else { 0.0 };
                verts.push([-margin + i as f32 * s + shift, -margin + j as f32 * row_h]);
            }
        }
        let id = |i: usize, j: usize| (j * cols + i) as u32;
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for j in 0..rows - 1 {
            for i in 0..cols - 1 {
                if j % 2 == 0 {
                    tris.push([id(i, j), id(i + 1, j), id(i, j + 1)]);
                    tris.push([id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
                } else {
                    tris.push([id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
                    tris.push([id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                }
            }
        }
        for t in tris.iter_mut() {
            if shoelace(&[verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]]) < 0.0 {
                t.swap(1, 2);
            }
        }

        // Merge random pairs of neighbouring triangles into quads.
        let mut edge_tris: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (t, tri) in tris.iter().enumerate() {
            for k in 0..3 {
                edge_tris.entry(key(tri[k], tri[(k + 1) % 3])).or_default().push(t as u32);
            }
        }
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        for (t, tri) in tris.iter().enumerate() {
            for k in 0..3 {
                let e = &edge_tris[&key(tri[k], tri[(k + 1) % 3])];
                if e.len() == 2 && e[0] == t as u32 {
                    pairs.push((e[0], e[1]));
                }
            }
        }
        rng.shuffle(&mut pairs);
        let mut merged = vec![false; tris.len()];
        let mut polys: Vec<Vec<u32>> = Vec::new();
        for (a, b) in pairs {
            if merged[a as usize] || merged[b as usize] {
                continue;
            }
            merged[a as usize] = true;
            merged[b as usize] = true;
            let ta = tris[a as usize];
            let tb = tris[b as usize];
            let o = *tb.iter().find(|v| !ta.contains(v)).unwrap();
            let k = (0..3).find(|&k| tb.contains(&ta[k]) && tb.contains(&ta[(k + 1) % 3])).unwrap();
            polys.push(vec![ta[k], o, ta[(k + 1) % 3], ta[(k + 2) % 3]]);
        }
        for (t, tri) in tris.iter().enumerate() {
            if !merged[t] {
                polys.push(tri.to_vec());
            }
        }

        // Subdivide every polygon into quads around its centroid.
        let mut mids: HashMap<(u32, u32), u32> = HashMap::new();
        let mut quads: Vec<[u32; 4]> = Vec::new();
        for poly in &polys {
            let m = poly.len();
            let mut c = [0f32; 2];
            for &v in poly {
                c[0] += verts[v as usize][0] / m as f32;
                c[1] += verts[v as usize][1] / m as f32;
            }
            let ci = verts.len() as u32;
            verts.push(c);
            let e: Vec<u32> = (0..m)
                .map(|k| {
                    let (a, b) = (poly[k], poly[(k + 1) % m]);
                    *mids.entry(key(a, b)).or_insert_with(|| {
                        let (pa, pb) = (verts[a as usize], verts[b as usize]);
                        verts.push([(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0]);
                        (verts.len() - 1) as u32
                    })
                })
                .collect();
            for k in 0..m {
                quads.push([poly[k], e[k], ci, e[(k + m - 1) % m]]);
            }
        }

        // Relax towards squares (vertices on the outer border stay put).
        let mut edge_count: HashMap<(u32, u32), u8> = HashMap::new();
        for q in &quads {
            for k in 0..4 {
                *edge_count.entry(key(q[k], q[(k + 1) % 4])).or_default() += 1;
            }
        }
        let mut pinned = vec![false; verts.len()];
        for q in &quads {
            for k in 0..4 {
                if edge_count[&key(q[k], q[(k + 1) % 4])] == 1 {
                    pinned[q[k] as usize] = true;
                    pinned[q[(k + 1) % 4] as usize] = true;
                }
            }
        }
        let nv = verts.len();
        for _ in 0..RELAX_ITERS {
            let mut acc = vec![[0f32; 2]; nv];
            let mut cnt = vec![0f32; nv];
            for q in &quads {
                let p = q.map(|v| verts[v as usize]);
                let c = [(p[0][0] + p[1][0] + p[2][0] + p[3][0]) / 4.0, (p[0][1] + p[1][1] + p[2][1] + p[3][1]) / 4.0];
                // The square that best fits the quad: each corner's offset
                // turned back by its quarter, averaged.
                let mut a = [0f32; 2];
                for k in 0..4 {
                    let r = rot([p[k][0] - c[0], p[k][1] - c[1]], 4 - k);
                    a[0] += r[0] / 4.0;
                    a[1] += r[1] / 4.0;
                }
                for k in 0..4 {
                    let t = rot(a, k);
                    let v = q[k] as usize;
                    acc[v][0] += c[0] + t[0] - p[k][0];
                    acc[v][1] += c[1] + t[1] - p[k][1];
                    cnt[v] += 1.0;
                }
            }
            for v in 0..nv {
                if !pinned[v] && cnt[v] > 0.0 {
                    verts[v][0] += RELAX_STEP * acc[v][0] / cnt[v];
                    verts[v][1] += RELAX_STEP * acc[v][1] / cnt[v];
                }
            }
        }

        // Keep the quads inside the map, in row-major order of their centroids.
        let centre = |q: &[u32; 4]| {
            let mut c = [0f32; 2];
            for &v in q {
                c[0] += verts[v as usize][0] / 4.0;
                c[1] += verts[v as usize][1] / 4.0;
            }
            c
        };
        let (wf, hf) = (w as f32, h as f32);
        let mut kept: Vec<([f32; 2], [u32; 4])> = quads
            .iter()
            .map(|q| (centre(q), *q))
            .filter(|(c, _)| c[0] >= 0.0 && c[1] >= 0.0 && c[0] < wf && c[1] < hf)
            .collect();
        kept.sort_by(|a, b| {
            (a.0[1].floor()).total_cmp(&b.0[1].floor()).then(a.0[0].total_cmp(&b.0[0])).then(a.1.cmp(&b.1))
        });
        let mut remap = vec![NONE; nv];
        let mut out_verts = Vec::new();
        let mut out_quads = Vec::with_capacity(kept.len());
        for (_, q) in &kept {
            out_quads.push(q.map(|v| {
                if remap[v as usize] == NONE {
                    remap[v as usize] = out_verts.len() as u32;
                    out_verts.push(verts[v as usize]);
                }
                remap[v as usize]
            }));
        }
        Mesh::from_quads(out_verts, out_quads, Dims { w, h })
    }

    /// Derived connectivity and lookup tables for a quad mesh.
    pub fn from_quads(verts: Vec<[f32; 2]>, quads: Vec<[u32; 4]>, raster: Dims) -> Mesh {
        let n = quads.len();
        let mut centroid = Vec::with_capacity(n);
        let mut area = Vec::with_capacity(n);
        for q in &quads {
            let p = q.map(|v| verts[v as usize]);
            centroid
                .push([(p[0][0] + p[1][0] + p[2][0] + p[3][0]) / 4.0, (p[0][1] + p[1][1] + p[2][1] + p[3][1]) / 4.0]);
            area.push(shoelace(&p));
        }
        let mut edges: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        let mut adj = vec![[NONE; 4]; n];
        for (i, q) in quads.iter().enumerate() {
            for k in 0..4 {
                let e = key(q[k], q[(k + 1) % 4]);
                if let Some(&(j, kj)) = edges.get(&e) {
                    adj[i][k] = j;
                    adj[j as usize][kj as usize] = i as u32;
                } else {
                    edges.insert(e, (i as u32, k as u32));
                }
            }
        }

        let Dims { w, h } = raster;
        let pix = |c: [f32; 2]| {
            let x = (c[0].floor().max(0.0) as usize).min(w - 1);
            let y = (c[1].floor().max(0.0) as usize).min(h - 1);
            (x, y)
        };
        let pixel: Vec<u32> = centroid
            .iter()
            .map(|&c| {
                let (x, y) = pix(c);
                (y * w + x) as u32
            })
            .collect();
        // Parcels bucketed by raster pixel (CSR).
        let mut bucket_start = vec![0u32; w * h + 1];
        for &p in &pixel {
            bucket_start[p as usize + 1] += 1;
        }
        for k in 0..w * h {
            bucket_start[k + 1] += bucket_start[k];
        }
        let mut fill = bucket_start.clone();
        let mut bucket = vec![0u32; n];
        for (i, &p) in pixel.iter().enumerate() {
            bucket[fill[p as usize] as usize] = i as u32;
            fill[p as usize] += 1;
        }
        let scan = |x: usize, y: usize, r: usize, f: &mut dyn FnMut(usize)| {
            for yy in y.saturating_sub(r)..=(y + r).min(h - 1) {
                for xx in x.saturating_sub(r)..=(x + r).min(w - 1) {
                    let b = yy * w + xx;
                    for k in bucket_start[b]..bucket_start[b + 1] {
                        f(bucket[k as usize] as usize);
                    }
                }
            }
        };
        let d2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);

        let mut pixel_cell = vec![0u32; w * h];
        for y in 0..h {
            for x in 0..w {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let mut best = (f32::INFINITY, 0usize);
                let mut r = 1;
                while best.0.is_infinite() && r < w.max(h) {
                    scan(x, y, r, &mut |j| {
                        let d = d2(p, centroid[j]);
                        if d < best.0 || (d == best.0 && j < best.1) {
                            best = (d, j);
                        }
                    });
                    r *= 2;
                }
                pixel_cell[y * w + x] = best.1 as u32;
            }
        }

        let mut near_start = Vec::with_capacity(n + 1);
        let mut near = Vec::new();
        let mut near_d = Vec::new();
        let reach = NEAR_MAX.ceil() as usize + 1;
        let mut tmp: Vec<(f32, u32)> = Vec::new();
        near_start.push(0u32);
        for i in 0..n {
            tmp.clear();
            let (x, y) = pix(centroid[i]);
            scan(x, y, reach, &mut |j| {
                let d = d2(centroid[i], centroid[j]).sqrt();
                if j != i && d <= NEAR_MAX {
                    tmp.push((d, j as u32));
                }
            });
            tmp.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            near.extend(tmp.iter().map(|t| t.1));
            near_d.extend(tmp.iter().map(|t| t.0));
            near_start.push(near.len() as u32);
        }

        Mesh { verts, quads, centroid, area, adj, raster, pixel, pixel_cell, near_start, near, near_d }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.quads.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// Edge neighbours.
    pub fn neighbors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        self.adj[i].iter().filter(|&&j| j != NONE).map(|&j| j as usize)
    }

    /// Straight-line distance between parcel centroids (cells).
    #[inline]
    pub fn dist(&self, a: usize, b: usize) -> f32 {
        let (p, q) = (self.centroid[a], self.centroid[b]);
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
    }

    /// Travel distance between parcels over the street network (cells).
    #[inline]
    pub fn route_dist(&self, a: usize, b: usize) -> f32 {
        ROUTE_FACTOR * self.dist(a, b)
    }

    /// Other parcels whose centroids lie within `r` (≤ [`NEAR_MAX`]) of
    /// parcel `i`'s, nearest first.
    pub fn near(&self, i: usize, r: f32) -> &[u32] {
        let (a, b) = (self.near_start[i] as usize, self.near_start[i + 1] as usize);
        let k = self.near_d[a..b].partition_point(|&d| d <= r);
        &self.near[a..a + k]
    }

    /// The parcel under map point `(x, y)` (nearest centroid on the raster).
    pub fn cell_at(&self, x: f32, y: f32) -> usize {
        let Dims { w, h } = self.raster;
        let px = (x.floor().max(0.0) as usize).min(w - 1);
        let py = (y.floor().max(0.0) as usize).min(h - 1);
        self.pixel_cell[py * w + px] as usize
    }

    fn splat(&self, src: &[f32]) -> Vec<f32> {
        let mut r = vec![0f32; self.raster.len()];
        for (i, &v) in src.iter().enumerate() {
            r[self.pixel[i] as usize] += v;
        }
        r
    }

    /// Bilinear sample of a raster at parcel `i`'s centroid.
    fn sample(&self, r: &[f32], i: usize) -> f32 {
        let Dims { w, h } = self.raster;
        let [x, y] = self.centroid[i];
        let fx = (x - 0.5).clamp(0.0, (w - 1) as f32);
        let fy = (y - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let a = r[y0 * w + x0] + (r[y0 * w + x1] - r[y0 * w + x0]) * tx;
        let b = r[y1 * w + x0] + (r[y1 * w + x1] - r[y1 * w + x0]) * tx;
        a + (b - a) * ty
    }

    /// out[i] ≈ Σ_j src[j] · a^(|dx|+|dy|), a = exp(−1/λ), via the raster.
    pub fn blur(&self, src: &[f32], lambda: f32) -> Vec<f32> {
        let mut r = self.splat(src);
        exp_blur_in_place(&mut r, self.raster, lambda);
        (0..self.len()).map(|i| self.sample(&r, i)).collect()
    }

    /// Weighted local mean: blur(values·weights) / blur(weights). Parcels with
    /// negligible weight nearby fall back to `fallback`.
    pub fn smoothed_mean(&self, values: &[f32], weights: &[f32], lambda: f32, fallback: f32) -> Vec<f32> {
        let vw: Vec<f32> = values.iter().zip(weights).map(|(v, w)| v * w).collect();
        let num = self.blur(&vw, lambda);
        let den = self.blur(weights, lambda);
        num.iter().zip(&den).map(|(n, d)| if *d > 1e-3 { n / d } else { fallback }).collect()
    }

    /// Distance (cells, along centroids of neighbouring parcels) from the
    /// nearest source, exploring only `passable` parcels and stopping beyond
    /// `max`. Unreached parcels get `f32::INFINITY`.
    pub fn distances(&self, sources: &[usize], max: f32, passable: impl Fn(usize) -> bool) -> Vec<f32> {
        let mut d = vec![f32::INFINITY; self.len()];
        let mut heap = BinaryHeap::new();
        for &s in sources {
            d[s] = 0.0;
            heap.push(Item(0.0, s as u32));
        }
        while let Some(Item(di, i)) = heap.pop() {
            let i = i as usize;
            if di > d[i] {
                continue;
            }
            for j in self.neighbors(i) {
                if !passable(j) {
                    continue;
                }
                let dj = di + self.dist(i, j);
                if dj < d[j] && dj <= max {
                    d[j] = dj;
                    heap.push(Item(dj, j as u32));
                }
            }
        }
        d
    }

    /// Parcels at most `max` edge steps from a source.
    pub fn within_steps(&self, sources: impl Iterator<Item = usize>, max: u32) -> Vec<bool> {
        let mut seen = vec![false; self.len()];
        let mut frontier: Vec<usize> = sources.collect();
        for &s in &frontier {
            seen[s] = true;
        }
        for _ in 0..max {
            let mut next = Vec::new();
            for &i in &frontier {
                for j in self.neighbors(i) {
                    if !seen[j] {
                        seen[j] = true;
                        next.push(j);
                    }
                }
            }
            frontier = next;
        }
        seen
    }

    /// The edge of parcel `j` shared with its neighbour `i`.
    pub fn edge_to(&self, j: usize, i: usize) -> usize {
        self.adj[j].iter().position(|&x| x as usize == i).unwrap_or(0)
    }

    /// Midpoint of edge `k` of parcel `i`.
    pub fn edge_mid(&self, i: usize, k: usize) -> [f32; 2] {
        let q = self.quads[i];
        let (a, b) = (self.verts[q[k] as usize], self.verts[q[(k + 1) % 4] as usize]);
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh() -> Mesh {
        Mesh::generate(48, 40, &mut Rng::new(3))
    }

    #[test]
    fn parcels_are_about_one_cell() {
        let m = mesh();
        let n = m.len() as f32;
        assert!((n / (48.0 * 40.0) - 1.0).abs() < 0.15, "{n} parcels on 1920 cells");
        let mean_area = m.area.iter().sum::<f32>() / n;
        assert!((mean_area - 1.0).abs() < 0.2, "mean area {mean_area}");
    }

    #[test]
    fn quads_stay_convex_and_counter_clockwise() {
        let m = mesh();
        for (i, q) in m.quads.iter().enumerate() {
            let p = q.map(|v| m.verts[v as usize]);
            assert!(m.area[i] > 0.15, "parcel {i} area {}", m.area[i]);
            for k in 0..4 {
                let (a, b, c) = (p[k], p[(k + 1) % 4], p[(k + 2) % 4]);
                let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
                assert!(cross > 0.0, "parcel {i} is not convex at corner {}", (k + 1) % 4);
            }
        }
    }

    #[test]
    fn adjacency_is_symmetric_and_irregular() {
        let m = mesh();
        let mut valence: HashMap<u32, u32> = HashMap::new();
        for (i, a) in m.adj.iter().enumerate() {
            for &j in a {
                if j != NONE {
                    assert!(m.adj[j as usize].contains(&(i as u32)));
                    assert!(m.dist(i, j as usize) < 2.0);
                }
            }
            for &v in &m.quads[i] {
                *valence.entry(v).or_default() += 1;
            }
        }
        // Not a square grid: interior corners where three or five parcels meet.
        assert!(valence.values().any(|&k| k == 3));
        assert!(valence.values().any(|&k| k >= 5));
    }

    #[test]
    fn blur_spreads_mass_like_the_grid_kernel() {
        let m = mesh();
        let mut src = vec![0f32; m.len()];
        let c = m.cell_at(24.0, 20.0);
        src[c] = 1.0;
        let lambda = 3.0;
        let out = m.blur(&src, lambda);
        // Total mass ≈ kernel mass per unit area × parcels per unit area.
        let a = (-1.0f32 / lambda).exp();
        let k = ((1.0 + a) / (1.0 - a)).powi(2);
        let total: f32 = out.iter().sum();
        assert!((total / k - 1.0).abs() < 0.15, "mass {total} vs {k}");
        // And it decays with distance.
        let far = (0..m.len()).max_by(|&a, &b| m.dist(a, c).total_cmp(&m.dist(b, c))).unwrap();
        assert!(out[c] > 10.0 * out[far]);
    }

    #[test]
    fn neighbourhoods_are_sorted_by_distance() {
        let m = mesh();
        let i = m.cell_at(20.0, 20.0);
        let ring = m.near(i, 2.5);
        assert!(ring.len() > 10 && ring.len() < 30, "{}", ring.len());
        assert!(ring.iter().all(|&j| m.dist(i, j as usize) <= 2.5));
        assert!(m.near(i, 1.5).len() < ring.len());
    }
}
