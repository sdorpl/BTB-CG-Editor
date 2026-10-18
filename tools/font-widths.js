#!/usr/bin/env node
// Szerokości tekstu z przeglądarki (Chromium) dla każdego kroju pakietu fontów — wzorzec dla testu parytetu egui ↔ przeglądarka (M3.0/M3.6).
//   node tools/font-widths.js [katalog-BTB-CG]   → zapisuje tests/fonts/browser-widths.json
// Strona ładuje te same pliki TTF, które egui czyta z assets/fonts; pomiar: span nowrap, 40 px, letter-spacing 0, interlinia dowolna.
const fs = require('fs'), path = require('path'), os = require('os');
const { createRequire } = require('module');
const repo = path.join(__dirname, '..');
const btbCg = path.resolve(process.argv[2] || process.env.BTB_CG_DIR || path.join(os.homedir(), 'Nextcloud/GitHub/BTB-CG-System-alpha'));
const { chromium } = createRequire(path.join(btbCg, 'package.json'))('playwright');
const SAMPLES = ['Zażółć gęślą jaźń ĄĆĘŁŃÓŚŹŻ', 'WAVE Typography 0123456789', 'Wydanie specjalne: na żywo z Białegostoku'];
const mf = JSON.parse(fs.readFileSync(path.join(repo, 'assets/fonts/manifest.json'), 'utf8'));

(async () => {
  const faces = [];
  let css = '';
  for (const f of mf.families.filter((x) => x.source === 'pack')) {
    for (const face of f.faces) {
      const files = face.ttf || {};
      for (const [sub, rel] of Object.entries(files)) {
        const range = sub === 'latin' ? 'U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,U+20AC,U+2122,U+2212'
          : sub === 'latin-ext' ? 'U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+1E00-1E9F,U+2020,U+20A0-20AB,U+2113' : 'U+0-10FFFF';
        css += `@font-face{font-family:"${f.id}-${face.weight}-${face.style}";font-style:${face.style};font-weight:${face.weight};src:url("file://${path.join(repo, 'assets/fonts', rel)}");unicode-range:${range};}\n`;
      }
      faces.push({ id: f.id, family: `${f.id}-${face.weight}-${face.style}`, weight: face.weight, italic: face.style === 'italic' });
    }
  }
  const html = `<!doctype html><meta charset=utf-8><style>${css} span{position:absolute;white-space:nowrap;font-size:40px;letter-spacing:0;font-kerning:normal}</style><body></body>`;
  const tmp = path.join(os.tmpdir(), 'btb-font-widths.html');
  fs.writeFileSync(tmp, html);
  const browser = await chromium.launch({ args: ['--allow-file-access-from-files'] });
  const page = await browser.newPage();
  await page.goto('file://' + tmp);
  const out = await page.evaluate(async ([faces, samples]) => {
    const res = [];
    for (const f of faces) {
      const spec = `${f.italic ? 'italic ' : ''}${f.weight} 40px "${f.family}"`;
      await document.fonts.load(spec, samples.join(' '));
      const widths = samples.map((t) => {
        const s = document.createElement('span');
        s.style.fontFamily = `"${f.family}"`; s.style.fontWeight = f.weight; s.style.fontStyle = f.italic ? 'italic' : 'normal';
        s.textContent = t; document.body.appendChild(s);
        const w = s.getBoundingClientRect().width; s.remove(); return Math.round(w * 100) / 100;
      });
      res.push({ id: f.id, weight: f.weight, italic: f.italic, widths });
    }
    return res;
  }, [faces, SAMPLES]);
  await browser.close();
  fs.mkdirSync(path.join(repo, 'tests/fonts'), { recursive: true });
  fs.writeFileSync(path.join(repo, 'tests/fonts/browser-widths.json'), JSON.stringify({ size: 40, samples: SAMPLES, faces: out }, null, 1) + '\n');
  console.log(`Zapisano ${out.length} krojów × ${SAMPLES.length} próbek → tests/fonts/browser-widths.json`);
})().catch((e) => { console.error(e); process.exit(1); });
