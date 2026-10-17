/**
 * CGMotion — CG System Animation Engine v1.1
 *
 * Superset of GSAPFX. Wszystkie metody GSAPFX są dostępne na CGMotion
 * z identycznymi sygnaturami (pełna zgodność wsteczna).
 *
 * Nowe możliwości względem GSAPFX:
 *  - Deklaratywne animacje przez atrybuty data-cg-anim / data-cg-anim-out
 *  - Sequence builder: CGMotion.build([...])
 *  - Attention animators: CGMotion.loop() / CGMotion.stopLoop()
 *  - Spring physics: CGMotion.spring()
 *  - Per-word/per-char text split: CGMotion.splitTextIn()
 *  - Auto-animate: CGMotion.autoShow() / CGMotion.autoHide()
 *    (renderer wywołuje to gdy szablon nie definiuje __slt_show)
 */
(function (window) {
    'use strict';

    if (window.gsap) {
        if (window.TextPlugin) gsap.registerPlugin(TextPlugin);
        if (window.EasePack) gsap.registerPlugin(EasePack);
    }

    // ─── helpers ────────────────────────────────────────────────────────────────

    // cubic-bezier(x1, y1, x2, y2) → funkcja easingu (GSAP przyjmuje funkcję zamiast nazwy).
    function _cubicBezier(str) {
        const m = /cubic-bezier\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\)/.exec(str);
        if (!m) return null;
        const [x1, y1, x2, y2] = m.slice(1).map(Number);
        if (x1 < 0 || x1 > 1 || x2 < 0 || x2 > 1) return null;
        const cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx;
        const cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
        const X = t => ((ax * t + bx) * t + cx) * t;
        const Y = t => ((ay * t + by) * t + cy) * t;
        const dX = t => (3 * ax * t + 2 * bx) * t + cx;
        return function (x) {
            if (x <= 0) return 0;
            if (x >= 1) return 1;
            let t = x;
            for (let i = 0; i < 8; i++) {          // Newton
                const e = X(t) - x;
                if (Math.abs(e) < 1e-6) return Y(t);
                const d = dX(t);
                if (Math.abs(d) < 1e-6) break;
                t -= e / d;
            }
            let lo = 0, hi = 1; t = x;               // bisekcja jako zabezpieczenie
            for (let i = 0; i < 30; i++) { const v = X(t); if (Math.abs(v - x) < 1e-6) break; if (v < x) lo = t; else hi = t; t = (lo + hi) / 2; }
            return Y(t);
        };
    }

    function normalizeEase(ease) {
        if (!ease) return 'power2.out';
        const map = {
            'linear': 'none',
            'ease-in': 'power2.in',
            'ease-out': 'power2.out',
            'ease-in-out': 'power2.inOut',
            'ease-in-sine': 'sine.in',
            'ease-out-sine': 'sine.out',
            'ease-in-out-sine': 'sine.inOut',
            'ease-in-quad': 'quad.in',
            'ease-out-quad': 'quad.out',
            'ease-in-out-quad': 'quad.inOut',
            'ease-in-cubic': 'cubic.in',
            'ease-out-cubic': 'cubic.out',
            'ease-in-out-cubic': 'cubic.inOut',
            'back-in': 'back.in',
            'back-out': 'back.out',
            'back-in-out': 'back.inOut',
            'bounce-in': 'bounce.in',
            'bounce-out': 'bounce.out',
            'bounce-in-out': 'bounce.inOut',
            'elastic-in': 'elastic.in',
            'elastic-out': 'elastic.out',
            'elastic-in-out': 'elastic.inOut',
            // Semantic aliases
            'spring': 'back.out(1.4)',
            'spring-soft': 'back.out(0.8)',
            'spring-hard': 'back.out(2.2)',
            'overshoot': 'back.out(1.7)',
            'broadcast': 'power3.out',
        };
        if (ease.startsWith('cubic-bezier')) {
            // zgodność wsteczna: dwa gotowe presety Spring / Bounce zostają na back.*
            if (ease.includes('1.56') || ease.includes('0.64')) return 'back.out(1.7)';
            if (ease.includes('-0.55')) return 'back.in(1.7)';
            const fn = _cubicBezier(ease);
            return fn || 'power2.out';
        }
        return map[ease] || ease;
    }

    // Parsuje "slideUp delay:0.2 duration:0.5 ease:spring" → { type, ...opts }
    function parseAttrSpec(str) {
        if (!str) return null;
        const parts = str.trim().split(/\s+/);
        const spec = { type: parts[0] };
        for (let i = 1; i < parts.length; i++) {
            const [k, v] = parts[i].split(':');
            if (k && v !== undefined) {
                spec[k] = isNaN(v) ? v : parseFloat(v);
            }
        }
        return spec;
    }

    // Zbiera wszystkie elementy z data-cg-anim w obrębie root
    function collectAnimTargets(root, attr = 'data-cg-anim') {
        if (!root) return [];
        const results = [];
        root.querySelectorAll(`[${attr}]`).forEach(el => {
            const spec = parseAttrSpec(el.getAttribute(attr));
            if (spec) results.push({ el, spec });
        });
        return results;
    }

    // ─── slide helpers ───────────────────────────────────────────────────────────

    const SLIDE_DIST = 1920;
    const SLIDE_DIST_Y = 1080;

    function slideFromVars(dir) {
        switch (dir) {
            case 'left':   return { x: -SLIDE_DIST };
            case 'right':  return { x: SLIDE_DIST };
            case 'top':    return { y: -SLIDE_DIST_Y };
            case 'bottom': return { y: SLIDE_DIST_Y };
            default:       return { x: -SLIDE_DIST };
        }
    }

    function slideToVars(dir) {
        switch (dir) {
            case 'left':   return { x: -SLIDE_DIST };
            case 'right':  return { x: SLIDE_DIST };
            case 'top':    return { y: -SLIDE_DIST_Y };
            case 'bottom': return { y: SLIDE_DIST_Y };
            default:       return { x: -SLIDE_DIST };
        }
    }


    // ─── NOWE typy wejścia/wyjścia (v1.1) ───────────────────────────────────────
    //
    // Każdy typ zwraca { from, to, ease? } dla wejścia (IN) i wyjścia (OUT).
    // `ease` w definicji typu jest używany zamiast easingu z konfiguracji
    // (np. pop → back.out, drop → bounce.out).

    const RESET_PROPS = 'clipPath,maskImage,webkitMaskImage,--cg-p,filter,rotation,rotationX,rotationY,skewX,skewY';

    function _clipInset(dir, amount) {
        // amount: '100%' = w pełni ukryty, '0%' = w pełni widoczny
        switch (dir) {
            case 'right':  return `inset(0 0 0 ${amount})`;
            case 'top':    return `inset(0 0 ${amount} 0)`;
            case 'bottom': return `inset(${amount} 0 0 0)`;
            default:       return `inset(0 ${amount} 0 0)`; // left
        }
    }

    // 8 pasków; wysokość nieprzezroczystej części rośnie ze zmienną --cg-p (0→1)
    const _SHUTTER = 'repeating-linear-gradient(180deg,#000 0,#000 calc(var(--cg-p) * 12.5%),transparent calc(var(--cg-p) * 12.5%),transparent 12.5%)';
    const EXTRA_TYPES = {
        // odsłonięcie maską clip-path od krawędzi
        clip: {
            in:  (d) => ({ from: { opacity: 1, clipPath: _clipInset(d, '100%') }, to: { opacity: 1, clipPath: _clipInset(d, '0%') } }),
            out: (d) => ({ to: { clipPath: _clipInset(d === 'left' ? 'right' : d === 'right' ? 'left' : d === 'top' ? 'bottom' : 'top', '100%') } }),
        },
        blur: {
            in:  () => ({ from: { opacity: 0, filter: 'blur(24px)' }, to: { opacity: 1, filter: 'blur(0px)' } }),
            out: () => ({ to: { opacity: 0, filter: 'blur(24px)' } }),
        },
        // sprężyste „wyskoczenie" ze skali
        pop: {
            in:  () => ({ from: { opacity: 0, scale: 0.3 }, to: { opacity: 1, scale: 1 }, ease: 'back.out(2)' }),
            out: () => ({ to: { opacity: 0, scale: 0.3 }, ease: 'back.in(2)' }),
        },
        // spadnięcie z odbiciem
        drop: {
            in:  (d) => {
                const v = d === 'bottom' ? { y: 160 } : d === 'left' ? { x: -160 } : d === 'right' ? { x: 160 } : { y: -160 };
                return { from: { opacity: 0, ...v }, to: { opacity: 1, x: 0, y: 0 }, ease: 'bounce.out' };
            },
            out: (d) => {
                const v = d === 'top' ? { y: -120 } : d === 'left' ? { x: -120 } : d === 'right' ? { x: 120 } : { y: 120 };
                return { to: { opacity: 0, ...v }, ease: 'power2.in' };
            },
        },
        rotate: {
            in:  (d) => ({ from: { opacity: 0, rotation: d === 'right' ? 90 : -90, transformOrigin: d === 'right' ? 'right bottom' : 'left bottom' },
                            to: { opacity: 1, rotation: 0 } }),
            out: (d) => ({ to: { opacity: 0, rotation: d === 'right' ? 90 : -90, transformOrigin: d === 'right' ? 'right bottom' : 'left bottom' } }),
        },
        'flip-x': {
            in:  (d) => ({ from: { opacity: 0, rotationX: d === 'bottom' ? 90 : -90, transformOrigin: d === 'bottom' ? 'center bottom' : 'center top' },
                            to: { opacity: 1, rotationX: 0 } }),
            out: (d) => ({ to: { opacity: 0, rotationX: d === 'top' ? -90 : 90, transformOrigin: d === 'top' ? 'center top' : 'center bottom' } }),
        },
        // wjazd z przestrzeleniem i osiadaniem
        'slide-overshoot': {
            in:  (d) => ({ from: { opacity: 0, ...slideFromVars(d) }, to: { opacity: 1, x: 0, y: 0 }, ease: 'back.out(1.2)' }),
            out: (d) => ({ to: { opacity: 0, ...slideToVars(d) }, ease: 'back.in(1.2)' }),
        },
        // odsłonięcie od środka (poziomo: left/right, pionowo: top/bottom)
        'split-reveal': {
            in:  (d) => {
                const v = (d === 'top' || d === 'bottom');
                return { from: { opacity: 1, clipPath: v ? 'inset(50% 0 50% 0)' : 'inset(0 50% 0 50%)' },
                         to:   { opacity: 1, clipPath: v ? 'inset(0% 0 0% 0)'   : 'inset(0 0% 0 0%)' } };
            },
            out: (d) => {
                const v = (d === 'top' || d === 'bottom');
                return { to: { clipPath: v ? 'inset(50% 0 50% 0)' : 'inset(0 50% 0 50%)' } };
            },
        },
        // odsłonięcie ukośną maską (pasuje do skośnych belek)
        'mask-diag': {
            in:  (d) => {
                const r = d === 'right';
                return { from: { opacity: 1, clipPath: r ? 'polygon(100% 0%,100% 0%,100% 100%,100% 100%)' : 'polygon(0% 0%,0% 0%,0% 100%,0% 100%)' },
                         to:   { opacity: 1, clipPath: r ? 'polygon(-25% 0%,100% 0%,100% 100%,0% 100%)'   : 'polygon(0% 0%,100% 0%,125% 100%,0% 100%)' } };
            },
            out: (d) => {
                const r = d === 'right';
                return { to: { clipPath: r ? 'polygon(100% 0%,100% 0%,100% 100%,100% 100%)' : 'polygon(0% 0%,0% 0%,0% 100%,0% 100%)' } };
            },
        },
        // żaluzja: N poziomych pasków odsłania się kolejno
        shutter: {
            in:  () => ({ from: { opacity: 1, webkitMaskImage: _SHUTTER, maskImage: _SHUTTER, '--cg-p': 0 }, to: { opacity: 1, '--cg-p': 1 } }),
            out: () => ({ from: { webkitMaskImage: _SHUTTER, maskImage: _SHUTTER, '--cg-p': 1 }, to: { '--cg-p': 0 } }),
        },
        // belka rośnie z cienkiej linii
        'bar-grow': {
            in:  () => ({ from: { opacity: 1, scaleY: 0.04, transformOrigin: 'center center' }, to: { opacity: 1, scaleY: 1 } }),
            out: () => ({ to: { opacity: 1, scaleY: 0.04, transformOrigin: 'center center' } }),
        },
        // wejście z rozdzieleniem kanałów (czerwony/cyjan), które się schodzą
        'rgb-split': {
            in:  () => ({ from: { opacity: 0, x: -14, filter: 'drop-shadow(8px 0 0 rgba(255,0,60,0.8)) drop-shadow(-8px 0 0 rgba(0,220,255,0.8))' },
                          to:   { opacity: 1, x: 0, filter: 'drop-shadow(0px 0 0 rgba(255,0,60,0)) drop-shadow(0px 0 0 rgba(0,220,255,0))' } }),
            out: () => ({ to: { opacity: 0, x: 14, filter: 'drop-shadow(-8px 0 0 rgba(255,0,60,0.8)) drop-shadow(8px 0 0 rgba(0,220,255,0.8))' } }),
        },
        // wjazd ze skosem (broadcast „speed" look)
        skew: {
            in:  (d) => {
                const r = d === 'right';
                return { from: { opacity: 0, x: r ? SLIDE_DIST : -SLIDE_DIST, skewX: r ? 25 : -25 }, to: { opacity: 1, x: 0, skewX: 0 } };
            },
            out: (d) => {
                const r = d === 'right';
                return { to: { opacity: 0, x: r ? SLIDE_DIST : -SLIDE_DIST, skewX: r ? -25 : 25 } };
            },
        },
    };

    // Typ złożony: miganie + drgnięcie (glitch) — buduje własny timeline.
    function _glitchIn(tl, target, duration) {
        const step = duration / 7;
        tl.set(target, { opacity: 0, x: 0 })
          .set(target, { opacity: 1, x: -12 }, step)
          .set(target, { opacity: 0.2, x: 9 }, step * 2)
          .set(target, { opacity: 1, x: -5 }, step * 3)
          .set(target, { opacity: 0.4, x: 4 }, step * 4)
          .set(target, { opacity: 1, x: 0 }, step * 6);
    }
    function _glitchOut(tl, target, duration) {
        const step = duration / 6;
        tl.set(target, { opacity: 0.3, x: 10 })
          .set(target, { opacity: 1, x: -8 }, step)
          .set(target, { opacity: 0.2, x: 6 }, step * 2)
          .set(target, { opacity: 1, x: -3 }, step * 3)
          .set(target, { opacity: 0, x: 0 }, step * 4);
    }

    const ANIM_TYPE_NAMES = [
        'slide', 'fade', 'zoom', 'zoom-in', 'wipe', 'wipe-y', 'flip',
        ...Object.keys(EXTRA_TYPES), 'glitch', 'none',
    ];

    // Zwraca true, jeśli typ obsłużono (dodano do tl).
    function _applyExtra(tl, target, type, dir, duration, ease, mode, stagger = 0) {
        if (type === 'glitch') {
            (mode === 'in' ? _glitchIn : _glitchOut)(tl, target, duration);
            return true;
        }
        const def = EXTRA_TYPES[type];
        if (!def) return false;
        const spec = def[mode](dir);
        const useEase = spec.ease || ease;
        if (mode === 'in') {
            tl.fromTo(target, spec.from, { ...spec.to, duration, ease: useEase, stagger });
        } else if (spec.from) {
            tl.fromTo(target, spec.from, { ...spec.to, duration, ease: useEase, stagger });
        } else {
            tl.to(target, { ...spec.to, duration, ease: useEase, stagger });
        }
        return true;
    }

    // Nieznany typ (np. z nowszej wersji edytora) → klucz `fallback`, w ostateczności slide.
    function _resolveType(config) {
        const t = config.type || 'slide';
        if (ANIM_TYPE_NAMES.includes(t)) return t;
        return (config.fallback && ANIM_TYPE_NAMES.includes(config.fallback)) ? config.fallback : t;
    }

    // ─── core show / hide (replaces GSAPFX.standardIn / standardOut) ────────────

    function standardIn(target, config = {}) {
        if (window._GSAP_DEBUG) console.log('[CGMotion] standardIn', target, config);
        const type = _resolveType(config);
        const dir = config.direction || 'left';
        const duration = config.duration ?? 0.5;
        const delay = config.delay ?? 0;
        const ease = normalizeEase(config.ease || 'power2.out');

        const tl = gsap.timeline({ delay });
        if (target) gsap.set(target, { transition: 'none', visibility: 'visible', clearProps: RESET_PROPS });

        if (type === 'none') {
            tl.set(target, { opacity: 1, x: 0, y: 0, scale: 1 });
            return tl;
        }
        if (type === 'fade') {
            tl.fromTo(target, { opacity: 0, scale: 1 }, { opacity: 1, scale: 1, duration, ease });
        } else if (type === 'zoom') {
            tl.fromTo(target, { opacity: 0, scale: 0.8 }, { opacity: 1, scale: 1, duration, ease });
        } else if (type === 'zoom-in') {
            tl.fromTo(target, { opacity: 0, scale: 1.15 }, { opacity: 1, scale: 1, duration, ease });
        } else if (type === 'wipe') {
            const origin = dir === 'right' ? 'right center' : 'left center';
            tl.fromTo(target, { opacity: 0, scaleX: 0, scaleY: 1, transformOrigin: origin }, { opacity: 1, scaleX: 1, scaleY: 1, duration, ease });
        } else if (type === 'wipe-y') {
            const origin = dir === 'bottom' ? 'center bottom' : 'center top';
            tl.fromTo(target, { opacity: 0, scaleX: 1, scaleY: 0, transformOrigin: origin }, { opacity: 1, scaleX: 1, scaleY: 1, duration, ease });
        } else if (type === 'flip') {
            tl.fromTo(target, { opacity: 0, rotationY: -90, transformOrigin: 'left center' }, { opacity: 1, rotationY: 0, duration, ease });
        } else if (_applyExtra(tl, target, type, dir, duration, ease, 'in')) {
            // typ z EXTRA_TYPES / glitch — dodano wyżej
        } else {
            // slide
            const from = { opacity: 0, scale: 1, ...slideFromVars(dir) };
            tl.fromTo(target, from, { opacity: 1, x: 0, y: 0, scale: 1, duration, ease });
        }
        return tl;
    }

    function standardOut(target, config = {}) {
        if (window._GSAP_DEBUG) console.log('[CGMotion] standardOut', target, config);
        const type = _resolveType(config);
        const dir = config.direction || 'left';
        const duration = config.duration ?? 0.5;
        const delay = config.delay ?? 0;
        const ease = normalizeEase(config.ease || 'power2.in');

        const tl = gsap.timeline({ delay });
        if (target) gsap.set(target, { transition: 'none' });

        if (type === 'none') {
            tl.set(target, { opacity: 0 });
            return tl;
        }
        if (type === 'fade') {
            tl.to(target, { opacity: 0, duration, ease });
        } else if (type === 'zoom') {
            tl.to(target, { opacity: 0, scale: 0.8, duration, ease });
        } else if (type === 'zoom-in') {
            tl.to(target, { opacity: 0, scale: 1.15, duration, ease });
        } else if (type === 'wipe') {
            const origin = dir === 'left' ? 'left center' : 'right center';
            tl.to(target, { opacity: 0, scaleX: 0, transformOrigin: origin, duration, ease });
        } else if (type === 'wipe-y') {
            const origin = dir === 'top' ? 'center top' : 'center bottom';
            tl.to(target, { opacity: 0, scaleY: 0, transformOrigin: origin, duration, ease });
        } else if (type === 'flip') {
            tl.to(target, { opacity: 0, rotationY: 90, transformOrigin: 'right center', duration, ease });
        } else if (_applyExtra(tl, target, type, dir, duration, ease, 'out')) {
            // typ z EXTRA_TYPES / glitch — dodano wyżej
        } else {
            // slide
            tl.to(target, { opacity: 0, duration, ease, ...slideToVars(dir) });
        }
        return tl;
    }

    // ─── text effects ────────────────────────────────────────────────────────────

    const TEXT_SELECTORS = [
        '.title', '.subtitle', '.modern-title', '.rep-title', '.na-zywo-text',
        '.rep-clock', '.ticker-label', '.text-overlay', '.sub-text-overlay',
        '.live-text', '.news-text-v8', '.wiper-text-v8', '.utk-msg-box', '.utk-item',
        '[data-cg-text]',
    ].join(', ');

    function revealText(target, duration = 1, delay = 0, ease = 'power2.out') {
        const tl = gsap.timeline({ delay });
        tl.fromTo(target,
            { clipPath: 'inset(0 100% 0 0)', opacity: 1 },
            { clipPath: 'inset(0 0% 0 0)', duration, ease }
        );
        return tl;
    }

    function hideText(target, duration = 0.5, delay = 0, ease = 'power2.in') {
        const tl = gsap.timeline({ delay });
        tl.to(target, { clipPath: 'inset(0 0 0 100%)', duration, ease });
        return tl;
    }

    function blurIn(target, duration = 1, delay = 0, ease = 'power2.out') {
        gsap.set(target, { filter: 'blur(20px)', opacity: 0 });
        return gsap.to(target, { duration, delay, filter: 'blur(0px)', opacity: 1, ease: normalizeEase(ease) });
    }

    function blurOut(target, duration = 0.5, delay = 0, ease = 'power2.in') {
        return gsap.to(target, { duration, delay, filter: 'blur(20px)', opacity: 0, ease: normalizeEase(ease) });
    }


    // ─── NOWE: dzielenie tekstu na słowa / znaki dla efektów tekstowych ──────────
    // Dzieli tylko elementy z samym tekstem (bez dzieci-elementów); idempotentne.

    function _splitUnits(el, by) {
        if (el._cgSplit && el._cgSplit.by === by) return el._cgSplit.units;
        if (el.children.length > 0 || !el.textContent.trim()) return null;
        if (el._cgSplit) return el._cgSplit.units; // inny tryb już podzielony — zostaw
        const text = el.textContent;
        const parts = by === 'chars' ? Array.from(text) : text.split(/(\s+)/);
        el.textContent = '';
        const units = [];
        parts.forEach(p => {
            if (/^\s+$/.test(p)) { el.appendChild(document.createTextNode(p)); return; }
            if (p === '') return;
            const sp = document.createElement('span');
            sp.style.display = 'inline-block';
            sp.style.whiteSpace = 'pre';
            sp.textContent = p;
            el.appendChild(sp);
            units.push(sp);
        });
        el._cgSplit = { by, units, text };
        return units;
    }

    const SPLIT_FROM = {
        up:    { y: 40, opacity: 0 },
        down:  { y: -40, opacity: 0 },
        fade:  { opacity: 0 },
        scale: { scale: 0.3, opacity: 0 },
        blur:  { filter: 'blur(14px)', opacity: 0 },
        clip:  { clipPath: 'inset(100% 0 0 0)', opacity: 1 },
    };
    const SPLIT_TO_OUT = {
        up:    { y: -40, opacity: 0 },
        down:  { y: 40, opacity: 0 },
        fade:  { opacity: 0 },
        scale: { scale: 0.3, opacity: 0 },
        blur:  { filter: 'blur(14px)', opacity: 0 },
        clip:  { clipPath: 'inset(0 0 100% 0)' },
    };

    // Zwraca true jeśli obsłużono typ „words-*" / „chars-*".
    // Grupuje słowa w linie wg offsetTop; zwraca tablicę tablic (GSAP animuje każdą linię jako jednostkę).
    function _groupLines(units) {
        const lines = [];
        let top = null;
        units.forEach(u => {
            const y = u.offsetTop;
            if (top === null || Math.abs(y - top) > 2) { lines.push([]); top = y; }
            lines[lines.length - 1].push(u);
        });
        return lines;
    }

    function _splitTextEffect(tl, targets, type, duration, stagger, ease, mode, from) {
        const m = /^(words|chars|lines)-(up|down|fade|scale|blur|clip)$/.exec(type);
        if (!m) return false;
        const by = m[1] === 'chars' ? 'chars' : 'words';
        const isLines = m[1] === 'lines';
        const variant = m[2];
        targets.forEach(t => {
            const units = _splitUnits(t, by);
            let list = units && units.length ? units : [t];
            if (isLines && units && units.length) list = _groupLines(units);
            const per = stagger > 0 ? stagger : (by === 'chars' && !isLines ? 0.04 : 0.09);
            const st = Math.min(per, 1.2 / Math.max(1, list.length));
            const stg = from && from !== 'start' ? { each: st, from } : st;
            if (mode === 'in') {
                tl.fromTo(list, { ...SPLIT_FROM[variant] },
                    { y: 0, x: 0, scale: 1, opacity: 1, filter: 'blur(0px)', ...(variant === 'clip' ? { clipPath: 'inset(0% 0 0% 0)' } : {}), duration, ease, stagger: stg }, 0);
            } else {
                tl.to(list, { ...SPLIT_TO_OUT[variant], duration, ease, stagger: stg }, 0);
            }
        });
        return true;
    }

    const TEXT_KNOWN = /^(typewriter|reveal|blur|blurIn|blurOut|fade|scale|slide-(up|down|left|right)|slideReveal|slideHide|hide|clip-(up|down)|(words|chars|lines)-(up|down|fade|scale|blur|clip)|scramble|counter|typewriter-caret)$/;
    function _resolveText(config) {
        const t = config.type;
        if (TEXT_KNOWN.test(t)) return t;
        return (config.fallback && TEXT_KNOWN.test(config.fallback)) ? config.fallback : t;
    }


    // ─── efekty znakowe operujące na treści (scramble / counter / typewriter-caret) ──
    // Działają tylko na elementach z samym tekstem; w innym wypadku wracają do fade.

    const SCRAMBLE_CHARS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789#%&@$';

    function _plainTextEl(el) { return el.children.length === 0 && el.textContent.trim() !== ''; }

    function _scrambleIn(tl, el, duration, ease) {
        const full = el._cgOrig != null ? el._cgOrig : (el._cgOrig = el.textContent);
        const chars = Array.from(full);
        const st = { p: 0 };
        tl.fromTo(st, { p: 0 }, { p: 1, duration, ease, onUpdate: () => {
            const done = Math.floor(st.p * chars.length);
            el.textContent = chars.map((c, i) => (i < done || /\s/.test(c)) ? c : SCRAMBLE_CHARS[Math.floor(Math.random() * SCRAMBLE_CHARS.length)]).join('');
        }, onComplete: () => { el.textContent = full; } }, 0);
    }

    // Pierwsza liczba w tekście (np. "Wynik: 1 250,5 zł") liczy się od 0 do wartości, zachowując format.
    function _counterIn(tl, el, duration, ease) {
        const full = el._cgOrig != null ? el._cgOrig : (el._cgOrig = el.textContent);
        const m = /-?\d[\d\s.,]*\d|-?\d/.exec(full);
        if (!m) return false;
        const raw = m[0];
        // Format: separator dziesiętny tylko gdy po nim nie ma dokładnie 3 cyfr; spacja/kropka/przecinek 3-cyfrowy = tysiące.
        const sepMatch = /[.,](\d+)$/.exec(raw);
        const isDec = !!sepMatch && sepMatch[1].length !== 3;
        const decimals = isDec ? sepMatch[1].length : 0;
        const decSep = isDec ? raw[raw.length - decimals - 1] : '.';
        const gm = isDec ? /\d([\s.,])\d{3}(?!\d)/.exec(raw.slice(0, raw.length - decimals - 1)) : /\d([\s.,])\d{3}(?!\d)/.exec(raw);
        const grp = gm ? gm[1] : '';
        const num = (isDec ? raw.slice(0, raw.length - decimals - 1) : raw).replace(/[\s.,]/g, '') + (isDec ? '.' + sepMatch[1] : '');
        const target = parseFloat(num);
        if (!isFinite(target)) return false;
        const fmt = v => {
            let [i, d] = v.toFixed(decimals).split('.');
            if (grp) i = i.replace(/\B(?=(\d{3})+(?!\d))/g, grp);
            return d ? i + decSep + d : i;
        };
        const st = { v: 0 };
        tl.fromTo(st, { v: 0 }, { v: target, duration, ease, onUpdate: () => {
            el.textContent = full.slice(0, m.index) + fmt(st.v) + full.slice(m.index + raw.length);
        }, onComplete: () => { el.textContent = full; } }, 0);
        return true;
    }

    function _caretType(tl, el, duration, ease) {
        const full = el._cgOrig != null ? el._cgOrig : (el._cgOrig = el.textContent);
        const chars = Array.from(full);
        const caret = document.createElement('span');
        caret.textContent = '|';
        caret.style.cssText = 'display:inline-block;margin-left:1px;font-weight:300;';
        const st = { p: 0 };
        tl.fromTo(st, { p: 0 }, { p: 1, duration, ease, onStart: () => {}, onUpdate: () => {
            el.textContent = chars.slice(0, Math.round(st.p * chars.length)).join('');
            el.appendChild(caret);
        }, onComplete: () => {
            el.textContent = full; el.appendChild(caret);
            gsap.to(caret, { opacity: 0, duration: 0.4, delay: 0.5, repeat: 3, yoyo: true, ease: 'none', onComplete: () => caret.remove() });
        } }, 0);
    }

    function _contentEffect(tl, targets, type, duration, ease) {
        if (type !== 'scramble' && type !== 'counter' && type !== 'typewriter-caret') return false;
        targets.forEach(t => {
            let ok = _plainTextEl(t);
            if (ok) {
                if (type === 'scramble') _scrambleIn(tl, t, duration, ease);
                else if (type === 'counter') ok = _counterIn(tl, t, duration, ease);
                else _caretType(tl, t, duration, ease);
            }
            if (!ok) tl.fromTo(t, { opacity: 0 }, { opacity: 1, duration, ease }, 0); // zapas
        });
        return true;
    }

    function applyTextEffect(root, config = {}) {
        if (!config || config.type === 'none') return null;

        const type = _resolveText(config);
        const duration = config.duration ?? 1;
        const delay = config.delay ?? 0.2;
        const stagger = config.stagger ?? 0;
        const ease = normalizeEase(config.ease || 'power2.out');

        const targets = root.querySelectorAll(TEXT_SELECTORS);
        if (targets.length === 0) return null;

        const tl = gsap.timeline({ delay });

        if (_contentEffect(tl, targets, type, duration, ease)) {
            // scramble / counter / typewriter-caret — dodano wyżej
        } else if (_splitTextEffect(tl, targets, type, duration, stagger, ease, 'in', config.staggerFrom)) {
            // words-* / chars-* — dodano wyżej
        } else if (type === 'typewriter' || type === 'reveal') {
            targets.forEach((t, i) => tl.add(revealText(t, duration, i * stagger, ease), 0));
        } else if (type === 'blur' || type === 'blurIn') {
            tl.fromTo(targets, { filter: 'blur(20px)', opacity: 0 }, { filter: 'blur(0px)', opacity: 1, duration, stagger, ease }, 0);
        } else if (type === 'fade') {
            tl.fromTo(targets, { opacity: 0 }, { opacity: 1, duration, stagger, ease }, 0);
        } else if (type === 'scale') {
            tl.fromTo(targets, { opacity: 0, scale: 0.5 }, { opacity: 1, scale: 1, duration, stagger, ease }, 0);
        } else if (type === 'slide-up' || type === 'slideReveal') {
            tl.fromTo(targets, { y: 50, opacity: 0 }, { y: 0, opacity: 1, duration, stagger, ease }, 0);
        } else if (type === 'slide-down') {
            tl.fromTo(targets, { y: -50, opacity: 0 }, { y: 0, opacity: 1, duration, stagger, ease }, 0);
        } else if (type === 'slide-left') {
            tl.fromTo(targets, { x: 50, opacity: 0 }, { x: 0, opacity: 1, duration, stagger, ease }, 0);
        } else if (type === 'slide-right') {
            tl.fromTo(targets, { x: -50, opacity: 0 }, { x: 0, opacity: 1, duration, stagger, ease }, 0);
        } else if (type === 'clip-up') {
            // Nowy: clip-path reveal z dołu (broadcast-style)
            tl.fromTo(targets,
                { clipPath: 'inset(100% 0 0 0)', opacity: 1 },
                { clipPath: 'inset(0% 0 0 0)', duration, stagger, ease }, 0);
        }
        return tl;
    }

    function applyTextOutEffect(root, config = {}) {
        if (!config || config.type === 'none') return null;

        const type = _resolveText(config);
        const duration = config.duration ?? 0.5;
        const delay = config.delay ?? 0;
        const stagger = config.stagger ?? 0;
        const ease = normalizeEase(config.ease || 'power2.in');

        const targets = root.querySelectorAll(TEXT_SELECTORS);
        if (targets.length === 0) return null;

        const tl = gsap.timeline({ delay });

        targets.forEach(t => { if (t.style.clipPath) gsap.set(t, { clipPath: 'none' }); });

        if (_splitTextEffect(tl, targets, type, duration, stagger, ease, 'out', config.staggerFrom)) {
            // words-* / chars-* — dodano wyżej
        } else if (type === 'fade') {
            tl.to(targets, { opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'blur' || type === 'blurOut') {
            tl.to(targets, { filter: 'blur(20px)', opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'scale') {
            tl.to(targets, { scale: 0.5, opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'hide' || type === 'slideHide') {
            targets.forEach((t, i) => tl.add(hideText(t, duration, i * stagger, ease), 0));
        } else if (type === 'slide-up') {
            tl.to(targets, { y: -50, opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'slide-down') {
            tl.to(targets, { y: 50, opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'slide-left') {
            tl.to(targets, { x: -50, opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'slide-right') {
            tl.to(targets, { x: 50, opacity: 0, duration, stagger, ease }, 0);
        } else if (type === 'clip-down') {
            tl.to(targets, { clipPath: 'inset(100% 0 0 0)', duration, stagger, ease }, 0);
        }
        return tl;
    }

    // ─── NOWE: Sequence Builder ──────────────────────────────────────────────────
    //
    // CGMotion.build([
    //   { target: '.plate', type: 'wipe', direction: 'left', duration: 0.4 },
    //   { target: '.title', type: 'clip-up', duration: 0.5, at: '>-0.1' },
    //   { target: '.subtitle', type: 'fade', duration: 0.4, at: '>-0.15' },
    // ])

    function build(steps, root = document) {
        const master = gsap.timeline();
        (steps || []).forEach(step => {
            const { target, type, at, ...opts } = step;
            const el = typeof target === 'string' ? root.querySelectorAll(target) : target;
            if (!el) return;
            const pos = at !== undefined ? at : '>';
            const subTl = _buildStepTl(el, type || 'fade', opts);
            master.add(subTl, pos);
        });
        return master;
    }

    function _buildStepTl(el, type, opts = {}) {
        const duration = opts.duration ?? 0.5;
        const ease = normalizeEase(opts.ease || 'power2.out');
        const stagger = opts.stagger ?? 0;
        const dir = opts.direction || 'left';
        const tl = gsap.timeline();

        switch (type) {
            case 'fade':
                tl.fromTo(el, { opacity: 0, scale: 1 }, { opacity: 1, scale: 1, duration, ease, stagger }); break;
            case 'fade-out':
                tl.to(el, { opacity: 0, duration, ease, stagger }); break;
            case 'slide':
                tl.fromTo(el, { opacity: 0, scale: 1, ...slideFromVars(dir) }, { opacity: 1, x: 0, y: 0, scale: 1, duration, ease, stagger }); break;
            case 'slide-out':
                tl.to(el, { opacity: 0, duration, ease, stagger, ...slideToVars(dir) }); break;
            // Typy zgodne z standardIn/standardOut — element z CSS-owym opacity:0 / transform musi
            // dojść do pełnej widoczności i skali 1 (bez tego belki zostają niewidoczne lub zmniejszone).
            case 'zoom':
                tl.fromTo(el, { opacity: 0, scale: 0.8 }, { opacity: 1, scale: 1, duration, ease, stagger }); break;
            case 'zoom-in':
                tl.fromTo(el, { opacity: 0, scale: 1.15 }, { opacity: 1, scale: 1, duration, ease, stagger }); break;
            case 'zoom-out':
                tl.to(el, { opacity: 0, scale: 0.8, duration, ease, stagger }); break;
            case 'none':
                tl.set(el, { opacity: 1, x: 0, y: 0, scale: 1 }); break;
            case 'none-out':
                tl.set(el, { opacity: 0 }); break;
            case 'flip':
                tl.fromTo(el, { opacity: 0, rotationY: -90, transformOrigin: 'left center' }, { opacity: 1, rotationY: 0, duration, ease, stagger }); break;
            case 'wipe':
                tl.fromTo(el, { opacity: 0, scaleX: 0, scaleY: 1, transformOrigin: dir === 'right' ? 'right center' : 'left center' }, { opacity: 1, scaleX: 1, scaleY: 1, duration, ease, stagger }); break;
            case 'wipe-out':
                tl.to(el, { opacity: 0, scaleX: 0, transformOrigin: dir === 'right' ? 'right center' : 'left center', duration, ease, stagger }); break;
            case 'wipe-y':
                tl.fromTo(el, { opacity: 0, scaleX: 1, scaleY: 0, transformOrigin: dir === 'bottom' ? 'center bottom' : 'center top' }, { opacity: 1, scaleX: 1, scaleY: 1, duration, ease, stagger }); break;
            case 'wipe-y-out':
                tl.to(el, { opacity: 0, scaleY: 0, transformOrigin: dir === 'top' ? 'center top' : 'center bottom', duration, ease, stagger }); break;
            case 'flip-in':
                tl.fromTo(el, { rotationY: -90, opacity: 0, transformOrigin: 'left center' }, { rotationY: 0, opacity: 1, duration, ease, stagger }); break;
            case 'clip-up':
                tl.fromTo(el, { clipPath: 'inset(100% 0 0 0)' }, { clipPath: 'inset(0% 0 0 0)', duration, ease, stagger }); break;
            case 'clip-down':
                tl.fromTo(el, { clipPath: 'inset(0 0 100% 0)' }, { clipPath: 'inset(0 0 0% 0)', duration, ease, stagger }); break;
            case 'clip-right':
                tl.fromTo(el, { clipPath: 'inset(0 100% 0 0)' }, { clipPath: 'inset(0 0% 0 0)', duration, ease, stagger }); break;
            case 'clip-left':
                tl.fromTo(el, { clipPath: 'inset(0 0 0 100%)' }, { clipPath: 'inset(0 0 0 0%)', duration, ease, stagger }); break;
            case 'scale-in':
                tl.fromTo(el, { opacity: 0, scale: opts.from ?? 0.8 }, { opacity: 1, scale: 1, duration, ease, stagger }); break;
            case 'scale-out':
                tl.to(el, { opacity: 0, scale: opts.to ?? 0.8, duration, ease, stagger }); break;
            case 'blur-in':
                tl.fromTo(el, { filter: 'blur(20px)', opacity: 0 }, { filter: 'blur(0px)', opacity: 1, duration, ease, stagger }); break;
            case 'blur-out':
                tl.to(el, { filter: 'blur(20px)', opacity: 0, duration, ease, stagger }); break;
            case 'flip-out':
                tl.to(el, { rotationY: 90, opacity: 0, transformOrigin: 'right center', duration, ease, stagger }); break;
            case 'set':
                tl.set(el, { opacity: 1, x: 0, y: 0, scale: 1, ...(opts.vars || {}) }); break;
            default: {
                const isOut = /-out$/.test(type);
                const base = type.replace(/-out$/, '');
                if (!_applyExtra(tl, el, base, dir, duration, ease, isOut ? 'out' : 'in', stagger)) {
                    tl.fromTo(el, { opacity: 0 }, { opacity: 1, duration, ease, stagger });
                }
            }
        }
        return tl;
    }

    // ─── NOWE: Deklaratywne auto-animacje (data-cg-anim) ────────────────────────
    //
    // Elementy w szablonie deklarują własne animacje:
    //   <div class="plate" data-cg-anim="wipe direction:left duration:0.4">
    //   <div class="title" data-cg-anim="clip-up duration:0.5 delay:0.15" data-cg-stagger="0.05">
    //
    // Renderer woła autoShow(root, config) gdy __slt_show nie istnieje.

    function autoShow(root, containerConfig = {}) {
        const targets = collectAnimTargets(root, 'data-cg-anim');

        if (targets.length === 0) {
            // Brak deklaracji — fallback do standardIn na pierwszym dziecku lub root
            const container = root.querySelector('[class]') || root;
            return standardIn(container, containerConfig);
        }

        const master = gsap.timeline();
        targets.forEach(({ el, spec }) => {
            const { type, delay = 0, at, stagger: staggerAttr, ...rest } = spec;
            const stagger = staggerAttr ?? parseFloat(el.getAttribute('data-cg-stagger') || '0');
            const pos = at !== undefined ? at : '>';
            const subTl = _buildStepTl(el, type || 'fade', { ...rest, stagger });
            master.add(gsap.timeline({ delay }).add(subTl), pos);
        });

        // Tekst — jeśli elementy mają data-cg-text, animuj je też
        const textTargets = root.querySelectorAll('[data-cg-text]');
        if (textTargets.length > 0) {
            const textCfg = (() => {
                try { return JSON.parse(root.getAttribute('data-cg-text-config') || '{}'); } catch { return {}; }
            })();
            if (textCfg.type && textCfg.type !== 'none') {
                const textTl = applyTextEffect(root, textCfg);
                if (textTl) master.add(textTl, textCfg.at || '>-0.1');
            }
        }

        return master;
    }

    function autoHide(root, containerConfig = {}) {
        const targets = collectAnimTargets(root, 'data-cg-anim-out');

        if (targets.length === 0) {
            const container = root.querySelector('[class]') || root;
            return standardOut(container, containerConfig);
        }

        const master = gsap.timeline();
        targets.forEach(({ el, spec }) => {
            const { type, delay = 0, at, stagger: staggerAttr, ...rest } = spec;
            const stagger = staggerAttr ?? parseFloat(el.getAttribute('data-cg-stagger') || '0');
            const pos = at !== undefined ? at : '>';
            const subTl = _buildStepTl(el, type || 'fade-out', { ...rest, stagger });
            master.add(gsap.timeline({ delay }).add(subTl), pos);
        });

        return master;
    }

    // ─── NOWE: Attention Animators (looping) ─────────────────────────────────────
    //
    // CGMotion.loop(element, 'pulse')
    // CGMotion.loop(element, 'float', { amplitude: 8, duration: 2 })
    // CGMotion.stopLoop(element)

    const _loopTimelines = new WeakMap();

    const ATTENTION_PRESETS = {
        pulse: (el, opts = {}) => {
            const scale = opts.scale ?? 1.04;
            const duration = opts.duration ?? 0.8;
            return gsap.timeline({ repeat: -1, yoyo: true })
                .to(el, { scale, duration, ease: 'sine.inOut' });
        },
        flash: (el, opts = {}) => {
            const duration = opts.duration ?? 0.5;
            const times = opts.times ?? 3;
            const tl = gsap.timeline({ repeat: -1, repeatDelay: opts.repeatDelay ?? 2 });
            for (let i = 0; i < times; i++) {
                tl.to(el, { opacity: 0, duration: duration / 2, ease: 'none' })
                  .to(el, { opacity: 1, duration: duration / 2, ease: 'none' });
            }
            return tl;
        },
        float: (el, opts = {}) => {
            const amplitude = opts.amplitude ?? 6;
            const duration = opts.duration ?? 2.2;
            return gsap.timeline({ repeat: -1, yoyo: true })
                .to(el, { y: -amplitude, duration, ease: 'sine.inOut' });
        },
        shake: (el, opts = {}) => {
            const amount = opts.amount ?? 5;
            const duration = opts.duration ?? 0.08;
            const tl = gsap.timeline({ repeat: -1, repeatDelay: opts.repeatDelay ?? 3 });
            [-1, 1, -1, 1, -0.5, 0.5, 0].forEach(m => {
                tl.to(el, { x: m * amount, duration, ease: 'none' });
            });
            return tl;
        },
        glow: (el, opts = {}) => {
            const color = opts.color ?? 'rgba(255,255,255,0.4)';
            const duration = opts.duration ?? 1.5;
            return gsap.timeline({ repeat: -1, yoyo: true })
                .to(el, { filter: `drop-shadow(0 0 8px ${color})`, duration, ease: 'sine.inOut' });
        },
        wobble: (el, opts = {}) => {
            const angle = opts.angle ?? 1.2;
            const duration = opts.duration ?? 0.9;
            return gsap.timeline({ repeat: -1, yoyo: true })
                .fromTo(el, { rotation: -angle }, { rotation: angle, duration, ease: 'sine.inOut' });
        },
        heartbeat: (el, opts = {}) => {
            const scale = opts.scale ?? 1.06;
            const tl = gsap.timeline({ repeat: -1, repeatDelay: opts.repeatDelay ?? 1.2 });
            tl.to(el, { scale, duration: 0.12, ease: 'power2.out' })
              .to(el, { scale: 1, duration: 0.12, ease: 'power2.in' })
              .to(el, { scale: scale * 0.99, duration: 0.1, ease: 'power2.out' })
              .to(el, { scale: 1, duration: 0.3, ease: 'power2.in' });
            return tl;
        },
        bounce: (el, opts = {}) => {
            const amplitude = opts.amplitude ?? 10;
            const tl = gsap.timeline({ repeat: -1, repeatDelay: opts.repeatDelay ?? 2 });
            tl.to(el, { y: -amplitude, duration: 0.25, ease: 'power2.out' })
              .to(el, { y: 0, duration: 0.45, ease: 'bounce.out' });
            return tl;
        },
        // Połysk przesuwający się po elemencie: nakładka z gradientem w osobnym wrapperze z overflow:hidden.
        shine: (el, opts = {}) => {
            const duration = opts.duration ?? 1.2;
            const color = opts.color ?? 'rgba(255,255,255,0.35)';
            const wrap = document.createElement('div');
            wrap.setAttribute('data-cg-shine', '1');
            wrap.style.cssText = 'position:absolute;inset:0;overflow:hidden;pointer-events:none;border-radius:inherit;z-index:2147483000;';
            const bar = document.createElement('div');
            bar.style.cssText = 'position:absolute;top:-10%;bottom:-10%;left:0;width:30%;transform:translateX(-150%) skewX(-20deg);' +
                `background:linear-gradient(90deg,transparent,${color},transparent);`;
            wrap.appendChild(bar);
            let madeRelative = false;
            if (window.getComputedStyle && getComputedStyle(el).position === 'static') { el.style.position = 'relative'; madeRelative = true; }
            el.appendChild(wrap);
            const tl = gsap.timeline({ repeat: -1, repeatDelay: opts.repeatDelay ?? 2.5 });
            tl.fromTo(bar, { xPercent: -150 }, { xPercent: 450, duration, ease: 'power1.inOut' });
            tl._cgCleanup = () => { wrap.remove(); if (madeRelative) el.style.position = ''; };
            return tl;
        },
        // Linia pod elementem „wyjeżdża" i znika (akcent do tytułów).
        'underline-sweep': (el, opts = {}) => {
            const duration = opts.duration ?? 1.0;
            const color = opts.color ?? 'rgba(255,255,255,0.9)';
            const wrap = document.createElement('div');
            wrap.setAttribute('data-cg-shine', '1');
            wrap.style.cssText = 'position:absolute;left:0;right:0;bottom:0;height:3px;overflow:hidden;pointer-events:none;z-index:2147483000;';
            const bar = document.createElement('div');
            bar.style.cssText = `position:absolute;inset:0;background:${color};transform:scaleX(0);transform-origin:left center;`;
            wrap.appendChild(bar);
            let madeRelative = false;
            if (window.getComputedStyle && getComputedStyle(el).position === 'static') { el.style.position = 'relative'; madeRelative = true; }
            el.appendChild(wrap);
            const tl = gsap.timeline({ repeat: -1, repeatDelay: opts.repeatDelay ?? 2 });
            tl.fromTo(bar, { scaleX: 0, transformOrigin: 'left center' }, { scaleX: 1, duration: duration * 0.6, ease: 'power2.out' })
              .set(bar, { transformOrigin: 'right center' })
              .to(bar, { scaleX: 0, duration: duration * 0.4, ease: 'power2.in' });
            tl._cgCleanup = () => { wrap.remove(); if (madeRelative) el.style.position = ''; };
            return tl;
        },
        breathe: (el, opts = {}) => {
            const duration = opts.duration ?? 3;
            return gsap.timeline({ repeat: -1, yoyo: true })
                .to(el, { opacity: opts.minOpacity ?? 0.6, duration, ease: 'sine.inOut' });
        },
    };

    function loop(target, preset, opts = {}) {
        stopLoop(target);
        const factory = ATTENTION_PRESETS[preset];
        if (!factory) {
            console.warn(`[CGMotion] Unknown attention preset: "${preset}"`);
            return null;
        }
        // Wspólne pole `amplitude` (z inspektora) → parametr właściwy dla presetu.
        if (opts.amplitude != null && opts.amplitude !== '') {
            const a = Number(opts.amplitude);
            if (preset === 'pulse' || preset === 'heartbeat') opts = { ...opts, scale: 1 + a / 100 };
            else if (preset === 'wobble') opts = { ...opts, angle: a };
            else if (preset === 'breathe') opts = { ...opts, minOpacity: Math.max(0, 1 - a / 100) };
        }
        const tl = factory(target, opts);
        _loopTimelines.set(target, tl);
        return tl;
    }

    function stopLoop(target) {
        const existing = _loopTimelines.get(target);
        if (existing) {
            existing.kill();
            if (existing._cgCleanup) existing._cgCleanup();
            // Nie czyścimy 'all' — to skasowałoby opacity/transform ustawione przez IN.
            gsap.set(target, { x: 0, y: 0, scale: 1, rotation: 0, opacity: 1, filter: 'none' });
            _loopTimelines.delete(target);
        }
    }

    // ─── NOWE: Spring physics ────────────────────────────────────────────────────
    //
    // CGMotion.spring(el, { x: 0, y: 0 }, { tension: 400, friction: 28 })
    // Mapuje spring params na GSAP elastic ease.

    function spring(target, toVars, opts = {}) {
        const tension = opts.tension ?? 300;
        const friction = opts.friction ?? 25;
        const duration = opts.duration ?? (Math.sqrt(tension / 100) * 0.8);
        // Przybliżenie sprężyny jako GSAP elastic
        const amplitude = Math.max(0.5, 1 + (tension - 100) / 800);
        const period = friction / 100;
        const ease = `elastic.out(${amplitude.toFixed(2)}, ${period.toFixed(2)})`;

        return gsap.to(target, { ...toVars, duration, ease });
    }

    // ─── NOWE: Split text in/out ─────────────────────────────────────────────────
    //
    // Animuje tekst znak po znaku lub słowo po słowie bez SplitText plugin.
    // CGMotion.splitTextIn(el, { by: 'words', type: 'slide-up', stagger: 0.04 })

    function splitTextIn(target, opts = {}) {
        const by = opts.by || 'words';
        const type = opts.type || 'fade';
        const duration = opts.duration ?? 0.5;
        const stagger = opts.stagger ?? 0.05;
        const ease = normalizeEase(opts.ease || 'power2.out');

        const el = typeof target === 'string' ? document.querySelector(target) : target;
        if (!el) return gsap.timeline();

        const original = el.innerHTML;
        const text = el.textContent;
        let units;

        if (by === 'chars') {
            units = text.split('');
        } else {
            units = text.split(/\s+/).filter(Boolean);
        }

        const spans = units.map(unit => {
            const s = document.createElement('span');
            s.style.display = 'inline-block';
            s.style.overflow = 'hidden';
            const inner = document.createElement('span');
            inner.style.display = 'inline-block';
            inner.textContent = unit;
            s.appendChild(inner);
            if (by === 'words') {
                const space = document.createTextNode(' ');
                s.appendChild(space);
            }
            return { outer: s, inner };
        });

        el.innerHTML = '';
        spans.forEach(({ outer }) => el.appendChild(outer));
        const targets = spans.map(s => s.inner);
        el._cgOriginalHTML = original;

        const tl = gsap.timeline();

        if (type === 'slide-up') {
            tl.fromTo(targets, { y: '100%', opacity: 0 }, { y: '0%', opacity: 1, duration, stagger, ease });
        } else if (type === 'slide-down') {
            tl.fromTo(targets, { y: '-100%', opacity: 0 }, { y: '0%', opacity: 1, duration, stagger, ease });
        } else if (type === 'fade') {
            tl.fromTo(targets, { opacity: 0 }, { opacity: 1, duration, stagger, ease });
        } else if (type === 'scale') {
            tl.fromTo(targets, { scale: 0, opacity: 0 }, { scale: 1, opacity: 1, duration, stagger, ease });
        }

        return tl;
    }

    // Przywraca oryginalny HTML po split-text animacji wyjścia
    function splitTextRestore(target) {
        const el = typeof target === 'string' ? document.querySelector(target) : target;
        if (el && el._cgOriginalHTML !== undefined) {
            el.innerHTML = el._cgOriginalHTML;
            delete el._cgOriginalHTML;
        }
    }


    // ─── NOWE: mount — jednolite podpięcie animacji szablonu ─────────────────────
    //
    // Szablon zamiast ręcznego __slt_show/__slt_hide woła:
    //   CGMotion.mount(root, {
    //       container: root.querySelector('.plate'),
    //       animIn:  {{{ANIMATION_IN_JSON}}},   animOut: {{{ANIMATION_OUT_JSON}}},
    //       text:    {{{TEXT_ANIM_JSON}}},      textSync: {{{TEXT_ANIM_SYNC}}},
    //       textOut: {{{TEXT_ANIM_OUT_JSON}}},  textOutSync: {{{TEXT_ANIM_OUT_SYNC}}},
    //       loop:    {{{LOOP_JSON}}},
    //       onShow() {}, onHide() {},           // opcjonalnie: logika szablonu
    //   });
    // Wszystko (typ/kierunek/czas/ease IN i OUT, efekt tekstu, pętla uwagi)
    // sterowane jest z inspektora — szablon nie definiuje własnych wartości.

    function mount(root, cfg = {}) {
        const container = cfg.container || root.firstElementChild || root;
        const animIn = cfg.animIn || {};
        const animOut = cfg.animOut || {};

        root.__slt_show = () => {
            const tl = standardIn(container, animIn);
            const textTl = applyTextEffect(root, cfg.text);
            if (textTl) tl.add(textTl, cfg.textSync ? '>-0.1' : 0);

            const lp = cfg.loop;
            if (lp && lp.type && lp.type !== 'none') {
                tl.add(() => loop(container, lp.type, lp));
            }
            let extra = null;
            if (typeof cfg.onShow === 'function') extra = cfg.onShow(tl);
            return extra && typeof extra.add === 'function' ? extra : tl;
        };

        root.__slt_hide = () => {
            stopLoop(container);
            if (typeof cfg.onHide === 'function') cfg.onHide();
            const textTl = applyTextOutEffect(root, cfg.textOut);
            const tl = standardOut(container, animOut);
            if (!textTl) return tl;
            if (cfg.textOutSync) {
                const combined = gsap.timeline();
                combined.add(textTl);
                combined.add(tl, '>-0.1');
                return combined;
            }
            tl.add(textTl, 0);
            return tl;
        };
    }

    // ─── PUBLIC API ──────────────────────────────────────────────────────────────

    const CGMotion = {
        // Core
        normalizeEase,
        TEXT_KNOWN,
        standardIn,
        standardOut,

        // Text
        revealText,
        hideText,
        blurIn,
        blurOut,
        applyTextEffect,
        applyTextOutEffect,

        // Declarative / auto
        autoShow,
        autoHide,

        // Sequence builder
        build,

        // Jednolite podpięcie animacji szablonu (inspektor → szablon)
        mount,
        ANIM_TYPE_NAMES,

        // Attention animators
        loop,
        stopLoop,

        // Spring
        spring,

        // Split text
        splitTextIn,
        splitTextRestore,

        // Helpers (advanced)
        parseAttrSpec,
        collectAnimTargets,
        ATTENTION_PRESETS,
    };

    window.CGMotion = CGMotion;

    // Pełna zgodność wsteczna — GSAPFX wskazuje na CGMotion
    window.GSAPFX = CGMotion;

})(window);
