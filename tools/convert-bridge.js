#!/usr/bin/env node
// Most Rust ↔ SceneConverter z BTB-CG: konwertuje elementy z kodem ręcznym (html/css/js_template) na scenę.
//   node convert-bridge.js <katalog-BTB-CG>   < elementy.json   > wynik.json
// stdin : tablica elementów (JSON); stdout: [{ ok, element?, warnings, stats, error? }]
// Używa prawdziwego renderera BTB-CG w headless Chromium (Playwright z node_modules BTB-CG),
// więc geometria i style pochodzą z wyrenderowanego DOM — tak samo jak przy konwersji w samym BTB-CG.
const fs = require('fs');
const path = require('path');
const { createRequire } = require('module');

let root, req, chromium, gsapPath, hbPath;
const fail = (msg) => { process.stdout.write(JSON.stringify([{ ok: false, error: msg, warnings: [] }])); process.exit(0); };

// Sprawdza środowisko BTB-CG (uruchamiane tylko przy użyciu jako program, nie przy require z innych narzędzi)
function init() {
    root = path.resolve(process.argv[2] || '.');
    req = createRequire(path.join(root, 'package.json'));
    try { ({ chromium } = req('playwright')); } catch (_) { fail('Brak pakietu playwright w ' + root + ' (npm i w katalogu BTB-CG).'); }
    try { gsapPath = req.resolve('gsap/dist/gsap.min.js'); } catch (_) { fail('Brak pakietu gsap w ' + root); }
    try { hbPath = process.env.HANDLEBARS_JS || req.resolve('handlebars/dist/handlebars.min.js'); } catch (_) { fail('Brak pakietu handlebars w ' + root + ' (npm i --no-save handlebars).'); }
    for (const f of ['cgmotion.js', 'element-model.js', 'scene-model.js', 'renderer.js', 'scene_convert.js'])
        if (!fs.existsSync(path.join(root, f))) fail('To nie jest katalog BTB-CG (brak ' + f + '): ' + root);
}

