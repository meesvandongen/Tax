// Headline indicators with deltas against the baseline run.

import type { Metrics } from '../sim/protocol';

interface Kpi {
  label: string;
  help: string;
  value: (m: Metrics) => number;
  fmt: (v: number) => string;
  /** +1 if higher is better, −1 if lower is better, 0 neutral */
  good: number;
}

const pct = (v: number) => `${(v * 100).toFixed(1)}%`;
const num = (v: number) => (Math.abs(v) >= 1e6 ? `${(v / 1e6).toFixed(1)}M` : Math.abs(v) >= 1e4 ? `${(v / 1e3).toFixed(0)}k` : v.toFixed(0));
const dec = (v: number) => v.toFixed(2);

export const KPIS: Kpi[] = [
  { label: 'Households', help: 'Population (households)', value: (m) => m.households, fmt: num, good: 1 },
  { label: 'Unhoused', help: 'Households without a home', value: (m) => m.unhoused, fmt: num, good: -1 },
  { label: 'Unemployment', help: 'Share of households without a job', value: (m) => m.unemployment, fmt: pct, good: -1 },
  { label: 'Median income', help: 'Monthly household income incl. transfers', value: (m) => m.median_income, fmt: num, good: 1 },
  { label: 'Median rent', help: 'Monthly rent of occupied dwellings', value: (m) => m.median_rent, fmt: num, good: -1 },
  { label: 'Rent burden', help: 'Median rent / income of renters', value: (m) => m.rent_burden, fmt: pct, good: -1 },
  { label: 'Gini wealth', help: 'Wealth inequality (0–1)', value: (m) => m.gini_wealth, fmt: dec, good: -1 },
  { label: 'Absentee land', help: 'Share of land value owned by outside investors', value: (m) => m.absentee_land_share, fmt: pct, good: -1 },
  { label: 'Avg floors', help: 'Average building height', value: (m) => m.avg_floors, fmt: dec, good: 0 },
  { label: 'Urban cells', help: 'Urban footprint (sprawl)', value: (m) => m.urban_cells, fmt: num, good: 0 },
  { label: 'Green cover', help: 'Mean vegetation over land', value: (m) => m.green_cover, fmt: pct, good: 1 },
  { label: 'Revenue', help: 'Tax revenue per month', value: (m) => m.revenue, fmt: num, good: 0 },
  { label: 'Mean welfare', help: 'Average money-metric utility of residents (income − rent − commute + amenities)', value: (m) => m.mean_utility, fmt: num, good: 1 },
];

/** Average over the last `n` months to reduce noise. */
function tailMean(h: Metrics[], f: (m: Metrics) => number, n = 12): number {
  const t = h.slice(-n);
  return t.reduce((s, m) => s + f(m), 0) / Math.max(1, t.length);
}

export function renderKpis(root: HTMLElement, main: Metrics[], base: Metrics[] | null) {
  if (!main.length) return;
  let html = '';
  for (const k of KPIS) {
    const v = tailMean(main, k.value);
    let delta = '';
    if (base && base.length) {
      const b = tailMean(base, k.value);
      const d = v - b;
      const rel = Math.abs(b) > 1e-9 ? d / Math.abs(b) : 0;
      const cls = k.good === 0 || Math.abs(rel) < 0.02 ? 'neutral' : Math.sign(d) === k.good ? 'better' : 'worse';
      delta = `<span class="delta ${cls}">${d >= 0 ? '+' : ''}${(rel * 100).toFixed(0)}%</span>`;
    }
    html += `<div class="kpi" title="${k.help}"><div class="kpi-label">${k.label}</div><div class="kpi-value">${k.fmt(v)}${delta}</div></div>`;
  }
  root.innerHTML = html;
}
