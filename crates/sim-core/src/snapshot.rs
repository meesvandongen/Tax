//! Compact, render-oriented views of the world. The browser copies these
//! buffers straight out of WebAssembly memory every frame or two.

use crate::types::*;
use crate::world::World;

/// Overlay channels exposed to the renderer (one f32 per cell each).
pub const OVERLAYS: &[&str] = &[
    "land_value",
    "rent",
    "tax",
    "pollution",
    "amenity",
    "service",
    "commute",
    "price",
    "occupancy",
    "owner_wealth",
    "speculation",
];

#[derive(Default)]
pub struct Snapshot {
    /// Per-cell bytes, `CELL_STRIDE` bytes per cell:
    /// [land_use, floors, condition, occupancy, nature, tents, owner_class, flags]
    pub cells: Vec<u8>,
    /// Per-cell overlays, `OVERLAYS.len()` floats per cell (channel-major).
    pub overlays: Vec<f32>,
    /// Commuter pairs (home cell, job cell), for traffic animation.
    pub commuters: Vec<u32>,
    /// Static terrain heights (sent once).
    pub heights: Vec<f32>,
}

pub const CELL_STRIDE: usize = 8;

pub const FLAG_ROAD_ACCESS: u8 = 1;
pub const FLAG_SPECULATIVE: u8 = 2;
pub const FLAG_DERELICT: u8 = 4;
pub const FLAG_WATER: u8 = 8;
pub const FLAG_FOR_CONSTRUCTION_RES: u8 = 16;
pub const FLAG_FOR_CONSTRUCTION_COM: u8 = 32;
pub const FLAG_FOR_CONSTRUCTION_IND: u8 = 64;
pub const FLAG_RECENT_SALE: u8 = 128;

impl Snapshot {
    pub fn update(&mut self, w: &World, max_commuters: usize) {
        let n = w.dims.len();
        let p = &w.p;
        self.cells.resize(n * CELL_STRIDE, 0);
        self.overlays.resize(n * OVERLAYS.len(), 0.0);
        if self.heights.len() != n {
            self.heights = w.terrain.height.clone();
        }

        // Owner wealth classes: deciles among resident households.
        let mut ws: Vec<f64> = w.hh.iter().filter(|h| h.alive).map(|h| h.wealth).collect();
        ws.sort_by(|a, b| a.total_cmp(b));
        let decile = |x: f64| -> u8 {
            if ws.is_empty() {
                return 0;
            }
            let pos = ws.partition_point(|&v| v < x);
            ((pos * 10) / ws.len()).min(9) as u8
        };

        for i in 0..n {
            let o = i * CELL_STRIDE;
            let u = p.land_use[i];
            let occ = if p.units[i] > 0 {
                (p.occupied[i] as f32 / p.units[i] as f32 * 255.0) as u8
            } else if u == LandUse::Commercial || u == LandUse::Industrial {
                if p.firm[i] != NONE {
                    255
                } else {
                    0
                }
            } else {
                0
            };
            let owner_class = match p.owner[i] {
                Owner::Absentee => 0,
                Owner::Gov => 1,
                Owner::Hh(id) => 2 + decile(w.hh[id as usize].wealth),
            };
            let mut flags = 0u8;
            if p.road_access[i] {
                flags |= FLAG_ROAD_ACCESS;
            }
            if p.months_held_empty[i] > 12 && !u.is_building() {
                flags |= FLAG_SPECULATIVE;
            }
            if u.is_building() && p.condition[i] < 0.25 {
                flags |= FLAG_DERELICT;
            }
            if w.terrain.water[i] {
                flags |= FLAG_WATER;
            }
            if u == LandUse::Construction {
                flags |= match p.target_use[i] {
                    LandUse::Residential => FLAG_FOR_CONSTRUCTION_RES,
                    LandUse::Commercial => FLAG_FOR_CONSTRUCTION_COM,
                    _ => FLAG_FOR_CONSTRUCTION_IND,
                };
            }
            if p.months_since_sale[i] < 3 {
                flags |= FLAG_RECENT_SALE;
            }
            let c = &mut self.cells[o..o + CELL_STRIDE];
            c[0] = u as u8;
            c[1] = p.floors[i];
            c[2] = (p.condition[i] * 255.0) as u8;
            c[3] = occ;
            c[4] = (p.nature[i] * 255.0) as u8;
            c[5] = p.tents[i].min(255) as u8;
            c[6] = owner_class;
            c[7] = flags;
        }

        let ch = |k: usize| k * n;
        let ov = &mut self.overlays;
        for i in 0..n {
            ov[ch(0) + i] = p.assessed_land[i];
            ov[ch(1) + i] = if p.land_use[i] == LandUse::Residential { p.asking_rent[i] } else { w.f.rent_res[i] };
            ov[ch(2) + i] = p.tax_last[i];
            ov[ch(3) + i] = w.f.pollution[i];
            ov[ch(4) + i] = w.f.amenity[i];
            ov[ch(5) + i] = w.f.service[i];
            ov[ch(6) + i] = w.f.commute_eff[i];
            ov[ch(7) + i] = p.price[i];
            ov[ch(8) + i] = if p.units[i] > 0 { p.occupied[i] as f32 / p.units[i] as f32 } else { 0.0 };
            ov[ch(9) + i] = match p.owner[i] {
                Owner::Hh(id) => w.hh[id as usize].wealth as f32,
                Owner::Absentee => -1.0,
                Owner::Gov => -2.0,
            };
            ov[ch(10) + i] = p.months_held_empty[i] as f32;
        }

        // Commuters: a deterministic stride sample of employed, housed households.
        self.commuters.clear();
        let employed: Vec<(u32, u32)> =
            w.hh.iter()
                .filter(|h| h.alive && h.home != NONE && h.job != NONE)
                .map(|h| (h.home, w.firms[h.job as usize].parcel))
                .collect();
        let stride = (employed.len() / max_commuters.max(1)).max(1);
        for (home, job) in employed.iter().step_by(stride).take(max_commuters) {
            self.commuters.push(*home);
            self.commuters.push(*job);
        }
    }
}

/// Count unhoused households per cell as tents near the centre / parks.
pub fn place_tents(w: &mut World) {
    for t in w.p.tents.iter_mut() {
        *t = 0;
    }
    let unhoused = w.hh.iter().filter(|h| h.alive && h.home == NONE).count();
    if unhoused == 0 {
        return;
    }
    // Camps form on parks and idle lots close to the centre.
    let mut spots: Vec<usize> = (0..w.p.len())
        .filter(|&i| matches!(w.p.land_use[i], LandUse::Park | LandUse::Vacant | LandUse::Wild) && w.p.road_access[i])
        .collect();
    let c = w.center;
    spots.sort_by(|&a, &b| w.dims.manhattan(a, c).total_cmp(&w.dims.manhattan(b, c)).then(a.cmp(&b)));
    let per_spot = 6usize;
    let mut left = unhoused;
    for i in spots {
        if left == 0 {
            break;
        }
        let k = left.min(per_spot);
        w.p.tents[i] = k as u16;
        left -= k;
    }
}