// Skrypt wstrzykiwany do strony: przechwytuje intencję animacji oryginalnego szablonu, której konwerter DOM
// nie widzi (czyta tylko stan końcowy): efektywną konfigurację `CGMotion.mount` (z domyślnymi efektami tekstu
// zapisanymi w kodzie szablonu) oraz dodatkowe animacje z `onShow` (`CGMotion.build` + `tl.add`) razem z
// elementami docelowymi, które potem przypisujemy warstwom sceny po pokryciu prostokątów.
const INTENT_JS = `
(function (root) {
    'use strict';
    const state = { rec: null };
    function install() {
        const CM = root.CGMotion;
        if (!CM || CM.__intentSpy) return;
        CM.__intentSpy = true;
        const om = CM.mount, ob = CM.build;
        CM.build = function (steps) {
            const tl = ob.apply(this, arguments);
            try { tl.__steps = (steps || []).map(s => Object.assign({}, s)); } catch (e) { /* ignoruj */ }
            return tl;
        };
        CM.mount = function (rootEl, cfg) {
            const rec = state.rec;
            if (rec && cfg) {
                rec.cfg = { animIn: cfg.animIn, animOut: cfg.animOut, text: cfg.text, textSync: cfg.textSync,
                            textOut: cfg.textOut, textOutSync: cfg.textOutSync, loop: cfg.loop };
                if (typeof cfg.onShow === 'function') {
                    const orig = cfg.onShow;
                    cfg = Object.assign({}, cfg, { onShow: function (tl) {
                        const spy = new Proxy(tl, { get(t, k) {
                            if (k === 'add') return function (child, pos) {
                                if (child && child.__steps) rec.extras.push({ steps: child.__steps, pos });
                                return t.add.apply(t, arguments);
                            };
                            const v = t[k];
                            return typeof v === 'function' ? v.bind(t) : v;
                        } });
                        return orig.call(this, spy);
                    } });
                }
            }
            return om.call(this, rootEl, cfg);
        };
    }
    const targetsOf = (stage, t) => typeof t === 'string' ? [...stage.querySelectorAll(t)]
        : (t && t.length !== undefined && !t.getBoundingClientRect ? [...t] : [t]);

    async function capture(el) {
        install();
        const rec = { cfg: null, extras: [] };
        state.rec = rec;
        let rendered;
        try { rendered = root.SceneConverter._render(el, {}); } finally { /* rec zostaje do końca czekania */ }
        await new Promise(r => setTimeout(r, 350));
        state.rec = null;
        const stage = rendered.stage, sr = stage.getBoundingClientRect(), extras = [];
        for (const ex of rec.extras) {
            const base = typeof ex.pos === 'number' ? ex.pos : 0;
            let cursor = 0;
            for (const s of ex.steps) {
                const start = typeof s.at === 'number' ? s.at : cursor;
                const dur = s.duration === undefined ? 0.5 : s.duration;
                cursor = start + dur;
                for (const t of targetsOf(stage, s.target)) {
                    if (!t || !t.getBoundingClientRect) continue;
                    const r = t.getBoundingClientRect();
                    extras.push({ type: s.type || 'fade', direction: s.direction, duration: dur, delay: base + start,
                                  ease: s.ease, cls: String(t.className || '').split(/\\s+/).filter(Boolean), rect: [r.left - sr.left, r.top - sr.top, r.width, r.height] });
                }
            }
        }
        // teksty ściśnięte przez renderer (.slt-squash = transform scaleX): układ ma szerokość nieściśniętego tekstu, a
        // konwerter bierze właśnie ją i gubi transformację, więc warstwa wychodzi szersza niż tło
        const squashMode = (rendered.rootEl && rendered.rootEl.getAttribute('data-squash-mode')) || 'fit';
        const squashed = [];
        stage.querySelectorAll('.slt-squash').forEach(n => {
            const r = n.getBoundingClientRect(), lw = n.offsetWidth;
            if (lw > 1 && r.width < lw * 0.98 && r.width > 1) {
                squashed.push({ cls: String(n.className || '').split(/\\s+/).filter(Boolean), layoutW: lw, rect: [r.left - sr.left, r.top - sr.top, r.width, r.height] });
            }
        });
        stage.remove();
        return { cfg: rec.cfg, extras, squashed, squashMode, autoSize: await probeAutoSize(el) };
    }

    // Czy zawartość oryginału zmienia szerokość wraz z tekstem (auto-szerokość)? Renderujemy z krótkim i długim
    // tytułem i porównujemy sumę prostokątów widocznych elementów. Scena ma stały rozmiar, więc taki element
    // po zmianie tekstu przestaje wyglądać jak oryginał.
    async function probeAutoSize(el) {
        const width = async (title) => {
            const g = JSON.parse(JSON.stringify(el));
            g.title = title; g.fields = Object.assign({}, g.fields || {}, { TITLE: title });
            const r = root.SceneConverter._render(g, {});
            await new Promise(res => setTimeout(res, 250));
            let x1 = Infinity, x2 = -Infinity;
            if (r.rootEl) r.rootEl.querySelectorAll('*').forEach(n => {
                const b = n.getBoundingClientRect();
                if (b.width < 1 || b.height < 1) return;
                x1 = Math.min(x1, b.left); x2 = Math.max(x2, b.right);
            });
            r.stage.remove();
            return x2 > x1 ? x2 - x1 : 0;
        };
        try {
            const w1 = await width('WWW'), w2 = await width('WWWWWWWWWWWWWWWWWWWWWWWW');
            return { short: Math.round(w1), long: Math.round(w2), auto: w2 > w1 * 1.15 + 6 };
        } catch (e) { return null; }
    }

    const iou = (a, b) => {
        const x1 = Math.max(a[0], b[0]), y1 = Math.max(a[1], b[1]);
        const x2 = Math.min(a[0] + a[2], b[0] + b[2]), y2 = Math.min(a[1] + a[3], b[1] + b[3]);
        const inter = Math.max(0, x2 - x1) * Math.max(0, y2 - y1);
        const uni = a[2] * a[3] + b[2] * b[3] - inter;
        return uni > 0 ? inter / uni : 0;
    };
    const has = (c) => c && c.type && c.type !== 'none';
    const same = (x, y) => JSON.stringify(x === undefined ? null : x) === JSON.stringify(y === undefined ? null : y);

    // Ramka sceny = suma obrysów warstw (jak w konwerterze): po zwężeniu warstwy przesuwamy warstwy do (0, 0)
    // i odpowiednio przesuwamy element w polu, żeby zawartość została dokładnie tam, gdzie była.
    function renormalizeFrame(el) {
        const layers = (el.scene && el.scene.layers) || [];
        if (!layers.length || !el.layout || el.layout.side !== 'custom') return;
        const scale = el.layout.scale || 1;
        let x1 = Infinity, y1 = Infinity, x2 = -Infinity, y2 = -Infinity;
        for (const l of layers) { x1 = Math.min(x1, l.x); y1 = Math.min(y1, l.y); x2 = Math.max(x2, l.x + l.w); y2 = Math.max(y2, l.y + l.h); }
        const r1 = (v) => Math.round(v * 10) / 10;
        for (const l of layers) { l.x = r1(l.x - x1); l.y = r1(l.y - y1); }
        el.scene.frame = { w: Math.max(1, Math.ceil(x2 - x1)), h: Math.max(1, Math.ceil(y2 - y1)) };
        el.layout.x = r1((el.layout.x || 0) + x1 * scale);
        el.layout.y = r1((el.layout.y || 0) + y1 * scale);
        el.layout.width = el.scene.frame.w;
        el.layout.height = el.scene.frame.h;
    }

    // Odtwarza intencję w skonwertowanym elemencie; zwraca listę notatek dla użytkownika.
    function apply(el, intent) {
        const notes = [];
        if (!intent) return notes;
        const a = el.animation = el.animation || {};
        const c = intent.cfg || {};   // szablony bez CGMotion.mount (np. data-cg-anim) nie mają konfiguracji, ale mają resztę intencji
        const describe = (t) => t.type + (t.duration !== undefined ? ' ' + t.duration + ' s' : '');
        // efekty tekstu: kod szablonu może mieć domyślne, gdy operator nie wybrał własnych (np. clip-up)
        if (c.text && !same(c.text, a.text) && (has(c.text) || has(a.text))) {
            a.text = c.text;
            notes.push('Efekt tekstu wejścia z kodu szablonu: ' + describe(c.text));
        }
        if (c.textOut && !same(c.textOut, a.textOut) && (has(c.textOut) || has(a.textOut))) {
            a.textOut = c.textOut;
            notes.push('Efekt tekstu wyjścia z kodu szablonu: ' + describe(c.textOut));
        }
        for (const k of ['textSync', 'textOutSync']) {
            if (typeof c[k] === 'boolean' && c[k] !== !!a[k]) a[k] = c[k];
        }
        // dodatkowe animacje elementów (np. wygaszanie grafiki bocznej w onShow) -> ruch IN odpowiedniej warstwy
        const layers = (el.scene && el.scene.layers) || [];
        const scale = (el.layout && el.layout.scale) || 1;
        const ox = (el.layout && el.layout.x) || 0, oy = (el.layout && el.layout.y) || 0;
        const known = (root.CGMotion && root.CGMotion.ANIM_TYPE_NAMES) || [];
        let fixed = false;
        for (const ex of intent.extras || []) {
            // Dopasowanie: konwerter nazywa warstwy po klasie elementu ("... — rep-side-graphic"), więc warstwa o nazwie
            // zawierającej klasę celu ma pierwszeństwo (element i jego ::before mają inne prostokąty, np. pasek
            // boczny w elemencie na całą belkę); bez takiej nazwy decyduje pokrycie prostokątów (IoU >= 0,5).
            const rectOf = (l) => [ox + l.x * scale, oy + l.y * scale, l.w * scale, l.h * scale];
            const named = layers.filter(l => (ex.cls || []).some(c => String(l.name || '').includes(c)));
            let best = null;
            if (named.length) {
                best = named.map(l => ({ l, v: iou(ex.rect, rectOf(l)) })).sort((p, q) => q.v - p.v)[0].l;
            } else {
                let bestIou = 0.5;
                for (const l of layers) { const v = iou(ex.rect, rectOf(l)); if (v > bestIou) { best = l; bestIou = v; } }
            }
            const name = ex.type;
            if (!best) { notes.push('Animacja "' + name + '" z kodu szablonu nie ma odpowiadającej warstwy — pominięto.'); continue; }
            if (!known.includes(name)) { notes.push('Animacja "' + name + '" (warstwa „' + (best.name || best.id) + '”) nie jest typem presetu — pominięto.'); continue; }
            best.motion = best.motion || {};
            if (best.motion.in && best.motion.in.type && best.motion.in.type !== 'none') continue;   // warstwa ma już własny ruch
            best.motion.in = { type: name, direction: ex.direction || 'left', duration: ex.duration, delay: Math.round(ex.delay * 1000) / 1000, ease: ex.ease || 'power2.out' };
            notes.push('Animacja z kodu szablonu przeniesiona na warstwę „' + (best.name || best.id) + '”: ' + name + ' ' + ex.duration + ' s, opóźnienie ' + best.motion.in.delay + ' s');
        }
        // Ściśnięty tekst: renderer skaluje element tekstu (scaleX) od jego LEWEJ krawędzi, a konwerter umieszcza warstwę
        // wokół środka wizualnego z szerokością układu, więc warstwa ląduje w złym miejscu.
        //  - tryb "fit" (większość typów): renderer dopasowuje tekst do szerokości warstwy, więc warstwa dostaje wizualną
        //    szerokość (tyle, ile zajmuje po ściśnięciu);
        //  - tryb "length" (BADGE): skala zależy od liczby znaków (max(0,5; 25/znaki)), więc warstwa zachowuje szerokość
        //    układu (nieściśniętą), a lewa krawędź ma leżeć tam, gdzie była w oryginale.
        for (const sq of intent.squashed || []) {
            const l = layers.find(x => x.type === 'text' && x.squash && (sq.cls || []).some(c => String(x.name || '').includes(c)) &&
                Math.abs(x.w * scale - sq.layoutW) < 4);
            if (!l) continue;
            const byLength = intent.squashMode === 'length';
            const was = Math.round(l.w);
            l.x = Math.round(((sq.rect[0] - ox) / scale) * 10) / 10;
            if (byLength) {
                l.font = Object.assign({}, l.font, { align: 'left' });
                notes.push('Tekst „' + (l.name || l.id) + '” jest ściskany w oryginale wg długości (belka BADGE): lewa krawędź warstwy ustawiona tam, gdzie była w oryginale.');
            } else {
                l.w = Math.round(sq.rect[2] / scale * 10) / 10;
                l.font = Object.assign({}, l.font, { align: 'center' });
                notes.push('Tekst „' + (l.name || l.id) + '” był ściśnięty w oryginale: szerokość warstwy zmniejszona z ' + was + ' do ' + Math.round(l.w) + ' px (tyle, ile zajmuje po ściśnięciu).');
            }
            fixed = true;
        }
        if (fixed) renormalizeFrame(el);
        // auto-szerokość: scena nie rośnie z tekstem, więc dbamy o ściskanie tekstu (zamiast wystawania poza tło)
        if (intent.autoSize && intent.autoSize.auto) {
            let enabled = 0;
            for (const l of layers) if (l.type === 'text' && !l.squash) { l.squash = true; enabled++; }
            const squashed = layers.filter(l => l.type === 'text' && l.squash).length;
            notes.push('Oryginał dopasowuje szerokość tła do tekstu (' + intent.autoSize.short + ' → ' + intent.autoSize.long + ' px), a scena ma stały rozmiar: tło nie urośnie' +
                (squashed ? ', a tekst będzie ściskany (' + squashed + ' warstw' + (enabled ? ', włączono ' + enabled : '') + '), żeby się zmieścił' : ', a długi tekst może wystawać poza tło') + '.');
        }
        return notes;
    }
    root.BTBIntent = { capture, apply, install };
})(window);
`;

