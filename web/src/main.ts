// Application wiring: simulation workers ↔ 3D view ↔ UI panels.

import './styles.css';
import { SimClient, type ReadyInfo } from './sim/client';
import { type Metrics, type Policy } from './sim/protocol';
import { OVERLAY_DEFS, rampCss } from './render/palette';
import { WorldView } from './render/world';
import { Charts } from './ui/charts';
import { renderInspector } from './ui/inspector';
import { renderKpis } from './ui/kpis';
import { PolicyPanel } from './ui/policy';

const WASM_URL = new URL('sim.wasm', document.baseURI).href;
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const view = new WorldView($('view'));
const policyPanel = new PolicyPanel($('policy'));
let ready: ReadyInfo | null = null;
let running = false;
let selected: number | null = null;
let focused = false;
let baseline: SimClient | null = null;
let baselineName = '';

const main = new SimClient('main');
const charts = new Charts($('charts'), () => main.history, () => (baseline ? baseline.history : null));

function speed(): number {
  return Number(($('speed') as HTMLSelectElement).value);
}

function config(policy: Policy) {
  const size = Number(($('size') as HTMLSelectElement).value);
  const seed = Number(($('seed') as HTMLInputElement).value) || 1;
  return { seed, width: size, height: size, policy };
}

function toast(msg: string) {
  const t = $('toast');
  t.textContent = msg;
  t.hidden = false;
  setTimeout(() => (t.hidden = true), 5000);
}

// ---------------------------------------------------------------- main sim

main.onError = toast;
main.onReady = (info) => {
  ready = info;
  focused = false;
  view.setWorld(info.width, info.height, info.heights, info.mesh);
  policyPanel.setPresets(info.presets);
  policyPanel.setPolicy(info.config.policy);
  fillBaselineSelect(info);
  charts.markDirty();
  if (running) main.run(speed());
};
main.onFrame = ({ frame, events }) => {
  view.update(frame, events);
  if (!focused) {
    view.focusOnCity(frame.cells);
    focused = true;
  }
  if (selected !== null) refreshInspector();
};
main.onMetrics = (ms: Metrics[]) => {
  const m = ms[ms.length - 1];
  $('clock').textContent = `year ${m.year.toFixed(1)}`;
  charts.markDirty();
  renderKpis($('kpis'), main.history, baseline?.history ?? null);
};
main.onPerf = (ms) => {
  $('perf').textContent = `${ms.toFixed(1)} ms/month`;
};

// ---------------------------------------------------------------- baseline

function fillBaselineSelect(info: ReadyInfo) {
  const sel = $('baseline') as HTMLSelectElement;
  if (sel.options.length > 1) return;
  for (const p of info.presets) {
    const o = document.createElement('option');
    o.value = p.name;
    o.textContent = p.name.replace(/_/g, ' ');
    o.title = p.description;
    sel.append(o);
  }
}

function startBaseline(name: string) {
  baseline?.terminate();
  baseline = null;
  baselineName = name;
  charts.setCompare(!!name);
  if (!name || !ready) {
    charts.markDirty();
    return;
  }
  const preset = ready.presets.find((p) => p.name === name)!;
  baseline = new SimClient('shadow');
  baseline.onError = toast;
  baseline.onMetrics = () => charts.markDirty();
  baseline.onReady = () => {
    if (running) baseline!.run(speed());
  };
  baseline.init(WASM_URL, config(preset.policy));
}

/** Restart main (and baseline) from the same seed so they stay comparable. */
function restart() {
  const pol = policyPanel.policy ?? ready?.config.policy;
  main.reset(config(pol));
  if (baselineName) startBaseline(baselineName);
  renderInspector($('inspector'), null);
  selected = null;
  view.setHighlight(null);
}

// ---------------------------------------------------------------- controls

function setRunning(r: boolean) {
  running = r;
  $('play').textContent = r ? '❚❚' : '▶';
  if (r) {
    main.run(speed());
    baseline?.run(speed());
  } else {
    main.pause();
    baseline?.pause();
  }
}

