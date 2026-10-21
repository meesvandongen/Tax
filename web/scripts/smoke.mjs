// Headless smoke test: serve dist/, load the app, run the simulation for a
// while, assert that it advanced without errors and save screenshots.
// Usage: node scripts/smoke.mjs [outDir] [years]
import { chromium } from 'playwright-core';
import { createServer } from 'http';
import { readFile } from 'fs/promises';
import { extname, join, resolve } from 'path';

const outDir = resolve(process.argv[2] ?? 'smoke-out');
const years = Number(process.argv[3] ?? 15);
const root = resolve('dist');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };
const server = createServer(async (req, res) => {
  let path = join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (path.endsWith('/')) path = join(path, 'index.html');
  try {
    const body = await readFile(path);
    res.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end();
  }
}).listen(0);
const port = server.address().port;

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
});
const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
page.on('worker', (w) => w.on('console', (m) => m.type() === 'error' && errors.push('worker: ' + m.text())));
await page.goto(`http://localhost:${port}/`);
await page.waitForFunction(() => document.getElementById('clock')?.textContent?.startsWith('year'), null, { timeout: 30000 });
await page.selectOption('#speed', '60');
await page.click('#play');
await page.waitForFunction((y) => parseFloat(document.getElementById('clock').textContent.slice(5)) >= y, years, { timeout: 240000 });
await page.click('#play');
await page.waitForTimeout(1500);
const { mkdir } = await import('fs/promises');
await mkdir(outDir, { recursive: true });
await page.screenshot({ path: join(outDir, 'city.png') });
for (const overlay of ['Land value', 'Ownership']) {
  await page.click(`#overlay-bar button:text-is("${overlay}")`);
  await page.waitForTimeout(800);
  await page.screenshot({ path: join(outDir, `overlay-${overlay.toLowerCase().replace(/ /g, '-')}.png`) });
}
await page.click('#overlay-bar button:text-is("City")');

// Comparison mode: a second worker runs the baseline from the same seed.
await page.selectOption('#baseline', 'georgist');
await page.waitForTimeout(1500);
await page.click('#play');
await page.waitForFunction(() => parseFloat(document.getElementById('clock').textContent.slice(5)) >= 6, null, { timeout: 240000 });
await page.click('#play');
await page.waitForTimeout(1200);
const deltas = await page.locator('.delta').count();
if (deltas === 0) errors.push('comparison mode produced no KPI deltas');
// Inspect a parcel in the middle of the view.
await page.mouse.click(750, 500);
await page.waitForTimeout(800);
await page.screenshot({ path: join(outDir, 'compare.png') });
const clock = await page.textContent('#clock');
const kpis = await page.textContent('#kpis');
console.log(clock, '|', kpis?.replace(/\s+/g, ' ').slice(0, 300));
await browser.close();
server.close();
if (errors.length) {
  console.error('page errors:\n' + errors.join('\n'));
  process.exit(1);
}