async function main() {
    init();
    const input = JSON.parse(fs.readFileSync(0, 'utf8'));
    const elements = Array.isArray(input) ? input : [input];
    const browser = await chromium.launch();
    try {
        const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
        await page.setContent('<!doctype html><html><body style="margin:0"></body></html>');
        await page.addScriptTag({ path: gsapPath });
        await page.addScriptTag({ path: hbPath });
        for (const f of ['cgmotion.js', 'element-model.js', 'scene-model.js', 'renderer.js', 'scene_convert.js'])
            await page.addScriptTag({ path: path.join(root, f) });

        await page.addScriptTag({ content: INTENT_JS });

        const out = [];
        for (const el of elements) {
            try {
                let intent = null;
                try { intent = await page.evaluate((g) => window.BTBIntent.capture(JSON.parse(JSON.stringify(g))), el); } catch (_) { intent = null; }
                const run = (g, retried) => page.evaluate(async ([g, retried]) => {
                    const r = await window.SceneConverter.convertAsync(g, {});
                    if (!r) return null;
                    // pomiar na wyłączonych animacjach: scena i tak jest czytana ze stanu końcowego
                    // (a oryginalne animacje elementu zostają w g.animation)
                    window.SceneConverter.apply(g, r);
                    return { element: g, warnings: r.warnings.concat(retried ? ['Pomiar wykonano z wyłączonymi animacjami (animacja wejścia ukrywała zawartość).'] : []), stats: r.stats };
                }, [g, retried]);
                let res = await run(el, false);
                if (!res) {
                    // niektóre animacje wejścia (np. flip-x) zostawiają element niewidocznym po zamrożeniu GSAP
                    const quiet = JSON.parse(JSON.stringify(el));
                    const off = { type: 'none', duration: 0, delay: 0 };
                    quiet.animation = { ...(quiet.animation || {}), in: off, out: off, loop: undefined };
                    delete quiet.animation.text; delete quiet.animation.textOut;
                    res = await run(quiet, true);
                    // zachowaj oryginalne animacje w wyniku
                    if (res) res.element.animation = el.animation;
                }
                let notes = [];
                if (res && intent) {
                    const r2 = await page.evaluate(([g, it]) => { const notes = window.BTBIntent.apply(g, it); return { g, notes }; }, [res.element, intent]);
                    res.element = r2.g; notes = r2.notes;
                }
                if (res) res.notes = notes;
                out.push(res ? { ok: true, ...res }
                             : { ok: false, warnings: [], error: 'Konwerter nie znalazł widocznej zawartości do przeniesienia na scenę.' });
            } catch (e) {
                out.push({ ok: false, warnings: [], error: String(e.message || e).split('\n')[0] });
            }
        }
        process.stdout.write(JSON.stringify(out));
    } finally { await browser.close(); }
}

module.exports = { INTENT_JS };
if (require.main === module) main().catch((e) => fail(String(e.message || e).split('\n')[0]));
