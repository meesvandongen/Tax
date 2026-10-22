//! Nature and pollution. Vegetation regrows on undeveloped land, is damaged
//! by pollution, and is cleared by construction. Idle urban lots go to weeds.

use crate::types::*;
use crate::world::World;

pub fn run(w: &mut World) {
    let g = w.econ.nature_regrowth;
    for i in 0..w.p.len() {
        let pol = (w.f.pollution[i] / 8.0).min(1.0);
        let v = &mut w.p.nature[i];
        match w.p.land_use[i] {
            LandUse::Wild => {
                let target = w.terrain.vegetation[i].max(0.35);
                *v += g * (target - *v) - 0.02 * pol * *v;
            }
            LandUse::Vacant => *v += 2.0 * g * (0.55 - *v),
            LandUse::Park => *v += g * (0.85 - *v) - 0.01 * pol * *v,
            LandUse::Farm => *v += 0.05 * (0.12 - *v),
            LandUse::Residential => {
                let target = if w.p.floors[i] <= 2 { 0.25 } else { 0.05 };
                *v += 0.02 * (target - *v) - 0.01 * pol * *v;
            }
            _ => *v += 0.05 * (0.0 - *v),
        }
        *v = v.clamp(0.0, 1.0);
    }
}
