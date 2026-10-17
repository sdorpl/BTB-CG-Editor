/**
 * SceneConverter — konwersja starego elementu (kod HTML/CSS/JS pisany ręcznie) na scenę edytowalną
 * w Kreatorze Scen, z zachowaniem wymiarów i położenia na polu 1920×1080.
 *
 * Metoda: element jest renderowany rzeczywistym rendererem (`__cgRenderer.renderPreview`) w ukrytym polu
 * 1920×1080, animacje są doprowadzane do stanu końcowego, a następnie z drzewa DOM odczytywane są
 * gotowe, obliczone geometrie i style (tła, ramki, cienie, teksty, obrazy, zegary, tickery).
 * Każdy widoczny fragment staje się warstwą sceny; położenie ramki sceny na polu (layout) jest dobierane tak,
 * by zawartość wylądowała dokładnie tam, gdzie była.
 *
 * API:
 *   SceneConverter.convert(el, { settings })  → { scene, layout, warnings, stats } | null
 *   SceneConverter.convertAsync(el, opts)     → to samo, po załadowaniu fontów
 *   SceneConverter.apply(el, result)          → zapisuje scenę + layout w elemencie (z kopią codeBackup)
 *   SceneConverter.compare(el, converted)     → { diff, pixels }  (opcjonalna weryfikacja: różnica renderów)
 */
