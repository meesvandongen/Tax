// Time-series charts (uPlot) with an optional baseline run for comparison.

import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import type { Metrics } from '../sim/protocol';

export interface SeriesDef {
  label: string;
  value: (m: Metrics) => number;
  color: string;
}

export interface ChartDef {
  id: string;
  title: string;
  help: string;
  series: SeriesDef[];
  /** y-axis formatter */
  fmt?: (v: number) => string;
}

const pct = (v: number) => `${(v * 100).toFixed(0)}%`;
const k = (v: number) => (Math.abs(v) >= 1e6 ? `${(v / 1e6).toFixed(1)}M` : Math.abs(v) >= 1e3 ? `${(v / 1e3).toFixed(0)}k` : v.toFixed(0));

const taxRevenue = (m: Metrics, kind: string) =>
  (m.revenue_by_tax ?? []).filter(([n]) => n === kind).reduce((s, [, v]) => s + v, 0);

export const CHARTS: ChartDef[] = [
  {
    id: 'population',
    title: 'Population',
    help: 'Households living in the city, and how many have no home (tents).',
    series: [
      { label: 'households', value: (m) => m.households, color: '#4e79a7' },
      { label: 'unhoused', value: (m) => m.unhoused, color: '#e15759' },
    ],
    fmt: k,
  },
  {
    id: 'housing',
    title: 'Housing',
    help: 'Median rent and rent as a share of income. Vacancy is the share of dwellings standing empty.',
    series: [
      { label: 'rent burden', value: (m) => m.rent_burden, color: '#e15759' },
      { label: 'vacancy', value: (m) => m.housing_vacancy, color: '#76b7b2' },
      { label: 'unemployment', value: (m) => m.unemployment, color: '#f28e2b' },
    ],
    fmt: pct,
  },
  {
    id: 'income',
    title: 'Incomes & rents',
    help: 'Median household income, median rent and mean wage (money per month).',
    series: [
      { label: 'median income', value: (m) => m.median_income, color: '#59a14f' },
      { label: 'median rent', value: (m) => m.median_rent, color: '#e15759' },
      { label: 'mean wage', value: (m) => m.mean_wage, color: '#4e79a7' },
    ],
    fmt: k,
  },
  {
    id: 'inequality',
    title: 'Inequality',
    help: 'Gini coefficients (0 = equal, 1 = one has all) and the wealth share of the richest 10%.',
    series: [
      { label: 'Gini wealth', value: (m) => m.gini_wealth, color: '#b07aa1' },
      { label: 'Gini income', value: (m) => m.gini_income, color: '#4e79a7' },
      { label: 'top 10% wealth', value: (m) => m.top10_wealth_share, color: '#edc948' },
      { label: 'absentee-owned land', value: (m) => m.absentee_land_share, color: '#9c755f' },
      { label: 'owner-occupiers', value: (m) => m.owner_occupier_share, color: '#59a14f' },
    ],
    fmt: pct,
  },
  {
    id: 'land',
    title: 'Land & buildings',
    help: 'Land price index (assessed land value per urban cell, thousands) and average building height.',
    series: [
      { label: 'land price index', value: (m) => m.land_price_index, color: '#59a14f' },
      { label: 'avg floors ×100', value: (m) => m.avg_floors * 100, color: '#4e79a7' },
      { label: 'derelict ×10', value: (m) => m.derelict * 10, color: '#9c755f' },
    ],
    fmt: k,
  },
  {
    id: 'sprawl',
    title: 'Sprawl & nature',
    help: 'Urban footprint (cells), green cover and average commute distance.',
    series: [
      { label: 'urban cells', value: (m) => m.urban_cells, color: '#9c755f' },
      { label: 'green cover ×1000', value: (m) => m.green_cover * 1000, color: '#59a14f' },
      { label: 'avg commute ×10', value: (m) => m.avg_commute * 10, color: '#f28e2b' },
    ],
    fmt: k,
  },
  {
    id: 'economy',
    title: 'Economy',
    help: 'Value added per month (GDP proxy), exports and number of firms.',
    series: [
      { label: 'GDP / month', value: (m) => m.gdp, color: '#4e79a7' },
      { label: 'exports', value: (m) => m.exports, color: '#76b7b2' },
    ],
    fmt: k,
  },
  {
    id: 'revenue',
    title: 'Tax revenue by instrument',
    help: 'Monthly revenue of each tax.',
    series: [
      { label: 'land value', value: (m) => taxRevenue(m, 'land_value'), color: '#59a14f' },
      { label: 'property', value: (m) => taxRevenue(m, 'property'), color: '#9c755f' },
      { label: 'income', value: (m) => taxRevenue(m, 'income'), color: '#4e79a7' },
      { label: 'sales', value: (m) => taxRevenue(m, 'sales'), color: '#f28e2b' },
      { label: 'wealth', value: (m) => taxRevenue(m, 'wealth'), color: '#b07aa1' },
      { label: 'other', value: (m) => m.revenue - ['land_value', 'property', 'income', 'sales', 'wealth'].reduce((s, kk) => s + taxRevenue(m, kk), 0), color: '#bab0ac' },
    ],
    fmt: k,
  },
  {
    id: 'budget',
    title: 'Government budget',
    help: 'Spending by programme per month, and cash reserves (negative = debt).',
    series: [
      { label: 'services', value: (m) => m.spend_services, color: '#4e79a7' },
      { label: 'infrastructure', value: (m) => m.spend_infrastructure, color: '#9c755f' },
      { label: 'dividend+rebate', value: (m) => m.spend_dividend + m.spend_rebate, color: '#59a14f' },
      { label: 'welfare', value: (m) => m.spend_welfare, color: '#e15759' },
      { label: 'reserves /12', value: (m) => m.gov_cash / 12, color: '#bab0ac' },
    ],
    fmt: k,
  },
  {
    id: 'distortion',
    title: 'Behavioural distortions',
    help: 'Counterfactual "no-tax" decisions: job seekers who would work without taxes, sales blocked by transfer tax, floors not built because of taxes, owners deferring maintenance, and households leaving with their wealth.',
    series: [
      { label: 'deterred workers', value: (m) => m.deterred_workers, color: '#4e79a7' },
      { label: 'blocked sales', value: (m) => m.blocked_sales, color: '#f28e2b' },
      { label: 'floors forgone', value: (m) => Math.max(0, m.floors_no_tax - m.floors_built), color: '#e15759' },
      { label: 'wealthy emigrants', value: (m) => m.wealthy_emigrants, color: '#edc948' },
    ],
    fmt: k,
  },
  {
    id: 'environment',
    title: 'Environment',
    help: 'Population-weighted pollution exposure and total emissions.',
    series: [
      { label: 'exposure ×100', value: (m) => m.pollution_exposure * 100, color: '#9c755f' },
      { label: 'emissions', value: (m) => m.emissions, color: '#e15759' },
      { label: 'parks', value: (m) => m.parks, color: '#59a14f' },
    ],
    fmt: k,
  },
];

