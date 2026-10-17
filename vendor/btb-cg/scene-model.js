/**
 * SceneModel — graficzny opis elementu jako drzewo warstw (scena) + kompilator do kodu.
 *
 * Element graficzny może mieć `scene`; wtedy html/css/js_template i ocgInputs są ARTEFAKTEM
 * kompilacji sceny (jednokierunkowo: scena → kod). Ręczna edycja kodu ustawia `scene.detached`.
 *
 * Współrzędne warstw są względem ramki elementu (scene.frame, px). Położenie ramki na ekranie
 * (kotwica, marginesy, skala, obrót) ustawia zwykły `layout` elementu — jak dla każdego elementu.
 *
 * Typy warstw: shape | text | image | video | clock | ticker
 * Moduł współdzielony: Node (window niepotrzebne) i przeglądarka (window.SceneModel).
 */
(function (root, factory) {
    if (typeof module === 'object' && module.exports) module.exports = factory();
    else root.SceneModel = factory();
})(typeof self !== 'undefined' ? self : this, function () {

    const VERSION = 1;
    const LAYER_TYPES = ['shape', 'text', 'image', 'video', 'clock', 'ticker'];
    const BG_EFFECTS = ['none', 'shimmer', 'pulse', 'glow', 'scanline'];
    const RESERVED_TOP = { TITLE: 'title', SUBTITLE: 'subtitle' };

    const clone = (o) => (o === undefined ? undefined : JSON.parse(JSON.stringify(o)));
    const num = (v, d) => (v === undefined || v === null || v === '' || isNaN(Number(v)) ? d : Number(v));
    const esc = (s) => String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    const cssStr = (s) => String(s ?? '').replace(/['"\\;{}<>]/g, '');
    const fieldId = (s) => String(s || '').toUpperCase().replace(/[^A-Z0-9_]/g, '_').replace(/^(\d)/, '_$1');

    // ── Domyślne wartości ───────────────────────────────────────────────────────

    // in/out: ruch warstwy; text/textOut: efekt tekstu warstwy (null = dziedziczy z elementu, {type:'none'} = wyłączony)
    const DEFAULT_MOTION = () => ({ in: null, out: null, text: null, textOut: null });

    const DEFAULT_TEXT = () => ({
        family: 'Bahnschrift', size: 36, weight: '700', color: '#ffffff',
        transform: 'none', align: 'left', valign: 'middle',
        letterSpacing: 0, lineHeight: 1.2, italic: false,
        shadow: '0 2px 8px rgba(0,0,0,0.7)'
    });

    function createLayer(type, props) {
        if (!LAYER_TYPES.includes(type)) throw new Error('SceneModel: nieznany typ warstwy ' + type);
        const base = {
            id: 'l_' + Math.random().toString(36).slice(2, 8),
            type, name: type, visible: true, locked: false,
            x: 0, y: 0, w: 200, h: 60,
            rotation: 0, skew: 0, opacity: 1,
            motion: DEFAULT_MOTION()
        };
        let extra = {};
        if (type === 'shape') {
            extra = {
                fill: { type: 'solid', color: '#0047AB', color2: '#000000', angle: 90 },
                stroke: { color: '#ffffff', width: 0 },
                radius: 0, shadow: false, effect: 'none'
            };
        } else if (type === 'text') {
            extra = { field: 'TITLE', text: '', defaultText: '', font: DEFAULT_TEXT(), squash: false, w: 600 };
        } else if (type === 'image') {
            extra = { src: '', bind: '', fit: 'contain', radius: 0 };
        } else if (type === 'video') {
            extra = { src: '', fit: 'cover', loop: true, muted: true, radius: 0 };
        } else if (type === 'clock') {
            extra = { format: 'HH:MM:SS', font: { ...DEFAULT_TEXT(), size: 28, align: 'center' } };
        } else if (type === 'ticker') {
            extra = { speed: 100, separator: '•', gap: 48, font: { ...DEFAULT_TEXT(), size: 30, weight: '600', shadow: 'none' } };
        }
        const layer = { ...base, ...extra, ...clone(props || {}) };
        if (props && props.font) layer.font = { ...(extra.font || {}), ...clone(props.font) };
        return layer;
    }

    function createScene(props) {
        return {
            version: VERSION,
            detached: false,
            frame: { w: 1000, h: 100 },
            layers: [],
            ...clone(props || {})
        };
    }

    /** Uzupełnia brakujące pola (po wczytaniu z bazy / importu). Nie mutuje wejścia. */
    function normalize(scene) {
        if (!scene || typeof scene !== 'object') return null;
        const s = createScene(clone(scene));
        s.version = VERSION;
        s.frame = { w: num(s.frame?.w, 1000), h: num(s.frame?.h, 100) };
        const seen = new Set();
        s.layers = (s.layers || []).filter(l => l && LAYER_TYPES.includes(l.type)).map(l => {
            const n = createLayer(l.type, l);
            if (seen.has(n.id)) n.id += '_' + seen.size;
            seen.add(n.id);
            n.motion = { ...DEFAULT_MOTION(), ...(l.motion || {}) };
            return n;
        });
        return s;
    }

    // ── Kompilator: scena → kod ─────────────────────────────────────────────────

    function fillCss(fill) {
        if (!fill || fill.type === 'none') return '';
        if (fill.type === 'gradient') {
            return `background: linear-gradient(${num(fill.angle, 90)}deg, ${cssStr(fill.color || '#1e3a8a')} 0%, ${cssStr(fill.color2 || '#3b82f6')} 100%);`;
        }
        return `background: ${cssStr(fill.color || 'transparent')};`;
    }

    function radiusCss(r) {
        if (Array.isArray(r)) return `border-radius: ${r.map(v => num(v, 0) + 'px').join(' ')};`;
        return num(r, 0) ? `border-radius: ${num(r, 0)}px;` : '';
    }

    function shadowCss(sh) {
        if (!sh) return '';
        if (sh === true) return 'box-shadow: 0 10px 25px -5px rgba(0,0,0,0.6);';
        return `box-shadow: ${num(sh.x, 0)}px ${num(sh.y, 10)}px ${num(sh.blur, 25)}px ${cssStr(sh.color || 'rgba(0,0,0,0.6)')};`;
    }

    function fontCss(f) {
        f = { ...DEFAULT_TEXT(), ...(f || {}) };
        const jc = f.align === 'center' ? 'center' : f.align === 'right' ? 'flex-end' : 'flex-start';
        const ai = f.valign === 'top' ? 'flex-start' : f.valign === 'bottom' ? 'flex-end' : 'center';
        return [
            'display: flex;', `justify-content: ${jc};`, `align-items: ${ai};`,
            `font-family: '${cssStr(f.family)}', sans-serif;`,
            `font-size: ${num(f.size, 36)}px;`, `font-weight: ${cssStr(f.weight)};`,
            `color: ${cssStr(f.color)};`,
            f.transform && f.transform !== 'none' ? `text-transform: ${cssStr(f.transform)};` : '',
            `text-align: ${cssStr(f.align)};`,
            num(f.letterSpacing, 0) ? `letter-spacing: ${num(f.letterSpacing, 0)}px;` : '',
            `line-height: ${num(f.lineHeight, 1.2)};`,
            f.italic ? 'font-style: italic;' : '',
            f.shadow && f.shadow !== 'none' ? `text-shadow: ${String(f.shadow).replace(/[;{}<>]/g, '')};` : '',
            'white-space: nowrap;'
        ].filter(Boolean).join(' ');
    }

    function effectCss(layer, sel) {
        const e = layer.effect;
        if (layer.type !== 'shape' || !e || e === 'none') return '';
        const kf = `cg-fx-${e}-{{ID}}`;
        if (e === 'shimmer') {
            return `${sel}.cg-fx-shimmer { overflow: hidden; }\n` +
                `${sel}.cg-fx-shimmer::after { content: ''; position: absolute; top: 0; left: -120%; width: 60%; height: 100%; ` +
                `background: linear-gradient(90deg, rgba(255,255,255,0), rgba(255,255,255,0.28), rgba(255,255,255,0)); ` +
                `transform: skewX(-20deg); animation: ${kf} 3s infinite; }\n` +
                `@keyframes ${kf} { 0% { left: -120%; } 60%, 100% { left: 160%; } }\n`;
        }
        if (e === 'pulse') {
            return `${sel}.cg-fx-pulse { animation: ${kf} 1.6s infinite ease-in-out; }\n` +
                `@keyframes ${kf} { 0%, 100% { opacity: ${num(layer.opacity, 1)}; } 50% { opacity: ${Math.max(0.35, num(layer.opacity, 1) * 0.55)}; } }\n`;
        }
        if (e === 'glow') {
            const c = cssStr(layer.fill?.color || '#ffffff');
            return `${sel}.cg-fx-glow { animation: ${kf} 2s infinite ease-in-out; }\n` +
                `@keyframes ${kf} { 0%, 100% { box-shadow: 0 0 6px ${c}; } 50% { box-shadow: 0 0 26px ${c}; } }\n`;
        }
        if (e === 'scanline') {
            return `${sel}.cg-fx-scanline { overflow: hidden; }\n` +
                `${sel}.cg-fx-scanline::after { content: ''; position: absolute; inset: 0; ` +
                `background: repeating-linear-gradient(0deg, rgba(0,0,0,0.18) 0 1px, transparent 1px 3px); animation: ${kf} 1s linear infinite; }\n` +
                `@keyframes ${kf} { from { background-position: 0 0; } to { background-position: 0 3px; } }\n`;
        }
        return '';
    }

    /**
     * Maska obrazu/wideo kształtem innej warstwy (shape): `layer.maskLayer` = id warstwy-maski.
     * Przycięcie liczone w układzie lokalnym warstwy; skos maski względem skosu warstwy daje wielokąt.
     */
    function maskCss(l, scene) {
        if (!l.maskLayer) return '';
        const m = scene.layers.find(x => x.id === l.maskLayer && x.type === 'shape' && x.visible !== false);
        if (!m || m.id === l.id) return '';
        const top = num(m.y, 0) - num(l.y, 0), left = num(m.x, 0) - num(l.x, 0);
        const right = (num(l.x, 0) + num(l.w, 0)) - (num(m.x, 0) + num(m.w, 0));
        const bottom = (num(l.y, 0) + num(l.h, 0)) - (num(m.y, 0) + num(m.h, 0));
        const sk = num(m.skew, 0) - num(l.skew, 0);
        if (Math.abs(sk) > 0.01) {
            const t = Math.tan(sk * Math.PI / 180), cy = top + num(m.h, 0) / 2;
            const x1 = left, x2 = left + num(m.w, 0), y1 = top, y2 = top + num(m.h, 0);
            const dx = (y) => (y - cy) * -t;     // skewX przesuwa górę w lewo przy dodatnim kącie
            return `clip-path: polygon(${x1 + dx(y1)}px ${y1}px, ${x2 + dx(y1)}px ${y1}px, ${x2 + dx(y2)}px ${y2}px, ${x1 + dx(y2)}px ${y2}px);`;
        }
        const r = Array.isArray(m.radius) ? m.radius.map(v => num(v, 0) + 'px').join(' ') : num(m.radius, 0) + 'px';
        return `clip-path: inset(${top}px ${right}px ${bottom}px ${left}px round ${r});`;
    }

    function compile(sceneIn) {
        const scene = normalize(sceneIn);
        const out = { html_template: '', css_template: '', js_template: '', ocgInputs: [], values: { title: undefined, subtitle: undefined, fields: {} } };
        if (!scene) return out;

        const layers = scene.layers.filter(l => l.visible !== false);
        const { w: FW, h: FH } = scene.frame;

        let html = `<div class="cg-scene">\n`;
        let css = `/* SceneModel v${VERSION} — wygenerowane ze sceny, nie edytuj ręcznie */\n` +
            `#{{ID}} .cg-scene { position: relative; width: ${FW}px; height: ${FH}px; pointer-events: none; transform: scale(var(--cg-sx, 1), var(--cg-sy, 1)); transform-origin: top left; }\n` +
            `#{{ID}} .cg-l, #{{ID}} .cg-i { box-sizing: border-box; }\n` +
            `#{{ID}} .cg-l { position: absolute; }\n` +
            `#{{ID}} .cg-i { position: absolute; inset: 0; }\n`;

        const clocks = [], tickers = [], motions = [], textNs = [];
        const seenField = new Set();

        layers.forEach((l, idx) => {
            const n = idx + 1;
            const cls = `cg-l${n}`;
            // Renderer dopisuje #id przed KAŻDĄ klasą cg-*, także po '>' — dlatego brak selektorów potomnych:
            // warstwa wewnętrzna ma własną, jednoklasową nazwę (cg-i<N>).
            const sel = `#{{ID}} .cg-i${n}`;
            const innerCls = ['cg-i', `cg-i${n}`];
            if (l.type === 'shape' && l.effect && l.effect !== 'none' && BG_EFFECTS.includes(l.effect)) innerCls.push(`cg-fx-${l.effect}`);

            // pozycja (animowana przez GSAP — transformacje wizualne siedzą w .cg-i<N>)
            css += `#{{ID}} .${cls} { left: ${num(l.x, 0)}px; top: ${num(l.y, 0)}px; width: ${num(l.w, 0)}px; height: ${num(l.h, 0)}px; z-index: ${idx + 1}; }\n`;

            const vis = [];
            const tf = [];
            if (num(l.skew, 0)) tf.push(`skewX(${num(l.skew, 0)}deg)`);
            if (num(l.rotation, 0)) tf.push(`rotate(${num(l.rotation, 0)}deg)`);
            if (tf.length) vis.push(`transform: ${tf.join(' ')};`);
            if (l.opacity !== undefined && num(l.opacity, 1) !== 1) vis.push(`opacity: ${num(l.opacity, 1)};`);

            let inner = '';
            if (l.type === 'shape') {
                vis.push(fillCss(l.fill), radiusCss(l.radius), shadowCss(l.shadow));
                if (num(l.stroke?.width, 0) > 0) vis.push(`border: ${num(l.stroke.width, 0)}px solid ${cssStr(l.stroke.color || '#ffffff')};`);
            } else if (l.type === 'text') {
                vis.push(fontCss(l.font));
                const cont = (l.squash ? 'cg-t slt-squash' : 'cg-t') + '" data-cg-text="1';
                let content;
                if (l.field) {
                    const id = fieldId(l.field) || 'TITLE';
                    content = `{{{${id}}}}`;
                    if (!seenField.has(id)) {
                        seenField.add(id);
                        const def = l.defaultText || '';
                        out.ocgInputs.push({ id, label: l.name || id, default: def, type: 'text' });
                        if (RESERVED_TOP[id]) out.values[RESERVED_TOP[id]] = def;
                        else out.values.fields[id] = def;
                    }
                } else {
                    content = esc(l.text);
                }
                inner = `<div class="${cont}">${content}</div>`;
            } else if (l.type === 'image') {
                vis.push(radiusCss(l.radius), 'overflow: hidden;', maskCss(l, scene));
                const src = l.bind ? `{{${fieldId(l.bind)}}}` : esc(l.src);
                inner = `<img src="${src}" alt="" style="width:100%;height:100%;object-fit:${cssStr(l.fit || 'contain')};display:block;" />`;
            } else if (l.type === 'video') {
                vis.push(radiusCss(l.radius), 'overflow: hidden;', maskCss(l, scene));
                inner = `<video src="${esc(l.src)}" autoplay playsinline ${l.loop !== false ? 'loop ' : ''}${l.muted !== false ? 'muted ' : ''}style="width:100%;height:100%;object-fit:${cssStr(l.fit || 'cover')};display:block;"></video>`;
            } else if (l.type === 'clock') {
                vis.push(fontCss(l.font));
                inner = `<span class="cg-clock" data-cg-text="1" data-format="${esc(l.format || 'HH:MM:SS')}">00:00:00</span>`;
                clocks.push(n);
            } else if (l.type === 'ticker') {
                vis.push(fontCss({ ...l.font, align: 'left' }), 'overflow: hidden;');
                inner = `<div class="cg-track" style="display:inline-flex;white-space:nowrap;will-change:transform;"></div>` +
                    `<span class="cg-data" data-b64="{{ITEMS_B64}}" style="display:none;"></span>`;
                tickers.push({ n, speed: num(l.speed, 100), sep: String(l.separator ?? '•'), gap: num(l.gap, 48) });
            }

            css += `${sel} { ${vis.filter(Boolean).join(' ')} }\n`;
            css += effectCss(l, sel);

            html += `    <div class="cg-l ${cls}" data-layer="${esc(l.id)}"><div class="${innerCls.join(' ')}">${inner}</div></div>\n`;

            const mo = l.motion || {};
            // ruch grupy obowiązuje członków, którzy nie mają własnego IN/OUT
            const gm = (l.group && scene.groups && scene.groups[l.group] && scene.groups[l.group].motion) || {};
            const mIn = mo.in || gm.in || null, mOut = mo.out || gm.out || null;
            if (mIn || mOut || mo.text || mo.textOut) {
                motions.push({ n, in: mIn, out: mOut, text: mo.text || null, textOut: mo.textOut || null });
            }
            if (l.type === 'text' || l.type === 'clock') textNs.push(n);
        });
        html += `</div>`;

        // ── JS ────────────────────────────────────────────────────────────────
        let js = `(() => {\n` +
            `    const root = document.getElementById("{{ID}}");\n` +
            `    if (!root) return;\n` +
            `    const scene = root.querySelector('.cg-scene') || root;\n` +
            `    const layer = n => root.querySelector('.cg-l' + n);\n` +
            `    const motions = ${JSON.stringify(motions)};\n`;

        if (clocks.length) {
            js += `    const pad = v => String(v).padStart(2, '0');\n` +
                `    const tick = () => {\n` +
                `        if (!root.isConnected) { clearInterval(root.__cgClock); return; }\n` +
                `        const d = new Date(), hm = pad(d.getHours()) + ':' + pad(d.getMinutes());\n` +
                `        root.querySelectorAll('.cg-clock').forEach(el => {\n` +
                `            el.textContent = el.getAttribute('data-format') === 'HH:MM' ? hm : hm + ':' + pad(d.getSeconds());\n` +
                `        });\n` +
                `    };\n` +
                `    tick(); root.__cgClock = setInterval(tick, 1000);\n`;
        }

        if (tickers.length) {
            js += `    const tickers = ${JSON.stringify(tickers)};\n` +
                `    let items = [];\n` +
                `    const dataEl = root.querySelector('.cg-data');\n` +
                `    try {\n` +
                `        const b64 = (dataEl && dataEl.getAttribute('data-b64') || '').trim();\n` +
                `        if (b64) items = JSON.parse(decodeURIComponent(escape(atob(b64 + '='.repeat((4 - b64.length % 4) % 4)))));\n` +
                `    } catch (e) { console.error('[Scene] items b64', e); }\n` +
                `    if (!Array.isArray(items) || !items.length) items = [{ text: 'BRAK WIADOMOŚCI' }];\n` +
                `    const txt = i => (i && typeof i === 'object') ? (i.text || '') : String(i);\n` +
                `    let tickerTweens = [];\n` +
                `    const startTickers = () => {\n` +
                `        tickers.forEach(t => {\n` +
                `            const trk = layer(t.n).querySelector('.cg-track');\n` +
                `            const one = items.map(i => '<span style="padding-right:' + t.gap + 'px">' + txt(i) + (t.sep ? ' <span style="padding-left:' + t.gap + 'px">' + t.sep + '</span>' : '') + '</span>').join('');\n` +
                `            trk.innerHTML = one + one;\n` +
                `            const half = trk.scrollWidth / 2;\n` +
                `            if (half > 0) tickerTweens.push(gsap.fromTo(trk, { x: 0 }, { x: -half, duration: half / t.speed, ease: 'none', repeat: -1 }));\n` +
                `        });\n` +
                `    };\n` +
                `    const stopTickers = () => { tickerTweens.forEach(t => t.kill()); tickerTweens = []; };\n`;
        }

        js += `    const textNs = ${JSON.stringify(textNs)};\n` +
            `    const motionOf = n => motions.find(m => m.n === n) || {};\n` +
            `    const on = c => c && c.type && c.type !== 'none';\n` +
            `    CGMotion.mount(root, {\n` +
            `        container: scene,\n` +
            `        animIn: {{{ANIMATION_IN_JSON}}}, animOut: {{{ANIMATION_OUT_JSON}}},\n` +
            `        text: { type: 'none' }, textOut: { type: 'none' },\n` +
            `        loop: {{{LOOP_JSON}}},\n` +
            `        onShow(tl) {\n` +
            `            const T = {{{TEXT_ANIM_JSON}}}, TS = {{{TEXT_ANIM_SYNC}}};\n` +
            `            const baseEnd = tl.duration();\n` +
            (tickers.length ? `            stopTickers(); startTickers();\n` : '') +
            `            motions.forEach(m => {\n` +
            `                const el = layer(m.n);\n` +
            `                if (el && on(m.in)) tl.add(CGMotion.standardIn(el, m.in), 0);\n` +
            `            });\n` +
            `            textNs.forEach(n => {\n` +
            `                const el = layer(n), own = motionOf(n).text;\n` +
            `                const eff = own || T;\n` +
            `                if (!el || !on(eff)) return;\n` +
            `                const sub = CGMotion.applyTextEffect(el, eff);\n` +
            `                if (sub) tl.add(sub, own ? 0 : (TS ? Math.max(0, baseEnd - 0.1) : 0));\n` +
            `            });\n` +
            `        },\n` +
            `        onHide() {\n` +
            (tickers.length ? `            stopTickers();\n` : '') +
            `        }\n` +
            `    });\n` +
            `    // Wyjście: najpierw OUT warstw (i ich efekty tekstu), potem OUT całego elementu z inspektora\n` +
            `    const baseHide = root.__slt_hide;\n` +
            `    root.__slt_hide = () => {\n` +
            `        const TO = {{{TEXT_ANIM_OUT_JSON}}}, TOS = {{{TEXT_ANIM_OUT_SYNC}}};\n` +
            `        const pre = gsap.timeline(), par = gsap.timeline();\n` +
            `        motions.forEach(m => {\n` +
            `            const el = layer(m.n);\n` +
            `            if (el && on(m.out)) pre.add(CGMotion.standardOut(el, m.out), 0);\n` +
            `        });\n` +
            `        textNs.forEach(n => {\n` +
            `            const el = layer(n), own = motionOf(n).textOut;\n` +
            `            const eff = own || TO;\n` +
            `            if (!el || !on(eff)) return;\n` +
            `            const sub = CGMotion.applyTextOutEffect(el, eff);\n` +
            `            if (!sub) return;\n` +
            `            if (own) pre.add(sub, 0); else par.add(sub, 0);\n` +
            `        });\n` +
            `        const base = baseHide();\n` +
            `        const tl = gsap.timeline();\n` +
            `        tl.add(pre);\n` +
            `        if (TOS) { tl.add(par); tl.add(base, '>-0.1'); }\n` +
            `        else { tl.add(base); tl.add(par, '<'); }\n` +
            `        return tl;\n` +
            `    };\n` +
            `})();\n`;

        out.html_template = html;
        out.css_template = css;
        out.js_template = js;
        return out;
    }

    /**
     * Kompiluje scenę i zapisuje wynik w elemencie (mutuje `el`).
     * TITLE/SUBTITLE trafiają do el.title/el.subtitle, reszta do el.fields.
     */
    function applyToElement(el, sceneIn, opts) {
        const scene = normalize(sceneIn);
        if (!scene) return el;
        const c = compile(scene);
        scene.detached = false;
        el.scene = scene;
        el.html_template = c.html_template;
        el.css_template = c.css_template;
        el.js_template = c.js_template;
        el.ocgInputs = c.ocgInputs;
        // wartości wpisane wcześniej (operator / edycja) wygrywają; domyślne sceny tylko uzupełniają braki
        el.fields = { ...c.values.fields, ...(el.fields || {}) };
        if (opts && opts.resetValues) {
            if (c.values.title !== undefined) el.title = c.values.title;
            if (c.values.subtitle !== undefined) el.subtitle = c.values.subtitle;
        } else {
            if (c.values.title !== undefined && !el.title) el.title = c.values.title;
            if (c.values.subtitle !== undefined && !el.subtitle) el.subtitle = c.values.subtitle;
        }
        el.layout = { ...(el.layout || {}), width: scene.frame.w, height: scene.frame.h };
        return el;
    }

    /** Odłącza scenę od kodu (po ręcznej edycji html/css/js). */
    function detach(el) {
        if (el && el.scene) el.scene.detached = true;
        return el;
    }

    // ── Migracje ────────────────────────────────────────────────────────────────

    const LEGACY_MOTION = {
        slide_left:  { type: 'slide', direction: 'left',   duration: 0.55, ease: 'power3.out' },
        wipe_left:   { type: 'wipe',  direction: 'left',   duration: 0.5,  ease: 'expo.out' },
        fade_up:     { type: 'fade',  direction: 'bottom', duration: 0.5,  ease: 'power2.out' },
        reveal_skew: { type: 'slide', direction: 'left',   duration: 0.6,  ease: 'power3.out' }
    };

    /**
     * Stary format sceny (scene_designer.js: SCENE_PRESETS / tpl._sceneData: warstwy w absolutnych
     * px płótna 1920×1080, typy plate/accent/badge/text/image/clock) → scena v1.
     * Zwraca { scene, origin:{x,y}, animation } — origin to lewy górny róg zawartości na płótnie
     * (można użyć jako layout.x/y przy side:'custom'), animation jako element.animation.in.
     */
    function fromLegacyScene(legacy) {
        if (!legacy || !Array.isArray(legacy.layers)) return null;
        const src = legacy.layers;
        let minX = Infinity, minY = Infinity, maxX = 0, maxY = 0;
        src.forEach(l => {
            const w = num(l.width, 0), h = num(l.height, 0);
            minX = Math.min(minX, num(l.x, 0)); minY = Math.min(minY, num(l.y, 0));
            maxX = Math.max(maxX, num(l.x, 0) + w); maxY = Math.max(maxY, num(l.y, 0) + h);
        });
        if (!isFinite(minX)) { minX = 0; minY = 0; }
        const layers = [];

        src.forEach(l => {
            const geo = { id: l.id, name: l.name || l.id, x: num(l.x, 0) - minX, y: num(l.y, 0) - minY,
                          w: num(l.width, 0) || 200, h: num(l.height, 0) || 60,
                          skew: num(l.skew, 0), opacity: l.opacity !== undefined ? num(l.opacity, 1) : 1 };
            const fill = l.bgType === 'gradient'
                ? { type: 'gradient', color: l.color, color2: l.color2, angle: num(l.angle, 90) }
                : { type: 'solid', color: l.color || '#0047AB' };
            const stroke = { color: l.borderColor || '#ffffff', width: l.borderColor ? num(l.borderWidth, 0) : 0 };

            if (l.type === 'plate' || l.type === 'accent') {
                layers.push(createLayer('shape', { ...geo, fill, stroke, radius: num(l.radius, 0), shadow: !!l.shadow }));
            } else if (l.type === 'badge') {
                layers.push(createLayer('shape', { ...geo, fill, stroke, radius: num(l.radius, 0), shadow: false,
                                                   effect: l.pulse ? 'pulse' : 'none' }));
                layers.push(createLayer('text', { id: l.id + '_label', name: (l.name || 'Badge') + ' — tekst',
                    x: geo.x, y: geo.y, w: geo.w, h: geo.h, skew: geo.skew, field: null, text: l.label || 'PILNE',
                    font: { family: 'Bahnschrift', size: 16, weight: '800', color: l.textColor || '#ffffff',
                            align: 'center', valign: 'middle', letterSpacing: 1.3, shadow: 'none' } }));
            } else if (l.type === 'text') {
                const size = num(l.fontSize, 36);
                layers.push(createLayer('text', { ...geo, h: num(l.height, 0) || Math.round(size * 1.3),
                    w: num(l.width, 0) || 900,
                    field: l.fieldId || 'TITLE', defaultText: l.defaultText || '',
                    font: { family: l.fontFamily || 'Bahnschrift', size, weight: String(l.fontWeight || '700'),
                            color: l.color || '#ffffff', transform: l.textTransform || 'none', align: 'left', valign: 'middle' } }));
            } else if (l.type === 'image') {
                layers.push(createLayer('image', { ...geo, src: l.url || 'icon.svg', radius: num(l.radius, 0) }));
            } else if (l.type === 'clock') {
                layers.push(createLayer('clock', { ...geo, format: l.format || 'HH:MM:SS',
                    font: { family: l.fontFamily || 'Bahnschrift', size: num(l.fontSize, 28), weight: '700',
                            color: l.textColor || '#ffffff', align: 'center', valign: 'middle', letterSpacing: 1 } }));
                // clock miał też tło (fill) — zachowaj jako osobną warstwę pod tekstem
                if (l.color) {
                    const bg = createLayer('shape', { ...geo, id: l.id + '_bg', name: (l.name || 'Zegar') + ' — tło', fill, stroke, radius: num(l.radius, 0) });
                    layers.splice(layers.length - 1, 0, bg);
                }
            }
        });

        const scene = createScene({
            frame: { w: Math.max(1, Math.round(maxX - minX)), h: Math.max(1, Math.round(maxY - minY)) },
            layers
        });
        const m = LEGACY_MOTION[legacy.animation?.in] || LEGACY_MOTION.slide_left;
        return {
            scene: normalize(scene),
            origin: { x: Math.round(minX), y: Math.round(minY) },
            animation: { ...m, ...(legacy.animation?.duration ? { duration: num(legacy.animation.duration, m.duration) } : {}) }
        };
    }

    /**
     * Element bez sceny (kod ręczny) → startowa scena: płyta z style.background + teksty
     * TITLE/SUBTITLE z style.typography. To punkt wyjścia 'Utwórz scenę'; po zastosowaniu
     * kod elementu jest NADPISYWANY wynikiem kompilacji.
     */
    function fromElement(el) {
        const st = el.style || {};
        const bg = st.background || {};
        const ty = st.typography || {};
        const sty = st.subtitleTypography || {};
        const hasSub = !!(el.subtitle || (el.ocgInputs || []).some(i => i.id === 'SUBTITLE'));
        const W = num(parseFloat(el.layout?.width), 0) || 1000;
        const H = num(parseFloat(el.layout?.height), 0) || (hasSub ? 120 : 80);
        const pad = 24;
        const titleH = hasSub ? Math.round(H * 0.55) : H;

        const layers = [
            createLayer('shape', {
                id: 'bg', name: 'Tło', x: 0, y: 0, w: W, h: H,
                fill: bg.type === 'gradient'
                    ? { type: 'gradient', color: bg.color, color2: bg.color2, angle: num(bg.gradientAngle, 135) }
                    : { type: 'solid', color: bg.color || '#0047AB' },
                stroke: { color: bg.borderColor || '#ffffff', width: num(bg.borderWidth, 0) },
                radius: num(bg.borderRadius, 0), opacity: num(bg.opacity, 1)
            }),
            createLayer('text', {
                id: 'title', name: 'Tytuł', x: pad, y: 0, w: W - pad * 2, h: titleH,
                field: 'TITLE', defaultText: el.title || '',
                font: { family: ty.fontFamily || 'Inter', size: num(ty.fontSize, 30), weight: String(ty.fontWeight || 'bold'),
                        color: ty.color || '#ffffff', align: 'left', valign: 'middle' }
            })
        ];
        if (hasSub) {
            layers.push(createLayer('text', {
                id: 'subtitle', name: 'Podtytuł', x: pad, y: titleH, w: W - pad * 2, h: H - titleH,
                field: 'SUBTITLE', defaultText: el.subtitle || '',
                font: { family: sty.fontFamily || 'Inter', size: num(sty.fontSize, 20), weight: String(sty.fontWeight || 'normal'),
                        color: sty.color || '#eeeeee', align: 'left', valign: 'middle' }
            }));
        }
        return normalize({ frame: { w: W, h: H }, layers });
    }

    // ── Pole wizji 1920×1080 ───────────────────────────────────────────────────

    const FIELD = { w: 1920, h: 1080 };

    /**
     * Gdzie na polu 1920×1080 leży ramka sceny — dokładnie tak, jak liczy to renderer
     * (`translate(x,y) scale(s) rotate(r)` na kontenerze 1920×1080; przy `side` ≠ custom ramka jest ułożona
     * flexboxem w kontenerze PRZED skalowaniem). Zwraca składniki transformacji CSS ramki:
     * `translate(tx,ty) scale(scale) rotate(rotation) translate(lx,ly)`.
     */
    function fieldPlacement(layout, frame) {
        const l = layout || {};
        const scale = num(l.scale, 1) || 1;
        const rotation = num(l.rotation, 0);
        const fw = num(frame && frame.w, 0), fh = num(frame && frame.h, 0);
        if (!l.side || l.side === 'custom') {
            return { tx: num(l.x, 0), ty: num(l.y, 0), lx: 0, ly: 0, scale, rotation, custom: true };
        }
        const parts = String(l.side).split('-');
        const ySide = l.side === 'center' ? 'center' : parts[0];
        const xSide = l.side === 'center' ? 'center' : (parts[1] || 'center');
        const mx = num(l.marginX, 0), my = num(l.marginY, 0);
        const lx = xSide === 'left' ? mx : xSide === 'right' ? FIELD.w - mx - fw : (FIELD.w - fw) / 2;
        const ly = ySide === 'top' ? my : ySide === 'bottom' ? FIELD.h - my - fh : (FIELD.h - fh) / 2;
        return { tx: 0, ty: 0, lx, ly, scale, rotation, custom: false };
    }

    /** Ustawia położenie lewego-górnego rogu ramki na polu (px ekranu): przełącza layout na side:'custom'. */
    function setFieldPosition(layout, frame, x, y) {
        const l = layout || {};
        if (l.posLocked) return l;          // pozycja zablokowana na ekranie — żadna ścieżka jej nie zmienia
        const p = fieldPlacement(l, frame);
        // przy custom środek obrotu/skali to lewy-górny róg ramki → x,y wprost; przy side zachowujemy to samo
        // punktowe położenie: ekran = (tx,ty) + s·R·(lx,ly) + s·R·p
        if (p.custom) { l.x = Math.round(x * 10) / 10; l.y = Math.round(y * 10) / 10; }
        else { l.side = 'custom'; l.x = Math.round(x * 10) / 10; l.y = Math.round(y * 10) / 10; }
        return l;
    }

    /** Pozycja lewego-górnego rogu ramki na ekranie (px pola), uwzględnia skalę i obrót. */
    function fieldPosition(layout, frame) {
        const p = fieldPlacement(layout, frame);
        const th = p.rotation * Math.PI / 180, c = Math.cos(th), s = Math.sin(th);
        return {
            x: p.tx + p.scale * (p.lx * c - p.ly * s),
            y: p.ty + p.scale * (p.lx * s + p.ly * c)
        };
    }

    /** Rozmiar ramki na ekranie (px pola) z uwzględnieniem skali; obrót liczony jako obwiednia osiowa. */
    function frameScreenSize(layout, frame) {
        const l = layout || {};
        const s = num(l.scale, 1) || 1;
        const w = num(frame && frame.w, 0) * s, h = num(frame && frame.h, 0) * s;
        const th = num(l.rotation, 0) * Math.PI / 180;
        const c = Math.abs(Math.cos(th)), sn = Math.abs(Math.sin(th));
        return { w, h, bw: w * c + h * sn, bh: w * sn + h * c };
    }

    /**
     * Wyrównanie ramki do pola 1920×1080 (lub strefy bezpiecznej `safe` px od krawędzi).
     * mode: left | hcenter | right | top | vmiddle | bottom. Zachowuje drugą oś. Zwraca true, gdy zmieniono.
     * Działa na obwiedni osiowej (obrót ≠ 0) — dlatego liczy przesunięcie względem bieżącej obwiedni.
     */
    function alignFrame(layout, frame, mode, safe) {
        const l = layout || {};
        if (l.posLocked) return false;
        const m = num(safe, 0);
        const p = fieldPlacement(l, frame);
        const th = p.rotation * Math.PI / 180, c = Math.cos(th), s = Math.sin(th);
        const fw = num(frame && frame.w, 0) * p.scale, fh = num(frame && frame.h, 0) * p.scale;
        // narożniki ramki na ekranie względem lewego-górnego rogu (origin = fieldPosition)
        const pts = [[0, 0], [fw, 0], [0, fh], [fw, fh]].map(([x, y]) => [x * c - y * s, x * s + y * c]);
        const minX = Math.min(...pts.map(q => q[0])), maxX = Math.max(...pts.map(q => q[0]));
        const minY = Math.min(...pts.map(q => q[1])), maxY = Math.max(...pts.map(q => q[1]));
        const pos = fieldPosition(l, frame);
        let nx = pos.x, ny = pos.y;
        if (mode === 'left') nx = m - minX;
        else if (mode === 'right') nx = FIELD.w - m - maxX;
        else if (mode === 'hcenter') nx = (FIELD.w - (maxX - minX)) / 2 - minX;
        else if (mode === 'top') ny = m - minY;
        else if (mode === 'bottom') ny = FIELD.h - m - maxY;
        else if (mode === 'vmiddle') ny = (FIELD.h - (maxY - minY)) / 2 - minY;
        else return false;
        setFieldPosition(l, frame, Math.round(nx), Math.round(ny));
        return true;
    }

    return {
        VERSION, LAYER_TYPES, BG_EFFECTS, FIELD,
        fieldPlacement, setFieldPosition, fieldPosition, frameScreenSize, alignFrame,
        createScene, createLayer, normalize,
        compile, applyToElement, detach,
        fromLegacyScene, fromElement
    };
});