$('play').addEventListener('click', () => setRunning(!running));
$('step').addEventListener('click', () => {
  main.step(12);
  baseline?.step(12);
});
$('speed').addEventListener('change', () => running && setRunning(true));
$('reset').addEventListener('click', restart);
$('baseline').addEventListener('change', (e) => {
  startBaseline((e.target as HTMLSelectElement).value);
  // Comparisons are only meaningful from the same starting point.
  restart();
});
window.addEventListener('keydown', (e) => {
  if (e.code === 'Space' && !(e.target instanceof HTMLInputElement) && !(e.target instanceof HTMLSelectElement)) {
    e.preventDefault();
    setRunning(!running);
  }
});

policyPanel.onChange = (p) => main.setPolicy(p);

// overlays
const bar = $('overlay-bar');
function overlayButton(key: string | null, label: string, help: string) {
  const b = document.createElement('button');
  b.textContent = label;
  b.title = help;
  b.dataset.key = key ?? '';
  b.addEventListener('click', () => {
    view.overlay = key;
    for (const x of bar.querySelectorAll('button[data-key]')) x.classList.toggle('active', (x as HTMLElement).dataset.key === (key ?? ''));
    view.paint();
    updateLegend();
  });
  if (key === null) b.classList.add('active');
  bar.append(b);
}
overlayButton(null, 'City', 'Normal view');
for (const d of OVERLAY_DEFS) overlayButton(d.key, d.label, d.help);
const sep = document.createElement('span');
sep.className = 'sep';
bar.append(sep);
for (const [label, prop] of [
  ['Traffic', 'showTraffic'],
  ['Smog', 'showSmog'],
] as const) {
  const b = document.createElement('button');
  b.textContent = label;
  b.classList.add('active');
  b.addEventListener('click', () => {
    view[prop] = !view[prop];
    b.classList.toggle('active', view[prop]);
    view.paint();
  });
  bar.append(b);
}

function fmtVal(v: number): string {
  return Math.abs(v) >= 1e6 ? `${(v / 1e6).toFixed(1)}M` : Math.abs(v) >= 1e3 ? `${(v / 1e3).toFixed(1)}k` : v.toFixed(2);
}

function updateLegend() {
  const lg = $('legend');
  const def = OVERLAY_DEFS.find((d) => d.key === view.overlay);
  const st = view.overlayStats;
  if (!def || !st) {
    lg.hidden = true;
    return;
  }
  const lo = st.log ? Math.expm1(st.min) : st.min;
  const hi = st.log ? Math.expm1(st.max) : st.max;
  lg.hidden = false;
  lg.innerHTML = `<b>${def.label}</b><div class="muted">${def.help}</div>
    <div class="bar" style="background:${rampCss(def.ramp)}"></div>
    <div class="ends"><span>${fmtVal(lo)}</span><span>${def.log ? 'log scale' : ''}</span><span>${fmtVal(hi)}</span></div>`;
}

// picking
let downAt: [number, number] | null = null;
const canvas = view.renderer.domElement;
canvas.addEventListener('pointerdown', (e) => (downAt = [e.clientX, e.clientY]));
canvas.addEventListener('pointerup', (e) => {
  if (!downAt || Math.hypot(e.clientX - downAt[0], e.clientY - downAt[1]) > 4) return;
  const cell = view.pick(e.clientX, e.clientY);
  selected = cell;
  view.setHighlight(cell);
  if (cell === null) renderInspector($('inspector'), null);
  else refreshInspector();
});

async function refreshInspector() {
  if (selected === null) return;
  const data = await main.inspect(selected);
  if (selected !== null) renderInspector($('inspector'), data);
  view.setHighlight(selected);
}

setInterval(() => view.overlay && updateLegend(), 1000);

// boot
main.init(WASM_URL, config(undefined as unknown as Policy));
(window as unknown as { taxcity: unknown }).taxcity = { main, view, get baseline() { return baseline; } };