(function (root) {
    'use strict';

    const FIELD_W = 1920, FIELD_H = 1080;
    let renderSeq = 0;      // unikalne id przy każdym renderze pomiarowym (renderer cache'uje po id)
    const TEXT_TAGS_SKIP = new Set(['STYLE', 'SCRIPT', 'LINK', 'META', 'TEMPLATE', 'NOSCRIPT', 'BR']);

    const num = (v, d) => { const n = parseFloat(v); return isFinite(n) ? n : d; };
    const r1 = (v) => Math.round(v * 10) / 10;

    // ── Kolory i style CSS ──────────────────────────────────────────────────────

    function colorAlpha(c) {
        if (!c || c === 'transparent') return 0;
        const m = String(c).match(/rgba?\(([^)]+)\)/);
        if (!m) return 1;
        const p = m[1].split(/[,/ ]+/).filter(Boolean);
        return p.length >= 4 ? num(p[3], 1) : 1;
    }

    function parseGradient(img) {
        if (!img || !/gradient\(/.test(img)) return null;
        const cols = img.match(/rgba?\([^)]+\)/g) || [];
        if (!cols.length) return null;
        const isLinear = /^linear-gradient/.test(img.trim());
        let angle = 180;
        const a = img.match(/linear-gradient\(\s*(-?[\d.]+)deg/);
        if (a) angle = num(a[1], 180);
        else {
            const t = img.match(/linear-gradient\(\s*to\s+([a-z\s]+?)\s*,/);
            if (t) {
                const k = t[1].trim();
                const map = { top: 0, right: 90, bottom: 180, left: 270, 'top right': 45, 'right top': 45,
                              'bottom right': 135, 'right bottom': 135, 'bottom left': 225, 'left bottom': 225,
                              'top left': 315, 'left top': 315 };
                angle = map[k] ?? 180;
            }
        }
        if (!isLinear) return { type: 'solid', color: cols[0] };   // radial/conic: przybliżenie pierwszym kolorem
        return cols.length === 1 ? { type: 'solid', color: cols[0] }
            : { type: 'gradient', color: cols[0], color2: cols[cols.length - 1], angle };
    }

    function parseShadow(s) {
        if (!s || s === 'none') return null;
        const first = s.split(/,(?![^(]*\))/)[0].trim();        // pierwszy cień, bez przecinków w rgb()
        if (/\binset\b/.test(first)) return null;
        const col = first.match(/rgba?\([^)]+\)/);
        const nums = (first.replace(/rgba?\([^)]+\)/, '').match(/-?[\d.]+px|-?0\b/g) || []).map(parseFloat);
        if (nums.length < 2) return null;
        return { x: nums[0], y: nums[1], blur: nums[2] || 0, color: col ? col[0] : 'rgba(0,0,0,0.5)' };
    }

    function parseRadius(cs, w, h) {
        const keys = ['borderTopLeftRadius', 'borderTopRightRadius', 'borderBottomRightRadius', 'borderBottomLeftRadius'];
        const v = keys.map(k => {
            const s = String(cs[k] || '0').split(' ')[0];
            if (s.endsWith('%')) return Math.round(num(s, 0) * Math.min(w, h) / 100);
            return Math.round(num(s, 0));
        });
        if (v.every(x => x === v[0])) return v[0];
        return v;
    }

    function firstFamily(f) {
        return String(f || 'Bahnschrift').split(',')[0].replace(/["']/g, '').trim() || 'Bahnschrift';
    }

    function fontOf(cs) {
        const size = num(cs.fontSize, 36);
        const lh = cs.lineHeight === 'normal' ? 1.2 : num(cs.lineHeight, size * 1.2) / size;
        return {
            family: firstFamily(cs.fontFamily), size: r1(size), weight: String(cs.fontWeight || '700'),
            color: cs.color || '#ffffff',
            transform: cs.textTransform && cs.textTransform !== 'none' ? cs.textTransform : 'none',
            align: 'left', valign: 'middle',
            letterSpacing: cs.letterSpacing === 'normal' ? 0 : r1(num(cs.letterSpacing, 0)),
            lineHeight: r1(lh), italic: cs.fontStyle === 'italic',
            shadow: cs.textShadow && cs.textShadow !== 'none' ? cs.textShadow : 'none'
        };
    }

    // Część liniowa skumulowanej macierzy transformacji → obrót i skos X (stopnie)
    function decompose(m) {
        const { a, b, c, d } = m;
        const det = a * d - b * c;
        if (!isFinite(det) || Math.abs(det) < 1e-6) return { rotation: 0, skew: 0 };
        if (Math.abs(b) < 1e-4 && Math.abs(a - 1) < 1e-3 && Math.abs(d - 1) < 1e-3) {
            return { rotation: 0, skew: r1(Math.atan(c) * 180 / Math.PI) };       // czysty skewX
        }
        const rotation = Math.atan2(b, a) * 180 / Math.PI;
        const skew = Math.atan((a * c + b * d) / det) * 180 / Math.PI;
        return { rotation: r1(rotation), skew: Math.abs(skew) < 0.05 ? 0 : r1(skew) };
    }

    function mulLinear(m, t) {
        // m, t = {a,b,c,d}; wynik = m · t
        return { a: m.a * t.a + m.c * t.b, b: m.b * t.a + m.d * t.b, c: m.a * t.c + m.c * t.d, d: m.b * t.c + m.d * t.d };
    }

    function ownLinear(cs) {
        const t = cs.transform;
        if (!t || t === 'none') return null;
        try {
            const m = new DOMMatrixReadOnly(t);
            return { a: m.a, b: m.b, c: m.c, d: m.d };
        } catch (_) { return null; }
    }

    // ── Wartości pól (mapowanie tekstów na pola) ────────────────────────────────

    function plain(html) {
        const d = document.createElement('div');
        d.innerHTML = String(html ?? '');
        return (d.textContent || '').replace(/\s+/g, ' ').trim();
    }

    function fieldCandidates(el) {
        const list = [];
        const add = (id, val) => {
            const t = plain(val);
            if (t && !list.some(c => c.id === id)) list.push({ id, text: t, key: t.toLowerCase() });
        };
        const f = el.fields || {};
        add('TITLE', f.TITLE || el.titleHtml || (el.titleLines?.length ? el.titleLines.map(l => l.text).join(' ') : '') || el.title);
        add('SUBTITLE', f.SUBTITLE || el.subtitle);
        (el.ocgInputs || []).forEach(i => {
            if (i.id === 'TITLE' || i.id === 'SUBTITLE' || (i.type && i.type !== 'text')) return;
            const v = f[i.id] !== undefined ? f[i.id] : i.default;
            if (typeof v === 'string') add(i.id, v);
        });
        Object.entries(f).forEach(([k, v]) => { if (typeof v === 'string' && !/^#|^\d+$/.test(v)) add(k, v); });
        return list;
    }

    // ── Render pomiarowy ────────────────────────────────────────────────────────

    function renderForMeasure(el, settings) {
        const stage = document.createElement('div');
        stage.style.cssText = 'position:fixed;left:-30000px;top:0;width:1920px;height:1080px;overflow:hidden;pointer-events:none;';
        document.body.appendChild(stage);

        const g = JSON.parse(JSON.stringify(el));
        g.id = 'scconv' + (++renderSeq) + '_' + String(el.id || 'x').replace(/[^a-zA-Z0-9]/g, '_');
        g.visible = true;
        g.layout = { ...(el.layout || {}), rotation: 0 };
        delete g.scene;                 // renderujemy kod, nie scenę
        delete g.motion;                // pomiar układu spoczynkowego: bez silnika WASM
        if (!g.title && !g.titleHtml && !(g.fields && g.fields.TITLE)) g.title = 'JAN KOWALSKI';
        if (!g.subtitle && (el.ocgInputs || []).some(i => i.id === 'SUBTITLE')) g.subtitle = 'Ekspert ds. Mediów';
        if (g.layout.width === undefined && g.layout.height === undefined) { /* bez zmian — renderer sam przyjmuje domyślne */ }

        const before = new Set(root.gsap ? root.gsap.globalTimeline.getChildren(false, true, true) : []);
        root.__cgRenderer.renderPreview(stage, [g], settings || {}, {});

        // stan końcowy: bez przejść i animacji CSS, tweeny GSAP na końcu (pętle zatrzymane na początku)
        const freeze = document.createElement('style');
        freeze.textContent = '*{transition:none!important;animation:none!important;}';
        stage.appendChild(freeze);
        if (root.gsap) {
            root.gsap.globalTimeline.getChildren(false, true, true).forEach(ch => {
                if (before.has(ch)) return;
                try {
                    const rep = ch.repeat ? ch.repeat() : 0;
                    if (rep === -1) ch.pause(0); else ch.progress(1);
                } catch (_) { /* tween usunięty */ }
            });
        }
        const rootEl = stage.querySelector('[id^="prev"]');
        return { stage, rootEl };
    }

    // ── Ekstrakcja warstw z DOM ─────────────────────────────────────────────────

    function extractLayers(stage, rootEl, el, scale) {
        const SM = root.SceneModel;
        const base = stage.getBoundingClientRect();
        const cand = fieldCandidates(el);
        const usedField = new Set();
        const layers = [];
        const warnings = [];
        let seq = 0;
        const id = (p) => `${p}${++seq}`;

        const isTickerEl = el.type === 'TICKER' || /ITEMS_B64/.test(el.html_template || '');
        const tickerDone = new Set();

        // współrzędne ekranowe (px pola) → współrzędne "lokalne" (przed layout.scale), tymczasowo względem stage
        const L = (v) => v / scale;

        function boxOf(node, cs) {
            const r = node.getBoundingClientRect();
            const isHtml = node instanceof HTMLElement;
            const w = isHtml && node.offsetWidth ? node.offsetWidth : L(r.width);
            const h = isHtml && node.offsetHeight ? node.offsetHeight : L(r.height);
            const cx = L(r.left - base.left + r.width / 2), cy = L(r.top - base.top + r.height / 2);
            return { cx, cy, w, h, rect: r };
        }

        function pushShape(props) {
            layers.push(SM.createLayer('shape', props));
        }

        function nameOf(node, fallback) {
            const c = (node.getAttribute && node.getAttribute('class') || '').trim().split(/\s+/)[0];
            return c || fallback;
        }

        function walk(node, ctx) {
            if (node.nodeType !== 1 || TEXT_TAGS_SKIP.has(node.tagName)) return;
            const cs = getComputedStyle(node);
            if (cs.display === 'none') return;
            if (node !== rootEl && cs.visibility === 'hidden') return;
            const own = num(cs.opacity, 1);
            const eff = ctx.opacity * own;
            if (eff < 0.01) return;

            const lin = ownLinear(cs);
            const cum = lin ? mulLinear(ctx.lin, lin) : ctx.lin;
            const tf = decompose(cum);

            const bx = boxOf(node, cs);
            const r = bx.rect;
            const zero = r.width < 0.5 || r.height < 0.5;
            const inField = zero || r.right > base.left && r.left < base.right && r.bottom > base.top && r.top < base.bottom;
            const clipped = !zero && ctx.clip && (r.right <= ctx.clip.left || r.left >= ctx.clip.right ||
                                         r.bottom <= ctx.clip.top || r.top >= ctx.clip.bottom);
            const next = { opacity: eff, lin: cum, clip: ctx.clip, squash: ctx.squash || node.classList.contains('slt-squash') };
            if (cs.overflow !== 'visible' && node !== rootEl) {
                const c = ctx.clip;
                next.clip = c ? { left: Math.max(c.left, r.left), top: Math.max(c.top, r.top),
                                  right: Math.min(c.right, r.right), bottom: Math.min(c.bottom, r.bottom) }
                              : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
            }
            if (!inField || clipped) return;

            const hasSize = bx.w >= 0.5 && bx.h >= 0.5;
            const name = nameOf(node, node.tagName.toLowerCase());
            const geo = { x: bx.cx - bx.w / 2, y: bx.cy - bx.h / 2, w: bx.w, h: bx.h, rotation: tf.rotation, skew: tf.skew };

            // ticker: kontener z torem przewijanych wiadomości → jedna warstwa ticker
            const cls = node.getAttribute('class') || '';
            if (isTickerEl && /(^|[-_\s])(track|marquee|scroller)([-_\s]|$)/i.test(cls)) {
                let vp = node.parentElement;
                while (vp && vp !== rootEl && getComputedStyle(vp).overflow === 'visible') vp = vp.parentElement;
                vp = vp && vp !== rootEl ? vp : node.parentElement;
                if (vp && !tickerDone.has(vp)) {
                    tickerDone.add(vp);
                    const vb = boxOf(vp, getComputedStyle(vp));
                    const sample = [...vp.querySelectorAll('*')].find(n => n.children.length === 0 && (n.textContent || '').trim()) || node.querySelector('*') || node;
                    const scs = getComputedStyle(sample);
                    const f = fontOf(scs);
                    f.align = 'left'; f.shadow = f.shadow || 'none';
                    layers.push(SM.createLayer('ticker', {
                        id: id('tk'), name: 'Ticker — ' + nameOf(vp, 'tor'),
                        x: vb.cx - vb.w / 2, y: vb.cy - vb.h / 2, w: vb.w, h: vb.h,
                        speed: num(el.speed, 100), separator: '•', gap: 48, font: f, opacity: eff
                    }));
                }
                return;
            }

            // ── tło / ramka / cień ──
            if (hasSize && node !== rootEl) {
                emitBox(node, cs, geo, tf, eff, name);
            } else if (hasSize && node === rootEl) {
                // sam korzeń (lt-root) zwykle nie ma własnego tła, ale może mieć
                emitBox(node, cs, geo, tf, eff, name);
            }

            // pseudo-elementy ::before (pod treścią)
            if (hasSize) emitPseudo(node, '::before', bx, eff, tf, name);

            // obrazy / wideo / svg
            if (node.tagName === 'IMG' && hasSize) {
                const src = node.getAttribute('src') || node.currentSrc || '';
                if (src) {
                    layers.push(SM.createLayer('image', { id: id('im'), name: 'Obraz — ' + name, ...geo, opacity: eff,
                        src, fit: cs.objectFit && cs.objectFit !== 'fill' ? cs.objectFit : 'fill',
                        radius: parseRadius(cs, bx.w, bx.h) }));
                }
                return;
            }
            if (node.tagName === 'VIDEO' && hasSize) {
                layers.push(SM.createLayer('video', { id: id('vd'), name: 'Wideo — ' + name, ...geo, opacity: eff,
                    src: node.getAttribute('src') || node.currentSrc || '', fit: cs.objectFit === 'contain' ? 'contain' : 'cover',
                    loop: node.loop, muted: true, radius: parseRadius(cs, bx.w, bx.h) }));
                return;
            }
            if (node.tagName.toLowerCase() === 'svg' && hasSize) {
                try {
                    const clone = node.cloneNode(true);
                    if (!clone.getAttribute('xmlns')) clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
                    clone.setAttribute('width', Math.round(bx.w)); clone.setAttribute('height', Math.round(bx.h));
                    const color = cs.color;
                    const svg = new XMLSerializer().serializeToString(clone).replace(/currentColor/g, color);
                    layers.push(SM.createLayer('image', { id: id('sv'), name: 'SVG — ' + name, ...geo, opacity: eff,
                        src: 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg), fit: 'contain' }));
                } catch (e) { warnings.push('SVG nie został przekonwertowany: ' + e.message); }
                return;
            }

            // ── teksty bezpośrednie ──
            const textNodes = [...node.childNodes].filter(n => n.nodeType === 3 && n.nodeValue.trim());
            if (textNodes.length) emitText(node, cs, textNodes, bx, geo, tf, eff, next.squash, name);

            [...node.children].forEach(ch => walk(ch, next));
            if (hasSize) emitPseudo(node, '::after', bx, eff, tf, name);
        }

        function emitBox(node, cs, geo, tf, eff, name) {
            const bgA = colorAlpha(cs.backgroundColor);
            const grad = parseGradient(cs.backgroundImage);
            const urlImg = /url\(/.test(cs.backgroundImage) ? (cs.backgroundImage.match(/url\(["']?([^"')]+)["']?\)/) || [])[1] : null;
            const shadow = parseShadow(cs.boxShadow);
            const bw = ['Top', 'Right', 'Bottom', 'Left'].map(s =>
                (cs['border' + s + 'Style'] !== 'none' && colorAlpha(cs['border' + s + 'Color']) > 0) ? num(cs['border' + s + 'Width'], 0) : 0);
            const uniform = bw[0] > 0 && bw.every(v => v === bw[0]);
            const radius = parseRadius(cs, geo.w, geo.h);
            const visibleFill = bgA > 0.01 || grad;

            if (visibleFill || uniform || shadow) {
                const fill = grad ? grad
                    : bgA > 0.01 ? { type: 'solid', color: cs.backgroundColor }
                    : { type: 'none' };
                pushShape({ id: id('sh'), name: 'Kształt — ' + name, ...geo, opacity: r1(eff),
                    fill, stroke: uniform ? { color: cs.borderTopColor, width: bw[0] } : { color: '#ffffff', width: 0 },
                    radius, shadow: shadow || false });
            }
            if (urlImg) {
                const sz = cs.backgroundSize;
                layers.push(SM.createLayer('image', { id: id('bg'), name: 'Tło obrazkowe — ' + name, ...geo, opacity: r1(eff),
                    src: urlImg, fit: sz === 'cover' ? 'cover' : sz === 'contain' ? 'contain' : 'fill',
                    radius }));
            }
            if (!uniform) {
                // pojedyncze krawędzie (np. akcent border-left) → cienkie prostokąty
                const sides = [
                    { i: 3, s: 'Left',   g: { x: geo.x, y: geo.y, w: bw[3], h: geo.h } },
                    { i: 1, s: 'Right',  g: { x: geo.x + geo.w - bw[1], y: geo.y, w: bw[1], h: geo.h } },
                    { i: 0, s: 'Top',    g: { x: geo.x, y: geo.y, w: geo.w, h: bw[0] } },
                    { i: 2, s: 'Bottom', g: { x: geo.x, y: geo.y + geo.h - bw[2], w: geo.w, h: bw[2] } }
                ];
                sides.forEach(({ i, s, g }) => {
                    if (bw[i] > 0) pushShape({ id: id('bd'), name: `Krawędź ${s.toLowerCase()} — ${name}`, ...g,
                        rotation: geo.rotation, skew: geo.skew, opacity: r1(eff),
                        fill: { type: 'solid', color: cs['border' + s + 'Color'] }, radius: 0 });
                });
            }
        }

        function emitPseudo(node, pseudo, bx, eff, tf, name) {
            let ps;
            try { ps = getComputedStyle(node, pseudo); } catch (_) { return; }
            if (!ps || ps.content === 'none' || ps.content === 'normal' || ps.display === 'none') return;
            const bgA = colorAlpha(ps.backgroundColor);
            const grad = parseGradient(ps.backgroundImage);
            if (bgA < 0.01 && !grad) {
                if (ps.content && ps.content !== '""') warnings.push(`Pseudo-element ${pseudo} ("${name}") z treścią nie został przeniesiony.`);
                return;
            }
            const w = num(ps.width, 0), h = num(ps.height, 0);
            if (w < 0.5 || h < 0.5) return;
            if (ps.position !== 'absolute' && ps.position !== 'fixed') { warnings.push(`Pseudo-element ${pseudo} ("${name}") nie jest pozycjonowany absolutnie — pominięto.`); return; }
            const nodeCs = getComputedStyle(node);
            const bl = num(nodeCs.borderLeftWidth, 0), bt = num(nodeCs.borderTopWidth, 0);
            const left = ps.left !== 'auto' ? num(ps.left, 0) : (ps.right !== 'auto' ? bx.w - num(ps.right, 0) - w - bl : 0);
            const top = ps.top !== 'auto' ? num(ps.top, 0) : (ps.bottom !== 'auto' ? bx.h - num(ps.bottom, 0) - h - bt : 0);
            const g = { x: bx.cx - bx.w / 2 + bl + left, y: bx.cy - bx.h / 2 + bt + top, w, h, rotation: tf.rotation, skew: tf.skew };
            pushShape({ id: id('ps'), name: `${pseudo} — ${name}`, ...g, opacity: r1(eff),
                fill: grad || { type: 'solid', color: ps.backgroundColor }, radius: parseRadius(ps, w, h),
                shadow: parseShadow(ps.boxShadow) || false });
        }

        function emitText(node, cs, textNodes, bx, geo, tf, eff, squash, name) {
            const range = document.createRange();
            range.setStartBefore(textNodes[0]);
            range.setEndAfter(textNodes[textNodes.length - 1]);
            const rr = range.getBoundingClientRect();
            if (rr.width < 0.5 || rr.height < 0.5) return;
            const raw = textNodes.map(n => n.nodeValue).join(' ').replace(/\s+/g, ' ').trim();
            const font = fontOf(cs);
            const lineH = num(cs.fontSize, 36) * font.lineHeight;
            const multiline = L(rr.height) > lineH * 1.6;

            // pole tekstu = pole treści elementu blokowego (zachowuje wyrównanie), inaczej obrys tekstu
            const blockish = /^(block|flex|inline-block|grid|flow-root|list-item|inline-flex|table-cell)$/.test(cs.display);
            let box;
            if (blockish && bx.w > 0) {
                const pl = num(cs.paddingLeft, 0) + num(cs.borderLeftWidth, 0), pr = num(cs.paddingRight, 0) + num(cs.borderRightWidth, 0);
                const pt = num(cs.paddingTop, 0) + num(cs.borderTopWidth, 0), pb = num(cs.paddingBottom, 0) + num(cs.borderBottomWidth, 0);
                box = { x: bx.cx - bx.w / 2 + pl, y: bx.cy - bx.h / 2 + pt, w: Math.max(1, bx.w - pl - pr), h: Math.max(1, bx.h - pt - pb) };
            } else {
                box = { x: L(rr.left - base.left), y: L(rr.top - base.top), w: L(rr.width), h: L(rr.height) };
            }
            const tx = L(rr.left - base.left), ty = L(rr.top - base.top), tw = L(rr.width), th = L(rr.height);
            // wyrównanie z pomiaru położenia tekstu w polu
            const lg = tx - box.x, rg = (box.x + box.w) - (tx + tw);
            font.align = tw >= box.w - 2 ? (cs.textAlign === 'right' || cs.textAlign === 'end' ? 'right' : 'left')
                : Math.abs(lg - rg) <= 2 ? 'center' : (lg < rg ? 'left' : 'right');
            const tg = ty - box.y, bg = (box.y + box.h) - (ty + th);
            font.valign = Math.abs(tg - bg) <= 2 ? 'middle' : (tg < bg ? 'top' : 'bottom');
            if (multiline) {
                warnings.push(`Tekst „${raw.slice(0, 40)}” jest wielowierszowy — scena renderuje tekst w jednej linii (nowrap).`);
                font.valign = 'middle';
            }

            const key = raw.toLowerCase();
            const clock = /^\d{1,2}:\d{2}(:\d{2})?$/.test(raw) &&
                (el.type === 'CLOCK' || /clock|time|zegar|hour/i.test((node.getAttribute('class') || '') + ' ' + (node.id || '')));
            if (clock) {
                layers.push(SM.createLayer('clock', { id: id('ck'), name: 'Zegar — ' + name, ...box,
                    rotation: tf.rotation, skew: tf.skew, opacity: r1(eff),
                    format: raw.split(':').length === 3 ? 'HH:MM:SS' : 'HH:MM', font }));
                return;
            }
            const match = cand.find(c => !usedField.has(c.id) && c.key === key.replace(/\s+/g, ' '));
            const props = { id: id('tx'), name: (match ? match.id : 'Tekst') + ' — ' + name, ...box,
                rotation: tf.rotation, skew: tf.skew, opacity: r1(eff), font, squash: !!squash };
            if (match) {
                usedField.add(match.id);
                Object.assign(props, { field: match.id, defaultText: raw });
            } else {
                Object.assign(props, { field: null, text: raw });
            }
            layers.push(SM.createLayer('text', props));
        }

        const rootCs = getComputedStyle(rootEl);
        walk(rootEl, { opacity: 1, lin: { a: 1, b: 0, c: 0, d: 1 }, clip: null, squash: false });
        void rootCs;
        return { layers, warnings };
    }

    // ── API ─────────────────────────────────────────────────────────────────────

    // Etap 1: render pomiarowy (po nim szablon może jeszcze dopisać treść, np. tor tickera)
    function prepare(el, opts) {
        if (!root.SceneModel || !root.__cgRenderer || !document.body) return null;
        return renderForMeasure(el, (opts || {}).settings);
    }

    function convert(el, opts) {
        const rendered = prepare(el, opts);
        return rendered ? finish(el, rendered) : null;
    }

    // Etap 2: odczyt geometrii i stylów z DOM → scena
    function finish(el, rendered) {
        const SM = root.SceneModel;
        const scale = num(el.layout?.scale, 1) || 1;
        const rot = num(el.layout?.rotation, 0);
        const custom = !el.layout?.side || el.layout.side === 'custom';
        const p0 = custom ? { x: num(el.layout?.x, 0), y: num(el.layout?.y, 0) } : { x: 0, y: 0 };

        const { stage, rootEl } = rendered;
        let res;
        try {
            if (!rootEl) return null;
            res = extractLayers(stage, rootEl, el, scale);
        } finally {
            stage.remove();
        }
        const { layers, warnings } = res;
        if (!layers.length) return null;

        // ramka = suma obrysów warstw (zawartość widoczna)
        let x1 = Infinity, y1 = Infinity, x2 = -Infinity, y2 = -Infinity;
        layers.forEach(l => {
            x1 = Math.min(x1, l.x); y1 = Math.min(y1, l.y);
            x2 = Math.max(x2, l.x + l.w); y2 = Math.max(y2, l.y + l.h);
        });
        // L/T w pikselach pola (ekranowych): współrzędne lokalne × scale
        const sx = x1 * scale, sy = y1 * scale;
        layers.forEach(l => {
            l.x = Math.round((l.x - x1) * 10) / 10;
            l.y = Math.round((l.y - y1) * 10) / 10;
            l.w = Math.round(l.w * 10) / 10;
            l.h = Math.round(l.h * 10) / 10;
        });
        const frame = { w: Math.max(1, Math.ceil(x2 - x1)), h: Math.max(1, Math.ceil(y2 - y1)) };

        // pozycja nowej ramki: screen = p0 + s·R·p. Zmierzony lewy-górny róg (bez obrotu) = sx,sy w polu.
        // Kotwica custom zachowuje stary punkt obrotu/skali (0,0), więc: nowy = p0 + R(rot)·(L − p0)
        const th = rot * Math.PI / 180, cos = Math.cos(th), sin = Math.sin(th);
        const dx = sx - p0.x, dy = sy - p0.y;
        const nx = p0.x + dx * cos - dy * sin, ny = p0.y + dx * sin + dy * cos;

        const layout = { ...(el.layout || {}), side: 'custom', x: Math.round(nx * 10) / 10, y: Math.round(ny * 10) / 10,
                         width: frame.w, height: frame.h };
        const scene = SM.normalize(SM.createScene({ frame, layers }));
        return { scene, layout, warnings, stats: { layers: layers.length, frame, origin: { x: layout.x, y: layout.y } } };
    }

    // Wersja zalecana: czeka na fonty i na treść doklejaną asynchronicznie przez kod szablonu
    async function convertAsync(el, opts) {
        const rendered = prepare(el, opts);
        if (!rendered) return null;
        try {
            await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
            await new Promise(r => setTimeout(r, 400));
            if (document.fonts && document.fonts.ready) await document.fonts.ready;
        } catch (_) { /* brak API */ }
        return finish(el, rendered);
    }

    function apply(el, result) {
        if (!el || !result) return el;
        const SM = root.SceneModel;
        if (!el.codeBackup && (el.html_template || el.css_template || el.js_template)) {
            el.codeBackup = { html_template: el.html_template, css_template: el.css_template, js_template: el.js_template,
                              ocgInputs: JSON.parse(JSON.stringify(el.ocgInputs || [])),
                              layout: JSON.parse(JSON.stringify(el.layout || {})) };
        }
        delete el._sceneData;
        SM.applyToElement(el, result.scene, { resetValues: false });
        el.layout = { ...(el.layout || {}), ...result.layout };
        return el;
    }

    root.SceneConverter = { convert, convertAsync, apply, _render: renderForMeasure, _extract: extractLayers };
})(typeof window !== 'undefined' ? window : this);
