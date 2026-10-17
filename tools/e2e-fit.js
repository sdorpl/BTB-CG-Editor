#!/usr/bin/env node
// Auto-szerokość (M2.0) na stronie wyjściowej z WASM: warstwa `autoW` + `fitText` (size/follow) w Chromium.
//   node tools/e2e-fit.js [katalog-BTB-CG]
const fs = require('fs'), path = require('path'), os = require('os');
const { execSync, execFileSync } = require('child_process');
const { createRequire } = require('module');

const repo = path.join(__dirname, '..');
const btbCg = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const { chromium } = createRequire(path.join(btbCg, 'package.json'))('playwright');
const targetDir = JSON.parse(execSync('cargo metadata --format-version 1 --no-deps', { cwd: repo })).target_directory;
const bin = path.join(targetDir, 'debug', 'btb-cgeditor');
const dir = path.join(targetDir, 'e2e-fit');
fs.mkdirSync(dir, { recursive: true });

const font = { family: 'Bahnschrift', size: 30, weight: '700', color: '#ffffff', align: 'left' };
const el = {
  id: 'fit1', name: 'fit1', type: 'BADGE', title: 'NA ŻYWO',
  scene: { version: 1, frame: { w: 300, h: 60 }, layers: [
    { id: 'bg', type: 'shape', name: 'bg', x: 0, y: 0, w: 220, h: 60, fill: { type: 'solid', color: '#c00' }, fitText: { to: 'tx', mode: 'size' } },
    { id: 'tx', type: 'text', name: 'tx', x: 40, y: 0, w: 140, h: 60, field: 'TITLE', defaultText: 'NA ŻYWO', autoW: true, squash: false, font },
    { id: 'dot', type: 'shape', name: 'dot', x: 190, y: 20, w: 20, h: 20, fill: { type: 'solid', color: '#fff' }, fitText: { to: 'tx', mode: 'follow' } },
  ] },
};
fs.writeFileSync(path.join(dir, 'fit.element.json'), JSON.stringify([el]));
execFileSync(bin, ['--write-motion-html', path.join(dir, 'fit.element.json'), path.join(dir, 'fit.html')], { stdio: 'inherit' });

let failed = 0;
const check = (ok, msg) => { if (!ok) { console.error('  FAIL: ' + msg); failed++; } };

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('file://' + path.join(dir, 'fit.html'));
  await page.waitForFunction(() => window.btb || window.btbError, null, { timeout: 10000 });
  check(!(await page.evaluate(() => window.btbError)), 'btbError');
  const read = () => page.evaluate(() => {
    const g = (n) => { const e = document.querySelector('.cg-l' + n); return { w: parseFloat(e.style.width || getComputedStyle(e).width), l: parseFloat(e.style.left || getComputedStyle(e).left) }; };
    return { bg: g(1), tx: g(2), dot: g(3) };
  });
  await page.evaluate(() => btb.update({ TITLE: 'NA ŻYWO' }));
  await page.waitForTimeout(300);
  const a = await read();
  await page.evaluate(() => btb.update({ TITLE: 'NA ŻYWO Z BIAŁEGOSTOKU' }));
  await page.waitForTimeout(300);
  const b = await read();
  const d = b.tx.w - a.tx.w;
  check(d > 50, `tekst szerszy o ${d}`);
  check(Math.abs((b.bg.w - a.bg.w) - d) < 1.5, `tło rośnie o deltę: ${a.bg.w} -> ${b.bg.w} (d=${d})`);
  check(Math.abs((b.dot.l - a.dot.l) - d) < 1.5, `kropka podąża o deltę: ${a.dot.l} -> ${b.dot.l} (d=${d})`);
  check(!errors.length, errors.join('; '));
  await browser.close();
  if (failed) { console.error(`e2e-fit: ${failed} błędów`); process.exit(1); }
  console.log(`OK: e2e-fit (delta ${d.toFixed(1)} px)`);
})().catch((e) => { console.error(e); process.exit(1); });
