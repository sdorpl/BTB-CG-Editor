; (function () {
    'use strict';

    // Injection of global styles for automatic text squashing (scaleX)
    // and browser-default reset (matches Tailwind preflight to ensure consistent rendering
    // in output.html which does not load Tailwind).
    const style = document.createElement('style');
    style.textContent = `
        .slt-squash {
            display: inline-block !important;
            transform-origin: left center !important;
            white-space: nowrap !important;
        }
        .lt-root p, .lt-root h1, .lt-root h2, .lt-root h3, .lt-root h4,
        .lt-root ul, .lt-root ol, .lt-root li, .lt-root blockquote {
            margin: 0;
            padding: 0;
        }
    `;
    document.head.appendChild(style);

    // Ręczne ściskanie (.cg-text-squash = transform scaleX z Edytora Tekstu). Transform nie zmienia
    // szerokości w układzie, więc ściśnięty tekst zostawiał pustą lukę po prawej, a .slt-squash mierzył
    // szerokość sprzed ściśnięcia — tekst kończył się przed krawędzią belki. Tu:
    //  1) tekst zawijany łamie się na szerokości kontenera / s, żeby po ściśnięciu wypełnił całą szerokość,
    //  2) ujemny margin-right zrównuje szerokość w układzie z szerokością wizualną.
    // Używane też przez Edytor Tekstu (wysiwyg-engine.js) na żywym DOM edytora.
    function fitManualSquash(root) {
        if (!root) return;
        root.querySelectorAll('.cg-text-squash').forEach(el => {
            el.style.width = '';
            el.style.marginRight = '';
            const m = /scaleX\(([\d.]+)\)/.exec(el.style.transform || '');
            const s = m ? parseFloat(m[1]) : 1;
            if (!(s > 0 && s < 1)) return;
            if (!/nowrap|pre$/.test(getComputedStyle(el).whiteSpace)) {
                // Najbliższy blok (kontener zawijania) — rodzic może być inline (np. <strong>).
                let host = el.parentElement;
                while (host && host !== root && getComputedStyle(host).display === 'inline') host = host.parentElement;
                if (host) {
                    const hs = getComputedStyle(host);
                    const avail = host.clientWidth - (parseFloat(hs.paddingLeft) || 0) - (parseFloat(hs.paddingRight) || 0);
                    if (avail > 0 && el.offsetWidth >= avail - 1) {
                        el.style.width = 'max-content';
                        const natural = el.offsetWidth;
                        el.style.width = Math.min(natural, avail / s) + 'px';
                    }
                }
            }
            el.style.marginRight = (-(1 - s) * el.offsetWidth) + 'px';
        });
    }
    // Interlinia wielowierszowego tytułu: wiersz (blok <div>/<p>) składający się wyłącznie z tekstu w <span style="font-size:…">
    // ma strut o rozmiarze fontu BLOKU (np. 48 px), więc mniejsza linia (26 px) dostaje 58-pikselowy wiersz i „ogromny margines” nad/pod
    // sobą. Przenosimy rozmiar na blok (tylko wartości bezwzględne i jeden rozmiar w bloku) — wiersz liczy się wtedy z własnego fontu.
    // Zmiana tylko stylu elementu `root` (bloki bezpośrednie); `data-cg-hoist` oznacza style nałożone przez nas (można je cofnąć).
    const HOIST_ABS = /^(\d+(\.\d+)?(px|pt)|xx-small|x-small|small|medium|large|x-large|xx-large|xxx-large)$/i;
    function hoistBlockFonts(root) {
        if (!root) return 0;
        const blocks = [...root.children].filter(n => n.tagName === 'DIV' || n.tagName === 'P');
        let n = 0;
        blocks.forEach(b => {
            const ours = b.hasAttribute('data-cg-hoist');
            if (b.style.fontSize && !ours) return;                    // rozmiar ustawiony świadomie na bloku
            let size = null, ok = true, any = false;
            const w = document.createTreeWalker(b, NodeFilter.SHOW_TEXT);
            while (ok && w.nextNode()) {
                const tn = w.currentNode;
                if (!tn.nodeValue.trim()) continue;
                any = true;
                let el = tn.parentElement, fs = null;
                while (el && el !== b) { if (el.style && el.style.fontSize) { fs = el.style.fontSize; break; } el = el.parentElement; }
                if (!fs || !HOIST_ABS.test(fs) || (size !== null && size !== fs)) ok = false; else size = fs;
            }
            if (any && ok && size) { b.style.fontSize = size; b.setAttribute('data-cg-hoist', '1'); n++; }
            else if (ours) { b.style.fontSize = ''; b.removeAttribute('data-cg-hoist'); }
        });
        return n;
    }
    /** Wersja dla tekstu HTML (renderer): tylko gdy są co najmniej 2 bloki (jednoliniowe tytuły bez zmian). */
    function hoistBlockFontsHtml(html) {
        if (typeof html !== 'string' || !/<(div|p)\b/i.test(html)) return html;
        const tmp = document.createElement('div');
        tmp.innerHTML = html;
        if ([...tmp.children].filter(n => n.tagName === 'DIV' || n.tagName === 'P').length < 2) return html;
        return hoistBlockFonts(tmp) ? tmp.innerHTML : html;
    }
    window.CGTextSquash = { fitManual: fitManualSquash, hoistBlockFonts, hoistBlockFontsHtml };

    function applyGlobalSquashing(root) {
        if (!root) return;
        fitManualSquash(root);
        const enabled = root.getAttribute('data-squash-enabled') !== 'false';
        if (!enabled) {
            // If disabled, reset any existing transforms on squashing targets
            root.querySelectorAll('.slt-squash').forEach(t => t.style.transform = 'none');
            return;
        }
        // BADGE: ściskaj tylko gdy tekst ma powyżej 25 znaków (niezależnie od tego, czy
        // mieści się w kontenerze), zamiast tylko po przepełnieniu szerokości.
        const lengthGated = root.getAttribute('data-squash-mode') === 'length';
        const targets = root.querySelectorAll('.slt-squash');
        targets.forEach(target => {
            const container = target.parentElement;
            if (!container) return;

            // Reset scale to measure natural width
            target.style.transform = 'none';
            target.style.marginRight = '';

            const len = (target.textContent || '').trim().length;
            if (lengthGated && len <= 25) {
                return;
            }

            const textWidth = target.scrollWidth;

            if (lengthGated) {
                // Kontener (np. badge inline-flex) rozszerza się do szerokości tekstu, więc
                // nie ma tu nigdy „przepełnienia" do zmierzenia. Ściskamy proporcjonalnie do
                // tego, o ile znaków tekst przekracza limit 25 (przybliżenie: szerokość na znak
                // jest w przybliżeniu stała w danym foncie/rozmiarze).
                const scale = Math.max(0.5, 25 / len);
                if (scale < 1) {
                    target.style.transform = `scaleX(${scale})`;
                    // transform nie zmienia szerokości w layoucie — kontener inline-flex
                    // (np. .na-zywo-container) został by szerszy niż ściśnięty tekst,
                    // zostawiając puste miejsce po prawej. Kompensujemy ujemnym marginesem.
                    target.style.marginRight = (-(1 - scale) * textWidth) + 'px';
                }
                return;
            }

            const containerWidth = container.clientWidth;
            if (textWidth > containerWidth && containerWidth > 0) {
                const scale = containerWidth / textWidth;
                target.style.transform = `scaleX(${scale})`;
            }
        });
    }

    // Ściskanie mierzy szerokość tekstu — przed załadowaniem fontu (np. Bahnschrift z @font-face) wynik jest błędny,
    // a po wczytaniu fontu tekst się poszerza i wychodzi poza warstwę. Dlatego liczymy ponownie, gdy fonty gotowe.
    function scheduleSquashing(rootEl) {
        requestAnimationFrame(() => applyGlobalSquashing(rootEl));
        if (document.fonts && document.fonts.ready) {
            const again = () => { if (rootEl.isConnected) applyGlobalSquashing(rootEl); };
            document.fonts.ready.then(() => requestAnimationFrame(again));
            if (document.fonts.addEventListener) document.fonts.addEventListener('loadingdone', () => requestAnimationFrame(again), { once: true });
        }
    }

    // ── Fonty elementu (D10, M3) ───────────────────────────────────────────────────────────────────────────────
    const FONT_SAMPLE = 'Zażółć gęślą jaźń ĄĆĘŁŃÓŚŹŻ 0123456789';
    const GENERIC_FONTS = /^(sans-serif|serif|monospace|cursive|fantasy|system-ui|inherit|initial|unset)$/i;
    const cleanFamily = (f) => String(f || '').split(',')[0].replace(/['"]/g, '').trim();
    /** Pary krój/waga/kursywa używane przez element: warstwy sceny, formatowanie inline w HTML pól, typografia, czcionka globalna. */
    function collectFontSpecs(graphic, settings) {
        const out = new Map();
        const add = (family, weight, italic) => {
            family = cleanFamily(family);
            if (!family || GENERIC_FONTS.test(family) || /\{\{/.test(family)) return;
            let w = String(weight == null || weight === '' ? 400 : weight).toLowerCase();
            w = w === 'bold' ? '700' : w === 'normal' ? '400' : (parseInt(w, 10) ? String(parseInt(w, 10)) : '400');
            out.set(`${italic ? 'italic ' : ''}${w} 40px "${family}"`, true);
        };
        const g = graphic || {};
        const baseFamilies = [];
        const baseWeights = new Set(['400']);
        const layers = g.scene && !g.scene.detached ? (g.scene.layers || []) : [];
        layers.filter(l => l.visible !== false && l.font).forEach(l => {
            const f = l.font;
            baseFamilies.push(f.family);
            baseWeights.add(String(f.weight || 400));
            add(f.family, f.weight, f.italic);
        });
        const typo = g.style && g.style.typography || {}, sub = g.style && g.style.subtitleTypography || {};
        [typo, sub].forEach(t => { if (t.fontFamily) { baseFamilies.push(t.fontFamily); add(t.fontFamily, t.fontWeight, false); } });
        const global = settings && settings.globalFontFamily && (settings.globalFontGraphics || []).includes(g.id) ? settings.globalFontFamily : null;
        if (global) { baseFamilies.push(global); add(global, typo.fontWeight, false); }
        [g.fontFamily, g.fields && g.fields.FONT_FAMILY, g.fields && g.fields.fontFamily].forEach(f => { if (f) { baseFamilies.push(f); add(f, typo.fontWeight, false); } });
        // formatowanie inline z Edytora Tekstu (kroje, grubości, pogrubienie, kursywa) w tytule i polach
        const htmls = [g.titleHtml, g.fields && g.fields.TITLE, g.fields && g.fields.SUBTITLE, ...Object.values(g.fields || {})].filter(v => typeof v === 'string' && v.includes('<'));
        htmls.forEach(h => {
            const fams = [...h.matchAll(/font-family\s*:\s*([^;"']+(?:'[^']*')?[^;"]*)/gi)].map(m => cleanFamily(m[1]));
            const ws = [...h.matchAll(/font-weight\s*:\s*(\d+|bold|normal)/gi)].map(m => m[1]);
            const bold = /<(strong|b)\b/i.test(h), ital = /<(em|i)\b/i.test(h) || /font-style\s*:\s*italic/i.test(h);
            const famSet = new Set([...fams, ...baseFamilies].filter(Boolean));
            const wSet = new Set([...baseWeights, ...ws, ...(bold ? ['700'] : [])]);
            famSet.forEach(f => wSet.forEach(w => { add(f, w, false); if (ital) add(f, w, true); }));
        });
        return [...out.keys()];
    }
    /** Ładuje kroje elementu. Zwraca Promise<{ ok, missing[] }>; nigdy nie odrzuca. Limit czasu domyślnie 2,5 s. */
    function ensureFonts(specs, timeoutMs) {
        if (!document.fonts || !specs.length) return Promise.resolve({ ok: true, missing: [] });
        const declared = new Set([...document.fonts].map(f => f.family.replace(/['"]/g, '')));
        const todo = specs.filter(s => declared.has(/"([^"]+)"$/.exec(s)[1]));
        if (!todo.length) return Promise.resolve({ ok: true, missing: [] });
        const loading = Promise.all(todo.map(s => document.fonts.load(s, FONT_SAMPLE).catch(() => []))).then(() => true);
        const timer = new Promise(res => setTimeout(() => res(false), timeoutMs || 2500));
        return Promise.race([loading, timer]).then((done) => {
            const missing = todo.filter(s => {
                const fam = /"([^"]+)"$/.exec(s)[1];
                return ![...document.fonts].some(f => f.family.replace(/['"]/g, '') === fam && f.status === 'loaded');
            });
            if (!done || missing.length) console.warn('[fonts] element nie czeka dłużej na kroje:', missing.length ? missing : todo);
            return { ok: done && !missing.length, missing };
        });
    }
    function ensureFontsFor(graphic, settings) {
        try { return ensureFonts(collectFontSpecs(graphic, settings)); } catch (e) { console.error('[fonts]', e); return Promise.resolve({ ok: false, missing: [] }); }
    }
    window.CGFonts = { collectFontSpecs, ensureFonts, ensureFontsFor };

    // ── Runtime WASM btb-motion (M2.3, opcja B) ────────────────────────────────────────────────────────
    // Elementy z artefaktem `element.motion` (klucze, ścieżka ruchu — funkcje spoza CGMotion) grają silnikiem btb-motion
    // z edytora; pozostałe jak dotąd przez CGMotion. `settings.motionEngine === 'cgmotion'` wyłącza WASM (domyślnie 'auto').
    // Każda awaria (ładowanie WASM, tworzenie instancji) = powrót do CGMotion, nigdy czarny ekran.
    const MOTION_BASE = '/vendor/btb-motion/';
    function wantsWasmMotion(graphic, settings) {
        const m = graphic && graphic.motion;
        return !!(m && m.version === 1 && m.director && typeof m.director === 'object' && typeof WebAssembly !== 'undefined'
            && !(settings && settings.motionEngine === 'cgmotion'));
    }
    function loadMotionRuntime() {
        if (window.__btbRuntime) return window.__btbRuntime;
        window.__btbRuntime = (async () => {
            if (!window.BtbMotion) {
                await new Promise((res, rej) => {
                    const sc = document.createElement('script');
                    sc.src = MOTION_BASE + 'btb-motion-player.js';
                    sc.onload = res; sc.onerror = () => rej(new Error('nie wczytano btb-motion-player.js'));
                    document.head.appendChild(sc);
                });
            }
            const r = await fetch(MOTION_BASE + 'btb_motion_wasm.wasm');
            if (!r.ok) throw new Error('btb_motion_wasm.wasm: HTTP ' + r.status);
            return window.BtbMotion.compile(await r.arrayBuffer());
        })();
        window.__btbRuntime.catch(() => { window.__btbRuntime = null; });   // kolejna próba przy następnym elemencie
        return window.__btbRuntime;
    }
    /** Uruchamia animację IN silnikiem WASM na zamontowanym elemencie. true = przejęte, false = użyj CGMotion. */
    async function startWasmMotion(rootEl, data, opts) {
        const instant = !!(opts && opts.instant);
        try {
            const mod = await Promise.race([loadMotionRuntime(), new Promise((_, rej) => setTimeout(() => rej(new Error('timeout ładowania WASM')), 3000))]);
            if (!rootEl.isConnected) return true;                                  // element już zdjęty
            const m = data.motion;
            const fx = rootEl.querySelector('.cg-scene') || rootEl;
            const p = await window.BtbMotion.create(mod, { director: m.director, tickers: m.tickers || [], squash: m.squash, fit: null, manual: instant, embedded: { root: rootEl, fx } });
            if (!rootEl.isConnected) { p.destroy(); return true; }
            rootEl.__btb = p;
            if (opts && opts.onEnd) p.on('end', opts.onEnd);
            rootEl.__slt_hide = () => new Promise((res) => { if (!p.visible) return res(); p.on('end', () => res()); p.off(); });
            p.take();
            if (instant) {
                // podgląd statyczny: przewiń do punktu Stop (hold) i zostaw nieruchomo (bez pętli rAF)
                for (let i = 0; i < 400 && p.visible && !p.holding; i++) p.tick(0.05);
                if (!p.holding) { p.destroy(); rootEl.__btb = null; return false; }
            }
            return true;
        } catch (e) {
            console.error('[btb-motion] powrót do CGMotion:', e);
            return false;
        }
    }

    const container = document.getElementById('render-container');
    // Socket only needed in output.html (where container exists)
    const socket = (typeof io !== 'undefined' && container) ? io() : null;

    let activeGraphics = {}; // Map of graphic.id -> DOM element metadata

    function buildGraphicHash(graphic, settings) {
        return JSON.stringify({
            title: graphic.title, subtitle: graphic.subtitle,
            titleHtml: graphic.titleHtml, titleLines: graphic.titleLines,
            layout: graphic.layout, animation: graphic.animation,
            style: graphic.style, sideImage: graphic.sideImage,
            speed: graphic.speed, items: graphic.items, wiper: graphic.wiper,
            fields: graphic.fields, url: graphic.url, mediaType: graphic.mediaType,
            motion: graphic.motion, motionEngine: settings && settings.motionEngine,
            videoLoop: graphic.videoLoop, videoMuted: graphic.videoMuted,
            backgroundVideo: graphic.backgroundVideo, backgroundEffect: graphic.backgroundEffect,
            activeGlobalFontFamily: (settings && settings.globalFontGraphics && settings.globalFontGraphics.includes(graphic.id)) ? settings.globalFontFamily : null
        });
    }

    // --- Attachment helpers ---

    // Expand group:xxx refs and plain IDs into a flat list of graphic IDs
    function resolveAttachIds(rawIds, graphics) {
        const ids = Array.isArray(rawIds) ? rawIds : (rawIds ? [rawIds] : []);
        const result = [];
        ids.forEach(pid => {
            if (typeof pid === 'string' && pid.startsWith('group:')) {
                const gid = pid.slice(6);
                graphics.forEach(g => { if (g.groupId === gid) result.push(g.id); });
            } else if (pid) {
                result.push(pid);
            }
        });
        return result;
    }

    // Scale factor: 1920px logical space → viewport px
    // getBoundingClientRect accounts for CSS transforms on ancestors (e.g. preview canvasWrap scaled down),
    // while offsetWidth does not — must use the same coordinate space as getBoundingClientRect calls below.
    function getRenderScale(containerEl) {
        const el = containerEl || container;
        if (!el) return 1;
        const w = el.getBoundingClientRect().width;
        return w > 0 ? w / 1920 : 1;
    }

    // Bounding box of graphic content in 1920-space px
    // wrapperEl = the layoutStyleWrapper div (1920×1080 with CSS transform)
    function getContentBounds1920(wrapperEl, containerEl) {
        if (!wrapperEl) return null;
        const root = wrapperEl.querySelector('.lt-root');
        if (!root) return null;
        const rect = root.getBoundingClientRect();
        if (rect.width === 0 && rect.height === 0) return null;
        const cEl = containerEl || container;
        if (!cEl) return null;
        const cRect = cEl.getBoundingClientRect();
        const scale = getRenderScale(cEl);
        return {
            top:    (rect.top    - cRect.top)  / scale,
            bottom: (rect.bottom - cRect.top)  / scale,
            left:   (rect.left   - cRect.left) / scale,
            right:  (rect.right  - cRect.left) / scale,
            width:  rect.width  / scale,
            height: rect.height / scale,
        };
    }

    // Compute attach offsets for a single graphic.
    // instancesMap: { [id]: { el } } or { [id]: DOMElement } — supports both activeGraphics and renderPreview instances
    // containerEl: the render container (for scale calculation)
    function computeAttachOffsets(graphic, graphics, instancesMap, containerEl) {
        const layout = graphic.layout;
        if (!layout || layout.posLocked) return { offsetY: 0, offsetX: 0 };

        let offsetY = 0;
        let offsetX = 0;

        function getWrapperEl(id) {
            const entry = instancesMap[id];
            if (!entry) return null;
            return entry.el || entry; // activeGraphics stores {el}, renderPreview instances store raw el
        }

        function computeAxisOffset(rawIds, edgeField, gapField, axis) {
            const parentIds = resolveAttachIds(rawIds, graphics);
            const visiblePids = parentIds.filter(pid => {
                const p = graphics.find(g => g.id === pid);
                return p && p.visible;
            });
            if (!visiblePids.length) return 0;

            const edge = layout[edgeField] || 'auto';
            const gap  = layout[gapField]  || 0;

            if (edge === 'manual') return gap;

            // Union bounding box of all visible parents (handles group docking correctly)
            let parentBounds = null;
            for (const pid of visiblePids) {
                const b = getContentBounds1920(getWrapperEl(pid), containerEl);
                if (!b) continue;
                if (!parentBounds) {
                    parentBounds = { ...b };
                } else {
                    parentBounds.top    = Math.min(parentBounds.top,    b.top);
                    parentBounds.bottom = Math.max(parentBounds.bottom, b.bottom);
                    parentBounds.left   = Math.min(parentBounds.left,   b.left);
                    parentBounds.right  = Math.max(parentBounds.right,  b.right);
                    parentBounds.width  = parentBounds.right - parentBounds.left;
                    parentBounds.height = parentBounds.bottom - parentBounds.top;
                }
            }
            if (!parentBounds) return gap;

            const selfEntry = instancesMap[graphic.id];
            const selfEl = getWrapperEl(graphic.id);
            const selfBounds = getContentBounds1920(selfEl, containerEl);

            if (axis === 'y') {
                // selfBaseTop: natural position of child content without any attach offset.
                // Works for both custom (layout.y) and side/flexbox (measured) modes.
                const prevOffsetY = selfEntry?.lastOffsetY ?? 0;
                const selfBaseTop = selfBounds ? selfBounds.top - prevOffsetY : (layout.y || 0);

                let targetEdge = edge;
                if (edge === 'auto') {
                    // Pick edge based on where child naturally sits relative to parent centre
                    const parentMidY = (parentBounds.top + parentBounds.bottom) / 2;
                    targetEdge = selfBaseTop >= parentMidY ? 'bottom' : 'top';
                }
                if (targetEdge === 'bottom') {
                    return parentBounds.bottom - selfBaseTop + gap;
                } else {
                    const selfH = selfBounds ? selfBounds.height : 0;
                    return parentBounds.top - selfBaseTop - selfH + gap;
                }
            } else {
                const prevOffsetX = selfEntry?.lastOffsetX ?? 0;
                const selfBaseLeft = selfBounds ? selfBounds.left - prevOffsetX : (layout.x || 0);

                let targetEdge = edge;
                if (edge === 'auto') {
                    const parentMidX = (parentBounds.left + parentBounds.right) / 2;
                    targetEdge = selfBaseLeft >= parentMidX ? 'right' : 'left';
                }
                if (targetEdge === 'right') {
                    return parentBounds.right - selfBaseLeft + gap;
                } else {
                    const selfW = selfBounds ? selfBounds.width : 0;
                    return parentBounds.left - selfBaseLeft - selfW + gap;
                }
            }
        }

        if (layout.attachedToGraphicId)  offsetY = computeAxisOffset(layout.attachedToGraphicId,  'attachToEdgeY', 'attachOffsetY', 'y');
        if (layout.attachedToGraphicIdX) offsetX = computeAxisOffset(layout.attachedToGraphicIdX, 'attachToEdgeX', 'attachOffsetX', 'x');

        return { offsetY, offsetX };
    }

    // --- End attachment helpers ---

    function getLayoutTransform(layout, autoScale = 1, offsetY = 0, offsetX = 0) {
        const isCustom = !layout?.side || layout.side === 'custom';
        const x = isCustom ? (layout?.x || 0) : 0;
        const y = isCustom ? (layout?.y || 0) : 0;
        const finalY = y + offsetY;
        const finalX = x + offsetX;
        const scale = layout?.scale || 1;
        const rotation = layout?.rotation || 0;
        return `scale(${autoScale}) translate(${finalX}px, ${finalY}px) scale(${scale}) rotate(${rotation}deg)`;
    }

    function formatDimension(val, fallback = 'auto') {
        if (val === undefined || val === null || val === '') return fallback;
        const sVal = String(val).trim();
        // If it already has a unit (%, px, vh, etc.), return as is
        if (/[a-z%]$/i.test(sVal)) return sVal;
        // If it's a pure number, append px
        if (!isNaN(parseFloat(sVal)) && isFinite(sVal)) return `${sVal}px`;
        return sVal;
    }

    // Safely assign GSAP global (may not be available in all contexts)
    try { if (typeof gsap !== 'undefined') window.gsap = gsap; } catch (e) { }

    function initRenderer() {
        if (!socket) return;
        socket.on('initialState', (state) => {
            handleStateUpdate(state);
        });
        socket.on('stateUpdated', (state) => {
            handleStateUpdate(state);
        });
        socket.on('continueGraphic', (id) => continueGraphic(id));
    }

    /** Local Stop: „Dalej” dla elementu na wizji grającego silnikiem WASM (z ostatniego Stopu zaczyna się OUT). */
    function continueGraphic(id) {
        const meta = activeGraphics[id];
        const root = meta && document.getElementById(meta.instanceId);
        if (root && root.__btb && !meta.isHiding) { try { return root.__btb['continue'](); } catch (e) { console.error('[btb-motion] continue', e); } }
        return false;
    }

    function handleStateUpdate(state) {
        const graphics = state.graphics || [];
        const settings = state.settings || {};

        // Diffing
        const currentActiveIds = Object.keys(activeGraphics);
        const newActiveGraphics = graphics.filter(g => g.visible);
        const newActiveIds = newActiveGraphics.map(g => g.id);

        // 1. Remove hidden/deleted graphics
        currentActiveIds.forEach(id => {
            if (!newActiveIds.includes(id)) {
                hideGraphic(id);
            }
        });

        // runtime WASM ładujemy z wyprzedzeniem, żeby TAKE nie czekał na pobranie
        if (newActiveGraphics.some(g => wantsWasmMotion(g, settings))) loadMotionRuntime().catch(() => { });

        // 2. Add or Update visible graphics
        newActiveGraphics.forEach(graphic => {
            if (!activeGraphics[graphic.id]) {
                showGraphic(graphic, settings, graphics);
            } else {
                // It's already visible but properties might have changed (text editing)
                // For now, if text is edited live, we re-render it. 
                // In a pro system we'd diff text and update DOM, but re-rendering is easier for GSAP
                const existingHash = activeGraphics[graphic.id].hash;
                const newHash = buildGraphicHash(graphic, settings);

                if (existingHash !== newHash) {
                    // Content changed while on air (e.g. Trio "Save & Take"): swap the DOM
                    // instantly — no OUT animation / disappearing — then replay the IN
                    // animation with the new content.
                    const oldMeta = activeGraphics[graphic.id];
                    disposeInstance(oldMeta.el);
                    if (oldMeta.el && oldMeta.el.parentNode) oldMeta.el.parentNode.removeChild(oldMeta.el);
                    delete activeGraphics[graphic.id];
                    showGraphic(graphic, settings, graphics);
                }
            }
        });
        
        recalculateAttachments(graphics);
    }

    function showGraphic(data, settings = {}, allGraphics = []) {
        // Element graficzny jest samowystarczalny: kod i wartości ma w sobie
        const tpl = data;
        if (!tpl.html_template && !tpl.css_template && !tpl.js_template) return;

        const instanceId = `lt_${data.id.replace(/-/g, '')}`;
        const ctx = buildPreviewContext(data, tpl, instanceId, settings);

        const prepareStr = (str) => {
            try {
                return Handlebars.compile(str)(ctx);
            } catch (e) {
                console.error(e); return str;
            }
        };

        // Construct Layout Wrapper Element
        const layoutStyleWrapper = document.createElement('div');
        const autoScale = 1;
        
        // Initial offset: measure parent DOM (parent already in DOM, child not yet)
        // computeAttachOffsets can't measure the new child yet — recalculateAttachments
        // corrects after 50ms once the child is in DOM.
        const { offsetY: initialOffsetY, offsetX: initialOffsetX } =
            computeAttachOffsets(data, allGraphics, activeGraphics, container);

        // Apply layout transform to outer wrapper
        layoutStyleWrapper.style.transform = getLayoutTransform(data.layout, autoScale, initialOffsetY, initialOffsetX);
        layoutStyleWrapper.style.transformOrigin = '0 0';
        layoutStyleWrapper.style.position = 'absolute';
        layoutStyleWrapper.style.top = '0';
        layoutStyleWrapper.style.left = '0';
        layoutStyleWrapper.style.width = '1920px';
        layoutStyleWrapper.style.height = '1080px';
        layoutStyleWrapper.style.pointerEvents = 'none';

        // Set custom CSS variables
        layoutStyleWrapper.style.setProperty('--v-width', formatDimension(data.layout?.width, '90%'));
        layoutStyleWrapper.style.setProperty('--v-height', formatDimension(data.layout?.height, 'auto'));
        if (data.scene?.frame) {
            const fw = data.scene.frame.w, fh = data.scene.frame.h;
            if (data.layout?.width) layoutStyleWrapper.style.setProperty('--cg-sx', String(parseFloat(data.layout.width) / fw));
            if (data.layout?.height) layoutStyleWrapper.style.setProperty('--cg-sy', String(parseFloat(data.layout.height) / fh));
        }
        layoutStyleWrapper.style.zIndex = data.layout?.layer || 1;
        layoutStyleWrapper.style.opacity = data.style?.opacity ?? 1;

        // Inner container that holds the Graphic
        const innerContainer = document.createElement('div');
        innerContainer.style.width = '100%';
        innerContainer.style.height = '100%';
        innerContainer.style.position = 'absolute';
        innerContainer.style.top = '0';
        innerContainer.style.left = '0';

        // Inject HTML scoped with ID (per-graphic override takes priority)
        const htmlSource = tpl.html_template;
        const html = prepareStr(htmlSource);
        innerContainer.innerHTML = `<div id="${instanceId}" class="lt-root">${html}</div>`;
        layoutStyleWrapper.appendChild(innerContainer);

        // --- Animated background layer (backgroundVideo + backgroundEffect) ---
        injectBackgroundLayer(data, instanceId, layoutStyleWrapper);

        // Inject Custom CSS (scoped to instanceId, per-graphic override takes priority)
        const cssSource = tpl.css_template;
        if (cssSource) {
            let cssStr = prepareStr(cssSource);
            // Clean out any self-injected #ids if the template author put them there natively.
            // NOTE: We strip the *compiled* #instanceId that came from {{ID}} in the template.
            // The scoping regex below will then re-add it properly for all known class prefixes.
            cssStr = cssStr.replace(new RegExp(`#${instanceId}\\s+`, 'g'), '');

            // Advanced Scoping:
            // By prefixing common generic classes with #instanceId, we sandbox the CSS.
            // IMPORTANT: 'utk-' must be here so that .utk-wiper, .utk-container etc. get scoped.
            // Without it, those rules would be global after the strip above.
            // Lookbehind ensures we only scope the FIRST class in a compound selector (e.g. .utk-wiper.utk-move-right)
            // so that .a.b → #id .a.b, not #id .a#id .b (which is invalid and breaks class-toggled animations).
            cssStr = cssStr.replace(/(?<![a-zA-Z0-9_-])\.(rep-|lt-|modern-|na-zywo-|plate|title|subtitle|ticker|dot|news-|wiper-|utk-|tpl-|cg-)[a-zA-Z0-9_-]*/g, `#${instanceId} $&`);

            // @keyframes scoping: rename all @keyframes to include instanceId so that
            // multiple instances of the same template do NOT overwrite each other's animations.
            const keyframeNames = [];
            cssStr = cssStr.replace(/@keyframes\s+([\w-]+)/g, (match, name) => {
                const scopedName = `${name}_${instanceId}`;
                keyframeNames.push({ original: name, scoped: scopedName });
                return `@keyframes ${scopedName}`;
            });
            // Replace animation references to use the scoped keyframe names
            keyframeNames.forEach(({ original, scoped }) => {
                // Match animation property values that reference the original name
                cssStr = cssStr.replace(
                    new RegExp(`(animation:[^;]*?)\\b${original}\\b`, 'g'),
                    (match, prefix) => `${prefix}${scoped}`
                );
            });

            // Gradient override: if the graphic uses a gradient background, force it onto
            // container elements specifically (but NOT wiper, ticker-track, msg-box which have their own colors).
            const bgData = data.style?.background || {};
            const globalRadiusGraphics = settings?.globalRadiusGraphics || [];
            const isGlobalRadius = globalRadiusGraphics.includes(data.id);
            const borderRadius = isGlobalRadius ? (settings.globalBorderRadius || 0) : (bgData.borderRadius || 0);

            if (bgData.type === 'gradient') {
                const angle = bgData.gradientAngle || 135;
                const c1 = bgData.color || '#1e3a8a';
                const c2 = bgData.color2 || '#3b82f6';
                const gradientVal = `linear-gradient(${angle}deg, ${c1}, ${c2})`;
                // Target container/bar elements specifically, but EXCLUDE wiper/ticker elements
                // that have their own distinct background colors (wiper, msg-box, track, etc.)
                cssStr += `\n/* gradient override */`;
                cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]):not([class*="msg"]):not([class*="track"]), #${instanceId} [class*="-bar"], #${instanceId} [class*="wrapper"]:not([class*="utk"]), #${instanceId} [class*="plate"] { background: ${gradientVal} !important; background-color: transparent !important; }`;
            }

            if (isGlobalRadius || bgData.borderRadius > 0) {
                cssStr += `\n/* border radius override */`;
                cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]), #${instanceId} [class*="-bar"], #${instanceId} [class*="wrapper"]:not([class*="utk"]), #${instanceId} [class*="plate"] { border-radius: ${borderRadius}px !important; overflow: hidden !important; }`;
            }

            if (data.layout?.width) {
                cssStr += `\n/* width override */`;
                cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]):not([class*="track"]), #${instanceId} [class*="-bar"]:not([class*="utk"]), #${instanceId} [class*="wrapper"]:not([class*="utk"]):not([class*="wiper"]), #${instanceId} [class*="plate"] { width: var(--v-width) !important; box-sizing: border-box !important; }`;
            }

            if (data.layout?.height) {
                cssStr += `\n/* height override */`;
                cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]):not([class*="track"]), #${instanceId} [class*="-bar"]:not([class*="utk"]), #${instanceId} [class*="wrapper"]:not([class*="utk"]):not([class*="wiper"]), #${instanceId} [class*="plate"] { height: var(--v-height) !important; max-height: var(--v-height) !important; overflow: hidden !important; }`;
            }

            // Layout side override
            if (data.layout?.side && data.layout.side !== 'custom') {
                const parts = data.layout.side.split('-');
                const ySide = parts[0];
                const xSide = parts.length > 1 ? parts[1] : 'center';
                
                const yS = (data.layout.side === 'center') ? 'center' : ySide;
                const xS = (data.layout.side === 'center') ? 'center' : xSide;

                const mx = data.layout?.marginX || 0;
                const my = data.layout?.marginY || 0;

                cssStr += `\n/* Side Layout */`;
                cssStr += `\n#${instanceId} { display: flex; width: 100%; height: 100%; box-sizing: border-box; pointer-events: none; }`;
                cssStr += `\n#${instanceId} > * { pointer-events: auto; position: relative !important; top: auto !important; left: auto !important; right: auto !important; bottom: auto !important; margin: 0 !important; }`;
                cssStr += `\n#${instanceId} p, #${instanceId} div { margin: 0; padding: 0; }`;
                
                if (yS === 'top') cssStr += `\n#${instanceId} { align-items: flex-start; padding-top: ${my}px; }`;
                else if (yS === 'bottom') cssStr += `\n#${instanceId} { align-items: flex-end; padding-bottom: ${my}px; }`;
                else if (yS === 'center') cssStr += `\n#${instanceId} { align-items: center; }`;

                if (xS === 'left') cssStr += `\n#${instanceId} { justify-content: flex-start; padding-left: ${mx}px; }`;
                else if (xS === 'right') cssStr += `\n#${instanceId} { justify-content: flex-end; padding-right: ${mx}px; }`;
                else if (xS === 'center') cssStr += `\n#${instanceId} { justify-content: center; }`;
            }

            // Apply line-height from context
            cssStr += `\n#${instanceId} { line-height: ${ctx.LINE_HEIGHT || 1.4}; }`;

            const style = document.createElement('style');
            style.textContent = cssStr;
            layoutStyleWrapper.appendChild(style);
        }


        container.appendChild(layoutStyleWrapper);

        // Save metadata
        activeGraphics[data.id] = {
            el: layoutStyleWrapper,
            instanceId: instanceId,
            hash: buildGraphicHash(data, settings),
            isHiding: false,
            lastOffsetY: initialOffsetY,
            lastOffsetX: initialOffsetX,
        };

        // Execute internal JS & Animation wrapper (per-graphic override takes priority)
        const jsSource = tpl.js_template;
        const rootEl = document.getElementById(instanceId);
        if (rootEl && jsSource) {
            const jsCode = prepareStr(jsSource);

            try {
                // new Function() isolates template code from the caller's lexical scope,
                // preventing access to local variables inside showGraphic().
                const wrappedCode = [
                    'try {',
                    jsCode,
                    '} catch (e) {',
                    '    console.error("INNER TEMPLATE ERROR:", e);',
                    '}'
                ].join('\n');
                runTemplateJs(wrappedCode, document.getElementById(instanceId));

                // Do startu animacji IN (30 ms niżej) element nie ma jeszcze stanu początkowego —
                // bez ukrycia pokazywał się na klatkę w pełnej postaci i dopiero potem znikał (migotanie).
                rootEl.style.visibility = 'hidden';

                // D10 (M3): font dodany do projektu ładuje się RAZEM z belką — animacja IN i pomiary startują dopiero po załadowaniu
                // wszystkich krojów elementu (z limitem czasu; przy braku: ostrzeżenie + fallback), nie po „doczytaniu się” fontu.
                // Allow DOM repaints before executing GSAP show animations logic
                ensureFontsFor(data, settings).then(() => setTimeout(() => {
                    if (rootEl) {
                        rootEl.classList.add('active');
                        // Legacy support: also add .active to the first child (often .lt-container)
                        if (rootEl.firstElementChild) rootEl.firstElementChild.classList.add('active');
                        rootEl.setAttribute('data-squash-enabled', data.style?.typography?.squashEnabled !== false);
                        if (data.type === 'BADGE') rootEl.setAttribute('data-squash-mode', 'length');
                    }
                    const legacyShow = () => {
                        try {
                            if (rootEl.__slt_show) {
                                rootEl.__slt_show();
                            } else if (window.CGMotion && rootEl.querySelector('[data-cg-anim]')) {
                                // Deklaratywne animacje CGMotion — autoShow buduje timeline z atrybutów
                                const animInCfg = (() => { try { return JSON.parse(rootEl.getAttribute('data-cg-config') || '{}'); } catch { return {}; } })();
                                window.CGMotion.autoShow(rootEl, animInCfg);
                            } else {
                                rootEl.style.display = 'block';
                            }
                        } finally {
                            rootEl.style.visibility = '';
                        }
                        // Apply global text squashing after initial render/animation trigger
                        scheduleSquashing(rootEl);
                    };
                    if (wantsWasmMotion(data, settings)) {
                        // element zostaje ukryty (visibility) do startu pierwszej klatki silnika WASM
                        startWasmMotion(rootEl, data, {
                            // OUT zagrany przez „Dalej” z ostatniego Stopu (nie przez zdjęcie ze stanu): zdejmij element i zgłoś to serwerowi
                            onEnd: () => {
                                const meta = activeGraphics[data.id];
                                if (!meta || meta.isHiding || !meta.el.contains(rootEl)) return;
                                meta.isHiding = true;
                                removeElement(data.id, meta);
                                if (socket) socket.emit('graphicEnded', data.id);
                            }
                        }).then((ok) => {
                            if (!ok) { legacyShow(); return; }
                            rootEl.style.visibility = '';
                            scheduleSquashing(rootEl);
                        });
                    } else {
                        legacyShow();
                    }
                }, 30));
            } catch (e) {
                console.error("[CGMotion] Template JS error", e);
            }
        } else if (rootEl) {
            // No JS template, but still trigger active class for CSS-only templates
            setTimeout(() => {
                rootEl.classList.add('active');
                if (rootEl.firstElementChild) rootEl.firstElementChild.classList.add('active');
                rootEl.style.display = 'block';
            }, 30);
        }
    }

    function hideGraphic(id) {
        const meta = activeGraphics[id];
        if (!meta) return;
        if (meta.isHiding) {
            // Safety net: a previous hide never resolved (e.g. stalled animation
            // promise). Force removal now instead of swallowing this call forever.
            if (meta.hideStuckSince && (Date.now() - meta.hideStuckSince) > 3000) {
                removeElement(id, meta);
            }
            return;
        }

        meta.isHiding = true;
        meta.hideStuckSince = Date.now();
        const rootEl = document.getElementById(meta.instanceId);

        if (rootEl) {
            rootEl.classList.remove('active');
            if (rootEl.firstElementChild) rootEl.firstElementChild.classList.remove('active');
        }

        // Hide animation — CGMotion timeline or CSS transition fallback
        if (rootEl && rootEl.__slt_hide && typeof rootEl.__slt_hide === 'function') {
            try {
                const hideResult = rootEl.__slt_hide();
                // Parse duration from hash (default 0.5s + 50ms buffer)
                const dataHash = JSON.parse(meta.hash);
                const durationMs = ((dataHash.animation?.out?.duration) || 0.5) * 1000 + 50;

                // GSAP/Promise check
                if (hideResult && typeof hideResult.then === 'function') {
                    // Race against a hard timeout so a stalled/never-resolving
                    // animation promise (e.g. due to a concurrent content edit
                    // reflowing the animated subtree) can't block removal forever.
                    let settled = false;
                    const settle = () => {
                        if (settled) return;
                        settled = true;
                        removeElement(id, meta);
                    };
                    hideResult.then(settle).catch(e => {
                        console.error("Anim promise error", e);
                        settle();
                    });
                    setTimeout(settle, rootEl.__btb ? 30000 : durationMs * 2 + 1000);
                } else {
                    // If it's a standard CSS transition (most templates in db.json), 
                    // __slt_hide just applies CSS. We MUST wait the duration then remove.
                    setTimeout(() => {
                        removeElement(id, meta);
                    }, durationMs);
                }
            } catch (e) {
                console.error("Hide execution error", e);
                removeElement(id, meta);
            }
        } else if (rootEl && window.CGMotion && rootEl.querySelector('[data-cg-anim-out]')) {
            // Deklaratywne animacje wyjścia CGMotion
            const dataHash = JSON.parse(meta.hash);
            const durationMs = ((dataHash.animation?.out?.duration) || 0.5) * 1000 + 50;
            window.CGMotion.autoHide(rootEl);
            setTimeout(() => removeElement(id, meta), durationMs);
        } else if (rootEl) {
            // CSS transitions triggered by .active removal
            setTimeout(() => {
                removeElement(id, meta);
            }, 600);
        } else {
            removeElement(id, meta);
        }
    }

    function recalculateAttachments(graphics) {
        // Wait for DOM layout after additions/removals, then measure actual bounds
        setTimeout(() => {
            graphics.forEach(graphic => {
                if (!graphic.visible) return;
                const meta = activeGraphics[graphic.id];
                if (!meta || !meta.el || meta.isHiding) return;
                if (!graphic.layout?.attachedToGraphicId && !graphic.layout?.attachedToGraphicIdX) return;

                const { offsetY, offsetX } = computeAttachOffsets(graphic, graphics, activeGraphics, container);
                meta.lastOffsetY = offsetY;
                meta.lastOffsetX = offsetX;
                const targetTransform = getLayoutTransform(graphic.layout, 1, offsetY, offsetX);

                if (meta.el.style.transform !== targetTransform) {
                    meta.el.style.transition = 'transform 0.6s cubic-bezier(0.22, 1, 0.36, 1)';
                    meta.el.style.transform = targetTransform;
                }
            });
        }, 50);
    }

    // Timery/rAF szablonu są przypięte do jego root: po odłączeniu elementu z DOM
    // same się zatrzymują (szablony nie muszą sprawdzać isConnected).
    function makeScopedTimers(root) {
        const rafs = new Set(), ints = new Set();
        const raf = (cb) => {
            const h = requestAnimationFrame((ts) => {
                rafs.delete(h);
                if (!root.isConnected) return;
                cb(ts);
            });
            rafs.add(h);
            return h;
        };
        const setInt = (cb, ms, ...args) => {
            const h = setInterval(() => {
                if (!root.isConnected) { clearInterval(h); ints.delete(h); return; }
                cb(...args);
            }, ms);
            ints.add(h);
            return h;
        };
        root.__cgDisposeTimers = () => {
            rafs.forEach(cancelAnimationFrame); rafs.clear();
            ints.forEach(clearInterval); ints.clear();
        };
        return { raf, setInt };
    }

    function runTemplateJs(wrappedCode, root) {
        const t = makeScopedTimers(root);
        // eslint-disable-next-line no-new-func
        new Function('root', 'gsap', 'requestAnimationFrame', 'setInterval', wrappedCode)(root, window.gsap, t.raf, t.setInt);
    }

    // Pełne sprzątanie instancji: timery szablonu, tweeny/pętle GSAP, wideo tła.
    function disposeInstance(wrapperEl) {
        if (!wrapperEl) return;
        try {
            const root = wrapperEl.querySelector ? wrapperEl.querySelector('.lt-root') : null;
            if (root && root.__cgDisposeTimers) root.__cgDisposeTimers();
            const nodes = [wrapperEl, ...wrapperEl.querySelectorAll('*')];
            if (window.CGMotion && window.CGMotion.stopLoop) {
                nodes.forEach(n => { try { window.CGMotion.stopLoop(n); } catch (e) { } });
            }
            if (window.gsap) window.gsap.killTweensOf(nodes);
            wrapperEl.querySelectorAll('video').forEach(v => {
                try { v.pause(); v.removeAttribute('src'); v.load(); } catch (e) { }
            });
            if (root && root.__btb) { try { root.__btb.destroy(); } catch (e) { } root.__btb = null; }
            if (root) { root.__slt_show = null; root.__slt_hide = null; }
        } catch (e) { console.error('disposeInstance error', e); }
    }

    function removeElement(id, meta) {
        disposeInstance(meta.el);
        if (meta.el && meta.el.parentNode) {
            meta.el.parentNode.removeChild(meta.el);
        }
        if (activeGraphics[id] === meta) {
            delete activeGraphics[id];
        }
    }

    // ===========================================================
    // PUBLIC API — Used by the control panel for monitor previews
    // ===========================================================
    const previewInstances = new WeakMap();

    window.__cgRenderer = {
        buildPreviewContext,
        renderPreview(containerEl, graphics, settings = {}, options = {}) {
            if (!containerEl) return;
            let instances = previewInstances.get(containerEl);
            if (!instances) { instances = {}; previewInstances.set(containerEl, instances); }

            const visibleGraphics = graphics.filter(g => g.visible);
            const visibleIds = new Set(visibleGraphics.map(g => g.id));
            const instantSettlePromises = [];

            // Remove stale
            Object.keys(instances).forEach(id => {
                if (!visibleIds.has(id)) {
                    const meta = instances[id];
                    const el = meta?.el || meta;
                    disposeInstance(el);
                    if (el && el.parentNode) el.parentNode.removeChild(el);
                    delete instances[id];
                }
            });

            // Add visible graphics
            visibleGraphics.forEach(graphic => {
                const tpl = graphic;
                if (!tpl.html_template && !tpl.css_template && !tpl.js_template) return;

                // Remove and re-render on update
                if (instances[graphic.id]) {
                    const oldMeta = instances[graphic.id];
                    const oldEl = oldMeta?.el || oldMeta;
                    disposeInstance(oldEl);
                    if (oldEl && oldEl.parentNode) oldEl.parentNode.removeChild(oldEl);
                    delete instances[graphic.id];
                }

                const instanceId = `prev${graphic.id.replace(/-/g, '')}`;
                const context = buildPreviewContext(graphic, tpl, instanceId, settings);

                const prepareStr = (str) => {
                    if (!str) return '';
                    try {
                        return Handlebars.compile(str)(context);
                    } catch (e) {
                        console.error('Preview Handlebars error:', e);
                        return str;
                    }
                };

                const htmlSource = tpl.html_template;
                const cssSourcePrev = tpl.css_template;
                const htmlStr = prepareStr(htmlSource);
                let cssStr = prepareStr(cssSourcePrev);

                cssStr = cssStr.replace(new RegExp(`#${instanceId}\\s+`, 'g'), '');
                cssStr = cssStr.replace(/(?<![a-zA-Z0-9_-])\.(rep-|lt-|modern-|na-zywo-|plate|title|subtitle|ticker|dot|news-|wiper-|utk-|tpl-|cg-)[a-zA-Z0-9_-]*/g, `#${instanceId} $&`);
                cssStr = cssStr.replace(/#\{\{ID\}\}|#GRAPHIC_ID/g, `#${instanceId}`);

                // @keyframes scoping (identical to output renderer)
                const keyframeNamesPrev = [];
                cssStr = cssStr.replace(/@keyframes\s+([\w-]+)/g, (match, name) => {
                    const scopedName = `${name}_${instanceId}`;
                    keyframeNamesPrev.push({ original: name, scoped: scopedName });
                    return `@keyframes ${scopedName}`;
                });
                keyframeNamesPrev.forEach(({ original, scoped }) => {
                    cssStr = cssStr.replace(
                        new RegExp(`(animation:[^;]*?)\\b${original}\\b`, 'g'),
                        (match, prefix) => `${prefix}${scoped}`
                    );
                });

                const layoutStyleWrapper = document.createElement('div');
                if (options.noLayout) {
                    layoutStyleWrapper.style.position = 'relative';
                    layoutStyleWrapper.style.width = '100%';
                } else {
                    const autoScale = 1;
                    layoutStyleWrapper.style.transform = getLayoutTransform(graphic.layout, autoScale);
                    layoutStyleWrapper.style.transformOrigin = '0 0';
                    layoutStyleWrapper.style.position = 'absolute';
                    layoutStyleWrapper.style.top = '0';
                    layoutStyleWrapper.style.left = '0';
                    layoutStyleWrapper.style.width = '1920px';
                    layoutStyleWrapper.style.height = '1080px';
                    layoutStyleWrapper.style.pointerEvents = 'none';
                }

                layoutStyleWrapper.style.setProperty('--v-width', formatDimension(graphic.layout?.width, '90%'));
                layoutStyleWrapper.style.setProperty('--v-height', formatDimension(graphic.layout?.height, 'auto'));
                if (graphic.scene?.frame) {
                    const fw = graphic.scene.frame.w, fh = graphic.scene.frame.h;
                    if (graphic.layout?.width) layoutStyleWrapper.style.setProperty('--cg-sx', String(parseFloat(graphic.layout.width) / fw));
                    if (graphic.layout?.height) layoutStyleWrapper.style.setProperty('--cg-sy', String(parseFloat(graphic.layout.height) / fh));
                }
                layoutStyleWrapper.style.zIndex = graphic.layout?.layer || 1;
                layoutStyleWrapper.style.opacity = graphic.style?.opacity ?? 1;

                const innerContainer = document.createElement('div');
                innerContainer.style.width = '100%';
                innerContainer.style.height = '100%';
                innerContainer.style.position = 'absolute';
                innerContainer.style.top = '0';
                innerContainer.style.left = '0';

                const styleEl = document.createElement('style');

                if (options.instant) {
                    // Rewrite the CSS to force-show elements:
                    // 1. Replace opacity: 0 with opacity: 1 (initial hidden state → visible)
                    cssStr = cssStr.replace(/opacity\s*:\s*0\b/g, 'opacity: 1');
                    // 2. Replace transform values that hide elements off-screen with identity
                    cssStr = cssStr.replace(/transform\s*:\s*(translateX\([^)]+\)|translateY\([^)]+\)|scale\([^)]+\)|scaleX\([^)]+\))/g, 'transform: none');
                }

                // Gradient override: if the graphic uses a gradient background, force it onto
                // all elements that only have background-color set (which doesn't support gradients).
                const bgData2 = graphic.style?.background || {};
                const globalRadiusGraphicsPrev = settings?.globalRadiusGraphics || [];
                const isGlobalRadiusPrev = globalRadiusGraphicsPrev.includes(graphic.id);
                const borderRadiusPrev = isGlobalRadiusPrev ? (settings.globalBorderRadius || 0) : (bgData2.borderRadius || 0);

                if (bgData2.type === 'gradient') {
                    const angle2 = bgData2.gradientAngle || 135;
                    const c1_2 = bgData2.color || '#1e3a8a';
                    const c2_2 = bgData2.color2 || '#3b82f6';
                    const gradientVal2 = `linear-gradient(${angle2}deg, ${c1_2}, ${c2_2})`;
                    // Target container/bar elements but exclude utk-* ticker/wiper elements with their own colors
                    cssStr += `\n/* gradient override */`;
                    cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]):not([class*="msg"]):not([class*="track"]), #${instanceId} [class*="-bar"], #${instanceId} [class*="wrapper"]:not([class*="utk"]), #${instanceId} [class*="plate"] { background: ${gradientVal2} !important; background-color: transparent !important; }`;
                }

                if (isGlobalRadiusPrev || bgData2.borderRadius > 0) {
                    cssStr += `\n/* border radius override */`;
                    cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]), #${instanceId} [class*="-bar"], #${instanceId} [class*="wrapper"]:not([class*="utk"]), #${instanceId} [class*="plate"] { border-radius: ${borderRadiusPrev}px !important; overflow: hidden !important; }`;
                }

                if (graphic.layout?.width) {
                    cssStr += `\n/* width override */`;
                    cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]):not([class*="track"]), #${instanceId} [class*="-bar"]:not([class*="utk"]), #${instanceId} [class*="wrapper"]:not([class*="utk"]):not([class*="wiper"]), #${instanceId} [class*="plate"] { width: var(--v-width) !important; box-sizing: border-box !important; }`;
                }

                if (graphic.layout?.height) {
                    cssStr += `\n/* height override */`;
                    cssStr += `\n#${instanceId} [class*="container"]:not([class*="utk"]):not([class*="wiper"]):not([class*="track"]), #${instanceId} [class*="-bar"]:not([class*="utk"]), #${instanceId} [class*="wrapper"]:not([class*="utk"]):not([class*="wiper"]), #${instanceId} [class*="plate"] { height: var(--v-height) !important; max-height: var(--v-height) !important; overflow: hidden !important; }`;
                }

                // Layout side override (skip in noLayout mode — graphic renders at natural size)
                if (!options.noLayout && graphic.layout?.side && graphic.layout.side !== 'custom') {
                    const parts = graphic.layout.side.split('-');
                    const ySide = parts[0];
                    const xSide = parts.length > 1 ? parts[1] : 'center';
                    
                    const yS = (graphic.layout.side === 'center') ? 'center' : ySide;
                    const xS = (graphic.layout.side === 'center') ? 'center' : xSide;

                    const mx = graphic.layout?.marginX || 0;
                    const my = graphic.layout?.marginY || 0;

                    cssStr += `\n/* Side Layout */`;
                    cssStr += `\n#${instanceId} { display: flex; width: 100%; height: 100%; box-sizing: border-box; pointer-events: none; }`;
                    cssStr += `\n#${instanceId} > * { pointer-events: auto; position: relative !important; top: auto !important; left: auto !important; right: auto !important; bottom: auto !important; margin: 0 !important; }`;
                    cssStr += `\n#${instanceId} p, #${instanceId} div { margin: 0; padding: 0; }`;
                    
                    if (yS === 'top') cssStr += `\n#${instanceId} { align-items: flex-start; padding-top: ${my}px; }`;
                    else if (yS === 'bottom') cssStr += `\n#${instanceId} { align-items: flex-end; padding-bottom: ${my}px; }`;
                    else if (yS === 'center') cssStr += `\n#${instanceId} { align-items: center; }`;

                    if (xS === 'left') cssStr += `\n#${instanceId} { justify-content: flex-start; padding-left: ${mx}px; }`;
                    else if (xS === 'right') cssStr += `\n#${instanceId} { justify-content: flex-end; padding-right: ${mx}px; }`;
                    else if (xS === 'center') cssStr += `\n#${instanceId} { justify-content: center; }`;
                }

                // Apply line-height from context (must be added to cssStr BEFORE setting styleEl.textContent)
                cssStr += `\n#${instanceId} { line-height: ${context.LINE_HEIGHT || 1.4}; }`;

                styleEl.textContent = cssStr;
                if (options.instant) {
                    // Also kill all transitions and animations globally
                    styleEl.textContent += `\n#${instanceId}, #${instanceId} * { transition: none !important; transition-delay: 0s !important; animation: none !important; }`;
                }

                innerContainer.appendChild(styleEl);

                // Inject HTML scoped with ID, matching showGraphic wrapping
                const wrapperDiv = document.createElement('div');
                wrapperDiv.id = instanceId;
                wrapperDiv.className = 'lt-root';
                wrapperDiv.setAttribute('data-squash-enabled', graphic.style?.typography?.squashEnabled !== false);
                if (graphic.type === 'BADGE') wrapperDiv.setAttribute('data-squash-mode', 'length');
                wrapperDiv.innerHTML = htmlStr;
                innerContainer.appendChild(wrapperDiv);

                layoutStyleWrapper.appendChild(innerContainer);

                // Background video/effect layer (identical to output renderer)
                injectBackgroundLayer(graphic, instanceId, layoutStyleWrapper);

                containerEl.appendChild(layoutStyleWrapper);
                instances[graphic.id] = layoutStyleWrapper;

                // Run template JS identical to showGraphic (per-graphic override takes priority)
                const jsSourcePrev = tpl.js_template;
                const rootEl = document.getElementById(instanceId);
                if (rootEl && jsSourcePrev) {
                    const jsCode = prepareStr(jsSourcePrev);
                    try {
                        const wrappedCode = [
                            'try {',
                            jsCode,
                            '} catch (e) {',
                            '    console.error("PREVIEW INNER TEMPLATE ERROR:", e);',
                            '}'
                        ].join('\n');
                        runTemplateJs(wrappedCode, document.getElementById(instanceId));
                    } catch (e) {
                        console.error("[CGMotion] Template JS error (preview)", e);
                    }
                }

                const legacyPreviewShow = () => {
                    let pendingSettle = null;
                    // Trigger __slt_show and force instant visibility
                    if (rootEl && typeof rootEl.__slt_show === 'function') {
                        try {
                            rootEl.__slt_show();
                        } catch (e) { console.error("__slt_show error:", e); }
                    }

                    if (rootEl) {
                        rootEl.classList.add('active');
                        if (rootEl.firstElementChild) rootEl.firstElementChild.classList.add('active');
                    }

                    if (options.instant && rootEl) {
                        // Use rAF to ensure DOM is committed before forcing styles
                        pendingSettle = new Promise((resolveSettle) => {
                            requestAnimationFrame(() => {
                                // Force GSAP tweens to complete. A plain progress(1) only forces the
                                // DISPLAY for the current tick — a tween that is still active (not
                                // paused/killed) keeps being driven by GSAP's real-time clock, so on
                                // the very next tick (GSAP's own internal rAF ticker, independent of
                                // ours) it recomputes from actual elapsed time and overwrites our
                                // forced end-state back to an early, mid-animation value. pause() after
                                // progress(1) locks it at the end state for good. Infinite tweens
                                // (repeat:-1, e.g. a scrolling ticker track) are left untouched so they
                                // keep animating in the preview as intended.
                                if (window.gsap) {
                                    try {
                                        const finite = (t) => t.repeat() !== -1;
                                        window.gsap.getTweensOf(rootEl).filter(finite).forEach(t => t.progress(1).pause());
                                        window.gsap.getTweensOf(rootEl.querySelectorAll('*')).filter(finite).forEach(t => t.progress(1).pause());
                                    } catch (e) { }
                                }
                                // Force all child elements visible - unconditionally override opacity and transform
                                // This handles CSS-transition based templates where __slt_show sets properties
                                // but they haven't taken effect yet
                                const forceVisible = (el) => {
                                    el.style.setProperty('opacity', '1', 'important');
                                    el.style.setProperty('visibility', 'visible', 'important');
                                    // Ręczne ściskanie tekstu (.cg-text-squash z Edytora Tekstu) to treść, nie
                                    // animacja — zachowaj jego scaleX zamiast zerować do 'none'.
                                    if (!el.classList.contains('cg-text-squash')) {
                                        el.style.setProperty('transform', 'none', 'important');
                                    }
                                    el.style.setProperty('transition', 'none', 'important');
                                    el.style.setProperty('clip-path', 'none', 'important');
                                    el.style.setProperty('animation', 'none', 'important');
                                };
                                rootEl.querySelectorAll('*').forEach(forceVisible);
                                resolveSettle();
                            });
                        });
                    } else if (!rootEl) {
                        // fallback
                    } else if (!rootEl.__slt_show) {
                        rootEl.style.display = 'block';
                    }
                    return pendingSettle;
                };
                // Podglądy edycyjne (`instant`: układ spoczynkowy — WYSIWYG, Studio, pomiar bbox) zostają bez WASM; WASM gra w podglądach animowanych
                // (monitory) oraz gdy wywołujący poprosi o klatkę w punkcie Stop: options.instant + options.motion = 'hold'.
                if (rootEl && wantsWasmMotion(graphic, settings) && (!options.instant || options.motion === 'hold')) {
                    // element z artefaktem motion: podgląd silnikiem WASM (hold = klatka w punkcie Stop, bez wymuszania widoczności);
                    // awaria → ten sam podgląd co dla CGMotion
                    instantSettlePromises.push(startWasmMotion(rootEl, graphic, { instant: !!options.instant }).then((ok) => {
                        if (ok) {
                            rootEl.classList.add('active');
                            if (rootEl.firstElementChild) rootEl.firstElementChild.classList.add('active');
                            return null;
                        }
                        return legacyPreviewShow();
                    }));
                } else {
                    const pr = legacyPreviewShow();
                    if (pr) instantSettlePromises.push(pr);
                }

                // D10: podgląd czeka na kroje elementu (pomiary i zrzuty po załadowaniu fontów)
                instantSettlePromises.push(ensureFontsFor(graphic, settings).then(() => { if (rootEl) applyGlobalSquashing(rootEl); }));

                // Apply global text squashing to preview instances
                scheduleSquashing(rootEl);
            });

            // Recalculate attachments for preview after DOM settles
            // instances stores raw layoutStyleWrapper elements; wrap in {el} for computeAttachOffsets
            requestAnimationFrame(() => {
                visibleGraphics.forEach(graphic => {
                    const wrapperEl = instances[graphic.id]?.el || instances[graphic.id];
                    if (!wrapperEl) return;
                    if (!graphic.layout?.attachedToGraphicId && !graphic.layout?.attachedToGraphicIdX) return;
                    const wrappedInstances = {};
                    Object.keys(instances).forEach(id => {
                        const v = instances[id];
                        wrappedInstances[id] = v && v.el ? v : { el: v };
                    });
                    const { offsetY, offsetX } = computeAttachOffsets(graphic, graphics, wrappedInstances, containerEl);
                    const targetTransform = getLayoutTransform(graphic.layout, 1, offsetY, offsetX);
                    wrapperEl.style.transition = 'none';
                    wrapperEl.style.transform = targetTransform;
                });
            });

            // Resolves once this frame's forced-visible/squash/attach rAFs have all run,
            // so callers (e.g. trio-elem-preview.html bbox measurement) can measure the
            // DOM without racing renderer-internal rAF chains or GSAP's lazy render queue.
            return Promise.all(instantSettlePromises).then(
                () => new Promise((resolve) => requestAnimationFrame(resolve))
            );
        }
    };

    function buildPreviewContext(graphic, tpl, instanceId, settings = {}) {
        const animIn = graphic.animation?.in || {};
        const animOut = graphic.animation?.out || {};
        
        const bgStyle = graphic.style?.background || {};
        const typo = graphic.style?.typography || {};
        const subTypo = graphic.style?.subtitleTypography || {};

        // Prepare OCG fields (serialize arrays to JSON)
        const customFields = {};
        if (graphic.fields) {
            Object.entries(graphic.fields).forEach(([k, v]) => {
                if (Array.isArray(v)) {
                    customFields[k] = JSON.stringify(v);
                } else {
                    customFields[k] = v;
                }
            });
        }

        let activeFontFamily = typo.fontFamily || 'Arial';
        if (settings && settings.globalFontGraphics && settings.globalFontGraphics.includes(graphic.id)) {
            activeFontFamily = settings.globalFontFamily || activeFontFamily;
        }

        const inDuration = animIn.duration ?? 0.5;
        const outDuration = animOut.duration ?? 0.3;
        const inDirection = animIn.direction || 'left';
        const outDirection = animOut.direction || 'left';
        const IDENTITY = 'translateX(0px) translateY(0px) scale(1)';
        const dirMap = { left: 'translateX(-1920px)', right: 'translateX(1920px)', top: 'translateY(-1080px)', bottom: 'translateY(1080px)' };
        const bg = bgStyle.type === 'gradient'
            ? `linear-gradient(${bgStyle.gradientAngle || 135}deg, ${bgStyle.color || '#1e3a8a'}, ${bgStyle.color2 || '#3b82f6'})`
            : (bgStyle.color || '#1e3a8a');

        let rawTitle = graphic.titleHtml || (graphic.titleLines?.length ? graphic.titleLines.map(l => `<div style="font-size:${l.fontSize || 48}px;font-weight:${l.fontWeight || '800'};color:${l.color || '#fff'};font-family:'${l.fontFamily || activeFontFamily}',sans-serif;text-transform:${l.textTransform || 'uppercase'}">${l.text}</div>`).join('') : (graphic.title || ''));

        if (rawTitle && typeof rawTitle === 'string' && rawTitle.includes('<font')) {
            const tempDiv = document.createElement('div');
            tempDiv.innerHTML = rawTitle;
            tempDiv.querySelectorAll('font').forEach(f => {
                const s = document.createElement('span');
                if (f.face) s.style.fontFamily = f.face;
                if (f.color) s.style.color = f.color;
                if (f.size) {
                    const sizes = { 1: '10', 2: '13', 3: '16', 4: '18', 5: '24', 6: '32', 7: '48' };
                    s.style.fontSize = (sizes[f.size] || '48') + 'px';
                }
                s.innerHTML = f.innerHTML;
                f.replaceWith(s);
            });
            rawTitle = tempDiv.innerHTML;
        }

        const hasActiveScene = !!(graphic.scene && !graphic.scene.detached);

        let rawItems = graphic.items || [];
        const itemsData = rawItems.map(item => {
            let text = '';
            let category = '';
            
            if (typeof item === 'string') {
                text = item;
            } else if (item && typeof item === 'object') {
                text = item.text || '';
                category = item.category || '';
            }
            
            if (text && typeof text === 'string' && text.includes('<font')) {
                const tempDiv = document.createElement('div');
                tempDiv.innerHTML = text;
                tempDiv.querySelectorAll('font').forEach(f => {
                    const s = document.createElement('span');
                    if (f.face) s.style.fontFamily = f.face;
                    if (f.color) s.style.color = f.color;
                    if (f.size) {
                        const sizes = { 1: '10', 2: '13', 3: '16', 4: '18', 5: '24', 6: '32', 7: '48' };
                        s.style.fontSize = (sizes[f.size] || '48') + 'px';
                    }
                    s.innerHTML = f.innerHTML;
                    f.replaceWith(s);
                });
                text = tempDiv.innerHTML;
            }
            return { text, category };
        });

        // ITEMS is still strings for legacy templates
        const itemsStrings = itemsData.map(id => id.text);

        const globalShadow = settings?.globalShadow || { enabled: false };
        const shadowStr = globalShadow.enabled
            ? `${globalShadow.offsetX ?? 0}px ${globalShadow.offsetY ?? 2}px ${globalShadow.blur ?? 4}px ${globalShadow.color || 'rgba(0,0,0,0.5)'}`
            : 'none';

        const sepColor = bgStyle.borderColor || '#3b82f6';
        const SEPARATOR_CSS = (() => {
            switch (graphic.separatorStyle || 'skewed') {
                case 'none':   return 'display: none;';
                case 'dot':    return `width: 10px; height: 10px; background: ${sepColor}; border-radius: 50%; margin: 0 15px; transform: none; flex-shrink: 0;`;
                case 'square': return `width: 10px; height: 10px; background: ${sepColor}; margin: 0 15px; transform: none; flex-shrink: 0;`;
                case 'pipe':   return `width: 2px; height: 24px; background: ${sepColor}; margin: 0 15px; transform: none; flex-shrink: 0;`;
                default:       return `width: 12px; height: 24px; background: ${sepColor}; transform: skewX(-30deg); margin: 0 10px; flex-shrink: 0;`;
            }
        })();

        const wiperSettings = graphic.wiper || {};
        const wiperBg = (() => {
            const base = wiperSettings.bgColor || graphic.fields?.secondaryColor || '#ff0000';
            if (wiperSettings.useGradient) {
                const c2 = wiperSettings.color2 || '#880000';
                const angle = wiperSettings.gradientAngle || 90;
                return `linear-gradient(${angle}deg, ${base} 0%, ${c2} 100%)`;
            }
            return base;
        })();

        const wiperGleamBg = (() => {
            const color = wiperSettings.gleamColor || '#ffffff';
            const opacity = wiperSettings.gleamOpacity ?? 0.4;
            let r = 255, g = 255, b = 255;
            // Support both #rrggbb hex and rgba(...) formats
            const rgbaMatch = color.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
            if (rgbaMatch) {
                r = parseInt(rgbaMatch[1]);
                g = parseInt(rgbaMatch[2]);
                b = parseInt(rgbaMatch[3]);
            } else if (color.startsWith('#') && color.length >= 7) {
                r = parseInt(color.slice(1, 3), 16);
                g = parseInt(color.slice(3, 5), 16);
                b = parseInt(color.slice(5, 7), 16);
            }
            return `linear-gradient(90deg, rgba(${r},${g},${b},0) 0%, rgba(${r},${g},${b},${opacity}) 50%, rgba(${r},${g},${b},0) 100%)`;
        })();

        const ctx = {
            ID: instanceId,
            TITLE: rawTitle,
            SUBTITLE: graphic.subtitle || '',
            INTRO_TEXT: graphic.introText !== undefined ? graphic.introText : '',
            WIPER_BG: wiperBg,
            WIPER_TEXT_COLOR: wiperSettings.textColor || '#ffffff',
            WIPER_FONT: wiperSettings.fontFamily || activeFontFamily,
            WIPER_FONT_SIZE: wiperSettings.fontSize || 35,
            WIPER_FONT_WEIGHT: wiperSettings.fontWeight || '900',
            WIPER_LETTER_SPACING: wiperSettings.letterSpacing ?? 1,
            WIPER_GLEAM_ENABLED: wiperSettings.gleamEnabled !== false,
            WIPER_GLEAM_BG: wiperGleamBg,
            WIPER_GLEAM_DURATION: wiperSettings.gleamDuration || 2,
            WIPER_GLEAM_HEIGHT: wiperSettings.gleamHeight || 100,
            WIPER_GLEAM_WIDTH: wiperSettings.gleamWidth || 150,
            WIPER_GLEAM_FREQUENCY: wiperSettings.gleamFrequency || 3,
            WIPER_GLEAM_OPACITY: wiperSettings.gleamOpacity ?? 0.4,
            ITEMS: itemsStrings,
            ITEMS_JSON: JSON.stringify(itemsData),
            ITEMS_B64: (() => { try { const s = JSON.stringify(itemsData); const b64 = (typeof btoa !== 'undefined') ? btoa(unescape(encodeURIComponent(s))) : Buffer.from(s).toString('base64'); return b64.replace(/=+$/, ''); } catch(e) { return ''; } })(),
            WIPER_TEXT: (graphic.introText !== undefined) ? graphic.introText : '',
            WIPER_SHOW: wiperSettings.show !== false,
            LOGO_URL: graphic.url || '',
            MEDIA_TYPE: graphic.mediaType || 'image',
            VIDEO_URL: graphic.url || '',
            VIDEO_LOOP: graphic.videoLoop !== false,
            VIDEO_MUTED: graphic.videoMuted !== false,
            BACKGROUND_VIDEO_URL: graphic.backgroundVideo || '',
            BACKGROUND_EFFECT: graphic.backgroundEffect || 'none',
            TICKER_SPEED: graphic.speed || 100,
            TICKER_MODE: graphic.tickerMode || 'whip',
            PRIMARY_COLOR: bgStyle.color || '#1e3a8a',
            PRIMARY_BG: bg,
            SECONDARY_COLOR: graphic.accentColor || bgStyle.borderColor || '#000000',
            BORDER_COLOR: bgStyle.borderColor || '#3b82f6',
            TITLE_COLOR: typo.color || '#ffffff',
            SUBTITLE_COLOR: subTypo.color || '#eeeeee',
            FONT_FAMILY: activeFontFamily,
            FONT_SIZE: typo.fontSize || 30,
            TITLE_SIZE: typo.fontSize || 48,
            TITLE_WEIGHT: typo.fontWeight || '800',
            TITLE_TRANSFORM: typo.textTransform || 'uppercase',
            PADDING_Y: typo.paddingY || 0,
            BOX_SHADOW: shadowStr,
            TITLE_FONT: activeFontFamily,
            SUBTITLE_SIZE: subTypo.fontSize || 24,
            BACKGROUND: bg,
            BORDER_RADIUS: (settings && settings.globalRadiusGraphics && settings.globalRadiusGraphics.includes(graphic.id)) ? (settings.globalBorderRadius || 0) : (bgStyle.borderRadius || 0),
            BORDER_WIDTH: bgStyle.borderWidth || 0,
            ANIMATION_DURATION: inDuration,
            ANIMATION_DELAY: (animIn.delay || 0),
            ANIMATION_EASE: animIn.ease || 'ease-out',
            ANIMATION_TRANSFORM: (() => {
                const type = animIn.type || 'slide';
                if (type === 'fade' || type === 'none') return 'none';
                if (type === 'zoom') return 'scale(0.85)';
                if (type === 'wipe') return 'scaleX(0)';
                return dirMap[inDirection] || 'translateX(-1920px)';
            })(),
            ANIMATION_IDENTITY: IDENTITY,
            ANIMATION_IN_TRANSFORM: (() => {
                const type = animIn.type || 'slide';
                if (type === 'fade' || type === 'none') return 'none';
                if (type === 'zoom') return 'scale(0.85)';
                if (type === 'wipe') return 'scaleX(0)';
                return dirMap[inDirection] || 'translateX(-1920px)';
            })(),
            ANIMATION_OUT_TRANSFORM: (() => {
                const type = animOut.type || 'slide';
                if (type === 'fade' || type === 'none') return 'none';
                if (type === 'zoom') return 'scale(0.85)';
                if (type === 'wipe') return 'scaleX(0)';
                return dirMap[outDirection] || 'translateX(-1920px)';
            })(),
            ANIMATION_OUT_DURATION: outDuration,
            ANIMATION_OUT_DELAY: (animOut.delay || 0),
            ANIMATION_OUT_EASE: animOut.ease || 'ease-in',
            LAYOUT_X: graphic.layout?.x || 0,
            LAYOUT_Y: graphic.layout?.y || 0,
            V_WIDTH: graphic.layout?.width || 1920,
            V_HEIGHT: graphic.layout?.height || 1080,
            SIDE_IMAGE: graphic.sideImage || '',
            LOGO_WIDTH: graphic.layout?.width || null,
            LOGO_HEIGHT: graphic.layout?.height || null,
            TRANSPARENT: (graphic.style?.background?.type === 'transparent' || !!graphic.style?.background?.transparent),
            LINE_HEIGHT: typo.lineHeight || '1.4',
            ANIMATION_IN_JSON: JSON.stringify(animIn),
            ANIMATION_OUT_JSON: JSON.stringify(animOut),
            TEXT_ANIM_JSON: JSON.stringify(graphic.animation?.text || { type: 'none' }),
            TEXT_ANIM_SYNC: !!graphic.animation?.textSync,
            TEXT_ANIM_OUT_JSON: JSON.stringify(graphic.animation?.textOut || { type: 'none' }),
            TEXT_ANIM_OUT_SYNC: !!graphic.animation?.textOut?.syncWithBase,
            LOOP_JSON: JSON.stringify(graphic.animation?.loop || { type: 'none' }),
            SEPARATOR_CSS: SEPARATOR_CSS,
            DOM_CONTEXT: `document.getElementById("${instanceId}")`,
            ...customFields
        };

        // Intelligent OCG mapping: if TITLE/SUBTITLE are still standard/empty, 
        // try to find matching OCG fields (e.g. f-tytul-nazwa -> TITLE)
        // Tylko pola tekstowe zadeklarowane w ocgInputs (np. f-tytul-nazwa). Wcześniej łapało też pola stylu,
        // np. titleColor → TITLE = '#000000'.
        const ocgIds = new Set((tpl.ocgInputs || []).map(i => i.id));
        const isStyleKey = (k) => /colou?r|size|font|weight|background|bg$|width|height|radius|transform|style|align|opacity|url|animation|layout/i.test(k);
        const textKey = (re, notRe) => Object.keys(customFields).find(k =>
            ocgIds.has(k) && k !== 'TITLE' && k !== 'SUBTITLE' && !isStyleKey(k) && re.test(k) && !(notRe && notRe.test(k)));
        const isStandardTitle = (ctx.TITLE === graphic.title || ctx.TITLE === tpl.name);
        if (isStandardTitle) {
            const titleKey = textKey(/title|tytul|nazwa/i, /sub/i);
            if (titleKey) ctx.TITLE = customFields[titleKey];
        }
        const isStandardSub = (!ctx.SUBTITLE || ctx.SUBTITLE === graphic.subtitle);
        if (isStandardSub) {
            const subKey = textKey(/sub|opis|funkcja/i);
            if (subKey) ctx.SUBTITLE = customFields[subKey];
        }

        // Handle LOGO_URL / IMAGE prioritisation: if standard/empty, use graphic.url (Alpha upload)
        const isStandardLogo = (!ctx.LOGO_URL || ctx.LOGO_URL === graphic.url);
        if (isStandardLogo && graphic.url) {
            ctx.LOGO_URL = graphic.url;
        }

        // TITLE_L1, TITLE_L2… — kolejne linie (bloki <div>/<p>, <br> lub \n) tytułu; używają ich warstwy tekstowe sceny z `line`
        const applyTitleLines = (ctx) => {
            if (typeof ctx.TITLE === 'string') ctx.TITLE = hoistBlockFontsHtml(ctx.TITLE);
            const titleLineVars = {};
            const t = String(ctx.TITLE || '');
            let parts = [];
            if (/<(div|p)\b/i.test(t)) {
                const tmp = document.createElement('div'); tmp.innerHTML = t;
                parts = [...tmp.childNodes].filter((n) => n.nodeType === 1 || (n.textContent || '').trim()).map((n) => n.nodeType === 1 ? n.outerHTML : n.textContent);
                parts = parts.map((h) => h.replace(/^<(div|p)\b[^>]*>([\s\S]*)<\/\1>$/i, '$2'));
            } else parts = t.split(/<br\s*\/?>|\r?\n/i);
            parts.forEach((h, i) => { titleLineVars['TITLE_L' + (i + 1)] = h; });
            titleLineVars.TITLE_L1 = titleLineVars.TITLE_L1 || '';
            Object.assign(ctx, titleLineVars);
        };
        applyTitleLines(ctx);

        // Ściskanie tekstu (.slt-squash) dla elementów bez własnej sceny — szablony pisane ręcznie
        // (LOWER_THIRD/GRAPHIC/TICKER/BADGE) nie niosą tej klasy w swoim html_template, więc checkbox
        // "Automatyczne ściskanie tekstu" w zakładce Typografia nie miałby żadnego efektu. Owijamy
        // wyjście TITLE uniwersalnie, tu (po ustaleniu ostatecznej wartości — w tym po nadpisaniu przez
        // fields.TITLE, którego używa edytor pola BADGE), zamiast wymagać tego od każdego szablonu z osobna.
        // Elementy ze sceną (nieodłączoną) mają własne ściskanie per-warstwa (SceneModel) — nie dublujemy.
        if (ctx.TITLE && typeof ctx.TITLE === 'string' && !hasActiveScene && graphic.type !== 'IMAGE' && graphic.type !== 'CLOCK' && !/\bslt-squash\b/.test(ctx.TITLE)) {
            // <span> jest bezpieczny jako dziecko dowolnego kontenera tytułu (div lub span) używanego
            // przez istniejące szablony — w przeciwieństwie do <div>, który byłby niepoprawny (i wyciągnięty
            // przez parser) wewnątrz kontenera <span> (np. .na-zywo-text). CSS .slt-squash i tak
            // wymusza display:inline-block, więc wizualnie działa identycznie.
            ctx.TITLE = `<span class="slt-squash">${ctx.TITLE}</span>`;
        }

        return ctx;
    }

    // ===========================================================
    // BACKGROUND LAYER — video loop + CSS effects
    // ===========================================================

    const BG_EFFECT_CSS = {
        shimmer: `
            @keyframes __bg_shimmer_INST { 0%,100%{opacity:0} 50%{opacity:1} }
            .__bg_layer_INST::after {
                content:''; position:absolute; inset:0; pointer-events:none; z-index:2;
                background: linear-gradient(105deg, transparent 35%, rgba(255,255,255,0.22) 50%, transparent 65%);
                background-size: 200% 100%;
                animation: __bg_shimmer_INST 2.8s ease-in-out infinite;
                mix-blend-mode: screen;
            }`,
        pulse: `
            @keyframes __bg_pulse_INST { 0%,100%{opacity:0.85} 50%{opacity:1} }
            .__bg_layer_INST { animation: __bg_pulse_INST 2s ease-in-out infinite; }`,
        glow: `
            @keyframes __bg_glow_INST { 0%,100%{box-shadow:0 0 0 0 rgba(255,255,255,0)} 50%{box-shadow:0 0 18px 4px rgba(255,255,255,0.18)} }
            .__bg_layer_INST { animation: __bg_glow_INST 2.5s ease-in-out infinite; }`,
        scanline: `
            @keyframes __bg_scan_INST { from{background-position:0 0} to{background-position:0 8px} }
            .__bg_layer_INST::before {
                content:''; position:absolute; inset:0; pointer-events:none; z-index:3;
                background: repeating-linear-gradient(0deg, transparent, transparent 3px, rgba(0,0,0,0.06) 3px, rgba(0,0,0,0.06) 4px);
                animation: __bg_scan_INST 0.15s steps(1) infinite;
            }`
    };

    function injectBackgroundLayer(data, instanceId, wrapper) {
        const bgVideo = data.backgroundVideo;
        const bgEffect = data.backgroundEffect;
        if (!bgVideo && (!bgEffect || bgEffect === 'none')) return;

        const layerClass = `__bg_layer_${instanceId}`;

        // Inject effect CSS
        if (bgEffect && bgEffect !== 'none' && BG_EFFECT_CSS[bgEffect]) {
            const effectStyle = document.createElement('style');
            effectStyle.textContent = BG_EFFECT_CSS[bgEffect]
                .replace(/_INST/g, `_${instanceId}`)
                .replace(/\.__bg_layer_INST/g, `.${layerClass}`);
            wrapper.appendChild(effectStyle);
        }

        // Build layer div
        const layer = document.createElement('div');
        layer.className = layerClass;
        layer.style.cssText = 'position:absolute;inset:0;pointer-events:none;z-index:0;overflow:hidden;';

        if (bgVideo) {
            const vid = document.createElement('video');
            vid.src = bgVideo;
            vid.autoplay = true;
            vid.loop = true;
            vid.muted = true;
            vid.playsInline = true;
            vid.style.cssText = 'width:100%;height:100%;object-fit:cover;display:block;';
            layer.appendChild(vid);
            vid.play().catch(() => {});
        }

        // Insert before innerContainer so it sits behind everything
        wrapper.insertBefore(layer, wrapper.firstChild);
    }

    // Start — only run if this is the output.html renderer context
    if (container) {
        initRenderer();
    }

})(); // end IIFE
