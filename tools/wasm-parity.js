#!/usr/bin/env node
// Uruchamia te same skrypty co natywny przebieg (btb-motion-wasm/examples/dump.rs) przez WASM
// i porównuje wyniki. Użycie: node tools/wasm-parity.js  (po tools/build-wasm.sh)
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const targetDir = JSON.parse(execSync('cargo metadata --format-version 1 --no-deps', { cwd: path.join(__dirname, '..') })).target_directory;
const parityDir = path.join(targetDir, 'parity');
const wasmPath = fs.readFileSync(path.join(parityDir, 'wasm-path.txt'), 'utf8').trim();

const EPS = 1e-4;
let failures = 0;

async function main() {
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
  const w = instance.exports;
  const dec = new TextDecoder();
  const out = () => dec.decode(new Uint8Array(w.memory.buffer, w.out_ptr(), w.out_len()));

  for (const f of fs.readdirSync(parityDir).filter(n => n.endsWith('.fixture.json'))) {
    const name = f.replace('.fixture.json', '');
    const json = fs.readFileSync(path.join(parityDir, f));
    const expected = JSON.parse(fs.readFileSync(path.join(parityDir, `${name}.expected.json`), 'utf8'));
    const ptr = w.alloc(json.length);
    new Uint8Array(w.memory.buffer, ptr, json.length).set(json);
    if (w.load(ptr, json.length) !== 0) { console.error(`${name}: load() nie powiódł się`); failures++; continue; }

    const step = (dt) => { w.advance(dt); const fired = JSON.parse(out()); w.sample(); return { fired, samples: JSON.parse(out()) }; };
    const log = [];
    const snap = (r) => log.push({ t: w.time(), playing: !!w.playing(), ...r });
    w.start();
    for (let i = 0; i < 40; i++) snap(step(1 / 60));
    for (let i = 0; i < 30; i++) snap(step(1 / 60));
    w.resume();
    for (let i = 0; i < 150; i++) snap(step(1 / 60));

    let bad = 0;
    if (log.length !== expected.length) bad++;
    log.forEach((g, i) => {
      const e = expected[i];
      if (!e) return;
      if (Math.abs(g.t - e.t) > EPS || g.playing !== e.playing || JSON.stringify(g.fired) !== JSON.stringify(e.fired)) bad++;
      if (g.samples.length !== e.samples.length) { bad++; return; }
      g.samples.forEach((s, j) => {
        const x = e.samples[j];
        if (s.target !== x.target || s.prop !== x.prop || Math.abs(s.value - x.value) > EPS) bad++;
      });
    });
    console.log(`${bad ? 'FAIL' : 'ok  '} ${name}: ${log.length} kroków, rozbieżności: ${bad}`);
    failures += bad ? 1 : 0;
  }
  if (failures) process.exit(1);
}
main().catch(e => { console.error(e); process.exit(1); });
