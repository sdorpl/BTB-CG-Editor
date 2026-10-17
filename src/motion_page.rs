//! Samodzielna strona wyjściowa (`output-motion.html`) dla OBS Browser Source: scena elementu +
//! silnik `btb-motion` w WASM, bez GSAP, `cgmotion.js`, `renderer.js` i serwera BTB-CG.
//!
//! Stan: transformacje elementu i warstw (IN/OUT/pętla), zegar i ticker. Efekty tekstowe
//! (stagger po znakach/słowach) nie są jeszcze obsługiwane przez ten silnik (patrz docs/PLAN-btb-motion.md).
use crate::model::{Element, LayerType};
use crate::motion_bridge;
use crate::playback;
use crate::render::Values;
use base64::Engine;
use serde_json::{json, Value};

const WASM: &[u8] = include_bytes!("../assets/btb_motion_wasm.wasm");
const PLAYER: &str = include_str!("../assets/btb-motion-player.js");
const FONT: &[u8] = include_bytes!("../assets/BAHNSCHRIFT.TTF");

fn script_safe(s: &str) -> String { s.replace("</script", "<\\/script") }

fn field_strings(el: &Element) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    for (k, v) in &el.fields {
        m.insert(k.clone(), Value::String(match v { Value::String(s) => s.clone(), Value::Null => String::new(), o => o.to_string() }));
    }
    if !el.title.is_empty() { m.insert("TITLE".into(), Value::String(el.title.clone())); }
    m
}

/// Konfiguracja przekazywana do runtime'u (`window.BTB_CONFIG` bez pola `wasm`).
pub fn config(el: &Element, manual: bool) -> Result<Value, String> {
    let scene = el.scene.as_ref().ok_or(crate::i18n::tr("motion_page.no_scene"))?;
    let values = Values { title: &el.title, fields: &el.fields, extra: &el.extra, base_url: "" };
    let scene = &crate::links::resolve(scene, &values);
    let plan = playback::plan(el, scene, &values);
    let director = motion_bridge::build(el, scene, &plan);
    let (fw, fh) = (scene.frame.w, scene.frame.h);
    let (x, y) = el.layout.frame_origin(fw, fh);
    let tickers: Vec<Value> = scene.layers.iter().filter(|l| l.visible).enumerate()
        .filter(|(_, l)| l.layer_type == LayerType::Ticker)
        .map(|(i, l)| json!({ "n": i + 1, "speed": l.speed.unwrap_or(100.0), "sep": l.separator.clone().unwrap_or_else(|| "•".into()), "gap": l.gap.unwrap_or(48.0) }))
        .collect();
    let visible: Vec<&crate::model::Layer> = scene.layers.iter().filter(|l| l.visible).collect();
    let fit: Value = crate::compile::fit_spec(&visible, fw).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    Ok(json!({
        "fit": fit,
        "director": director,
        "html": el.html_template,
        "css": el.css_template.replace("{{ID}}", "btb-root"),
        "data": field_strings(el),
        "tickers": tickers,
        "manual": manual,
        "squash": if el.elem_type == "BADGE" { "length" } else { "fit" },
        "frame": { "w": fw, "h": fh, "x": x, "y": y, "scale": el.layout.scale, "rotation": el.layout.rotation },
    }))
}

pub fn build_html(el: &Element, manual: bool) -> Result<String, String> {
    let mut cfg = config(el, manual)?;
    cfg["wasm"] = Value::String(base64::engine::general_purpose::STANDARD.encode(WASM));
    let f = &cfg["frame"];
    let op = el.layout.opacity;
    let num = |k: &str| f[k].as_f64().ok_or_else(|| format!("frame.{k}: brak liczby"));
    let (fw, fh, x, y, sc, rot) = (num("w")?, num("h")?, num("x")?, num("y")?, num("scale")?, num("rotation")?);
    let css = cfg["css"].as_str().unwrap_or("").to_string();
    let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
    Ok(TEMPLATE
        .replace("@@TITLE@@", &el.name.replace('<', "&lt;"))
        .replace("@@FONT@@", &base64::engine::general_purpose::STANDARD.encode(FONT))
        .replace("@@ROOT@@", &format!("left:{x}px;top:{y}px;width:{fw}px;height:{fh}px;transform:scale({sc}) rotate({rot}deg);opacity:{op};"))
        .replace("@@SCENE_CSS@@", &css.replace("</style", "<\\/style"))
        .replace("@@CONFIG@@", &script_safe(&json))
        .replace("@@PLAYER@@", &script_safe(PLAYER)))
}

const TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="pl"><head><meta charset="UTF-8"><title>@@TITLE@@</title>
<style>
@font-face { font-family: 'Bahnschrift'; src: url(data:font/ttf;base64,@@FONT@@) format('truetype'); font-weight: 100 900; }
html, body { margin: 0; width: 1920px; height: 1080px; background: transparent; overflow: hidden; }
#btb-root { position: absolute; transform-origin: 0 0; visibility: hidden; pointer-events: none; @@ROOT@@ }
#btb-root .btb-fx { position: absolute; inset: 0; }
.cg-t p, .cg-t h1, .cg-t h2, .cg-t h3, .cg-t h4, .cg-t ul, .cg-t ol, .cg-t li, .cg-t blockquote { margin: 0; padding: 0; }
.cg-t p + p { margin-top: 0.4em; }
.slt-squash { display: inline-block !important; transform-origin: left center !important; white-space: nowrap !important; }
.cg-i > .cg-t { width: 100%; height: 100%; overflow: hidden; }
@@SCENE_CSS@@
</style></head>
<body>
<div id="btb-root"><div class="btb-fx"></div></div>
<script>window.BTB_CONFIG = @@CONFIG@@;</script>
<script>@@PLAYER@@</script>
</body></html>"##;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    /// Eksportuje elementy dla `tools/e2e-motion.js` (przeglądarkowy test end-to-end).
    #[test]
    fn export_e2e_elements() {
        let Ok(exe) = std::env::current_exe() else { return };
        let Some(target) = exe.ancestors().nth(3) else { return };
        let dir = target.join("e2e");
        std::fs::create_dir_all(&dir).unwrap();
        let presets: [(&str, &str, &str, &str, &str); 6] = [
            ("lt_slide_fade", "LOWER_THIRD", "slide", "fade", "none"),
            ("lt_wipe_clip", "LOWER_THIRD", "wipe", "clip", "float"),
            ("badge_pop_zoom", "BADGE", "pop", "zoom", "pulse"),
            ("lt_overshoot_blur", "LOWER_THIRD", "slide-overshoot", "blur", "breathe"),
            ("graphic_drop_rotate", "GRAPHIC", "drop", "rotate", "none"),
            ("ticker_fade", "TICKER", "fade", "fade", "none"),
        ];
        // element sterowany wyłącznie kluczami użytkownika (bez presetów): oczekiwane wartości wynikają z kluczy
        {
            let mut el = Element::new_typed("keys_pure", "BADGE");
            el.animation.anim_in = AnimSpec { anim_type: "none".into(), ..Default::default() };
            el.animation.out = AnimSpec { anim_type: "none".into(), ..Default::default() };
            el.animation.loop_spec = None;
            el.animation.text = None;
            el.animation.text_out = None;
            el.animation.extra.insert("stopAt".into(), serde_json::Value::from(1.2));
            let s = el.scene.as_mut().unwrap();
            for l in s.layers.iter_mut() { l.motion = Motion::default(); }
            let key = |t: f32, v: f32| btb_motion::Key::linear(t, v);
            let ch = |prop: &str, keys: Vec<btb_motion::Key>| { let mut c = btb_motion::Channel::new(prop); c.keys = keys; c };
            // tx: 0 -> 100 w 1 s (IN), dalej stała; krycie: 0 -> 1 w 0,5 s (IN), po Stop 1 -> 0 w 0,5 s (OUT)
            s.layers[0].keys = vec![ch("tx", vec![key(0.0, 0.0), key(1.0, 100.0)]), ch("opacity", vec![key(0.0, 0.0), key(0.5, 1.0), key(1.2, 1.0), key(1.7, 0.0)])];
            std::fs::write(dir.join("keys_pure.element.json"), serde_json::to_string(&[&el]).unwrap()).unwrap();
        }
        // ścieżka ruchu: dwa klucze = prosta o stałej prędkości (0,0) -> (100,60) w 1 s, potem stoi
        {
            let mut el = Element::new_typed("path_pure", "BADGE");
            el.animation.anim_in = AnimSpec { anim_type: "none".into(), ..Default::default() };
            el.animation.out = AnimSpec { anim_type: "none".into(), ..Default::default() };
            el.animation.loop_spec = None;
            el.animation.text = None;
            el.animation.text_out = None;
            el.animation.extra.insert("stopAt".into(), serde_json::Value::from(1.2));
            let s = el.scene.as_mut().unwrap();
            for l in s.layers.iter_mut() { l.motion = Motion::default(); }
            s.layers[0].path = Some(btb_motion::PathChannel { keys: vec![btb_motion::PathKey::new(0.0, 0.0, 0.0), btb_motion::PathKey::new(1.0, 100.0, 60.0)] });
            std::fs::write(dir.join("path_pure.element.json"), serde_json::to_string(&[&el]).unwrap()).unwrap();
        }
        for (name, ty, tin, tout, lp) in presets {
            let mut el = Element::new_typed(name, ty);
            el.animation.anim_in = AnimSpec { anim_type: tin.into(), direction: Some("left".into()), duration: 0.7, ..Default::default() };
            el.animation.out = AnimSpec { anim_type: tout.into(), direction: Some("right".into()), duration: 0.5, ..Default::default() };
            el.animation.loop_spec = Some(LoopSpec { loop_type: lp.into(), duration: None, ..Default::default() });
            el.animation.text = None;
            if let Some(s) = el.scene.as_mut() {
                for l in s.layers.iter_mut() { l.motion.text = None; l.motion.text_out = None; }
            }
            std::fs::write(dir.join(format!("{name}.element.json")), serde_json::to_string(&[&el]).unwrap()).unwrap();
            write_converted(&dir, &name, &el);
        }
    }

    /// Zapisuje wersję ze zamienionymi presetami na klucze (`<name>.converted.json`): strona z kluczami jest
    /// porównywana z GSAP oryginału (presety).
    fn write_converted(dir: &std::path::Path, name: &str, el: &Element) {
        let mut c = el.clone();
        crate::keyconv::convert_presets(&mut c, crate::keyconv::Scope::All);
        std::fs::write(dir.join(format!("{name}.converted.json")), serde_json::to_string(&[&c]).unwrap()).unwrap();
    }

    /// Macierz typ x kierunek animacji elementu (bez ruchów warstw i tekstu) dla `E2E_SET=e2e-matrix`.
    #[test]
    fn export_e2e_matrix() {
        use crate::anim::{ANIM_TYPES, DIRECTIONAL};
        let Ok(exe) = std::env::current_exe() else { return };
        let Some(target) = exe.ancestors().nth(3) else { return };
        let dir = target.join("e2e-matrix");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for ty in ANIM_TYPES.iter().filter(|t| **t != "none") {
            let dirs: &[&str] = if DIRECTIONAL.contains(ty) { &["left", "bottom"] } else { &["left"] };
            for d in dirs {
                let name = format!("m_{ty}_{d}");
                let mut el = Element::new_typed(&name, "BADGE");
                el.animation.anim_in = AnimSpec { anim_type: (*ty).into(), direction: Some((*d).into()), duration: 0.6, ease: "power2.out".into(), ..Default::default() };
                el.animation.out = AnimSpec { anim_type: (*ty).into(), direction: Some((*d).into()), duration: 0.6, ease: "power2.in".into(), ..Default::default() };
                el.animation.loop_spec = None;
                el.animation.text = None;
                el.animation.text_out = None;
                if let Some(s) = el.scene.as_mut() {
                    for l in s.layers.iter_mut() { l.motion = Motion::default(); }
                }
                std::fs::write(dir.join(format!("{name}.element.json")), serde_json::to_string(&[&el]).unwrap()).unwrap();
                write_converted(&dir, &name, &el);
            }
        }
    }

    /// Macierz efektów tekstowych IN x OUT (bez ruchów elementu i warstw) dla `E2E_SET=e2e-text`.
    #[test]
    fn export_e2e_text() {
        use crate::anim::{TEXT_IN, TEXT_OUT};
        let Ok(exe) = std::env::current_exe() else { return };
        let Some(target) = exe.ancestors().nth(3) else { return };
        let dir = target.join("e2e-text");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ins: Vec<&str> = TEXT_IN.iter().copied().filter(|t| *t != "none").collect();
        let outs: Vec<&str> = TEXT_OUT.iter().copied().filter(|t| *t != "none").collect();
        let mut cases: Vec<(String, String, String, &str, Option<f32>)> = vec![];
        for (i, tin) in ins.iter().enumerate() {
            cases.push((format!("t_{tin}__{}", outs[i % outs.len()]), tin.to_string(), outs[i % outs.len()].to_string(), "start", None));
        }
        // warianty staggera: kierunek (from) i jawny odstęp
        for from in ["end", "center", "edges"] {
            cases.push((format!("t_chars-fade__chars-fade__{from}"), "chars-fade".into(), "chars-fade".into(), from, None));
            cases.push((format!("t_words-up__words-up__{from}"), "words-up".into(), "words-up".into(), from, Some(0.15)));
        }
        cases.push(("t_lines-up__lines-fade__start".into(), "lines-up".into(), "lines-fade".into(), "start", Some(0.2)));
        for (name, tin, tout, from, stagger) in cases {
            let mut el = Element::new_typed(&name, "LOWER_THIRD");
            el.title = "Pierwsze drugie trzecie czwarte piąte szóste 1 250,5 zł".into();
            el.animation.anim_in = AnimSpec { anim_type: "none".into(), ..Default::default() };
            el.animation.out = AnimSpec { anim_type: "none".into(), ..Default::default() };
            el.animation.loop_spec = None;
            let mut t_in = TextEffect::new(&tin, 0.6, 0.1);
            let mut t_out = TextEffect::new(&tout, 0.5, 0.05);
            t_in.stagger = stagger;
            t_out.stagger = stagger;
            for t in [&mut t_in, &mut t_out] { t.extra.insert("staggerFrom".into(), serde_json::Value::from(from)); }
            el.animation.text = Some(t_in);
            el.animation.text_out = Some(t_out);
            if let Some(s) = el.scene.as_mut() {
                for l in s.layers.iter_mut() {
                    l.motion = Motion::default();
                    if l.layer_type == LayerType::Text { l.w = 230.0; l.h = 120.0; l.squash = Some(false); }
                }
            }
            std::fs::write(dir.join(format!("{name}.element.json")), serde_json::to_string(&[&el]).unwrap()).unwrap();
        }
    }

    #[test]
    fn layout_opacity_is_applied_to_the_page_root() {
        let mut el = Element::new_typed("t", "LOWER_THIRD");
        el.layout.opacity = 0.5;
        assert!(build_html(&el, true).unwrap().contains("opacity:0.5;"));
    }

    #[test]
    fn page_contains_scene_wasm_and_no_gsap() {
        let el = Element::new_typed("t", "LOWER_THIRD");
        let html = build_html(&el, true).unwrap();
        assert!(html.contains("BTB_CONFIG") && html.contains("cg-scene") && html.contains("#btb-root"));
        assert!(!html.contains("gsap") && !html.contains("{{ID}}"));
        assert!(html.len() > 300_000);
        assert!(build_html(&Element { scene: None, ..el }, true).is_err());
    }
}
