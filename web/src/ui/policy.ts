// Policy editor: presets, tax instruments with live sliders, spending mix.

import type { Policy, Preset, SpendingPolicy, TaxKind, TaxSpec } from '../sim/protocol';

interface ParamDef {
  key: string;
  label: string;
  min: number;
  max: number;
  step: number;
  fmt: 'pct' | 'money' | 'num';
}

interface TaxMeta {
  kind: TaxKind;
  label: string;
  help: string;
  params: ParamDef[];
  make: () => TaxSpec;
}

const pct = (key: string, label: string, max: number, step = 0.001): ParamDef => ({ key, label, min: 0, max, step, fmt: 'pct' });
const money = (key: string, label: string, max: number, step: number): ParamDef => ({ key, label, min: 0, max, step, fmt: 'money' });

export const TAX_META: TaxMeta[] = [
  {
    kind: 'land_value',
    label: 'Land value tax',
    help: 'Annual % of assessed land value (as if vacant). Independent of what is built: idle land costs the same as used land. Capitalises into lower land prices.',
    params: [pct('rate', 'rate / year', 0.15)],
    make: () => ({ kind: 'land_value', rate: 0.04 }),
  },
  {
    kind: 'property',
    label: 'Property tax',
    help: 'Annual % of assessed land and building value. Set the rates apart for a split-rate tax. The building part penalises building and maintaining.',
    params: [pct('land_rate', 'land rate / year', 0.1), pct('building_rate', 'building rate / year', 0.1)],
    make: () => ({ kind: 'property', land_rate: 0.01, building_rate: 0.01 }),
  },
  {
    kind: 'income',
    label: 'Income tax',
    help: 'Progressive brackets on annual household income. Affects whether people work, where they live and whether they stay.',
    params: [],
    make: () => ({ kind: 'income', brackets: [{ threshold: 0, rate: 0 }, { threshold: 20000, rate: 0.25 }], include_capital: true }),
  },
  {
    kind: 'wealth',
    label: 'Wealth tax',
    help: 'Annual % of net worth above the threshold, payable in cash: asset-rich owners may have to sell; the mobile rich may leave.',
    params: [money('threshold', 'exempt up to', 5_000_000, 50_000), pct('rate', 'rate / year', 0.1)],
    make: () => ({ kind: 'wealth', threshold: 1_000_000, rate: 0.02 }),
  },
  {
    kind: 'sales',
    label: 'Sales tax / VAT',
    help: 'Added to consumer prices. Reduces real consumption and local shop revenue.',
    params: [pct('rate', 'rate', 0.4, 0.005)],
    make: () => ({ kind: 'sales', rate: 0.1 }),
  },
  {
    kind: 'corporate',
    label: 'Corporate tax',
    help: 'Tax on firm profits. Lowers the payoff of starting firms and the rent firms can bid for space.',
    params: [pct('rate', 'rate', 0.6, 0.005)],
    make: () => ({ kind: 'corporate', rate: 0.2 }),
  },
  {
    kind: 'payroll',
    label: 'Payroll tax',
    help: 'Employer tax on wages: raises the cost of each worker.',
    params: [pct('rate', 'rate', 0.4, 0.005)],
    make: () => ({ kind: 'payroll', rate: 0.1 }),
  },
  {
    kind: 'transfer',
    label: 'Transfer tax',
    help: 'Stamp duty on property sales, paid by the buyer. Blocks trades that would otherwise happen (lock-in).',
    params: [pct('rate', 'rate', 0.2, 0.0025)],
    make: () => ({ kind: 'transfer', rate: 0.03 }),
  },
  {
    kind: 'harberger',
    label: 'Harberger tax (COST)',
    help: 'Owners self-assess their property and pay a % of that value; anyone may buy at the declared price.',
    params: [pct('rate', 'rate / year', 0.2)],
    make: () => ({ kind: "harberger", rate: 0.04 }),
  },
  {
    kind: 'pollution',
    label: 'Pollution tax',
    help: 'Per unit of industrial emissions. Firms abate when cheaper than paying.',
    params: [money('per_unit', 'per unit / month', 3000, 25)],
    make: () => ({ kind: 'pollution', per_unit: 500 }),
  },
  {
    kind: 'vacancy',
    label: 'Vacancy tax',
    help: 'Annual % of the value of empty dwellings and idle lots.',
    params: [pct('rate', 'rate / year', 0.2)],
    make: () => ({ kind: 'vacancy', rate: 0.03 }),
  },
  {
    kind: 'estate',
    label: 'Estate tax',
    help: 'Tax on estates above the threshold when a household head dies.',
    params: [money('threshold', 'exempt up to', 5_000_000, 50_000), pct('rate', 'rate', 0.9, 0.01)],
    make: () => ({ kind: 'estate', threshold: 500_000, rate: 0.4 }),
  },
  {
    kind: 'head',
    label: 'Head (poll) tax',
    help: 'Same amount for every household each month. Distorts no margin, but is highly regressive.',
    params: [money('amount', 'per household / month', 2000, 10)],
    make: () => ({ kind: 'head', amount: 300 }),
  },
];

