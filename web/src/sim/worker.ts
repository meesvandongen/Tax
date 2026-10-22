/// <reference lib="webworker" />
// Runs one simulation (WebAssembly) off the main thread. Steps at the
// requested speed, streams metrics every tick and render frames ~10×/s.

import type { FromWorker, Policy, SimEvent, ToWorker } from './protocol';

interface Exports {
  memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  out_ptr(): number;
  out_len(): number;
  sim_new(ptr: number, len: number): number;
  sim_width(): number;
  sim_height(): number;
  sim_tick(): number;
  sim_step(months: number): void;
  sim_set_policy(ptr: number, len: number): number;
  sim_config_json(): number;
  sim_history_json(from: number): number;
  sim_events_json(): number;
  sim_inspect_json(cell: number): number;
  sim_presets_json(): number;
  sim_overlays_json(): number;
  sim_snapshot(maxCommuters: number): void;
  snap_cells_ptr(): number;
  snap_cells_len(): number;
  snap_overlays_ptr(): number;
  snap_overlays_len(): number;
  snap_commuters_ptr(): number;
  snap_commuters_len(): number;
  snap_heights_ptr(): number;
  snap_heights_len(): number;
  snap_mesh_verts_ptr(): number;
  snap_mesh_verts_len(): number;
  snap_mesh_quads_ptr(): number;
  snap_mesh_quads_len(): number;
  snap_mesh_adj_ptr(): number;
  snap_mesh_adj_len(): number;
  snap_vert_heights_ptr(): number;
  snap_vert_heights_len(): number;
  snap_pixel_cells_ptr(): number;
  snap_pixel_cells_len(): number;
}

const ctx = self as unknown as DedicatedWorkerGlobalScope;
let x: Exports;
let role: 'main' | 'shadow' = 'main';
let running = false;
let monthsPerSecond = 4;
let lastSentTick = 0;
let lastFrameAt = 0;
let carry = 0;
let pendingEvents: SimEvent[] = [];
let timer: number | undefined;

const enc = new TextEncoder();
const dec = new TextDecoder();

function post(msg: FromWorker, transfer: Transferable[] = []) {
  ctx.postMessage(msg, transfer);
}

function readOut(ptr: number): string {
  return dec.decode(new Uint8Array(x.memory.buffer, ptr, x.out_len()).slice());
}

function withString<T>(s: string, f: (ptr: number, len: number) => T): T {
  const bytes = enc.encode(s);
  const ptr = x.alloc(bytes.length);
  new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
  try {
    return f(ptr, bytes.length);
  } finally {
    x.dealloc(ptr, bytes.length);
  }
}

function create(config: unknown) {
  const err = withString(config ? JSON.stringify(config) : '', (p, l) => x.sim_new(p, l));
  if (err) throw new Error('config: ' + readOut(x.out_ptr()));
  lastSentTick = 0;
  pendingEvents = [];
  x.sim_snapshot(0);
  const buf = x.memory.buffer;
  const heights = new Float32Array(buf, x.snap_heights_ptr(), x.snap_heights_len()).slice();
  const mesh = {
    verts: new Float32Array(buf, x.snap_mesh_verts_ptr(), x.snap_mesh_verts_len()).slice(),
    quads: new Uint32Array(buf, x.snap_mesh_quads_ptr(), x.snap_mesh_quads_len()).slice(),
    adj: new Uint32Array(buf, x.snap_mesh_adj_ptr(), x.snap_mesh_adj_len()).slice(),
    vertHeights: new Float32Array(buf, x.snap_vert_heights_ptr(), x.snap_vert_heights_len()).slice(),
    pixelCells: new Uint32Array(buf, x.snap_pixel_cells_ptr(), x.snap_pixel_cells_len()).slice(),
  };
  const cfg = JSON.parse(readOut(x.sim_config_json()));
  post(
    {
      type: 'ready',
      width: x.sim_width(),
      height: x.sim_height(),
      heights,
      mesh,
      overlays: JSON.parse(readOut(x.sim_overlays_json())),
      presets: JSON.parse(readOut(x.sim_presets_json())),
      config: cfg,
    },
    [heights.buffer, mesh.verts.buffer, mesh.quads.buffer, mesh.adj.buffer, mesh.vertHeights.buffer, mesh.pixelCells.buffer],
  );
  sendMetrics();
  sendFrame();
}

function sendMetrics() {
  const tick = x.sim_tick();
  if (tick <= lastSentTick && lastSentTick !== 0) return;
  const metrics = JSON.parse(readOut(x.sim_history_json(lastSentTick)));
  lastSentTick = tick;
  if (metrics.length) post({ type: 'metrics', metrics });
}

function sendFrame() {
  if (role === 'shadow') return;
  x.sim_snapshot(2500);
  const buf = x.memory.buffer;
  const cells = new Uint8Array(buf, x.snap_cells_ptr(), x.snap_cells_len()).slice();
  const overlays = new Float32Array(buf, x.snap_overlays_ptr(), x.snap_overlays_len()).slice();
  const commuters = new Uint32Array(buf, x.snap_commuters_ptr(), x.snap_commuters_len()).slice();
  const events = pendingEvents;
  pendingEvents = [];
  post({ type: 'frame', frame: { tick: x.sim_tick(), cells, overlays, commuters }, events }, [
    cells.buffer,
    overlays.buffer,
    commuters.buffer,
  ]);
  lastFrameAt = performance.now();
}

function stepMonths(n: number) {
  const t0 = performance.now();
  for (let k = 0; k < n; k++) {
    x.sim_step(1);
    if (role === 'main') {
      const ev: SimEvent[] = JSON.parse(readOut(x.sim_events_json()));
      // Keep the most recent events only (rendering budget).
      pendingEvents.push(...ev);
      if (pendingEvents.length > 1500) pendingEvents = pendingEvents.slice(-1500);
    }
  }
  if (n > 0) post({ type: 'perf', msPerMonth: (performance.now() - t0) / n });
  sendMetrics();
}

function loop() {
  if (!running) return;
  const interval = 50; // ms
  carry += (monthsPerSecond * interval) / 1000;
  const n = Math.floor(carry);
  carry -= n;
  if (n > 0) stepMonths(Math.min(n, 24));
  if (performance.now() - lastFrameAt > 110) sendFrame();
  timer = setTimeout(loop, interval) as unknown as number;
}

ctx.onmessage = async (e: MessageEvent<ToWorker>) => {
  const msg = e.data;
  try {
    switch (msg.type) {
      case 'init': {
        role = msg.role;
        const resp = await fetch(msg.wasmUrl);
        const { instance } = await WebAssembly.instantiate(await resp.arrayBuffer(), {});
        x = instance.exports as unknown as Exports;
        create(msg.config);
        break;
      }
      case 'reset':
        create(msg.config);
        break;
      case 'setPolicy': {
        const err = withString(JSON.stringify(msg.policy as Policy), (p, l) => x.sim_set_policy(p, l));
        if (err) post({ type: 'error', message: 'policy: ' + readOut(x.out_ptr()) });
        break;
      }
      case 'run':
        monthsPerSecond = msg.monthsPerSecond;
        if (!running) {
          running = true;
          loop();
        }
        break;
      case 'pause':
        running = false;
        if (timer !== undefined) clearTimeout(timer);
        sendFrame();
        break;
      case 'step':
        stepMonths(msg.months);
        sendFrame();
        break;
      case 'inspect':
        post({ type: 'inspect', id: msg.id, data: JSON.parse(readOut(x.sim_inspect_json(msg.cell))) });
        break;
    }
  } catch (err) {
    post({ type: 'error', message: String(err) });
  }
};
