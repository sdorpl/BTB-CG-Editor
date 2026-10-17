#!/usr/bin/env node
// Porównanie z oryginałem: ta sama scena w (a) prawdziwym rendererze BTB-CG (GSAP, `--write-preview`)
// i (b) nowej stronie btb-motion/WASM, w tych samych chwilach IN i OUT. Porównuje pozycję i rozmiar
// każdej warstwy (względem stanu spoczynku) oraz efektywną przezroczystość.
//   node tools/e2e-gsap-compare.js [katalog-BTB-CG]      STRICT=1 -> kod wyjścia 1 przy przekroczeniu progów
const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync, execFileSync } = require('child_process');
const { createRequire } = require('module');

const repo = path.join(__dirname, '..');
const btbCg = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const req = createRequire(path.join(btbCg, 'package.json'));
const { chromium } = req('playwright');
const gsapJs = fs.readFileSync(req.resolve('gsap/dist/gsap.min.js'), 'utf8');
const hbJs = fs.readFileSync(req.resolve('handlebars/dist/handlebars.min.js'), 'utf8');
const targetDir = JSON.parse(execSync('cargo metadata --format-version 1 --no-deps', { cwd: repo })).target_directory;
const bin = path.join(targetDir, 'debug', 'btb-cgeditor');
const dir = path.join(targetDir, process.env.E2E_SET || 'e2e');
const ONLY = process.env.ONLY ? new RegExp(process.env.ONLY) : null;
const POS_TOL = Number(process.env.POS_TOL || 3), OP_TOL = Number(process.env.OP_TOL || 0.05);
const CLIP_TOL = 0.03, BLUR_TOL = 1.5;

// Co mierzymy na stronie: prostokąty warstw i efektywna przezroczystość.
const MEASURE = () => {
  const out = {};
  // strona podglądu skaluje scenę (#stage) i ma pasek narzędzi: przeliczamy na piksele sceny 1920x1080
  const st = document.getElementById('stage');
  const sr = st ? st.getBoundingClientRect() : { x: 0, y: 0, width: 1920 };
  const k = sr.width / 1920;
  const metric = (el) => {
    const q = el.getBoundingClientRect();
    const r = { x: (q.x - sr.x) / k, y: (q.y - sr.y) / k, width: q.width / k, height: q.height / k };
    let op = 1;
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) op *= parseFloat(getComputedStyle(e).opacity);
    const vis = (() => { for (let e = el; e; e = e.parentElement) if (getComputedStyle(e).visibility === 'hidden' || getComputedStyle(e).display === 'none') return false; return true; })();
    // przycięcie (clip-path: inset) i rozmycie, kumulowane po przodkach: [góra, prawa, dół, lewa] jako ułamki
    const clip = [0, 0, 0, 0]; let blur = 0;
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      const cs = getComputedStyle(e);
      const m = /inset\(([^)]*?)(?:\s+round[^)]*)?\)/.exec(cs.clipPath || '');
      if (m) {
        const v = m[1].trim().split(/\s+/);
        const full = [e.offsetHeight || 1, e.offsetWidth || 1, e.offsetHeight || 1, e.offsetWidth || 1];
        const four = [v[0], v[1] ?? v[0], v[2] ?? v[0], v[3] ?? v[1] ?? v[0]].map((x, i) => x.endsWith('%') ? parseFloat(x) / 100 : parseFloat(x) / full[i]);
        for (let i = 0; i < 4; i++) clip[i] = Math.max(clip[i], four[i] || 0);
      }
      const b = /blur\(([\d.]+)px\)/.exec(cs.filter || '');
      if (b) blur += parseFloat(b[1]);
    }
    return { x: r.x, y: r.y, w: r.width, h: r.height, op: vis ? op : 0, clip, blur };
  };
  document.querySelectorAll('[data-layer]').forEach(el => { out[el.getAttribute('data-layer')] = metric(el); });
  // jednostki tekstu: element tekstu i jego dzieci-spany (znaki / słowa), plus sama treść
  document.querySelectorAll('[data-cg-text]').forEach(t => {
    const lid = t.closest('[data-layer]').getAttribute('data-layer');
    [t, ...t.querySelectorAll(':scope > span')].forEach((el, i) => { out[`${lid}:t${i}`] = metric(el); });
    out[`${lid}:txt`] = { txt: t.textContent };
  });
  return out;
};

