#!/usr/bin/env node
// D10 po stronie edytora: strona wyjściowa (--write-motion-html) osadza krój z pakietu fontów użyty w elemencie (Oswald 600) i tylko ten.
//   node tools/e2e-fonts.js [katalog-BTB-CG]    (wymaga: cargo test export_e2e_elements; cargo build)
const fs = require('fs'), path = require('path'), os = require('os');
const { execSync, execFileSync } = require('child_process');
const { createRequire } = require('module');
const repo = path.join(__dirname, '..');
const btbCg = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const { chromium } = createRequire(path.join(btbCg, 'package.json'))('playwright');
const targetDir = JSON.parse(execSync('cargo metadata --format-version 1 --no-deps', { cwd: repo })).target_directory;
const bin = path.join(targetDir, 'debug', 'btb-cgeditor'), dir = path.join(targetDir, 'e2e');
const src = path.join(dir, 'font_oswald.fonts.json'), html = path.join(dir, 'font_oswald.motion.html');
let failed = 0;
const check = (ok, msg) => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${msg}`); if (!ok) failed++; };
(async () => {
  execFileSync(bin, ['--write-motion-html', src, html, '0', 'manual']);
  const text = fs.readFileSync(html, 'utf8');
  check(/font-family: 'Oswald'; font-style: normal; font-weight: 600/.test(text), 'strona zawiera @font-face Oswald 600');
  check(!/font-family: 'Montserrat'/.test(text) && !/font-family: 'Inter'/.test(text), 'strona nie osadza nieużywanych rodzin');
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  const external = [];
  await page.route(/^https?:/, (rt) => { external.push(rt.request().url()); rt.abort(); });
  await page.goto('file://' + html);
  await page.waitForFunction(() => window.btb || window.btbError, null, { timeout: 10000 });
  const r = await page.evaluate(async () => {
    await document.fonts.load('600 40px "Oswald"', 'Zażółć gęślą');
    const faces = [...document.fonts].map((f) => `${f.family}:${f.weight}:${f.status}`);
    const t = document.querySelector('[data-cg-text]');
    return { faces, family: t ? getComputedStyle(t).fontFamily : null, weight: t ? getComputedStyle(t).fontWeight : null };
  });
  check(r.faces.some((f) => /Oswald:600:loaded/.test(f)), 'Oswald 600 załadowany z osadzonych danych: ' + r.faces.filter((f) => /Oswald/.test(f)).join(', '));
  check(/Oswald/.test(r.family || '') && String(r.weight) === '600', `element używa Oswald 600 (${r.family} ${r.weight})`);
  check(external.length === 0, 'brak żądań sieciowych: ' + external.join(','));
  await browser.close();
  if (failed) process.exit(1);
  console.log('OK: e2e-fonts');
})().catch((e) => { console.error(e); process.exit(1); });
