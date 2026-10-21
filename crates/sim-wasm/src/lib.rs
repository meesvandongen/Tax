//! Minimal C-ABI over `sim-core` for WebAssembly.
//!
//! No wasm-bindgen: the host instantiates the module, passes JSON in through
//! `alloc`ated buffers and reads results (JSON strings and typed snapshot
//! buffers) straight out of linear memory. Every function returning a
//! string returns a pointer; its byte length is available from `out_len`.
//! One simulation per module instance (one per Web Worker).

use sim_core::presets::{preset, PRESETS};
use sim_core::snapshot::{place_tents, Snapshot, CELL_STRIDE, OVERLAYS};
use sim_core::{Policy, SimConfig, Simulation};
use std::cell::RefCell;

struct State {
    sim: Option<Simulation>,
    snap: Snapshot,
    out: Vec<u8>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { sim: None, snap: Snapshot::default(), out: Vec::new() });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn emit(s: &mut State, text: String) -> *const u8 {
    s.out = text.into_bytes();
    s.out.as_ptr()
}

unsafe fn read_str<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("")
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` must come from `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    with(|s| s.out.as_ptr())
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    with(|s| s.out.len())
}

/// Create a simulation from a JSON `SimConfig` (empty → defaults).
/// Returns 0 on success, 1 on parse error (message via `out`).
/// # Safety
/// `ptr..ptr+len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn sim_new(ptr: *const u8, len: usize) -> u32 {
    let text = read_str(ptr, len);
    let cfg: Result<SimConfig, _> = if text.trim().is_empty() { Ok(SimConfig::default()) } else { serde_json::from_str(text) };
    with(|s| match cfg {
        Ok(cfg) => {
            s.sim = Some(Simulation::new(cfg));
            s.snap = Snapshot::default();
            0
        }
        Err(e) => {
            emit(s, e.to_string());
            1
        }
    })
}

#[no_mangle]
pub extern "C" fn sim_width() -> u32 {
    with(|s| s.sim.as_ref().map(|x| x.world.dims.w as u32).unwrap_or(0))
}

#[no_mangle]
pub extern "C" fn sim_height() -> u32 {
    with(|s| s.sim.as_ref().map(|x| x.world.dims.h as u32).unwrap_or(0))
}

#[no_mangle]
pub extern "C" fn sim_tick() -> u32 {
    with(|s| s.sim.as_ref().map(|x| x.world.tick as u32).unwrap_or(0))
}

/// Advance `months` ticks.
#[no_mangle]
pub extern "C" fn sim_step(months: u32) {
    with(|s| {
        if let Some(sim) = s.sim.as_mut() {
            for _ in 0..months {
                sim.step();
            }
        }
    })
}

/// Replace the policy (JSON `Policy`). Returns 0 on success.
/// # Safety
/// `ptr..ptr+len` must be valid UTF-8 in linear memory.
#[no_mangle]
pub unsafe extern "C" fn sim_set_policy(ptr: *const u8, len: usize) -> u32 {
    let text = read_str(ptr, len);
    with(|s| match serde_json::from_str::<Policy>(text) {
        Ok(p) => {
            if let Some(sim) = s.sim.as_mut() {
                sim.set_policy(p);
            }
            0
        }
        Err(e) => {
            emit(s, e.to_string());
            1
        }
    })
}

#[no_mangle]
pub extern "C" fn sim_policy_json() -> *const u8 {
    with(|s| {
        let t = s.sim.as_ref().map(|x| serde_json::to_string(&x.world.cfg.policy).unwrap()).unwrap_or_default();
        emit(s, t)
    })
}

#[no_mangle]
pub extern "C" fn sim_config_json() -> *const u8 {
    with(|s| {
        let t = s.sim.as_ref().map(|x| serde_json::to_string(&x.world.cfg).unwrap()).unwrap_or_default();
        emit(s, t)
    })
}

/// Latest metrics as JSON.
#[no_mangle]
pub extern "C" fn sim_metrics_json() -> *const u8 {
    with(|s| {
        let t = s
            .sim
            .as_ref()
            .and_then(|x| x.world.history.last())
            .map(|m| serde_json::to_string(m).unwrap())
            .unwrap_or_else(|| "null".into());
        emit(s, t)
    })
}

/// Metrics for ticks >= `from` as a JSON array.
#[no_mangle]
pub extern "C" fn sim_history_json(from: u32) -> *const u8 {
    with(|s| {
        let t = s
            .sim
            .as_ref()
            .map(|x| serde_json::to_string(&x.world.history[(from as usize).min(x.world.history.len())..]).unwrap())
            .unwrap_or_else(|| "[]".into());
        emit(s, t)
    })
}

/// Events of the last tick as JSON.
#[no_mangle]
pub extern "C" fn sim_events_json() -> *const u8 {
    with(|s| {
        let t = s.sim.as_ref().map(|x| serde_json::to_string(&x.world.events).unwrap()).unwrap_or_else(|| "[]".into());
        emit(s, t)
    })
}

#[no_mangle]
pub extern "C" fn sim_inspect_json(cell: u32) -> *const u8 {
    with(|s| {
        let t = s
            .sim
            .as_ref()
            .map(|x| sim_core::inspect::parcel(&x.world, cell as usize).to_string())
            .unwrap_or_else(|| "null".into());
        emit(s, t)
    })
}

#[no_mangle]
pub extern "C" fn sim_presets_json() -> *const u8 {
    with(|s| {
        let list: Vec<_> = PRESETS
            .iter()
            .map(|(n, d)| serde_json::json!({"name": n, "description": d, "policy": preset(n)}))
            .collect();
        emit(s, serde_json::to_string(&list).unwrap())
    })
}

#[no_mangle]
pub extern "C" fn sim_overlays_json() -> *const u8 {
    with(|s| emit(s, serde_json::to_string(OVERLAYS).unwrap()))
}

/// Rebuild render buffers. Afterwards read them with the `snap_*` getters.
#[no_mangle]
pub extern "C" fn sim_snapshot(max_commuters: u32) {
    with(|s| {
        if let Some(sim) = s.sim.as_mut() {
            place_tents(&mut sim.world);
            s.snap.update(&sim.world, max_commuters as usize);
        }
    })
}

#[no_mangle]
pub extern "C" fn snap_cell_stride() -> u32 {
    CELL_STRIDE as u32
}
#[no_mangle]
pub extern "C" fn snap_cells_ptr() -> *const u8 {
    with(|s| s.snap.cells.as_ptr())
}
#[no_mangle]
pub extern "C" fn snap_cells_len() -> usize {
    with(|s| s.snap.cells.len())
}
#[no_mangle]
pub extern "C" fn snap_overlays_ptr() -> *const f32 {
    with(|s| s.snap.overlays.as_ptr())
}
#[no_mangle]
pub extern "C" fn snap_overlays_len() -> usize {
    with(|s| s.snap.overlays.len())
}
#[no_mangle]
pub extern "C" fn snap_commuters_ptr() -> *const u32 {
    with(|s| s.snap.commuters.as_ptr())
}
#[no_mangle]
pub extern "C" fn snap_commuters_len() -> usize {
    with(|s| s.snap.commuters.len())
}
#[no_mangle]
pub extern "C" fn snap_heights_ptr() -> *const f32 {
    with(|s| s.snap.heights.as_ptr())
}
#[no_mangle]
pub extern "C" fn snap_heights_len() -> usize {
    with(|s| s.snap.heights.len())
}