function diff(a, b, rest, ignoreTxt) {
  let worst = { pos: 0, op: 0, clip: 0, blur: 0, miss: 0, txt: 0, where: '' };
  for (const id of Object.keys(a)) {
    if (a[id].txt !== undefined) { if (!ignoreTxt && b[id] && a[id].txt !== b[id].txt) worst.txt++; continue; }
    if (!b[id]) { worst.miss++; continue; }
    // jednostki powstałe dopiero po pomiarze spoczynku (podział tekstu na wyjściu): porównanie bezwzględne
    const zero = { x: 0, y: 0, w: 0, h: 0 };
    const ra = rest.a[id] || zero, rb = rest.b[id] || zero;
    const da = { x: a[id].x - ra.x, y: a[id].y - ra.y, w: a[id].w - ra.w, h: a[id].h - ra.h };
    const db = { x: b[id].x - rb.x, y: b[id].y - rb.y, w: b[id].w - rb.w, h: b[id].h - rb.h };
    const pos = Math.max(...['x', 'y', 'w', 'h'].map(k => Math.abs(da[k] - db[k])));
    const op = Math.abs(a[id].op - b[id].op);
    const clip = Math.max(...a[id].clip.map((v, i) => Math.abs(v - b[id].clip[i])));
    worst.clip = Math.max(worst.clip, clip);
    worst.blur = Math.max(worst.blur, Math.abs(a[id].blur - b[id].blur));
    if (pos > worst.pos) worst.pos = pos, worst.where = id;
    if (op > worst.op) worst.op = op;
  }
  for (const id of Object.keys(b)) if (!(id in a)) worst.miss++;
  return worst;
}

