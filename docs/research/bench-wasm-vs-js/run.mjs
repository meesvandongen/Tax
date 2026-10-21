// Runs the same kernel as (a) Rust->wasm and (b) hand-written JS with typed arrays (SoA).
import { readFileSync } from 'node:fs';

const [, , wasmPath, nArg, wArg, tArg] = process.argv;
const N = +(nArg ?? 200000), W = +(wArg ?? 512), T = +(tArg ?? 200);
const now = () => performance.now();

// ---------- wasm ----------
{
  const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
  const e = instance.exports;
  e.init(N, W, 12345);
  e.step(10);
  const t0 = now();
  e.step(T);
  const ms = (now() - t0) / T;
  console.log(`wasm(${wasmPath.split('/').pop()}) n=${N} w=${W} ms/tick=${ms.toFixed(3)} hash=${(e.hash() >>> 0).toString(16).padStart(8, '0')} tax=${e.tax_total()}`);
}

// ---------- JS SoA typed arrays ----------
function makeSim(n, w, seed) {
  const cells = w * w;
  const s = {
    n, w,
    wealth: new Float64Array(n), income: new Float64Array(n),
    parcel: new Uint32Array(n), rng: new Uint32Array(n),
    lv: new Float64Array(cells), lv2: new Float64Array(cells), occ: new Int32Array(cells),
    tax: 0,
  };
  const g = new Uint32Array([seed | 1]);
  const xs = () => { let x = g[0]; x ^= x << 13; x ^= x >>> 17; x ^= x << 5; g[0] = x; return x >>> 0; };
  for (let c = 0; c < cells; c++) s.lv[c] = xs() * (1 / 4294967296);
  for (let i = 0; i < n; i++) {
    s.rng[i] = (xs() | 1) >>> 0;
    s.income[i] = 10 + xs() * (90 / 4294967296);
    s.wealth[i] = 100;
    const p = xs() % cells;
    s.parcel[i] = p; s.occ[p] += 1;
  }
  return s;
}

function step(s) {
  const w = s.w, n = s.n, occ = s.occ, parcel = s.parcel, rng = s.rng, income = s.income, wealth = s.wealth;
  let lv = s.lv; const lv2 = s.lv2;
  for (let y = 1; y < w - 1; y++) {
    const row = y * w;
    for (let x = 1; x < w - 1; x++) {
      const i = row + x;
      const nb = lv[i - 1] + lv[i + 1] + lv[i - w] + lv[i + w];
      lv2[i] = lv[i] * 0.6 + nb * 0.1 + occ[i] * 0.001 - 0.0005;
    }
  }
  s.lv = lv2; s.lv2 = lv; lv = lv2;
  const cells = w * w;
  let tax = 0;
  for (let i = 0; i < n; i++) {
    let r = rng[i];
    const cur = parcel[i];
    const cx = cur % w, cy = (cur / w) | 0;
    const inc = income[i];
    let best = cur;
    let bestU = lv[cur] * inc / (1 + occ[cur]);
    const curU = bestU;
    for (let k = 0; k < 4; k++) {
      r ^= r << 13; r ^= r >>> 17; r ^= r << 5; r >>>= 0;
      const c = r % cells;
      const px = c % w, py = (c / w) | 0;
      const d = Math.abs(px - cx) + Math.abs(py - cy);
      const u = lv[c] * inc / (1 + occ[c]) - d * 0.01;
      if (u > bestU) { bestU = u; best = c; }
    }
    if (best !== cur && bestU > curU * 1.05) {
      occ[cur] -= 1; occ[best] += 1; parcel[i] = best;
    }
    const lvp = lv[parcel[i]];
    const t = inc * 0.2 + lvp * 0.5;
    const rent = lvp * 2.0;
    wealth[i] += inc - t - rent;
    tax += t;
    rng[i] = r;
  }
  s.tax += tax;
}

function hash(s) {
  let h = 2166136261 >>> 0;
  const mix = (v) => { h ^= v; h = Math.imul(h, 16777619) >>> 0; };
  const wv = new Uint32Array(s.wealth.buffer); for (let i = 0; i < wv.length; i++) mix(wv[i]);
  const lv = new Uint32Array(s.lv.buffer); for (let i = 0; i < lv.length; i++) mix(lv[i]);
  for (let i = 0; i < s.parcel.length; i++) mix(s.parcel[i]);
  return h >>> 0;
}

{
  const s = makeSim(N, W, 12345);
  for (let k = 0; k < 10; k++) step(s);
  const t0 = now();
  for (let k = 0; k < T; k++) step(s);
  const ms = (now() - t0) / T;
  console.log(`js-soa n=${N} w=${W} ms/tick=${ms.toFixed(3)} hash=${hash(s).toString(16).padStart(8, '0')} tax=${s.tax}`);
}
