#!/usr/bin/env node
// Test end-to-end strony wyjściowej (output-motion.html) w prawdziwym Chromium (Playwright z repo BTB-CG).
//   node tools/e2e-motion.js [katalog-BTB-CG]
// Dla każdego <target>/e2e/*.element.json: generuje stronę (tryb manual = deterministyczny czas), robi
// pełny cykl TAKE -> hold -> OFF -> koniec, sprawdza zdarzenia i stan DOM, zapisuje zrzuty do <target>/e2e/shots.
const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync, execFileSync } = require('child_process');
const { createRequire } = require('module');

const repo = path.join(__dirname, '..');
const btbCg = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const { chromium } = createRequire(path.join(btbCg, 'package.json'))('playwright');
const targetDir = JSON.parse(execSync('cargo metadata --format-version 1 --no-deps', { cwd: repo })).target_directory;
const bin = path.join(targetDir, 'debug', 'btb-cgeditor');
const dir = path.join(targetDir, 'e2e');
const shots = path.join(dir, 'shots');
fs.mkdirSync(shots, { recursive: true });

let failed = 0;
const check = (ok, msg) => { if (!ok) { console.error('  FAIL: ' + msg); failed++; } };

// Klucze użytkownika: element bez presetów, więc wartości wynikają wprost z kluczy
// (tx 0->100 w 1 s, krycie 0->1 w 0,5 s; po Stop (1,2 s) krycie 1->0 w 0,5 s).
async function checkKeys(page, tickTo, check) {
  const read = () => page.evaluate(() => {
    const el = document.querySelector('[data-layer]');
    const m = /translate\(([-\d.e]+)px/.exec(el.style.transform || '');
    return { tx: m ? parseFloat(m[1]) : 0, op: el.style.opacity === '' ? 1 : parseFloat(el.style.opacity) };
  });
  await page.evaluate(() => btb.take());
  const near = (a, b, tol, msg) => check(Math.abs(a - b) <= tol, `${msg}: ${a} != ${b}`);
  await tickTo(0.25); let v = await read();
  near(v.tx, 25, 1.0, 'tx w 0,25 s'); near(v.op, 0.5, 0.03, 'krycie w 0,25 s');
  await tickTo(0.25); v = await read(); // czas osi: 0,5 s
  near(v.tx, 50, 1.0, 'tx w 0,5 s'); near(v.op, 1, 0.03, 'krycie w 0,5 s');
  await tickTo(1.0); v = await read(); // minął Stop (1,2 s): oś stoi, tx doszło do 100
  near(v.tx, 100, 1.0, 'tx w hold'); near(v.op, 1, 0.03, 'krycie w hold');
  await page.evaluate(() => btb.off());
  await tickTo(0.25); v = await read();
  near(v.tx, 100, 1.0, 'tx w OUT (stała po ostatnim kluczu)'); near(v.op, 0.5, 0.05, 'krycie w połowie OUT');
}

// Ścieżka ruchu: prosta (0,0) -> (100,60) w 1 s ze stałą prędkością, potem stoi (Stop w 1,2 s).
async function checkPath(page, tickTo, check) {
  const read = () => page.evaluate(() => {
    const el = document.querySelector('[data-layer]');
    const m = /translate\(([-\d.e]+)px,\s*([-\d.e]+)px/.exec(el.style.transform || '');
    return m ? { x: parseFloat(m[1]), y: parseFloat(m[2]) } : { x: 0, y: 0 };
  });
  const near = (a, b, tol, msg) => check(Math.abs(a - b) <= tol, `${msg}: ${a} != ${b}`);
  await page.evaluate(() => btb.take());
  await tickTo(0.5); let v = await read();
  near(v.x, 50, 1.0, 'x w 0,5 s'); near(v.y, 30, 1.0, 'y w 0,5 s');
  await tickTo(1.0); v = await read(); // minął koniec ścieżki i Stop
  near(v.x, 100, 1.0, 'x po końcu ścieżki'); near(v.y, 60, 1.0, 'y po końcu ścieżki');
}

// Local Stop (M2.6): tx 0 -> 90 w 3 s; Stopy w 1 s i 2 s (continue), ostatni w 3 s; OFF na pośrednim Stopie dogrywa wejście i wychodzi.
async function runStops(browser) {
  const src = path.join(dir, 'stops_pure.stops.json');
  if (!fs.existsSync(src)) { check(false, 'brak stops_pure.stops.json (cargo test export_e2e_elements)'); return; }
  console.log('stops_pure');
  const html = path.join(dir, 'stops_pure.motion.html');
  execFileSync(bin, ['--write-motion-html', src, html, '0', 'manual']);
  const open = async () => {
    const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
    await page.goto('file://' + html);
    await page.waitForFunction(() => window.btb || window.btbError, null, { timeout: 10000 });
    await page.evaluate(() => { window.__ev = []; ['hold', 'continue', 'end'].forEach(e => btb.on(e, (d) => window.__ev.push(e + ':' + ((d && d.name) || '')))); });
    return page;
  };
  const tickTo = (page, t) => page.evaluate((t) => { const n = Math.round(t * 100); for (let i = 0; i < n; i++) btb.tick(0.01); }, t);
  const tx = (page) => page.evaluate(() => { const m = /translate\(([-\d.e]+)px/.exec(document.querySelector('[data-layer]').style.transform || ''); return m ? parseFloat(m[1]) : 0; });
  const ev = (page) => page.evaluate(() => window.__ev.slice());
  const near = (a, b, tol, msg) => check(Math.abs(a - b) <= tol, `${msg}: ${a} != ${b}`);

  // A) Continue krok po kroku
  let page = await open();
  await page.evaluate(() => btb.take());
  await tickTo(page, 1.5);                                  // minął Stop w 1 s: oś stoi w 1 s
  near(await tx(page), 30, 1.0, 'tx na Stopie w 1 s');
  await tickTo(page, 1.0);                                  // stoi dalej
  near(await tx(page), 30, 1.0, 'oś stoi na pośrednim Stopie');
  check((await ev(page)).includes('hold:stop1'), 'zdarzenie hold:stop1, mamy ' + (await ev(page)));
  check(await page.evaluate(() => btb['continue']()), 'continue na Stopie zwraca true');
  await tickTo(page, 1.5);                                  // do Stopu w 2 s
  near(await tx(page), 60, 1.0, 'tx na Stopie w 2 s');
  await page.evaluate(() => btb['continue']());
  await tickTo(page, 1.5);                                  // do ostatniego Stopu w 3 s
  near(await tx(page), 90, 1.0, 'tx na ostatnim Stopie');
  check((await ev(page)).includes('hold:hold'), 'zdarzenie hold:hold, mamy ' + (await ev(page)));
  await page.evaluate(() => btb['continue']());             // z ostatniego Stopu: OUT
  await tickTo(page, 1.0);
  check((await ev(page)).includes('end:'), 'zdarzenie end po continue z ostatniego Stopu, mamy ' + (await ev(page)));
  check(!(await page.evaluate(() => btb['continue']())), 'continue bez Stopu zwraca false');
  await page.close();

  // B) OFF na pośrednim Stopie: dogrywa wejście bez zatrzymań i wychodzi
  page = await open();
  await page.evaluate(() => btb.take());
  await tickTo(page, 1.5);
  await page.evaluate(() => btb.off());
  await tickTo(page, 6.0);
  const e2 = await ev(page);
  check(e2.includes('end:'), 'OFF na pośrednim Stopie kończy się end, mamy ' + e2);
  check(!e2.includes('hold:stop2') || e2.indexOf('end:') > e2.indexOf('hold:stop2'), 'OFF przejeżdża pośrednie Stopy');
  await page.close();
}

(async () => {
  const browser = await chromium.launch();
  const files = fs.readdirSync(dir).filter(f => f.endsWith('.element.json'));
  if (!files.length) { console.error('Brak elementów e2e (uruchom: cargo test export_e2e_elements)'); process.exit(1); }
  for (const f of files) {
    const name = f.replace('.element.json', '');
    const html = path.join(dir, `${name}.motion.html`);
    execFileSync(bin, ['--write-motion-html', path.join(dir, f), html, '0', 'manual']);
    const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
    const errors = [];
    page.on('pageerror', e => errors.push(String(e)));
    page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto('file://' + html);
    await page.waitForFunction(() => window.btb || window.btbError, null, { timeout: 10000 });
    await page.evaluate(() => { document.body.style.background = '#2b3550'; });
    console.log(name);
    check(!(await page.evaluate(() => window.btbError)), 'błąd inicjalizacji: ' + await page.evaluate(() => window.btbError));
    const vis = () => page.evaluate(() => getComputedStyle(document.getElementById('btb-root')).visibility);
    const snap = async (label) => page.screenshot({ path: path.join(shots, `${name}.${label}.png`) });

    check(await vis() === 'hidden', 'przed TAKE element ma być ukryty');
    const events = await page.evaluate(() => { window.__ev = []; ['hold', 'end', 'take', 'off'].forEach(e => btb.on(e, () => window.__ev.push(e))); btb.take(); return window.__ev; });
    check(await vis() === 'visible', 'po TAKE element ma być widoczny');
    const tickTo = (t) => page.evaluate((t) => { const n = Math.round(t * 60); for (let i = 0; i < n; i++) btb.tick(1 / 60); }, t);
    await snap('0.in0');
    await tickTo(0.2); await snap('1.in200ms');
    await tickTo(0.2); await snap('2.in400ms');
    await tickTo(1.0); // przejście przez koniec IN do hold
    const ev1 = await page.evaluate(() => window.__ev.slice());
    check(ev1.includes('hold'), 'brak zdarzenia hold po IN, mamy: ' + ev1);
    await snap('3.hold');
    await tickTo(0.9); await snap('4.hold+0.9s');
    await page.evaluate(() => btb.off());
    await tickTo(0.25); await snap('5.out250ms');
    await tickTo(2.0);
    const ev2 = await page.evaluate(() => window.__ev.slice());
    check(ev2.includes('end'), 'brak zdarzenia end po OUT, mamy: ' + ev2);
    check(await vis() === 'hidden', 'po OUT element ma być ukryty');
    check(errors.length === 0, 'błędy w konsoli: ' + errors.join(' | '));
    // aktualizacja pola i ponowny TAKE
    await page.evaluate(() => { btb.update({ TITLE: 'Zmieniony tytuł' }); btb.take(); });
    await tickTo(1.5);
    const r = await page.evaluate(() => ({ uses: /\{\{\{?TITLE\}?\}\}/.test(btb.cfg.html), txt: document.getElementById('btb-root').innerText }));
    check(!r.uses || r.txt.toLowerCase().includes('zmieniony tytuł'), 'brak zaktualizowanego tekstu: ' + JSON.stringify(r.txt));
    if (name === 'keys_pure') await checkKeys(page, tickTo, check);
    if (name === 'path_pure') await checkPath(page, tickTo, check);
    await page.close();
  }
  await runStops(browser);
  await browser.close();
  if (failed) { console.error(`\n${failed} niepowodzeń`); process.exit(1); }
  console.log(`\nOK (${files.length} elementów, zrzuty: ${shots})`);
})().catch(e => { console.error(e); process.exit(1); });