const SPENDING: { key: keyof SpendingPolicy; label: string; help: string }[] = [
  { key: 'services', label: 'Public services', help: 'Schools and clinics: public jobs, service quality raises nearby land values.' },
  { key: 'infrastructure', label: 'Roads', help: 'Builds new streets when serviced land runs short and the land value a street unlocks exceeds its cost (buying the land on its route), and maintains them.' },
  { key: 'parks', label: 'Parks', help: 'Buys land at market price and turns it into parks (amenity).' },
  { key: 'dividend', label: 'Citizen dividend', help: 'Equal payment to every household (UBI).' },
  { key: 'welfare', label: 'Welfare', help: 'Means-tested support with 50% withdrawal, extra for the unhoused.' },
];

function fmt(v: number, f: ParamDef['fmt']): string {
  if (f === 'pct') return `${(v * 100).toFixed(v < 0.1 ? 1 : 0)}%`;
  if (f === 'money') return v >= 1e6 ? `${(v / 1e6).toFixed(2)}M` : v >= 1e3 ? `${(v / 1e3).toFixed(0)}k` : v.toFixed(0);
  return v.toFixed(2);
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Record<string, string> = {}, text = ''): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (text) e.textContent = text;
  return e;
}

function slider(label: string, value: number, def: Omit<ParamDef, 'key' | 'label'>, onInput: (v: number) => void, help = ''): HTMLElement {
  const row = el('label', { class: 'slider', title: help });
  const name = el('span', { class: 'slider-label' }, label);
  const out = el('span', { class: 'slider-value' }, fmt(value, def.fmt));
  const input = el('input', { type: 'range', min: String(def.min), max: String(def.max), step: String(def.step) });
  input.value = String(value);
  input.addEventListener('input', () => {
    const v = Number(input.value);
    out.textContent = fmt(v, def.fmt);
    onInput(v);
  });
  row.append(name, input, out);
  return row;
}

export class PolicyPanel {
  policy!: Policy;
  private presets: Preset[] = [];
  onChange: (p: Policy) => void = () => {};
  private debounce?: number;

  constructor(private root: HTMLElement) {}

  setPresets(presets: Preset[]) {
    this.presets = presets;
  }

  setPolicy(p: Policy) {
    this.policy = structuredClone(p);
    this.render();
  }

  private changed() {
    clearTimeout(this.debounce);
    this.debounce = window.setTimeout(() => this.onChange(structuredClone(this.policy)), 150);
  }

