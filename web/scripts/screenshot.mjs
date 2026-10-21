// Capture README screenshots: node scripts/screenshot.mjs city.jpg land-value.jpg (after npm run build).
import { chromium } from 'playwright-core';
import { createServer } from 'http';
import { readFile } from 'fs/promises';
import { extname, join, resolve } from 'path';
const root = resolve('dist');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml' };
const server = createServer(async (req, res) => {
  let path = join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (path.endsWith('/')) path = join(path, 'index.html');
  try { const body = await readFile(path); res.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' }); res.end(body); }
  catch { res.writeHead(404); res.end(); }
}).listen(0);
const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium-1194/chrome-linux/chrome', args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
await page.goto(`http://localhost:${server.address().port}/`);
await page.waitForFunction(() => document.getElementById('clock')?.textContent?.startsWith('year'));
await page.selectOption('#baseline', 'georgist');
await page.waitForTimeout(1500);
await page.selectOption('#speed', '60');
await page.click('#play');
await page.waitForFunction(() => parseFloat(document.getElementById('clock').textContent.slice(5)) >= 35, null, { timeout: 400000 });
await page.click('#play');
await page.waitForTimeout(1500);
await page.screenshot({ path: process.argv[2], type: 'jpeg', quality: 82 });
await page.click('#overlay-bar button:text-is("Land value")');
await page.waitForTimeout(800);
await page.screenshot({ path: process.argv[3], type: 'jpeg', quality: 82 });
await browser.close(); server.close();
