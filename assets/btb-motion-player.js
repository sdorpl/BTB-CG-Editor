/* btb-motion-player: odtwarza animację elementu BTB-CG silnikiem btb-motion (WASM) bez biblioteki animacji.
 * Konfiguracja: window.BTB_CONFIG = { wasm (base64), director, html, css, data, frame, manual }.
 * API: btb.take() / btb.off() / btb.reset() / btb.update(fields) / btb.tick(dt) (tryb manual).
 * Sterowanie: ?auto=1 (TAKE po załadowaniu), ?obs=1 (źródło aktywne = TAKE, nieaktywne = reset),
 * ?ws=ws://host:port (JSON {cmd:"take"|"off"|"reset"|"update", fields}), postMessage z tym samym JSON. */
(function () {
  'use strict';
  var DEF = { tx: 0, ty: 0, sx: 1, sy: 1, rot: 0, skew: 0, opacity: 1, ox: 0.5, oy: 0.5, cl: 0, ct: 0, cr: 1, cb: 1, blur: 0, fx: 1, fy: 1, sh: 1, dg: -1, dr: 0, rgb: 0 };
  // żaluzja: 8 poziomych pasków, nieprzezroczysta część rośnie ze zmienną --cg-p (0..1), jak w cgmotion
  var SHUTTER = 'repeating-linear-gradient(180deg,#000 0,#000 calc(var(--cg-p) * 12.5%),transparent calc(var(--cg-p) * 12.5%),transparent 12.5%)';
  var ADD = { tx: 1, ty: 1, rot: 1, skew: 1, blur: 1 }, MUL = { sx: 1, sy: 1, opacity: 1, fx: 1, fy: 1, sh: 1 };
  var CLIP_FULL = function (t) { return t.cl === 0 && t.ct === 0 && t.cr === 1 && t.cb === 1; };

  function apply(el, s) {
    var t = Object.assign({}, DEF, s);
    el.style.transformOrigin = t.ox * 100 + '% ' + t.oy * 100 + '%';
    el.style.transform = 'translate(' + t.tx + 'px,' + t.ty + 'px) rotate(' + t.rot + 'deg) skewX(' + t.skew + 'deg) scale(' + t.sx * t.fx + ',' + t.sy * t.fy + ')';
    el.style.opacity = t.opacity;
    // przycięcie: ukośny wielokąt (mask-diag) albo prostokąt
    if (t.dg >= 0) {
      var q = t.dg;
      el.style.clipPath = t.dr > 0.5
        ? 'polygon(' + (100 - 125 * q) + '% 0%,100% 0%,100% 100%,' + (100 - 100 * q) + '% 100%)'
        : 'polygon(0% 0%,' + 100 * q + '% 0%,' + 125 * q + '% 100%,0% 100%)';
    } else {
      el.style.clipPath = CLIP_FULL(t) ? '' : 'inset(' + t.ct * 100 + '% ' + (1 - t.cr) * 100 + '% ' + (1 - t.cb) * 100 + '% ' + t.cl * 100 + '%)';
    }
    var f = [];
    if (t.blur > 0) f.push('blur(' + t.blur + 'px)');
    if (t.rgb !== 0) { var a = Math.abs(t.rgb); f.push('drop-shadow(' + 8 * t.rgb + 'px 0 0 rgba(255,0,60,' + 0.8 * a + ')) drop-shadow(' + -8 * t.rgb + 'px 0 0 rgba(0,220,255,' + 0.8 * a + '))'); }
    el.style.filter = f.join(' ');
    if (t.sh < 1) { el.style.setProperty('--cg-p', t.sh); el.style.webkitMaskImage = SHUTTER; el.style.maskImage = SHUTTER; }
    else { el.style.removeProperty('--cg-p'); el.style.webkitMaskImage = ''; el.style.maskImage = ''; }
  }
  function clear(el) { ['transformOrigin', 'transform', 'opacity', 'clipPath', 'filter', 'maskImage', 'webkitMaskImage'].forEach(function (p) { el.style[p] = ''; }); el.style.removeProperty('--cg-p'); }

  function esc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'); }
  function renderTpl(tpl, data) {
    var val = function (k) { var v = data[k]; return v == null ? '' : v; };
    return tpl.replace(/\{\{\{\s*([\w.-]+)\s*\}\}\}/g, function (m, k) { return val(k); })
              .replace(/\{\{\s*([\w.-]+)\s*\}\}/g, function (m, k) { return esc(val(k)); });
  }


  // ── Efekty tekstowe: dzielenie DOM i nakładanie właściwości jednostek ────────
  var SCRAMBLE = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789#%&@$';
  function splitKind(effect) { return /^words-/.test(effect) ? 'words' : /^chars-/.test(effect) ? 'chars' : /^lines-/.test(effect) ? 'lines' : 'whole'; }
  function collectTracks(d, out) { (d.texts || []).forEach(function (t) { out.push(t); }); (d.children || []).forEach(function (c) { collectTracks(c, out); }); return out; }

  // jak _splitUnits w cgmotion: znaki (Array.from) albo słowa; białe znaki zostają węzłami tekstowymi
  function splitUnits(el, by) {
    var text = el.textContent;
    var parts = by === 'chars' ? Array.from(text) : text.split(/(\s+)/);
    el.textContent = '';
    var units = [];
    parts.forEach(function (p) {
      if (/^\s+$/.test(p)) { el.appendChild(document.createTextNode(p)); return; }
      if (p === '') return;
      var sp = document.createElement('span');
      sp.style.display = 'inline-block'; sp.style.whiteSpace = 'pre'; sp.textContent = p;
      el.appendChild(sp); units.push(sp);
    });
    return units;
  }
  function countLines(units) {
    var n = 0, top = null;
    units.forEach(function (u) { var y = u.offsetTop; if (top === null || Math.abs(y - top) > 2) { n++; top = y; } });
    return Math.max(1, n);
  }

  function applyUnit(el, s) {
    var id = s.x === 0 && s.y === 0 && s.s === 1;
    el.style.transform = id ? '' : 'translate(' + s.x + 'px,' + s.y + 'px) scale(' + s.s + ')';
    el.style.opacity = s.o === 1 ? '' : s.o;
    el.style.filter = s.b > 0 ? 'blur(' + s.b + 'px)' : '';
    var c = s.cl;
    el.style.clipPath = (c[0] === 0 && c[1] === 0 && c[2] === 1 && c[3] === 1) ? '' : 'inset(' + c[1] * 100 + '% ' + (1 - c[2]) * 100 + '% ' + (1 - c[3]) * 100 + '% ' + c[0] * 100 + '%)';
  }
  function clearUnit(el) { el.style.transform = ''; el.style.opacity = ''; el.style.filter = ''; el.style.clipPath = ''; }

  // pierwsza liczba w tekście liczona od 0 do wartości, z zachowaniem formatu (jak _counterIn)
  function parseCounter(full) {
    var m = /-?\d[\d\s.,]*\d|-?\d/.exec(full); if (!m) return null;
    var raw = m[0], sepMatch = /[.,](\d+)$/.exec(raw), isDec = !!sepMatch && sepMatch[1].length !== 3;
    var decimals = isDec ? sepMatch[1].length : 0, decSep = isDec ? raw[raw.length - decimals - 1] : '.';
    var gm = isDec ? /\d([\s.,])\d{3}(?!\d)/.exec(raw.slice(0, raw.length - decimals - 1)) : /\d([\s.,])\d{3}(?!\d)/.exec(raw);
    var grp = gm ? gm[1] : '';
    var num = (isDec ? raw.slice(0, raw.length - decimals - 1) : raw).replace(/[\s.,]/g, '') + (isDec ? '.' + sepMatch[1] : '');
    var target = parseFloat(num); if (!isFinite(target)) return null;
    var fmt = function (v) { var a = v.toFixed(decimals).split('.'), i = a[0]; if (grp) i = i.replace(/\B(?=(\d{3})+(?!\d))/g, grp); return a[1] ? i + decSep + a[1] : i; };
    return { m: m, raw: raw, target: target, fmt: fmt };
  }

  function Player(cfg, w) {
    this.cfg = cfg; this.w = w; this.dec = new TextDecoder();
    this.root = document.getElementById('btb-root');
    this.fx = this.root.querySelector('.btb-fx');
    this.data = Object.assign({}, cfg.data);
    this.applied = {}; this.layers = {};
    this.holding = false; this.pendingOff = false; this.visible = false; this.running = false;
    this.listeners = {}; this.wall = 0;
    this.tracks = collectTracks(cfg.director, []); this.tx = {}; this.txApplied = {};
    var json = new TextEncoder().encode(JSON.stringify(cfg.director));
    var ptr = w.alloc(json.length);
    new Uint8Array(w.memory.buffer, ptr, json.length).set(json);
    if (w.load(ptr, json.length) !== 0) throw new Error('btb-motion: niepoprawny Director');
    w.dealloc(ptr, json.length);
    this.render();
  }
  var P = Player.prototype;

  P.out = function (ptr, len) { return JSON.parse(this.dec.decode(new Uint8Array(this.w.memory.buffer, ptr === undefined ? this.w.out_ptr() : ptr, len === undefined ? this.w.out_len() : len))); };
  P.on = function (ev, fn) { (this.listeners[ev] = this.listeners[ev] || []).push(fn); };
  P.emit = function (ev, d) { (this.listeners[ev] || []).forEach(function (f) { f(d); }); };

  P.render = function () {
    this.fx.innerHTML = renderTpl(this.cfg.html, this.data);
    this.layers = {};
    var self = this;
    this.fx.querySelectorAll('[data-layer]').forEach(function (e) { self.layers[e.getAttribute('data-layer')] = e; });
    this.applied = {};
    this.setupClocks(); this.setupTickers(); this.setupText();
    this.fit(); this.squash();
    if (document.fonts && document.fonts.ready && !this._fontsSquash) { this._fontsSquash = true; document.fonts.ready.then(function () { self.fit(); self.squash(); }); }
  };
  // auto-szerokość (M2.0): ta sama reguła co fitRun w SceneModel.compile (BTB-CG) i compile.rs; cfg.fit = fitSpec albo null
  P.fit = function () {
    var spec = this.cfg.fit; if (!spec) return;
    var fx = this.fx, layer = function (n) { return fx.querySelector('.cg-l' + n); }, dl = {}, grow = null;
    spec.auto.forEach(function (a) {
      var cl = layer(a.n), t = cl && cl.querySelector('.cg-t'); if (!t) return;
      var pw = t.style.getPropertyValue('width'), pp = t.style.getPropertyPriority('width');
      var pf = t.style.getPropertyValue('flex'), pfp = t.style.getPropertyPriority('flex');
      t.style.setProperty('width', 'max-content', 'important'); t.style.setProperty('flex', 'none', 'important');
      var nat = t.offsetWidth;
      if (pw) t.style.setProperty('width', pw, pp); else t.style.removeProperty('width');
      if (pf) t.style.setProperty('flex', pf, pfp); else t.style.removeProperty('flex');
      if (!(nat > 0)) return;
      var W = nat;
      if (a.minW !== null) W = Math.max(W, a.minW);
      if (a.maxW !== null) W = Math.min(W, a.maxW);
      var d = W - a.w, sh = a.align === 'right' ? -d : a.align === 'center' ? -d / 2 : 0;
      dl[a.n] = { d: d, sh: sh };
      cl.style.width = W + 'px'; cl.style.left = (a.x + sh) + 'px';
      grow = grow === null ? d + sh : Math.max(grow, d + sh);
    });
    spec.layers.forEach(function (f) {
      var cl = layer(f.n), k = dl[f.to]; if (!cl || !k) return;
      if (f.mode === 'follow') cl.style.left = (f.x + k.d + k.sh) + 'px';
      else { cl.style.width = (f.w + k.d) + 'px'; cl.style.left = (f.x + k.sh) + 'px'; }
    });
    var sc = fx.querySelector('.cg-scene');
    if (grow !== null && sc) sc.style.width = (spec.frameW + grow) + 'px';
  };
  // ściskanie tekstu jak applyGlobalSquashing w renderer.js: dopasowanie do kontenera albo (BADGE) wg długości
  P.squash = function () {
    var byLen = this.cfg.squash === 'length';
    this.fx.querySelectorAll('.slt-squash').forEach(function (t) {
      var c = t.parentElement; if (!c) return;
      t.style.transform = 'none'; t.style.marginRight = '';
      var len = (t.textContent || '').trim().length, tw = t.scrollWidth;
      if (byLen) {
        if (len <= 25) return;
        var sc = Math.max(0.5, 25 / len);
        t.style.transform = 'scaleX(' + sc + ')'; t.style.marginRight = (-(1 - sc) * tw) + 'px';
      } else if (tw > c.clientWidth && c.clientWidth > 0) {
        t.style.transform = 'scaleX(' + (c.clientWidth / tw) + ')';
      }
    });
  };
  P.update = function (fields) { Object.assign(this.data, fields || {}); this.render(); if (this.visible) this.frame(); };

  P.setupClocks = function () {
    var els = this.fx.querySelectorAll('.cg-clock');
    if (!els.length) return;
    var pad = function (v) { return String(v).padStart(2, '0'); };
    var tick = function () {
      var d = new Date(), hm = pad(d.getHours()) + ':' + pad(d.getMinutes());
      els.forEach(function (e) { e.textContent = e.getAttribute('data-format') === 'HH:MM' ? hm : hm + ':' + pad(d.getSeconds()); });
    };
    tick(); clearInterval(this._clock); this._clock = setInterval(tick, 1000);
  };

  P.setupTickers = function () {
    this.tickers = [];
    var self = this;
    (this.cfg.tickers || []).forEach(function (t) {
      var layer = self.fx.querySelector('.cg-l' + t.n); if (!layer) return;
      var trk = layer.querySelector('.cg-track'), dataEl = layer.querySelector('.cg-data'), items = [];
      try {
        var b64 = (dataEl && dataEl.getAttribute('data-b64') || '').trim();
        if (b64) items = JSON.parse(decodeURIComponent(escape(atob(b64 + '='.repeat((4 - b64.length % 4) % 4)))));
      } catch (e) { console.error('[btb-motion] items', e); }
      if (!Array.isArray(items) || !items.length) items = [{ text: 'BRAK WIADOMOŚCI' }];
      var one = items.map(function (i) {
        var txt = (i && typeof i === 'object') ? (i.text || '') : String(i);
        return '<span style="padding-right:' + t.gap + 'px">' + txt + (t.sep ? ' <span style="padding-left:' + t.gap + 'px">' + t.sep + '</span>' : '') + '</span>';
      }).join('');
      trk.innerHTML = one + one;
      self.tickers.push({ trk: trk, speed: t.speed, half: trk.scrollWidth / 2 });
    });
  };


  P.setupText = function () {
    var self = this; this.tx = {}; this.txApplied = {};
    var ids = {}; this.tracks.forEach(function (t) { ids[t.target] = 1; });
    Object.keys(ids).forEach(function (id) {
      var layer = self.layers[id]; if (!layer) return;
      var el = layer.querySelector('[data-cg-text]'); if (!el) return;
      var plain = el.children.length === 0 && el.textContent.trim() !== '';
      self.tx[id] = { el: el, units: null, plain: plain, orig: el.textContent, counter: plain ? parseCounter(el.textContent) : null, dirty: false, caretAt: null, caret: null };
    });
    // podział wg pierwszego efektu dzielącego wśród śladów IN (kolejne tryby go nie zmieniają, jak w cgmotion)
    Object.keys(this.tx).forEach(function (id) {
      var first = self.tracks.filter(function (t) { return t.target === id && t.mode === 'in' && splitKind(t.spec.effect) !== 'whole'; })[0];
      if (first) self.ensureSplit(id, first.spec.effect);
    });
    this.pushUnits();
  };
  // jak _splitUnits: dzieli tylko element z samym tekstem (sprawdzane na żywo: kareta po typewriter-caret to
  // element potomny); gdy element już podzielono (w dowolnym trybie) albo ma dzieci, efekt animuje całość
  P.ensureSplit = function (id, effect) {
    var x = this.tx[id], kind = splitKind(effect);
    if (!x || kind === 'whole' || x.units) return;
    if (x.el.children.length > 0 || !x.el.textContent.trim()) return;
    x.by = kind === 'chars' ? 'chars' : 'words';
    x.units = splitUnits(x.el, x.by);
  };
  P.pushUnits = function () {
    var self = this, o = {};
    Object.keys(this.tx).forEach(function (id) {
      var x = self.tx[id], usesLines = self.tracks.some(function (t) { return t.target === id && splitKind(t.spec.effect) === 'lines'; });
      o[id] = { n: x.units ? x.units.length : 1, by: x.units ? x.by : '', lines: (x.units && usesLines) ? countLines(x.units) : 1, plain: x.plain, num: !!x.counter };
    });
    var j = new TextEncoder().encode(JSON.stringify(o)), p = this.w.alloc(j.length);
    new Uint8Array(this.w.memory.buffer, p, j.length).set(j);
    this.w.set_units(p, j.length); this.w.dealloc(p, j.length);
  };
  // wyjście: podział tekstu wymagany przez efekty OUT (cgmotion robi to przy hide)
  P.beforeOut = function () {
    var self = this, changed = false;
    this.tracks.forEach(function (t) { if (t.mode === 'out' && self.tx[t.target] && !self.tx[t.target].units) { self.ensureSplit(t.target, t.spec.effect); if (self.tx[t.target].units) changed = true; } });
    if (changed) this.pushUnits();
  };
  P.setText = function (x, str) { x.el.textContent = str; x.dirty = true; x.caret = null; };
  P.content = function (id, effect, p) {
    var x = this.tx[id]; if (!x) return;
    var full = x.orig, chars;
    if (effect === 'scramble') {
      chars = Array.from(full); var done = Math.floor(p * chars.length);
      this.setText(x, p >= 1 ? full : chars.map(function (c, i) { return (i < done || /\s/.test(c)) ? c : SCRAMBLE[Math.floor(Math.random() * SCRAMBLE.length)]; }).join(''));
    } else if (effect === 'counter' && x.counter) {
      var c = x.counter; this.setText(x, p >= 1 ? full : full.slice(0, c.m.index) + c.fmt(c.target * p) + full.slice(c.m.index + c.raw.length));
    } else if (effect === 'typewriter-caret') {
      chars = Array.from(full); this.setText(x, chars.slice(0, Math.round(p * chars.length)).join(''));
      var caret = document.createElement('span'); caret.textContent = '|'; caret.style.cssText = 'display:inline-block;margin-left:1px;font-weight:300;';
      x.el.appendChild(caret); x.caret = caret;
      if (p >= 1 && x.caretAt === null) x.caretAt = this.wall;
    }
  };
  // po zakończeniu efektu treści: pełny tekst (kareta miga 4x po 0,4 s po 0,5 s, jak yoyo z repeat:3 w cgmotion)
  P.finalize = function (id) {
    var x = this.tx[id]; if (!x || !x.dirty) return;
    x.el.textContent = x.orig; x.dirty = false;
    if (x.caretAt !== null) {
      var caret = document.createElement('span'); caret.textContent = '|'; caret.style.cssText = 'display:inline-block;margin-left:1px;font-weight:300;';
      x.el.appendChild(caret); x.caret = caret;
    }
  };
  P.blinkCarets = function () {
    for (var id in this.tx) {
      var x = this.tx[id]; if (!x.caret || x.caretAt === null) continue;
      var u = this.wall - x.caretAt - 0.5; if (u < 0) continue;
      var cyc = u / 0.4;
      if (cyc >= 4) { x.caret.remove(); x.caret = null; x.caretAt = null; continue; }
      var fr = cyc - Math.floor(cyc); x.caret.style.opacity = (Math.floor(cyc) % 2 === 0) ? 1 - fr : fr;
    }
  };
  P.frameText = function () {
    var raw = this.out(this.w.tout_ptr(), this.w.tout_len()), by = {}, seen = {}, self = this;
    raw.forEach(function (s) { (by[s.target] = by[s.target] || []).push(s); });
    Object.keys(by).forEach(function (id) {
      var x = self.tx[id]; if (!x) return; seen[id] = 1;
      by[id].forEach(function (s) {
        if (s.split) { var u = x.units && x.units[s.unit]; if (u) applyUnit(u, s); else applyUnit(x.el, s); }
        else {
          var tr = self.tracks.filter(function (t) { return t.target === id && t.mode === s.mode; })[0];
          var eff = tr ? tr.spec.effect : '';
          if (eff === 'scramble' || eff === 'counter' || eff === 'typewriter-caret') {
            if (x.plain && (eff !== 'counter' || x.counter)) { self.content(id, eff, s.p); if (s.p >= 1) { /* ukończony */ } return; }
          }
          applyUnit(x.el, s);
        }
      });
    });
    Object.keys(this.txApplied).forEach(function (id) {
      if (seen[id]) return; var x = self.tx[id]; if (!x) return;
      clearUnit(x.el); (x.units || []).forEach(clearUnit); self.finalize(id);
    });
    this.txApplied = seen; this.blinkCarets();
  };

  P.frame = function () {
    var w = this.w; w.sample();
    var samples = this.out(), by = {}, seen = {};
    // wiele źródeł tej samej właściwości (preset i klucze użytkownika): przesunięcia sumują się, skale i krycie mnożą
    samples.forEach(function (s) {
      var o = by[s.target] = by[s.target] || {};
      o[s.prop] = !(s.prop in o) ? s.value : ADD[s.prop] ? o[s.prop] + s.value : MUL[s.prop] ? o[s.prop] * s.value : s.value;
    });
    for (var id in by) { var el = id === 'element' ? this.fx : this.layers[id]; if (el) { apply(el, by[id]); seen[id] = 1; } }
    for (var old in this.applied) if (!seen[old]) { var e2 = old === 'element' ? this.fx : this.layers[old]; if (e2) clear(e2); }
    this.applied = seen;
    this.frameText();
  };

  P.tick = function (dt) {
    if (!this.visible) return;
    var w = this.w;
    w.advance(dt);
    this.wall += dt;
    var ev = this.out(), self = this;
    ev.forEach(function (e) {
      if (e.kind === 'stopped') { self.holding = true; self.emit('hold', e); if (self.pendingOff) { self.pendingOff = false; self.holding = false; self.beforeOut(); w.resume(); } }
      else if (e.kind === 'tag') self.emit('tag', e);
      else if (e.kind === 'end') { self.emit('end', e); self.hide(); }
    });
    if (this.visible) this.frame();
    var T = this.tickers || [];
    for (var i = 0; i < T.length; i++) if (T[i].half > 0) T[i].trk.style.transform = 'translateX(' + (-((this.wall * T[i].speed) % T[i].half)) + 'px)';
  };

  P.take = function () {
    this.render();
    this.w.start(); this.holding = false; this.pendingOff = false; this.wall = 0;
    this.visible = true; this.root.style.visibility = 'visible';
    this.frame(); this.emit('take', {});
    this.startLoop();
  };
  P.off = function () {
    if (!this.visible) return;
    if (this.holding) { this.holding = false; this.beforeOut(); this.w.resume(); } else if (this.w.playing()) this.pendingOff = true;
    this.emit('off', {});
  };
  P.hide = function () { this.visible = false; this.holding = false; this.root.style.visibility = 'hidden'; this.stopLoop(); };
  P.reset = function () { this.w.stop(); this.hide(); };
  P.cycle = function (holdMs) { var s = this; this.take(); setTimeout(function () { s.off(); }, holdMs || 2500); };

  P.startLoop = function () {
    if (this.cfg.manual || this.running) return;
    this.running = true;
    var self = this, last = performance.now();
    (function loop(now) {
      if (!self.running) return;
      self.tick(Math.min(0.1, (now - last) / 1000)); last = now;
      requestAnimationFrame(loop);
    })(last);
  };
  P.stopLoop = function () { this.running = false; };

  function command(p, msg) {
    if (!msg || !msg.cmd) return;
    if (msg.fields) p.update(msg.fields);
    if (msg.cmd === 'take') p.take(); else if (msg.cmd === 'off') p.off(); else if (msg.cmd === 'reset') p.reset();
    else if (msg.cmd === 'update') p.update(msg.fields);
  }

  async function boot() {
    var cfg = window.BTB_CONFIG;
    var bin = Uint8Array.from(atob(cfg.wasm), function (c) { return c.charCodeAt(0); });
    var inst = await WebAssembly.instantiate(bin, {});
    var p = new Player(cfg, inst.instance.exports);
    window.btb = p;
    var q = new URLSearchParams(location.search);
    window.addEventListener('message', function (e) { command(p, e.data); });
    if (q.get('ws')) { try { var ws = new WebSocket(q.get('ws')); ws.onmessage = function (e) { try { command(p, JSON.parse(e.data)); } catch (_) {} }; } catch (_) {} }
    if (q.get('obs') === '1' || window.obsstudio) {
      window.addEventListener('obsSourceActiveChanged', function (e) { if (e.detail && e.detail.active) p.take(); else p.reset(); });
    }
    if (q.get('auto') === '1') p.take();
    window.dispatchEvent(new Event('btb-ready'));
  }
  boot().catch(function (e) { console.error('[btb-motion]', e); window.btbError = String(e); });
})();
