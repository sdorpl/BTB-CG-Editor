#!/usr/bin/env node
// Zrzut wyniku prawdziwego kompilatora BTB-CG (SceneModel.applyToElement) dla elementów ze sceną.
//   node js-compile-dump.js <katalog-BTB-CG>   → stdout: [{ file, id, html, css, js, ocgInputs }]
// Używany przez test zgodności Rust ↔ JS (src/compile.rs: compile_matches_btb_cg_scene_model).
const fs = require('fs'), path = require('path');
const root = path.resolve(process.argv[2] || '.');
const SceneModel = require(path.join(root, 'scene-model.js'));
// tryb pojedynczej sceny: node js-compile-dump.js <BTB-CG> --scene plik.json → { html, css, js, ocgInputs }
const si = process.argv.indexOf('--scene');
if (si > 0) {
    const el = { id: 'x', name: 'x', type: 'BADGE', title: '' };
    SceneModel.applyToElement(el, JSON.parse(fs.readFileSync(process.argv[si + 1], 'utf8')));
    process.stdout.write(JSON.stringify({ html: el.html_template, css: el.css_template, js: el.js_template, ocgInputs: el.ocgInputs }));
    process.exit(0);
}
const dir = path.join(root, 'templates');
const out = [];
for (const f of fs.readdirSync(dir).filter((x) => x.endsWith('.json')).sort()) {
    const j = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8'));
    for (const el of (j.elements || [j])) {
        if (!el.scene || el.scene.detached) continue;
        const c = JSON.parse(JSON.stringify(el));
        SceneModel.applyToElement(c, c.scene);
        out.push({ file: f, id: el.id, html: c.html_template, css: c.css_template, js: c.js_template, ocgInputs: c.ocgInputs });
    }
}
process.stdout.write(JSON.stringify(out));
