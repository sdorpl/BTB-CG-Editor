#!/usr/bin/env node
// E2E „podgląd na żywo” (M1 4.3.11): prawdziwy serwer BTB-CG + binarka edytora z --live → element trafia do banku;
// element na wizji nie jest podmieniany (kolejka pendingUpdates).
//   node tools/e2e-live.js [katalog-BTB-CG] [binarka-edytora]
const { spawn } = require('child_process'); const fs = require('fs'), os = require('os'), path = require('path');
const root = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const bin = process.argv[3] || '/tmp/btb-cgeditor-target/release/btb-cgeditor';
const { io } = require(path.join(root, 'node_modules/socket.io-client'));
const PORT = 3500 + Math.floor(Math.random() * 90), url = `http://localhost:${PORT}`;
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'cg-live-')); const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0; const check = (n, ok, x) => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${n}${!ok && x !== undefined ? ' → ' + JSON.stringify(x) : ''}`); if (!ok) failed++; };
const el = (html) => ({ id: 'live1', name: 'Live', type: 'LOWER_THIRD', html_template: html, css_template: '', js_template: '', ocgInputs: [] });
const file = path.join(tmp, 'live.json');
const runEditor = (html) => new Promise((res) => {
  fs.writeFileSync(file, JSON.stringify({ _exportVersion: 3, elements: [el(html)] }));
  const p = spawn(bin, ['--open', file, '--server', `localhost:${PORT}`, '--live', '--shot', path.join(tmp, 's.png'), '--frames', '240'], { stdio: 'ignore' });
  p.on('exit', res); setTimeout(() => p.kill(), 60000);
});
(async () => {
  const server = spawn('node', ['server.js'], { cwd: root, stdio: 'ignore', env: { ...process.env, PORT: String(PORT), DATABASE_URL: path.join(tmp, 'd.sqlite'), APPDATA_PATH: tmp } });
  let sock;
  try {
    for (let i = 0; i < 80; i++) { try { if ((await fetch(url + '/')).ok) break; } catch (_) {} await wait(250); }
    sock = io(url); let state = null, pending = [];
    sock.on('initialState', (s) => { state = s; }); sock.on('stateUpdated', (s) => { state = s; }); sock.on('pendingUpdates', (i) => { pending = i; });
    for (let i = 0; i < 40 && !state; i++) await wait(100);
    await runEditor('<div>v1</div>');
    const g = () => (state.graphics || []).find((x) => x.id === 'live1');
    check('zmiana z edytora (--live) trafia do banku', g() && g().html_template.includes('v1'), g() && g().html_template);
    // element na wizji
    const s2 = JSON.parse(JSON.stringify(state)); s2.graphics.find((x) => x.id === 'live1').visible = true; sock.emit('updateState', s2); await wait(500);
    await runEditor('<div>v2</div>');
    check('element na wizji nie jest podmieniany', g().html_template.includes('v1') && g().visible === true, g().html_template);
    check('nowa wersja czeka w kolejce', pending.includes('live1'), pending);
  } finally { if (sock) sock.close(); server.kill(); fs.rmSync(tmp, { recursive: true, force: true }); }
  console.log(failed ? `\n${failed} niezgodności` : '\nOK'); process.exit(failed ? 1 : 0);
})();