  render() {
    const root = this.root;
    root.innerHTML = '';

    // presets
    const presetRow = el('div', { class: 'preset-row' });
    const sel = el('select');
    sel.append(el('option', { value: '' }, 'Load preset…'));
    for (const p of this.presets) sel.append(el('option', { value: p.name, title: p.description }, p.name.replace(/_/g, ' ')));
    sel.addEventListener('change', () => {
      const p = this.presets.find((x) => x.name === sel.value);
      if (p) {
        this.setPolicy(p.policy);
        this.onChange(structuredClone(this.policy));
      }
    });
    presetRow.append(sel);
    root.append(presetRow);

    // taxes
    root.append(el('h3', {}, 'Taxes'));
    this.policy.taxes.forEach((t, idx) => root.append(this.taxCard(t, idx)));
    const add = el('select', { class: 'add-tax' });
    add.append(el('option', { value: '' }, '+ Add tax…'));
    for (const m of TAX_META) add.append(el('option', { value: m.kind, title: m.help }, m.label));
    add.addEventListener('change', () => {
      const m = TAX_META.find((x) => x.kind === add.value);
      if (m) {
        this.policy.taxes.push(m.make());
        this.render();
        this.changed();
      }
    });
    root.append(add);

    // spending
    root.append(el('h3', {}, 'Spending mix'));
    const sp = this.policy.spending;
    const total = SPENDING.reduce((s, d) => s + (sp[d.key] as number), 0) || 1;
    for (const d of SPENDING) {
      const share = (sp[d.key] as number) / total;
      root.append(
        slider(d.label, sp[d.key] as number, { min: 0, max: 1, step: 0.01, fmt: 'num' }, (v) => {
          (sp[d.key] as number) = v;
          this.changed();
        }, `${d.help} (currently ${(share * 100).toFixed(0)}% of the budget)`),
      );
    }

    // rules
    root.append(el('h3', {}, 'Rules'));
    const zoning = el('label', { class: 'check', title: 'Forbid housing within 2 cells of industry and vice versa.' });
    const zbox = el('input', { type: 'checkbox' });
    zbox.checked = this.policy.zoning;
    zbox.addEventListener('change', () => {
      this.policy.zoning = zbox.checked;
      this.changed();
    });
    zoning.append(zbox, el('span', {}, ' Separate housing from industry (zoning)'));
    root.append(zoning);
    root.append(
      slider('Height limit (0 = none)', this.policy.height_limit, { min: 0, max: 40, step: 1, fmt: 'num' }, (v) => {
        this.policy.height_limit = v;
        this.changed();
      }),
    );
    root.append(
      slider(
        'Land purchase premium',
        sp.purchase_premium ?? 0.25,
        { min: 0, max: 1, step: 0.05, fmt: 'pct' },
        (v) => {
          sp.purchase_premium = v;
          this.changed();
        },
        'What the government pays above market value when it buys land for roads, parks and civic buildings. No land is reserved in advance: a costlier buy-out makes the road planner route around buildings and build less.',
      ),
    );
  }

  private taxCard(t: TaxSpec, idx: number): HTMLElement {
    const meta = TAX_META.find((m) => m.kind === t.kind)!;
    const card = el('div', { class: 'tax-card' });
    const head = el('div', { class: 'tax-head' });
    head.append(el('span', { class: 'tax-name', title: meta.help }, meta.label));
    const rm = el('button', { class: 'icon', title: 'Remove' }, '×');
    rm.addEventListener('click', () => {
      this.policy.taxes.splice(idx, 1);
      this.render();
      this.changed();
    });
    head.append(rm);
    card.append(head, el('div', { class: 'tax-help' }, meta.help));
    const rec = t as unknown as Record<string, number>;
    for (const p of meta.params) {
      card.append(
        slider(p.label, rec[p.key], p, (v) => {
          rec[p.key] = v;
          this.changed();
        }),
      );
    }
    if (t.kind === 'income') {
      t.brackets.forEach((b, bi) => {
        const row = el('div', { class: 'bracket' });
        row.append(
          slider(`above ${fmt(b.threshold, 'money')}`, b.rate, { min: 0, max: 0.9, step: 0.01, fmt: 'pct' }, (v) => {
            b.rate = v;
            this.changed();
          }),
        );
        const th = el('input', { type: 'number', min: '0', step: '1000', class: 'threshold', title: 'annual income threshold' });
        th.value = String(b.threshold);
        th.addEventListener('change', () => {
          b.threshold = Number(th.value);
          t.brackets.sort((a, c) => a.threshold - c.threshold);
          this.render();
          this.changed();
        });
        const del = el('button', { class: 'icon', title: 'Remove bracket' }, '−');
        del.addEventListener('click', () => {
          t.brackets.splice(bi, 1);
          this.render();
          this.changed();
        });
        row.append(th, del);
        card.append(row);
      });
      const addB = el('button', { class: 'small' }, '+ bracket');
      addB.addEventListener('click', () => {
        const last = t.brackets[t.brackets.length - 1];
        t.brackets.push({ threshold: (last?.threshold ?? 0) + 30000, rate: Math.min(0.9, (last?.rate ?? 0.2) + 0.1) });
        this.render();
        this.changed();
      });
      const cap = el('label', { class: 'check' });
      const cb = el('input', { type: 'checkbox' });
      cb.checked = t.include_capital;
      cb.addEventListener('change', () => {
        t.include_capital = cb.checked;
        this.changed();
      });
      cap.append(cb, el('span', {}, ' also tax rents, dividends & interest'));
      card.append(addB, cap);
    }
    return card;
  }
}
