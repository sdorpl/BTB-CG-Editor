//! Port of SceneModel.compile / applyToElement (scene-model.js): scene → HTML/CSS/JS artefacts
//! that the BTB-CG renderer runs. Output is meant to be semantically identical to the JS compiler.
use crate::model::*;
use serde_json::{json, Value};

fn fmt(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { "0".into() } else { format!("{}", r) }
}

fn css_str(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\'' | '"' | '\\' | ';' | '{' | '}' | '<' | '>')).collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn field_id(s: &str) -> String {
    let mut out: String = s
        .to_uppercase()
        .chars()
        .map(|c| if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' { c } else { '_' })
        .collect();
    if out.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn or<'a>(s: &'a str, d: &'a str) -> &'a str { if s.is_empty() { d } else { s } }

fn fill_css(fill: Option<&Fill>) -> String {
    let Some(f) = fill else { return String::new() };
    match f.fill_type {
        FillType::None => String::new(),
        FillType::Gradient => format!(
            "background: linear-gradient({}deg, {} 0%, {} 100%);",
            fmt(f.angle), css_str(or(&f.color, "#1e3a8a")), css_str(or(&f.color2, "#3b82f6"))
        ),
        FillType::Solid => format!("background: {};", css_str(or(&f.color, "transparent"))),
    }
}

fn radius_css(r: Option<&Radius>) -> String {
    match r {
        Some(Radius::Corners(c)) => format!(
            "border-radius: {};",
            c.iter().map(|v| format!("{}px", fmt(*v))).collect::<Vec<_>>().join(" ")
        ),
        Some(Radius::Uniform(v)) if *v != 0.0 => format!("border-radius: {}px;", fmt(*v)),
        _ => String::new(),
    }
}

fn shadow_css(s: Option<&Shadow>) -> String {
    match s {
        None | Some(Shadow::Flag(false)) => String::new(),
        Some(Shadow::Flag(true)) => "box-shadow: 0 10px 25px -5px rgba(0,0,0,0.6);".into(),
        Some(Shadow::Custom { x, y, blur, color }) => format!(
            "box-shadow: {}px {}px {}px {};", fmt(*x), fmt(*y), fmt(*blur), css_str(or(color, "rgba(0,0,0,0.6)"))
        ),
    }
}

fn font_css(f: &FontStyle) -> String {
    let jc = match f.align.as_str() { "center" => "center", "right" => "flex-end", _ => "flex-start" };
    let ai = match f.valign.as_str() { "top" => "flex-start", "bottom" => "flex-end", _ => "center" };
    let mut parts: Vec<String> = vec![
        "display: flex;".into(),
        format!("justify-content: {};", jc),
        format!("align-items: {};", ai),
        format!("font-family: '{}', sans-serif;", css_str(&f.family)),
        format!("font-size: {}px;", fmt(f.size)),
        format!("font-weight: {};", css_str(&f.weight)),
        format!("color: {};", css_str(&f.color)),
    ];
    if !f.transform.is_empty() && f.transform != "none" {
        parts.push(format!("text-transform: {};", css_str(&f.transform)));
    }
    parts.push(format!("text-align: {};", css_str(&f.align)));
    if f.letter_spacing != 0.0 { parts.push(format!("letter-spacing: {}px;", fmt(f.letter_spacing))); }
    parts.push(format!("line-height: {};", fmt(f.line_height)));
    if f.italic { parts.push("font-style: italic;".into()); }
    if !f.shadow.is_empty() && f.shadow != "none" {
        let sh: String = f.shadow.chars().filter(|c| !matches!(c, ';' | '{' | '}' | '<' | '>')).collect();
        parts.push(format!("text-shadow: {};", sh));
    }
    parts.push("white-space: nowrap;".into());
    parts.join(" ")
}

fn effect_css(l: &Layer, sel: &str) -> String {
    let e = l.effect.as_deref().unwrap_or("none");
    if l.layer_type != LayerType::Shape || e == "none" || e.is_empty() { return String::new(); }
    let kf = format!("cg-fx-{}-{{{{ID}}}}", e);
    let op = l.opacity;
    match e {
        "shimmer" => format!(
            "{sel}.cg-fx-shimmer {{ overflow: hidden; }}\n\
             {sel}.cg-fx-shimmer::after {{ content: ''; position: absolute; top: 0; left: -120%; width: 60%; height: 100%; \
             background: linear-gradient(90deg, rgba(255,255,255,0), rgba(255,255,255,0.28), rgba(255,255,255,0)); \
             transform: skewX(-20deg); animation: {kf} 3s infinite; }}\n\
             @keyframes {kf} {{ 0% {{ left: -120%; }} 60%, 100% {{ left: 160%; }} }}\n"),
        "pulse" => format!(
            "{sel}.cg-fx-pulse {{ animation: {kf} 1.6s infinite ease-in-out; }}\n\
             @keyframes {kf} {{ 0%, 100% {{ opacity: {}; }} 50% {{ opacity: {}; }} }}\n",
            fmt(op), fmt((op * 0.55).max(0.35))),
        "glow" => {
            let c = css_str(l.fill.as_ref().map(|f| or(&f.color, "#ffffff")).unwrap_or("#ffffff"));
            format!(
                "{sel}.cg-fx-glow {{ animation: {kf} 2s infinite ease-in-out; }}\n\
                 @keyframes {kf} {{ 0%, 100% {{ box-shadow: 0 0 6px {c}; }} 50% {{ box-shadow: 0 0 26px {c}; }} }}\n")
        }
        "scanline" => format!(
            "{sel}.cg-fx-scanline {{ overflow: hidden; }}\n\
             {sel}.cg-fx-scanline::after {{ content: ''; position: absolute; inset: 0; \
             background: repeating-linear-gradient(0deg, rgba(0,0,0,0.18) 0 1px, transparent 1px 3px); animation: {kf} 1s linear infinite; }}\n\
             @keyframes {kf} {{ from {{ background-position: 0 0; }} to {{ background-position: 0 3px; }} }}\n"),
        _ => String::new(),
    }
}

fn mask_css(l: &Layer, scene: &Scene) -> String {
    let Some(mid) = &l.mask_layer else { return String::new() };
    let Some(m) = scene.layers.iter().find(|x| &x.id == mid && x.layer_type == LayerType::Shape && x.visible) else {
        return String::new();
    };
    if m.id == l.id { return String::new(); }
    let top = m.y - l.y;
    let left = m.x - l.x;
    let right = (l.x + l.w) - (m.x + m.w);
    let bottom = (l.y + l.h) - (m.y + m.h);
    let sk = m.skew - l.skew;
    if sk.abs() > 0.01 {
        let tn = (sk.to_radians()).tan();
        let cy = top + m.h / 2.0;
        let (x1, x2, y1, y2) = (left, left + m.w, top, top + m.h);
        let dx = |y: f32| (y - cy) * -tn;
        return format!(
            "clip-path: polygon({}px {}px, {}px {}px, {}px {}px, {}px {}px);",
            fmt(x1 + dx(y1)), fmt(y1), fmt(x2 + dx(y1)), fmt(y1), fmt(x2 + dx(y2)), fmt(y2), fmt(x1 + dx(y2)), fmt(y2)
        );
    }
    let r = match &m.radius {
        Some(Radius::Corners(c)) => c.iter().map(|v| format!("{}px", fmt(*v))).collect::<Vec<_>>().join(" "),
        Some(Radius::Uniform(v)) => format!("{}px", fmt(*v)),
        None => "0px".into(),
    };
    format!("clip-path: inset({}px {}px {}px {}px round {});", fmt(top), fmt(right), fmt(bottom), fmt(left), r)
}

pub struct Compiled {
    pub html: String,
    pub css: String,
    pub js: String,
    pub ocg_inputs: Vec<Value>,
    pub title: Option<String>,
    pub fields: serde_json::Map<String, Value>,
}

fn json_opt<T: serde::Serialize>(v: &Option<T>) -> String {
    match v { Some(x) => serde_json::to_string(x).unwrap_or_else(|_| "null".into()), None => "null".into() }
}

pub fn group_motion_pair(scene: &Scene, l: &Layer) -> (Option<MotionSpec>, Option<MotionSpec>) {
    let Some(g) = &l.group else { return (None, None) };
    let Some(m) = scene.extra.get("groups").and_then(|v| v.get(g)).and_then(|v| v.get("motion")) else {
        return (None, None);
    };
    let get = |k: &str| m.get(k).filter(|v| !v.is_null()).and_then(|v| serde_json::from_value::<MotionSpec>(v.clone()).ok());
    (get("in"), get("out"))
}

pub fn compile(scene: &Scene) -> Compiled {
    let resolved;
    let scene = if crate::links::has_links(scene) {
        let (empty, ex) = (serde_json::Map::new(), Extra::new());
        resolved = crate::links::resolve(scene, &crate::render::Values { title: "", fields: &empty, extra: &ex, base_url: "" });
        &resolved
    } else { scene };
    let layers: Vec<&Layer> = scene.layers.iter().filter(|l| l.visible).collect();
    let (fw, fh) = (scene.frame.w, scene.frame.h);

    let mut html = String::from("<div class=\"cg-scene\">\n");
    let mut css = format!(
        "/* SceneModel v1 — wygenerowane ze sceny, nie edytuj ręcznie */\n\
         #{{{{ID}}}} .cg-scene {{ position: relative; width: {}px; height: {}px; pointer-events: none; transform: scale(var(--cg-sx, 1), var(--cg-sy, 1)); transform-origin: top left; }}\n\
         #{{{{ID}}}} .cg-l, #{{{{ID}}}} .cg-i {{ box-sizing: border-box; }}\n\
         #{{{{ID}}}} .cg-l {{ position: absolute; }}\n\
         #{{{{ID}}}} .cg-i {{ position: absolute; inset: 0; }}\n",
        fmt(fw), fmt(fh)
    );

    let mut clocks = 0usize;
    let mut tickers: Vec<String> = vec![];
    let mut motions: Vec<String> = vec![];
    let mut text_ns: Vec<usize> = vec![];
    let mut seen_field: Vec<String> = vec![];
    let mut ocg: Vec<Value> = vec![];
    let mut title: Option<String> = None;
    let mut fields = serde_json::Map::new();

    for (idx, l) in layers.iter().enumerate() {
        let n = idx + 1;
        let cls = format!("cg-l{}", n);
        let sel = format!("#{{{{ID}}}} .cg-i{}", n);
        let mut inner_cls = vec!["cg-i".to_string(), format!("cg-i{}", n)];
        let eff = l.effect.as_deref().unwrap_or("none");
        if l.layer_type == LayerType::Shape && matches!(eff, "shimmer" | "pulse" | "glow" | "scanline") {
            inner_cls.push(format!("cg-fx-{}", eff));
        }

        css += &format!(
            "#{{{{ID}}}} .{} {{ left: {}px; top: {}px; width: {}px; height: {}px; z-index: {}; }}\n",
            cls, fmt(l.x), fmt(l.y), fmt(l.w), fmt(l.h), idx + 1
        );

        let mut vis: Vec<String> = vec![];
        let mut tf: Vec<String> = vec![];
        if l.skew != 0.0 { tf.push(format!("skewX({}deg)", fmt(l.skew))); }
        if l.rotation != 0.0 { tf.push(format!("rotate({}deg)", fmt(l.rotation))); }
        if !tf.is_empty() { vis.push(format!("transform: {};", tf.join(" "))); }
        if l.opacity != 1.0 { vis.push(format!("opacity: {};", fmt(l.opacity))); }

        let inner: String;
        match l.layer_type {
            LayerType::Shape => {
                vis.push(fill_css(l.fill.as_ref()));
                vis.push(radius_css(l.radius.as_ref()));
                vis.push(shadow_css(l.shadow.as_ref()));
                if let Some(s) = &l.stroke {
                    if s.width > 0.0 {
                        vis.push(format!("border: {}px solid {};", fmt(s.width), css_str(or(&s.color, "#ffffff"))));
                    }
                }
                inner = String::new();
            }
            LayerType::Text => {
                vis.push(font_css(&l.font_or_default()));
                let cont = if l.squash == Some(true) { "cg-t slt-squash" } else { "cg-t" };
                let content;
                match l.field.as_deref().filter(|f| !f.is_empty()) {
                    Some(fld) => {
                        let mut id = field_id(fld);
                        if id.is_empty() { id = "TITLE".into(); }
                        content = format!("{{{{{{{}}}}}}}", id);
                        if !seen_field.contains(&id) {
                            seen_field.push(id.clone());
                            let def = l.default_text.clone().unwrap_or_default();
                            ocg.push(json!({
                                "id": id,
                                "label": if l.name.is_empty() { id.clone() } else { l.name.clone() },
                                "default": def,
                                "type": "text"
                            }));
                            if id == "TITLE" { title = Some(def); } else { fields.insert(id, Value::String(def)); }
                        }
                    }
                    None => content = esc(l.text.as_deref().unwrap_or("")),
                }
                inner = format!("<div class=\"{}\" data-cg-text=\"1\">{}</div>", cont, content);
            }
            LayerType::Image => {
                vis.push(radius_css(l.radius.as_ref()));
                vis.push("overflow: hidden;".into());
                vis.push(mask_css(l, scene));
                let src = match l.bind.as_deref().filter(|b| !b.is_empty()) {
                    Some(b) => format!("{{{{{}}}}}", field_id(b)),
                    None => esc(l.src.as_deref().unwrap_or("")),
                };
                inner = format!(
                    "<img src=\"{}\" alt=\"\" style=\"width:100%;height:100%;object-fit:{};display:block;\" />",
                    src, css_str(l.fit.as_deref().filter(|f| !f.is_empty()).unwrap_or("contain"))
                );
            }
            LayerType::Video => {
                vis.push(radius_css(l.radius.as_ref()));
                vis.push("overflow: hidden;".into());
                vis.push(mask_css(l, scene));
                inner = format!(
                    "<video src=\"{}\" autoplay playsinline {}{}style=\"width:100%;height:100%;object-fit:{};display:block;\"></video>",
                    esc(l.src.as_deref().unwrap_or("")),
                    if l.video_loop != Some(false) { "loop " } else { "" },
                    if l.muted != Some(false) { "muted " } else { "" },
                    css_str(l.fit.as_deref().filter(|f| !f.is_empty()).unwrap_or("cover"))
                );
            }
            LayerType::Clock => {
                vis.push(font_css(&l.font_or_default()));
                inner = format!(
                    "<span class=\"cg-clock\" data-cg-text=\"1\" data-format=\"{}\">00:00:00</span>",
                    esc(l.format.as_deref().filter(|f| !f.is_empty()).unwrap_or("HH:MM:SS"))
                );
                clocks += 1;
            }
            LayerType::Ticker => {
                let mut f = l.font_or_default();
                f.align = "left".into();
                vis.push(font_css(&f));
                vis.push("overflow: hidden;".into());
                inner = "<div class=\"cg-track\" style=\"display:inline-flex;white-space:nowrap;will-change:transform;\"></div>\
                         <span class=\"cg-data\" data-b64=\"{{ITEMS_B64}}\" style=\"display:none;\"></span>".into();
                tickers.push(format!(
                    "{{\"n\":{},\"speed\":{},\"sep\":{},\"gap\":{}}}",
                    n, fmt(l.speed.unwrap_or(100.0)),
                    serde_json::to_string(l.separator.as_deref().unwrap_or("•")).unwrap(),
                    fmt(l.gap.unwrap_or(48.0))
                ));
            }
        }

        let vis_joined = vis.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        css += &format!("{} {{ {} }}\n", sel, vis_joined);
        css += &effect_css(l, &sel);

        html += &format!(
            "    <div class=\"cg-l {}\" data-layer=\"{}\"><div class=\"{}\">{}</div></div>\n",
            cls, esc(&l.id), inner_cls.join(" "), inner
        );

        let mo = &l.motion;
        let (gin, gout) = group_motion_pair(scene, l);
        let m_in = mo.motion_in.clone().or(gin);
        let m_out = mo.out.clone().or(gout);
        if m_in.is_some() || m_out.is_some() || mo.text.is_some() || mo.text_out.is_some() {
            motions.push(format!(
                "{{\"n\":{},\"in\":{},\"out\":{},\"text\":{},\"textOut\":{}}}",
                n, json_opt(&m_in), json_opt(&m_out), json_opt(&mo.text), json_opt(&mo.text_out)
            ));
        }
        if matches!(l.layer_type, LayerType::Text | LayerType::Clock) { text_ns.push(n); }
    }
    html += "</div>";

    // ── JS ──
    let mut js = String::from(
        "(() => {\n    const root = document.getElementById(\"{{ID}}\");\n    if (!root) return;\n    \
         const scene = root.querySelector('.cg-scene') || root;\n    \
         const layer = n => root.querySelector('.cg-l' + n);\n",
    );
    js += &format!("    const motions = [{}];\n", motions.join(","));

    if clocks > 0 {
        js += "    const pad = v => String(v).padStart(2, '0');\n    const tick = () => {\n        \
               if (!root.isConnected) { clearInterval(root.__cgClock); return; }\n        \
               const d = new Date(), hm = pad(d.getHours()) + ':' + pad(d.getMinutes());\n        \
               root.querySelectorAll('.cg-clock').forEach(el => {\n            \
               el.textContent = el.getAttribute('data-format') === 'HH:MM' ? hm : hm + ':' + pad(d.getSeconds());\n        });\n    };\n    \
               tick(); root.__cgClock = setInterval(tick, 1000);\n";
    }
    let has_tickers = !tickers.is_empty();
    if has_tickers {
        js += &format!("    const tickers = [{}];\n", tickers.join(","));
        js += "    let items = [];\n    const dataEl = root.querySelector('.cg-data');\n    try {\n        \
               const b64 = (dataEl && dataEl.getAttribute('data-b64') || '').trim();\n        \
               if (b64) items = JSON.parse(decodeURIComponent(escape(atob(b64 + '='.repeat((4 - b64.length % 4) % 4)))));\n    \
               } catch (e) { console.error('[Scene] items b64', e); }\n    \
               if (!Array.isArray(items) || !items.length) items = [{ text: 'BRAK WIADOMOŚCI' }];\n    \
               const txt = i => (i && typeof i === 'object') ? (i.text || '') : String(i);\n    \
               let tickerTweens = [];\n    const startTickers = () => {\n        tickers.forEach(t => {\n            \
               const trk = layer(t.n).querySelector('.cg-track');\n            \
               const one = items.map(i => '<span style=\"padding-right:' + t.gap + 'px\">' + txt(i) + (t.sep ? ' <span style=\"padding-left:' + t.gap + 'px\">' + t.sep + '</span>' : '') + '</span>').join('');\n            \
               trk.innerHTML = one + one;\n            const half = trk.scrollWidth / 2;\n            \
               if (half > 0) tickerTweens.push(gsap.fromTo(trk, { x: 0 }, { x: -half, duration: half / t.speed, ease: 'none', repeat: -1 }));\n        });\n    };\n    \
               const stopTickers = () => { tickerTweens.forEach(t => t.kill()); tickerTweens = []; };\n";
    }
    js += &format!("    const textNs = {};\n", serde_json::to_string(&text_ns).unwrap());
    js += "    const motionOf = n => motions.find(m => m.n === n) || {};\n    \
           const on = c => c && c.type && c.type !== 'none';\n    CGMotion.mount(root, {\n        container: scene,\n        \
           animIn: {{{ANIMATION_IN_JSON}}}, animOut: {{{ANIMATION_OUT_JSON}}},\n        \
           text: { type: 'none' }, textOut: { type: 'none' },\n        loop: {{{LOOP_JSON}}},\n        onShow(tl) {\n            \
           const T = {{{TEXT_ANIM_JSON}}}, TS = {{{TEXT_ANIM_SYNC}}};\n            const baseEnd = tl.duration();\n";
    if has_tickers { js += "            stopTickers(); startTickers();\n"; }
    js += "            motions.forEach(m => {\n                const el = layer(m.n);\n                \
           if (el && on(m.in)) tl.add(CGMotion.standardIn(el, m.in), 0);\n            });\n            \
           textNs.forEach(n => {\n                const el = layer(n), own = motionOf(n).text;\n                \
           const eff = own || T;\n                if (!el || !on(eff)) return;\n                \
           const sub = CGMotion.applyTextEffect(el, eff);\n                \
           if (sub) tl.add(sub, own ? 0 : (TS ? Math.max(0, baseEnd - 0.1) : 0));\n            });\n        },\n        onHide() {\n";
    if has_tickers { js += "            stopTickers();\n"; }
    js += "        }\n    });\n    \
           // Wyjście: najpierw OUT warstw (i ich efekty tekstu), potem OUT całego elementu z inspektora\n    \
           const baseHide = root.__slt_hide;\n    root.__slt_hide = () => {\n        \
           const TO = {{{TEXT_ANIM_OUT_JSON}}}, TOS = {{{TEXT_ANIM_OUT_SYNC}}};\n        \
           const pre = gsap.timeline(), par = gsap.timeline();\n        motions.forEach(m => {\n            \
           const el = layer(m.n);\n            if (el && on(m.out)) pre.add(CGMotion.standardOut(el, m.out), 0);\n        });\n        \
           textNs.forEach(n => {\n            const el = layer(n), own = motionOf(n).textOut;\n            \
           const eff = own || TO;\n            if (!el || !on(eff)) return;\n            \
           const sub = CGMotion.applyTextOutEffect(el, eff);\n            if (!sub) return;\n            \
           if (own) pre.add(sub, 0); else par.add(sub, 0);\n        });\n        const base = baseHide();\n        \
           const tl = gsap.timeline();\n        tl.add(pre);\n        \
           if (TOS) { tl.add(par); tl.add(base, '>-0.1'); }\n        else { tl.add(base); tl.add(par, '<'); }\n        return tl;\n    };\n})();\n";

    Compiled { html, css, js, ocg_inputs: ocg, title, fields }
}

fn dedupe_ids(scene: &mut Scene) {
    let mut seen: Vec<String> = vec![];
    for l in scene.layers.iter_mut() {
        if seen.contains(&l.id) { l.id = format!("{}_{}", l.id, seen.len()); }
        seen.push(l.id.clone());
    }
}

/// Compiles `scene` into `el` (code, ocgInputs, default values, layout size). Values already
/// present on the element (operator edits) win over scene defaults, like applyToElement.
pub fn apply_to_element(el: &mut Element, mut scene: Scene) {
    dedupe_ids(&mut scene);
    scene.detached = false;
    scene.version = 1;
    let c = compile(&scene);
    el.html_template = c.html;
    el.css_template = c.css;
    el.js_template = c.js;
    el.ocg_inputs = c.ocg_inputs;
    for (k, v) in c.fields {
        el.fields.entry(k).or_insert(v);
    }
    if let Some(t) = c.title {
        if el.title.is_empty() { el.title = t; }
    }
    el.layout.width = Some(scene.frame.w);
    el.layout.height = Some(scene.frame.h);
    el.scene = Some(scene);
}

/// Re-runs the compiler on the element's current scene (after any scene edit).
/// Poprawka pionowego wyśrodkowania tekstu dla podglądu HTML (poza kontraktem z `scene-model.js`, więc nie trafia do
/// `css_template`). `.cg-t` jest blokiem o wysokości warstwy (a `.slt-squash` ma `display: inline-block !important`),
/// więc wiersz wyższy od pudełka (line-height × rozmiar) uciekał w dół. Tekst jest jednowierszowy (nowrap): wiersz
/// o wysokości pudełka daje dokładne wyśrodkowanie, a `render.rs` liczy tak samo. Bahnschrift (metryki typo: ascent
/// 0,794 em, descent 0,206 em) ląduje wtedy kapitalikami ok. 0,073 em nad środkiem; `top` w em to koryguje
/// (`relative` nie koliduje z `transform` ściskania). Selektory bez `#id` i bez klasy `.cg-t` (patrz skoper w renderer.js).
pub fn text_centering_css(scene: &Scene) -> String {
    let mut css = String::new();
    for (idx, l) in scene.layers.iter().filter(|l| l.visible).enumerate() {
        let f = l.font_or_default();
        if l.layer_type != LayerType::Text || matches!(f.valign.as_str(), "top" | "bottom") { continue; }
        let fix = if f.family.eq_ignore_ascii_case("bahnschrift") { " position: relative; top: 0.073em;" } else { "" };
        css += &format!(".cg-i{} > [data-cg-text] {{ line-height: {}px;{fix} }}\n", idx + 1, fmt(l.h));
    }
    css
}

pub fn recompile(el: &mut Element) {
    if let Some(scene) = el.scene.take() {
        apply_to_element(el, scene);
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;

    /// Kod wygenerowany przez kompilator w Rust musi być taki sam jak z `SceneModel.applyToElement`
    /// w BTB-CG (ten sam element ze `templates/` → te same html/css/js/ocgInputs).
    #[test]
    fn compile_matches_btb_cg_scene_model() {
        let root = std::path::Path::new("/home/maciej/Nextcloud/GitHub/BTB-CG-System-alpha");
        if !root.join("scene-model.js").exists() { return; }
        let Ok(out) = std::process::Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tools/js-compile-dump.js")).arg(root).output() else { return };
        assert!(out.status.success(), "node: {}", String::from_utf8_lossy(&out.stderr));
        let js: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
        assert!(js.len() >= 5, "expected scene templates, got {}", js.len());
        let mut diffs = vec![];
        for item in &js {
            let file = item["file"].as_str().unwrap();
            let text = std::fs::read_to_string(root.join("templates").join(file)).unwrap();
            let loaded = crate::importer::parse_elements(&text).unwrap();
            let mut el = loaded.into_iter().map(|l| l.element).find(|e| Some(e.id.as_str()) == item["id"].as_str()).unwrap();
            recompile(&mut el);
            for (name, mine, theirs) in [
                ("html", el.html_template.as_str(), item["html"].as_str().unwrap_or("")),
                ("css", el.css_template.as_str(), item["css"].as_str().unwrap_or("")),
                ("js", el.js_template.as_str(), item["js"].as_str().unwrap_or("")),
            ] {
                if mine != theirs {
                    let at = mine.chars().zip(theirs.chars()).position(|(a, b)| a != b).unwrap_or(mine.len().min(theirs.len()));
                    let ctx = |s: &str| s.chars().skip(at.saturating_sub(40)).take(100).collect::<String>();
                    diffs.push(format!("{file} {name}: len {} vs {} @ {at}\n  rust: {}\n  js:   {}", mine.len(), theirs.len(), ctx(mine), ctx(theirs)));
                }
            }
            let a = serde_json::to_value(&el.ocg_inputs).unwrap();
            if a != item["ocgInputs"] { diffs.push(format!("{file} ocgInputs differ")); }
        }
        assert!(diffs.is_empty(), "Rust ≠ JS compiler:\n{}", diffs.join("\n"));
    }
}
