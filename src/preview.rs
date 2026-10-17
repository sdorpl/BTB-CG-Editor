//! "HTML preview": runs the element through BTB-CG's own renderer (renderer.js + cgmotion.js) in the
//! system browser. A fake socket feeds it the same `stateUpdated` messages the server would send,
//! so IN/OUT animations, Handlebars, CSS scoping etc. behave exactly as on air.
use crate::model::Element;
use base64::Engine;
use std::path::PathBuf;

const FONT: &[u8] = include_bytes!("../assets/BAHNSCHRIFT.TTF");

fn candidate_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(d) = std::env::var("BTB_CG_DIR") { v.push(d.into()); }
    if let Ok(cwd) = std::env::current_dir() {
        v.push(cwd.join("../BTB-CG-System-alpha"));
        v.push(cwd.join("../BTB-CG-System-main"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        v.push(h.join("Nextcloud/GitHub/BTB-CG-System-alpha"));
        v.push(h.join("Documents/GitHub/BTB-CG-System_main"));
    }
    v
}

/// (renderer.js, cgmotion.js) from a local checkout or, failing that, from the running server.
fn load_runtime(base_url: &str) -> Result<(String, String), String> {
    for d in candidate_dirs() {
        if let (Ok(r), Ok(c)) = (std::fs::read_to_string(d.join("renderer.js")), std::fs::read_to_string(d.join("cgmotion.js"))) {
            return Ok((r, c));
        }
    }
    if !base_url.is_empty() {
        let get = |p: &str| ureq::get(&format!("{base_url}/{p}")).timeout(std::time::Duration::from_secs(6)).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string());
        return Ok((get("renderer.js")?, get("cgmotion.js")?));
    }
    Err(crate::i18n::tr("preview.no_renderer").into())
}

fn script_safe(s: &str) -> String { s.replace("</script", "<\\/script") }

pub fn build_html(el: &Element, base_url: &str) -> Result<String, String> {
    let (renderer, cgmotion) = load_runtime(base_url)?;
    let mut shown = el.clone();
    shown.extra.insert("visible".into(), serde_json::Value::Bool(false));
    let json = serde_json::to_string(&shown).map_err(|e| e.to_string())?;
    let font_b64 = base64::engine::general_purpose::STANDARD.encode(FONT);
    Ok(TEMPLATE
        .replace("@@TITLE@@", &el.name.replace('<', "&lt;"))
        .replace("@@FONT@@", &font_b64)
        .replace("@@FIXES@@", &el.scene.as_ref().map(crate::compile::text_centering_css).unwrap_or_default())
        .replace("@@ELEMENT@@", &script_safe(&json))
        .replace("@@CGMOTION@@", &script_safe(&cgmotion))
        .replace("@@RENDERER@@", &script_safe(&renderer)))
}

pub fn open(el: &Element, base_url: &str) -> Result<PathBuf, String> {
    let html = build_html(el, base_url)?;
    let dir = std::env::temp_dir().join("btb-cgeditor-preview");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.html", el.id.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_")));
    std::fs::write(&path, html).map_err(|e| e.to_string())?;
    std::process::Command::new("xdg-open").arg(&path).spawn().map_err(|e| format!("xdg-open: {e}"))?;
    Ok(path)
}

const TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="pl"><head><meta charset="UTF-8"><title>Podgląd — @@TITLE@@</title>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800;900&family=Roboto+Condensed:ital,wght@0,300;0,400;0,700;1,400&display=swap" rel="stylesheet">
<style>
@font-face { font-family: 'Bahnschrift'; src: url(data:font/ttf;base64,@@FONT@@) format('truetype'); font-weight: 100 900; }
html, body { margin: 0; height: 100%; background: #111; color: #ddd; font: 13px system-ui, sans-serif; overflow: hidden; }
#bar { height: 44px; display: flex; gap: 8px; align-items: center; padding: 0 12px; background: #1c1c1f; border-bottom: 1px solid #333; }
#bar button { background: #2d2d33; color: #eee; border: 1px solid #444; border-radius: 4px; padding: 6px 12px; cursor: pointer; font-size: 13px; }
#bar button:hover { background: #3a3a42; }
#bar .take { background: #15803d; } #bar .off { background: #b91c1c; }
#bar .grow { flex: 1; } #state { color: #fbbf24; min-width: 90px; }
#wrap { position: absolute; top: 44px; left: 0; right: 0; bottom: 0; overflow: hidden;
  background: repeating-conic-gradient(#3a3a3a 0% 25%, #2e2e2e 0% 50%) 0 0 / 40px 40px; }
#stage { position: absolute; top: 0; left: 0; width: 1920px; height: 1080px; transform-origin: 0 0; }
#render-container { position: absolute; top: 0; left: 0; width: 1920px; height: 1080px; overflow: hidden; pointer-events: none; }
.cg-i > .cg-t { width: 100%; height: 100%; overflow: hidden; }
.cg-t p, .cg-t h1, .cg-t h2, .cg-t h3, .cg-t h4, .cg-t ul, .cg-t ol, .cg-t li, .cg-t blockquote { margin: 0; padding: 0; }
.cg-t p + p { margin-top: 0.4em; }
@@FIXES@@
</style>
<script src="https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.2/gsap.min.js"></script>
<script src="https://cdnjs.cloudflare.com/ajax/libs/handlebars.js/4.7.7/handlebars.min.js"></script>
<script>
// Fake socket.io: renderer.js subscribes to initialState/stateUpdated, we feed it the same messages the server sends.
window.__handlers = {};
window.io = function () { return { on: function (ev, cb) { window.__handlers[ev] = cb; }, emit: function () {} }; };
</script>
<script>@@CGMOTION@@</script>
</head><body>
<div id="bar">
  <strong>@@TITLE@@</strong>
  <button class="take" onclick="take()">▶ TAKE (IN)</button>
  <button class="off" onclick="off()">■ OFF (OUT)</button>
  <button onclick="cycle()">⟳ Cykl</button>
  <span id="state">gotowe</span>
  <span class="grow"></span>
  <button onclick="bg('checker')">szachownica</button><button onclick="bg('#00b140')">chroma</button><button onclick="bg('#111')">ciemne</button><button onclick="bg('#fff')">białe</button>
</div>
<div id="wrap"><div id="stage"><div id="render-container"></div></div></div>
<script>@@RENDERER@@</script>
<script>
const EL = @@ELEMENT@@;
let on = false;
function push(v) {
  on = v;
  const g = Object.assign({}, EL, { visible: v });
  const state = { graphics: [g], settings: {} };
  if (window.__handlers.stateUpdated) window.__handlers.stateUpdated(state);
  else if (window.__handlers.initialState) window.__handlers.initialState(state);
  document.getElementById('state').textContent = v ? 'na wizji' : 'zdjęte';
}
function take() { if (!on) push(true); }
function off() { if (on) push(false); }
function cycle() { take(); setTimeout(off, 2500); }
function bg(c) { const w = document.getElementById('wrap'); w.style.background = c === 'checker' ? '' : c; }
function fit() {
  const w = document.getElementById('wrap');
  const s = Math.min(w.clientWidth / 1920, w.clientHeight / 1080);
  document.getElementById('stage').style.transform = 'scale(' + s + ')';
}
window.addEventListener('resize', fit); fit();
setTimeout(take, 500);
</script></body></html>"##;