/** Moving average to tame month-to-month noise. */
function smooth(v: number[], win: number): number[] {
  if (win <= 1) return v;
  const out = new Array(v.length);
  let s = 0;
  for (let i = 0; i < v.length; i++) {
    s += v[i];
    if (i >= win) s -= v[i - win];
    out[i] = s / Math.min(i + 1, win);
  }
  return out;
}

export class Charts {
  private plots: { def: ChartDef; plot: uPlot; el: HTMLElement }[] = [];
  private dirty = true;
  private compare = false;
  smoothing = 6;

  constructor(private root: HTMLElement, private getMain: () => Metrics[], private getBase: () => Metrics[] | null) {
    this.build();
    new ResizeObserver(() => this.resize()).observe(root);
    const tick = () => {
      if (this.dirty) this.redraw();
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }

  private build() {
    for (const p of this.plots) {
      p.plot.destroy();
      p.el.remove();
    }
    this.plots = [];
    const width = Math.max(200, this.root.clientWidth - 16);
    for (const def of CHARTS) {
      const el = document.createElement('div');
      el.className = 'chart';
      el.innerHTML = `<div class="chart-title" title="${def.help}">${def.title}${
        this.compare ? ' <span class="muted">— dashed: baseline</span>' : ''
      }</div>`;
      this.root.appendChild(el);
      const plot = new uPlot(this.opts(def, width), [[]], el);
      this.plots.push({ def, plot, el });
    }
    this.dirty = true;
  }

  /** Show or hide the dashed baseline series. */
  setCompare(on: boolean) {
    if (on === this.compare) return;
    this.compare = on;
    this.build();
  }

  private opts(def: ChartDef, width: number): uPlot.Options {
    const series: uPlot.Series[] = [{ label: 'year', value: (_u, v) => (v == null ? '' : v.toFixed(1)) }];
    for (const s of def.series) {
      series.push({ label: s.label, stroke: s.color, width: 1.6, value: (_u, v) => (v == null ? '–' : (def.fmt ?? String)(v)) });
    }
    if (this.compare) {
      for (const s of def.series) {
        series.push({ label: `${s.label} (baseline)`, stroke: s.color, width: 1, dash: [4, 4], value: (_u, v) => (v == null ? '–' : (def.fmt ?? String)(v)) });
      }
    }
    return {
      width,
      height: 140,
      legend: { live: false },
      cursor: { drag: { x: false, y: false } },
      scales: { x: { time: false } },
      axes: [
        { stroke: '#9aa4b2', grid: { stroke: '#2a3140', width: 1 }, ticks: { stroke: '#2a3140' } },
        { stroke: '#9aa4b2', grid: { stroke: '#2a3140', width: 1 }, ticks: { stroke: '#2a3140' }, size: 48, values: (_u, vals) => vals.map((v) => (def.fmt ?? String)(v)) },
      ],
      series,
    };
  }

  markDirty() {
    this.dirty = true;
  }

  private resize() {
    const width = this.root.clientWidth - 16;
    for (const p of this.plots) p.plot.setSize({ width, height: 140 });
  }

  private redraw() {
    this.dirty = false;
    const main = this.getMain();
    const base = this.getBase();
    for (const { def, plot } of this.plots) {
      const n = Math.max(main.length, base?.length ?? 0);
      const xs: number[] = [];
      for (let i = 0; i < n; i++) xs.push(i / 12);
      const data: (number | null)[][] = [xs];
      for (const s of def.series) {
        const v = smooth(main.map(s.value), this.smoothing);
        data.push(xs.map((_, i) => (i < v.length ? v[i] : null)));
      }
      if (this.compare) {
        for (const s of def.series) {
          if (!base) {
            data.push(xs.map(() => null));
            continue;
          }
          const v = smooth(base.map(s.value), this.smoothing);
          data.push(xs.map((_, i) => (i < v.length ? v[i] : null)));
        }
      }
      plot.setData(data as uPlot.AlignedData);
    }
  }
}