(async () => {
  const browser = await chromium.launch();
  const files = fs.readdirSync(dir).filter(f => f.endsWith('.element.json') && !/^(keys|path)_/.test(f) && (!ONLY || ONLY.test(f))); // klucze użytkownika nie mają odpowiednika w GSAP
  let bad = 0;
  console.log(`tolerancje: pozycja ${POS_TOL}px, przezroczystość ${OP_TOL}\n`);
  for (const f of files) {
    const name = f.replace('.element.json', '');
    const mine = path.join(dir, `${name}.motion.html`), ref = path.join(dir, `${name}.ref.html`);
    // jeśli istnieje wersja ze skonwertowanymi presetami (klucze), to ona trafia na stronę silnika, a referencją GSAP jest oryginał
    const conv = path.join(dir, `${name}.converted.json`);
    execFileSync(bin, ['--write-motion-html', fs.existsSync(conv) ? conv : path.join(dir, f), mine, '0', 'manual']);
    execFileSync(bin, ['--write-preview', path.join(dir, f), ref, '0'], { env: { ...process.env, BTB_CG_DIR: btbCg } });

    // (b) nasza strona
    const pb = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
    await pb.goto('file://' + mine);
    await pb.waitForFunction(() => window.btb);
    const tIn = await pb.evaluate(() => btb.cfg.director.events.find(e => e.kind === 'stop').t);
    const tOut = await pb.evaluate(() => btb.cfg.director.length) - tIn;

    // (a) oryginał z GSAP, czas globalny wstrzymany i sterowany ręcznie
    const pa = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
    await pa.route('**/*', r => {
      const u = r.request().url();
      if (u.includes('gsap.min.js')) return r.fulfill({ contentType: 'text/javascript', body: gsapJs });
      if (u.includes('handlebars')) return r.fulfill({ contentType: 'text/javascript', body: hbJs });
      if (u.startsWith('file:')) return r.continue();
      return r.abort();
    });
    await pa.addInitScript(() => { window.addEventListener('DOMContentLoaded', () => { gsap.globalTimeline.pause(); }); });
    await pa.goto('file://' + ref);
    await pa.waitForTimeout(1500); // auto-TAKE po 500 ms; czas GSAP stoi
    // renderer usuwa scenę po wyjściu w czasie rzeczywistym (setTimeout ~ długość OUT); przy sterowaniu ręcznym
    // pomiary trwają dłużej, więc długie timery wstrzymujemy
    await pa.evaluate(() => { const o = window.setTimeout.bind(window); window.setTimeout = (fn, ms, ...a) => (ms >= 300 ? 0 : o(fn, ms, ...a)); });
    // gsap.set(..., {clearProps}) to zero-długościowy tween w osi globalnej: przy przewijaniu odpala się
    // ponownie i kasuje transformacje (w prawdziwym odtwarzaniu wykonuje się raz, przy tworzeniu).
    const killSets = () => pa.evaluate(() => gsap.globalTimeline.getChildren(false, true, true).filter(c => c.duration() === 0 && c.vars && c.vars.clearProps !== undefined).forEach(c => c.kill()));
    await killSets();
    const T0 = await pa.evaluate(() => gsap.globalTimeline.time());
    await pa.evaluate((t) => { gsap.globalTimeline.time(t, false); gsap.globalTimeline.time(t + 0.001, false); }, T0); // rozgrzewka: pierwszy render tweenów
    const scrubA = (t) => pa.evaluate((t) => { gsap.globalTimeline.time(t, false); return (window.__m = null, null); }, T0 + t);
    const measA = async (t) => { await scrubA(t); return pa.evaluate(MEASURE); };

    const rows = [];
    const ignoreTxt = /scramble/.test(name);
    // chwile = wielokrotności klatki 1/60, dobrane tak, by nie wypadały na granicach kroków animacji skokowych (glitch)
    const isText = name.startsWith('t_');
    const snap = (x) => Math.max(1, Math.round(x * 60)) / 60;
    const times = (isText ? [0.12, 0.27, 0.41, 0.56, 0.7, 0.85, 0.97].map(f => snap(f * tIn)) : [4, 9, 14, 20, 27, 36, 48].map(n => n / 60)).filter(t => t < tIn - 0.02);
    // spoczynek po stronie WASM: stan na końcu IN (oś GSAP przewijamy tylko do przodu, żeby nie odpalać setów wstecz)
    await pb.evaluate(() => btb.take());
    await pb.evaluate((t) => { for (let i = 0; i < Math.round(t * 60); i++) btb.tick(1 / 60); }, tIn - 1 / 60 + 0.001);
    const restB = await pb.evaluate(MEASURE);
    // IN
    await pb.evaluate(() => btb.take());
    let tb = 0;
    const inSamples = [];
    for (const t of times) {
      const steps = Math.round((t - tb) * 60); tb += steps / 60;
      await pb.evaluate((n) => { for (let i = 0; i < n; i++) btb.tick(1 / 60); }, steps);
      inSamples.push({ t: tb, a: await measA(tb), b: await pb.evaluate(MEASURE) });
    }
    const restA = await measA(tIn - 0.001);
    const rest = { a: restA, b: restB };
    for (const { t, a, b } of inSamples) {
      if (process.env.RAW) { const id = Object.keys(a)[0]; const f = o => ['x', 'y', 'w', 'h', 'op'].map(k => (+o[k]).toFixed(1)).join(' '); console.log(`   RAW IN t=${t.toFixed(3)} ref[${f(a[id])}] mine[${f(b[id])}] restA[${f(restA[id])}] restB[${f(restB[id])}]`); }
      rows.push(['IN ', t, diff(a, b, rest, ignoreTxt)]);
    }
    // HOLD: po Stop (bez pętli) stan musi pozostać taki jak na końcu IN; łapie wyciek stanu początkowego OUT w hold
    const hasLoop = await pb.evaluate(() => (btb.cfg.director.children || []).length > 0);
    const blinks = /caret/.test(name); // kursor z założenia miga w hold
    if (!hasLoop && !blinks) {
      await pb.evaluate((t) => { for (let i = 0; i < Math.round(t * 60) + 2; i++) btb.tick(1 / 60); }, tIn - tb + 0.5);
      const heldB = await pb.evaluate(MEASURE);
      const dh = diff(restB, heldB, { a: restB, b: restB }, true);
      rows.push(['HOLD', tIn + 0.5, dh]);
    }
    // OUT
    if (hasLoop || blinks) await pb.evaluate((t) => { for (let i = 0; i < Math.round(t * 60) + 2; i++) btb.tick(1 / 60); }, tIn - tb);
    await measA(tIn + 0.0005);
    await pa.evaluate(() => off());
    await pa.waitForTimeout(150);
    await killSets();
    // oś globalna jest przycinana do czasu końca ostatniego tweenu, więc start OUT odczytujemy z niej, zamiast zakładać T0+tIn
    const Tout = await pa.evaluate(() => gsap.globalTimeline.time());
    await pb.evaluate(() => btb.off());
    if (process.env.RAW) console.log('   RAW off: T0+tIn=', (T0 + tIn).toFixed(4), JSON.stringify(await pa.evaluate(() => ({ now: +gsap.globalTimeline.time().toFixed(4), kids: gsap.globalTimeline.getChildren(false, true, true).map(c => [c.constructor.name, +c.startTime().toFixed(4), +c.duration().toFixed(3)]) }))));
    tb = 0;
    for (const t of (isText ? [0.12, 0.3, 0.5, 0.7, 0.9].map(f => snap(f * tOut)) : [4, 9, 14, 20, 24].map(n => n / 60)).filter(t => t < tOut - 0.02)) {
      const steps = Math.round((t - tb) * 60); tb += steps / 60;
      await pb.evaluate((n) => { for (let i = 0; i < n; i++) btb.tick(1 / 60); }, steps);
      await pa.evaluate((t) => { gsap.globalTimeline.time(t, false); }, Tout + tb);
      const a = await pa.evaluate(MEASURE), b = await pb.evaluate(MEASURE);
      // oś OUT oryginału już się skończyła i renderer usunął scenę (np. efekt na całym elemencie krótszy niż plan)
      if (!Object.keys(a).length) continue;
      if (process.env.RAW) { const id = Object.keys(a)[0]; const f = o => ['x', 'y', 'w', 'h', 'op'].map(k => (+o[k]).toFixed(1)).join(' '); console.log(`   RAW OUT t=${tb.toFixed(3)} ref[${f(a[id])}] mine[${f(b[id])}]`); }
      if (process.env.DOM) console.log('   DOM ref:', JSON.stringify(await pa.evaluate(() => { const sc = document.querySelector('.cg-scene'); const t = document.querySelector('[data-cg-text]'); return { scene: !!sc, texts: document.querySelectorAll('[data-cg-text]').length, tHtml: t ? t.innerHTML.slice(0, 160) : null, vis: sc ? getComputedStyle(sc).visibility + '/' + getComputedStyle(sc).display + '/' + getComputedStyle(sc).opacity : null }; })));
      if (process.env.UNIT) { const k = Object.keys(a).find(q => q.endsWith(':t0')); const f = o => o ? ['x', 'y', 'w', 'h', 'op'].map(z => (+o[z]).toFixed(1)).join(' ') : '-'; console.log(`   UNIT OUT t=${tb.toFixed(3)} ref[${f(a[k])}] mine[${f(b[k])}]`); }
      if (process.env.KEYS) console.log('   KEYS ref:', Object.keys(a).filter(k => k.includes(':t')).length, 'mine:', Object.keys(b).filter(k => k.includes(':t')).length, JSON.stringify(Object.keys(a).filter(k=>k.includes(':txt')).map(k => a[k].txt)), JSON.stringify(Object.keys(b).filter(k=>k.includes(':txt')).map(k => b[k].txt)));
      rows.push(['OUT', tb, diff(a, b, rest, ignoreTxt)]);
    }
    const maxMiss = Math.max(...rows.map(r => r[2].miss)), maxTxt = Math.max(...rows.map(r => r[2].txt));
    const maxPos = Math.max(...rows.map(r => r[2].pos)), maxOp = Math.max(...rows.map(r => r[2].op));
    const maxClip = Math.max(...rows.map(r => r[2].clip)), maxBlur = Math.max(...rows.map(r => r[2].blur));
    const over = (w) => w.pos > POS_TOL || w.op > OP_TOL || w.clip > CLIP_TOL || w.blur > BLUR_TOL || w.miss > 0 || w.txt > 0;
    const ok = !rows.some(r => over(r[2]));
    if (!ok) bad++;
    console.log(`${ok ? 'ok  ' : 'DIFF'} ${name.padEnd(22)} poz ${maxPos.toFixed(1)}px  op ${maxOp.toFixed(3)}  clip ${maxClip.toFixed(3)}  blur ${maxBlur.toFixed(1)}${maxMiss ? '  BRAK-JEDNOSTEK ' + maxMiss : ''}${maxTxt ? '  TEKST ' + maxTxt : ''}`);
    if (process.env.VERBOSE) for (const [ph, t, w] of rows) if (over(w) || process.env.VERBOSE === '2') console.log(`      ${ph} t=${t.toFixed(3)}  pos ${w.pos.toFixed(2)} op ${w.op.toFixed(3)} clip ${w.clip.toFixed(3)} blur ${w.blur.toFixed(1)} miss ${w.miss} txt ${w.txt}`);
    await pa.close(); await pb.close();
  }
  await browser.close();
  console.log(bad ? `\n${bad} elementów przekracza progi` : '\nWszystkie zgodne z GSAP w tolerancji');
  if (bad && process.env.STRICT) process.exit(1);
})().catch(e => { console.error(e); process.exit(1); });
