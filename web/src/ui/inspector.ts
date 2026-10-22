// Parcel inspector: shows what the simulation knows about a clicked cell.

/* eslint-disable @typescript-eslint/no-explicit-any */
const money = (v: number) =>
  v == null ? '–' : Math.abs(v) >= 1e6 ? `${(v / 1e6).toFixed(2)}M` : Math.abs(v) >= 1e3 ? `${(v / 1e3).toFixed(1)}k` : v.toFixed(0);
const pct = (v: number) => `${(v * 100).toFixed(0)}%`;

function row(k: string, v: string | number, help = '') {
  return `<tr title="${help}"><td>${k}</td><td>${v}</td></tr>`;
}

export function renderInspector(root: HTMLElement, d: any) {
  if (!d) {
    root.innerHTML = '';
    root.hidden = true;
    return;
  }
  root.hidden = false;
  const use = String(d.land_use);
  const owner =
    d.owner.kind === 'resident'
      ? `resident (wealth ${money(d.owner.wealth)}${d.owner.lives_here ? ', lives here' : ''})`
      : d.owner.kind === 'absentee'
        ? 'absentee investor'
        : 'government';
  let html = `<div class="insp-head"><b>${use === 'Construction' ? `Construction → ${d.target_use}` : use}</b>
    <span class="muted">(${Number(d.x).toFixed(0)}, ${Number(d.y).toFixed(0)})</span><button class="icon" data-close>×</button></div><table>`;
  if (d.floors) html += row('Floors', d.floors);
  if (d.units) html += row(use === 'Residential' ? 'Dwellings' : 'Job slots', `${d.occupied} / ${d.units} used`);
  if (use === 'Residential' || use === 'Commercial' || use === 'Industrial') html += row('Asking rent', money(d.asking_rent) + ' / unit');
  if (d.floors) html += row('Condition', pct(d.condition), 'Falls when owners defer maintenance');
  html += row('Owner', owner);
  html += row('Land value', money(d.assessed_land), 'Assessed, as if vacant');
  if (d.assessed_building) html += row('Building value', money(d.assessed_building));
  if (d.declared) html += row('Self-assessed', money(d.declared), 'Harberger declaration');
  html += row('Taxes last month', money(d.tax_last_month));
  if (d.mortgage > 0) html += row('Mortgage', money(d.mortgage));
  if (d.months_held_idle > 0) html += row('Held idle', `${d.months_held_idle} months`, 'Owner judged waiting more profitable than building');
  if (d.owner.kind !== 'government' || d.last_price > 0)
    html += row('Market value', money(d.last_price), d.months_since_sale < 24 ? `sold ${d.months_since_sale} months ago` : 'assessment (no recent sale)');
  if (d.best_development) {
    const b = d.best_development;
    html += row('Best option', `${b.use} × ${b.floors} floors (residual ${money(b.residual)})`, "Owner's most profitable redevelopment");
  }
  html += row('Pollution', d.fields.pollution.toFixed(2));
  html += row('Amenity', money(d.fields.amenity) + ' / mo');
  html += row('Services', pct(d.fields.service));
  html += row('Job distance', d.fields.commute_eff.toFixed(1) + ' cells');
  html += '</table>';
  if (d.firm) {
    const f = d.firm;
    html += `<h4>${f.sector} firm (${f.owner} owned)</h4><table>`;
    html += row('Workers', `${f.workers} (wants ${f.desired})`);
    html += row('Wage / skill', money(f.wage_rate));
    html += row('Revenue', money(f.revenue));
    html += row('Profit after tax', money(f.profit));
    if (f.sector === 'Industry') html += row('Emission intensity', pct(f.emission_intensity));
    html += '</table>';
  }
  if (d.residents?.length) {
    html += `<h4>Residents</h4><table class="residents"><tr><th>skill</th><th>job</th><th>income</th><th>rent</th><th>taxes</th><th>wealth</th></tr>`;
    for (const r of d.residents) {
      html += `<tr><td>${r.skill.toFixed(2)}</td><td>${r.employed ? '✓' : '–'}</td><td>${money(r.income)}</td><td>${money(r.rent)}</td><td>${money(r.taxes)}</td><td>${money(r.wealth)}</td></tr>`;
    }
    html += '</table>';
  }
  root.innerHTML = html;
  root.querySelector('[data-close]')?.addEventListener('click', () => renderInspector(root, null));
}
