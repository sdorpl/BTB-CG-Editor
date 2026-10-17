#!/usr/bin/env node
// Jakość konwersji ręcznych szablonów (kod HTML/CSS/JS -> scena): dla każdego elementu bez sceny renderuje
// oryginał, konwertuje (SceneConverter z BTB-CG), renderuje wynik i liczy różnicę pikseli w stanie końcowym.
//   node tools/convert-quality.js [katalog-BTB-CG] [plik.json]     (domyślnie db.json z BTB-CG)
//   ONLY=regex  filtruje po nazwie;  SAVE=katalog  zapisuje zrzuty old/new;  JSON=plik  zapisuje wyniki
const fs = require('fs');
const path = require('path');
const os = require('os');
const { createRequire } = require('module');

const root = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const dbFile = process.argv[3] || path.join(root, 'db.json');
const req = createRequire(path.join(root, 'package.json'));
const { chromium } = req('playwright');
const ONLY = process.env.ONLY ? new RegExp(process.env.ONLY, 'i') : null;

const isCode = (e) => !e.scene && ((e.html_template || '').trim() || (e.css_template || '').trim() || (e.js_template || '').trim());

(async () => {
  const db = JSON.parse(fs.readFileSync(dbFile, 'utf8'));
  const all = [...(db.elements || []), ...(db.graphics || []), ...(db.library || [])];
  const seen = new Set();
  const els = all.filter(isCode).filter(e => ONLY ? ONLY.test(e.name || e.id) : true).filter(e => {
    const k = JSON.stringify([e.html_template, e.css_template, e.js_template, e.title, e.fields, e.layout, e.animation]);
    if (seen.has(k)) return false; seen.add(k); return true;
  });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  await page.clock.install({ time: new Date('2026-01-01T12:34:56') }); // zegary pokazują ten sam czas w obu zrzutach
  page.on('pageerror', e => console.log('  [pageerror]', String(e.message).slice(0, 160)));
  await page.setContent('<!doctype html><html><body style="margin:0;background:#00b140"></body></html>');
  await page.addScriptTag({ path: req.resolve('gsap/dist/gsap.min.js') });
  await page.addScriptTag({ path: process.env.HANDLEBARS_JS || req.resolve('handlebars/dist/handlebars.min.js') });
  for (const f of ['cgmotion.js', 'element-model.js', 'scene-model.js', 'renderer.js', 'scene_convert.js']) await page.addScriptTag({ path: path.join(root, f) });

  await page.addScriptTag({ content: require('./convert-bridge.js').INTENT_JS });

  const shot = async (g) => {
    await page.evaluate((g) => {
      document.querySelectorAll('.__t').forEach(n => n.remove());
      const { stage } = window.SceneConverter._render(g, {});
      stage.classList.add('__t'); stage.style.left = '0';
    }, g);
    await page.waitForTimeout(400);
    const buf = await page.screenshot({ clip: { x: 0, y: 0, width: 1920, height: 1080 } });
    await page.evaluate(() => document.querySelectorAll('.__t').forEach(n => n.remove()));
    return buf;
  };
  // Animacja wejścia: render bez zamrażania, oś globalna GSAP wstrzymana i przewijana do chwil `times`.
  const animShots = async (g, times) => {
    const out = [];
    for (const t of times) {
      await page.evaluate((g) => {
        document.querySelectorAll('.__t').forEach(n => n.remove());
        const stage = document.createElement('div');
        stage.className = '__t';
        stage.style.cssText = 'position:fixed;left:0;top:0;width:1920px;height:1080px;overflow:hidden;pointer-events:none;';
        document.body.appendChild(stage);
        const c = JSON.parse(JSON.stringify(g));
        c.id = 'anim' + Math.random().toString(36).slice(2, 8); c.visible = true;
        c.layout = { ...(c.layout || {}), rotation: 0 };
        window.gsap.globalTimeline.pause();
        window.__T0 = window.gsap.globalTimeline.time();
        window.__cgRenderer.renderPreview(stage, [c], {}, {});
      }, g);
      await page.waitForTimeout(150); // renderer tworzy osie asynchronicznie; czas GSAP stoi
      await page.evaluate((t) => { window.gsap.globalTimeline.time(window.__T0 + t, false); }, t);
      out.push(await page.screenshot({ clip: { x: 0, y: 0, width: 1920, height: 1080 } }));
      await page.evaluate(() => { document.querySelectorAll('.__t').forEach(n => n.remove()); window.gsap.globalTimeline.clear(); window.gsap.globalTimeline.resume(); });
    }
    return out;
  };
  const pixelDiff = (a, b) => page.evaluate(async ([a, b]) => {
    const load = (s) => new Promise(r => { const i = new Image(); i.onload = () => r(i); i.src = 'data:image/png;base64,' + s; });
    const [ia, ib] = await Promise.all([load(a), load(b)]);
    const cv = (i) => { const c = document.createElement('canvas'); c.width = 1920; c.height = 1080; const x = c.getContext('2d'); x.drawImage(i, 0, 0); return x.getImageData(0, 0, 1920, 1080).data; };
    const da = cv(ia), db = cv(ib);
    let n = 0, content = 0;
    for (let p = 0; p < da.length; p += 4) {
      const d = Math.abs(da[p] - db[p]) + Math.abs(da[p + 1] - db[p + 1]) + Math.abs(da[p + 2] - db[p + 2]);
      const bgA = da[p] === 0 && da[p + 1] === 177 && da[p + 2] === 64, bgB = db[p] === 0 && db[p + 1] === 177 && db[p + 2] === 64;
      if (!bgA || !bgB) content++;
      if (d > 60) n++;
    }
    return { n, content };
  }, [a.toString('base64'), b.toString('base64')]);

  const results = [];
  if (process.env.SAVE) fs.mkdirSync(process.env.SAVE, { recursive: true });
  for (const el0 of els) {
    const el = JSON.parse(JSON.stringify(el0));
    // dane inne niż domyślne: TITLE=... podmienia tytuł (scena ma go odtworzyć tak samo jak oryginał)
    if (process.env.TITLE !== undefined) { el.title = process.env.TITLE; el.fields = { ...(el.fields || {}), TITLE: process.env.TITLE }; }
    const name = `${el.type || '?'} / ${el.name || el.id}`;
    const r = { name, type: el.type, id: el.id };
    try {
      const before = await shot(el);
      const intentOrig = await page.evaluate((g) => window.BTBIntent.capture(JSON.parse(JSON.stringify(g))), el);
      const times = (process.env.TIMES || '0.15,0.4,0.8,1.2').split(',').map(Number);
      const animBefore = process.env.ANIM ? await animShots(el, times) : null;
      const run = (g) => page.evaluate(async (g) => {
        const r = await window.SceneConverter.convertAsync(g, {});
        if (!r) return null;
        window.SceneConverter.apply(g, r);
        return { g, warnings: r.warnings, stats: r.stats };
      }, g);
      let res = await run(el);
      if (!res) {
        // jak w produkcyjnym moście: niektóre animacje wejścia (np. flip-x) zostawiają element niewidocznym po zamrożeniu GSAP
        const quiet = JSON.parse(JSON.stringify(el));
        const off = { type: 'none', duration: 0, delay: 0 };
        quiet.animation = { ...(quiet.animation || {}), in: off, out: off, loop: undefined };
        delete quiet.animation.text; delete quiet.animation.textOut;
        res = await run(quiet);
        if (res) res.g.animation = el.animation;
      }
      if (!res) { r.status = 'NO-RESULT'; results.push(r); console.log(`✗ ${name}: brak wyniku`); if (process.env.SAVE) save(name, before, null); continue; }
      const after = await shot(res.g);
      const diff = await page.evaluate(async ([a, b]) => {
        const load = (s) => new Promise(r => { const i = new Image(); i.onload = () => r(i); i.src = 'data:image/png;base64,' + s; });
        const [ia, ib] = await Promise.all([load(a), load(b)]);
        const cv = (i) => { const c = document.createElement('canvas'); c.width = 1920; c.height = 1080; const x = c.getContext('2d'); x.drawImage(i, 0, 0); return x.getImageData(0, 0, 1920, 1080).data; };
        const da = cv(ia), db = cv(ib);
        let n = 0, content = 0;
        for (let p = 0; p < da.length; p += 4) {
          const d = Math.abs(da[p] - db[p]) + Math.abs(da[p + 1] - db[p + 1]) + Math.abs(da[p + 2] - db[p + 2]);
          if ((da[p] | da[p + 1] | da[p + 2]) && !(da[p] === 0 && da[p + 1] === 177 && da[p + 2] === 64)) content++;
          if (d > 60) n++;
        }
        return { n, content };
      }, [before.toString('base64'), after.toString('base64')]);
      if (animBefore) {
        const animAfter = await animShots(res.g, times);
        r.anim = [];
        for (let i = 0; i < times.length; i++) { const d = await pixelDiff(animBefore[i], animAfter[i]); r.anim.push({ t: times[i], ratio: d.content ? d.n / d.content : (d.n ? 1 : 0) }); }
        if (process.env.SAVE) times.forEach((t, i) => { const dir = process.env.SAVE; const slug = name.replace(/[^a-z0-9]+/gi, '_'); fs.writeFileSync(path.join(dir, `${slug}_a${t}_old.png`), animBefore[i]); fs.writeFileSync(path.join(dir, `${slug}_a${t}_new.png`), animAfter[i]); });
      }
      if (!diff.content) {
        // oryginał jest niewidoczny po zamrożeniu (np. flip-x): porównujemy z jego wersją bez animacji wejścia
        const quiet = JSON.parse(JSON.stringify(el0));
        const off = { type: 'none', duration: 0, delay: 0 };
        quiet.animation = { ...(quiet.animation || {}), in: off, out: off, loop: undefined };
        delete quiet.animation.text; delete quiet.animation.textOut;
        const d2 = await pixelDiff(await shot(quiet), after);
        diff.n = d2.n; diff.content = d2.content; r.note = 'oryginał niewidoczny w stanie końcowym: porównano z wersją bez animacji wejścia';
      }
      if (!diff.content) {
        // oryginał jest niewidoczny po zamrożeniu (np. flip-x): porównujemy z jego wersją bez animacji wejścia
        const quiet = JSON.parse(JSON.stringify(el0));
        const off = { type: 'none', duration: 0, delay: 0 };
        quiet.animation = { ...(quiet.animation || {}), in: off, out: off, loop: undefined };
        delete quiet.animation.text; delete quiet.animation.textOut;
        const d2 = await pixelDiff(await shot(quiet), after);
        diff.n = d2.n; diff.content = d2.content; r.note = 'oryginał niewidoczny w stanie końcowym: porównano z wersją bez animacji wejścia';
      }
      if (process.env.DYN) {
        const setT = (g) => { const c = JSON.parse(JSON.stringify(g)); c.title = process.env.DYN; c.fields = { ...(c.fields || {}), TITLE: process.env.DYN }; return c; };
        const dd = await pixelDiff(await shot(setT(el0)), await shot(setT(res.g)));
        r.dyn = dd.content ? dd.n / dd.content : (dd.n ? 1 : 0);
      }
      r.ratio = diff.content ? diff.n / diff.content : (diff.n ? 1 : 0);
      r.layers = res.stats.layers; r.warnings = res.warnings;
      // zgodność intencji animacji: efektywny efekt tekstu oryginału vs element po konwersji; ruchy warstw z onShow
      {
        const has = (c) => c && c.type && c.type !== 'none';
        const eq = (x, y) => JSON.stringify(x || null) === JSON.stringify(y || null) || (!has(x) && !has(y));
        const cfg = intentOrig.cfg || {};
        const before = res.g.animation || {};
        const motionsBefore = (res.g.scene.layers || []).filter(l => l.motion && l.motion.in && l.motion.in.type !== 'none').length;
        // jak w produkcji: intencje zmieniają scenę po kompilacji, więc kod jest odtwarzany ze sceny
        const after = await page.evaluate(([g, it]) => { const notes = window.BTBIntent.apply(g, it); if (notes.length) window.SceneModel.applyToElement(g, g.scene, { resetValues: false }); return { g, notes }; }, [res.g, intentOrig]);
        res.g = after.g;
        const a = after.g.animation || {};
        const motionsAfter = (after.g.scene.layers || []).filter(l => l.motion && l.motion.in && l.motion.in.type !== 'none').length;
        r.intent = {
          text: { orig: has(cfg.text) ? cfg.text.type : '-', before: eq(cfg.text, before.text), after: eq(cfg.text, a.text) },
          textOut: { orig: has(cfg.textOut) ? cfg.textOut.type : '-', before: eq(cfg.textOut, before.textOut), after: eq(cfg.textOut, a.textOut) },
          extras: { orig: (intentOrig.extras || []).length, before: motionsBefore, after: motionsAfter },
          notes: after.notes,
        };
      }
      r.moving = el.type === 'TICKER' && /ITEMS_B64/.test(el.html_template || '');
      r.status = (r.moving ? r.ratio < 1.6 : r.ratio < 0.12) ? 'ok' : 'DIFF';
      const ix = r.intent ? `  intencja: tekst ${r.intent.text.orig} ${r.intent.text.before ? '✓' : '✗'}→${r.intent.text.after ? '✓' : '✗'}, wyjście ${r.intent.textOut.orig} ${r.intent.textOut.before ? '✓' : '✗'}→${r.intent.textOut.after ? '✓' : '✗'}, dodatkowe ${r.intent.extras.orig} (warstwy z ruchem ${r.intent.extras.before}→${r.intent.extras.after})` : '';
      console.log(`${r.status === 'ok' ? '✓' : '✗'} ${name}: warstw ${r.layers}, różnica ${(r.ratio * 100).toFixed(1)}%${r.moving ? ' (ticker, ruchomy)' : ''}${r.dyn !== undefined ? `  inne dane: ${(r.dyn * 100).toFixed(1)}%` : ''}${r.anim ? '  animacja: ' + r.anim.map(a => `${a.t}s ${(a.ratio * 100).toFixed(0)}%`).join(' ') : ''}${ix}`);
      res.warnings.forEach(w => console.log('    ! ' + w.slice(0, 150)));
      if (process.env.SAVE) save(name, before, after);
    } catch (e) { r.status = 'ERROR'; r.error = String(e.message).split('\n')[0]; console.log(`✗ ${name}: błąd ${r.error}`); }
    results.push(r);
  }
  function save(name, a, b) {
    const dir = process.env.SAVE; fs.mkdirSync(dir, { recursive: true });
    const slug = name.replace(/[^a-z0-9]+/gi, '_');
    fs.writeFileSync(path.join(dir, slug + '_old.png'), a); if (b) fs.writeFileSync(path.join(dir, slug + '_new.png'), b);
  }
  await browser.close();
  const withIntent = results.filter(r => r.intent);
  const textBefore = withIntent.filter(r => r.intent.text.before && r.intent.textOut.before).length, textAfter = withIntent.filter(r => r.intent.text.after && r.intent.textOut.after).length;
  console.log(`\nintencja efektów tekstu zgodna z oryginałem: przed ${textBefore}/${withIntent.length}, po ${textAfter}/${withIntent.length}`);
  let bad = results.filter(r => r.status !== 'ok').length;
  if (textAfter < withIntent.length) { bad++; console.log('✗ intencja efektów tekstu niezgodna z oryginałem po konwersji'); }
  console.log(`\n${results.length} elementów, poza tolerancją: ${bad}`);
  if (process.env.JSON) fs.writeFileSync(process.env.JSON, JSON.stringify(results, null, 1));
  if (bad && process.env.STRICT) process.exit(1);
})();
