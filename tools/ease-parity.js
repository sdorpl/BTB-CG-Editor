#!/usr/bin/env node
// Porównuje easingi silnika (WASM `ease_at`) z GSAP po normalizacji nazw przez CGMotion.normalizeEase.
//   node tools/ease-parity.js [katalog-BTB-CG]      (po tools/build-wasm.sh)
const fs = require('fs');
const path = require('path');
const os = require('os');
const vm = require('vm');
const { createRequire } = require('module');

const btbCg = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const req = createRequire(path.join(btbCg, 'package.json'));
const { gsap } = req('gsap');
// cgmotion.js jest skryptem przeglądarkowym: udostępniamy mu gsap i minimalne window/document
globalThis.window = globalThis; globalThis.gsap = gsap; globalThis.document = { querySelectorAll: () => [], createElement: () => ({ style: {} }) };
vm.runInThisContext(fs.readFileSync(path.join(btbCg, 'cgmotion.js'), 'utf8'));
const normalize = globalThis.CGMotion.normalizeEase;

const wasmPath = path.join(__dirname, '..', 'assets', 'btb_motion_wasm.wasm');
const NAMES = [
  'power1.out', 'power1.in', 'power1.inOut', 'power2.out', 'power2.in', 'power2.inOut', 'power3.out', 'power3.in', 'power3.inOut',
  'power4.out', 'power4.in', 'back.out', 'back.in', 'back.out(1.2)', 'back.out(1.4)', 'back.out(2)', 'back.in(1.2)', 'back.in(2)',
  'bounce.out', 'bounce.in', 'elastic.out', 'sine.out', 'sine.in', 'sine.inOut', 'quad.out', 'cubic.in', 'linear', 'none',
  'ease-out', 'ease-in', 'ease-in-out', 'spring', 'spring-soft', 'spring-hard', 'overshoot', 'broadcast', 'back-out', 'back-in', 'bounce-out', 'elastic-out',
];
const TOL = Number(process.env.EASE_TOL || 0.02);

(async () => {
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
  const w = instance.exports;
  const enc = new TextEncoder();
  const rust = (name, x) => {
    const b = enc.encode(name), p = w.alloc(b.length);
    new Uint8Array(w.memory.buffer, p, b.length).set(b);
    const v = w.ease_at(p, b.length, x); w.dealloc(p, b.length); return v;
  };
  let bad = 0;
  for (const name of NAMES) {
    let ref;
    try { const n = normalize(name); ref = typeof n === 'function' ? n : gsap.parseEase(n); } catch (e) { console.log(`?    ${name}: ${e.message}`); bad++; continue; }
    let worst = 0, at = 0;
    for (let i = 0; i <= 40; i++) {
      const x = i / 40, d = Math.abs(rust(name, x) - ref(x));
      if (d > worst) worst = d, at = x;
    }
    const ok = worst <= TOL;
    if (!ok) bad++;
    console.log(`${ok ? 'ok  ' : 'DIFF'} ${name.padEnd(16)} maks. różnica ${worst.toFixed(4)} (x=${at.toFixed(3)})  GSAP: ${String(normalize(name)).slice(0, 28)}`);
  }
  console.log(bad ? `\n${bad} easingów poza tolerancją ${TOL}` : '\nWszystkie easingi zgodne z GSAP');
  if (bad && process.env.STRICT) process.exit(1);
})();
