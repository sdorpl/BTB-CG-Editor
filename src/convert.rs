//! Conversion of hand-written elements (html/css/js_template, no `scene`) into editable scenes.
//!
//! The DOM-measuring converter lives in BTB-CG (`scene_convert.js`: renders the element with the real
//! renderer and reads geometry/styles back as layers). There is no DOM here, so we drive that very
//! converter through `tools/convert-bridge.js` (Node + Playwright from the BTB-CG tree) and parse the
//! element it returns; the result is then an ordinary scene element that `compile` can regenerate.
use crate::model::Element;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

const BRIDGE_JS: &str = include_str!("../tools/convert-bridge.js");

pub struct Converted {
    pub element: Option<Element>,
    pub warnings: Vec<String>,
    /// Co odtworzono z kodu szablonu poza geometrią (domyślne efekty tekstu, animacje z `onShow`).
    pub notes: Vec<String>,
    pub error: Option<String>,
    pub summary: String,
}

pub type ConvertResult = Result<Vec<Converted>, String>;

pub fn needs_conversion(el: &Element) -> bool {
    el.scene.is_none() && !(el.html_template.trim().is_empty() && el.css_template.trim().is_empty() && el.js_template.trim().is_empty())
}

fn is_btb_root(p: &Path) -> bool { p.join("scene_convert.js").exists() && p.join("renderer.js").exists() }

/// `$BTB_CG_ROOT`, else a `BTB-CG-System-alpha` checkout next to (or above) the working directory / editor.
pub fn find_btb_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BTB_CG_ROOT") { let p = PathBuf::from(p); if is_btb_root(&p) { return Some(p); } }
    let mut starts: Vec<PathBuf> = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))];
    if let Ok(c) = std::env::current_dir() { starts.push(c); }
    for s in starts {
        for dir in s.ancestors() {
            for cand in [dir.to_path_buf(), dir.join("BTB-CG-System-alpha"), dir.join("BTB-CG")] {
                if is_btb_root(&cand) { return Some(cand); }
            }
        }
    }
    None
}

pub fn convert_async(root: &str, elements: Vec<Element>) -> Receiver<ConvertResult> {
    let (tx, rx): (Sender<ConvertResult>, Receiver<ConvertResult>) = std::sync::mpsc::channel();
    let root = PathBuf::from(root.trim());
    thread::spawn(move || { let _ = tx.send(convert_blocking(&root, &elements)); });
    rx
}

pub fn convert_blocking(root: &Path, elements: &[Element]) -> ConvertResult {
    if !is_btb_root(root) {
        return Err(format!("„{}” nie wygląda na katalog BTB-CG (brak scene_convert.js / renderer.js). Podaj ścieżkę do BTB-CG-System.", root.display()));
    }
    let bridge = std::env::temp_dir().join("btb-cgeditor-convert-bridge.js");
    std::fs::write(&bridge, BRIDGE_JS).map_err(|e| format!("Nie można zapisać mostka konwersji: {e}"))?;
    let input = serde_json::to_vec(elements).map_err(|e| e.to_string())?;
    let mut child = Command::new("node").arg(&bridge).arg(root)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("Nie można uruchomić Node.js (wymagany do konwersji): {e}"))?;
    let mut stdin = child.stdin.take().ok_or("brak stdin")?;
    let writer = thread::spawn(move || { let _ = stdin.write_all(&input); });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    parse_bridge_output(&out.stdout).map_err(|e| {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        if err.is_empty() { e } else { format!("{e} ({err})") }
    })
}

fn parse_bridge_output(stdout: &[u8]) -> ConvertResult {
    let v: Value = serde_json::from_slice(stdout).map_err(|e| format!("Nieprawidłowa odpowiedź konwertera: {e}"))?;
    let arr = v.as_array().ok_or("Konwerter zwrócił nieoczekiwany format.")?;
    Ok(arr.iter().map(|r| {
        let warnings: Vec<String> = r.get("warnings").and_then(|w| w.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let notes: Vec<String> = r.get("notes").and_then(|w| w.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if r.get("ok").and_then(|x| x.as_bool()) != Some(true) {
            let error = r.get("error").and_then(|x| x.as_str()).unwrap_or("Nieznany błąd konwersji.").to_string();
            return Converted { element: None, warnings, notes, error: Some(error), summary: String::new() };
        }
        match serde_json::from_value::<Element>(r["element"].clone()) {
            Ok(mut el) if el.scene.is_some() => {
                // odtworzone z kodu animacje zmieniły scenę po kompilacji w JS: kod musi odpowiadać scenie
                if !notes.is_empty() { crate::compile::recompile(&mut el); }
                let st = &r["stats"];
                let summary = format!("{} warstw, ramka {}×{}", st["layers"], st["frame"]["w"], st["frame"]["h"]);
                Converted { element: Some(el), warnings, notes, error: None, summary }
            }
            Ok(_) => Converted { element: None, warnings, notes, error: Some("Konwerter nie zwrócił sceny.".into()), summary: String::new() },
            Err(e) => Converted { element: None, warnings, notes, error: Some(format!("Błąd odczytu wyniku: {e}")), summary: String::new() },
        }
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LayerType;

    #[test]
    fn detects_code_only_elements() {
        let mut el = Element::new_typed("x", "LOWER_THIRD");
        assert!(!needs_conversion(&el));           // scene-based
        el.scene = None;
        assert!(needs_conversion(&el));            // has compiled code but no scene
        el.html_template.clear(); el.css_template.clear(); el.js_template.clear();
        assert!(!needs_conversion(&el));           // nothing to convert
    }

    #[test]
    fn parses_bridge_failure_and_garbage() {
        let r = parse_bridge_output(br#"[{"ok":false,"error":"boom","warnings":["w"]}]"#).unwrap();
        assert_eq!(r[0].error.as_deref(), Some("boom"));
        assert_eq!(r[0].warnings, vec!["w"]);
        assert!(parse_bridge_output(b"not json").is_err());
    }

    /// End-to-end against the real BTB-CG tree (skipped when it or Node/Playwright is unavailable).
    #[test]
    fn real_db_code_elements_convert_to_scenes() {
        let Some(root) = find_btb_root() else { return };
        let Ok(text) = std::fs::read_to_string(root.join("db.json")) else { return };
        let list = crate::importer::parse_elements(&text).unwrap();
        let code: Vec<Element> = list.into_iter().map(|l| l.element).filter(needs_conversion).take(4).collect();
        assert!(!code.is_empty());
        let res = match convert_blocking(&root, &code) { Ok(r) => r, Err(e) if e.contains("Node.js") || e.contains("playwright") => return, Err(e) => panic!("{e}") };
        assert_eq!(res.len(), code.len());
        for (c, src) in res.iter().zip(&code) {
            let el = c.element.as_ref().unwrap_or_else(|| panic!("{}: {:?}", src.name, c.error));
            assert!(!el.scene.as_ref().unwrap().layers.is_empty());
            assert!(!el.html_template.is_empty(), "scene must be compiled into code");
        }
    }

    /// Intencja animacji z kodu szablonu, której konwerter DOM nie widzi: domyślne efekty tekstu (np. clip-up),
    /// dodatkowe animacje z `onShow` przeniesione na właściwą warstwę oraz ostrzeżenie o auto-szerokości.
    #[test]
    fn real_db_templates_keep_their_animation_intent() {
        let Some(root) = find_btb_root() else { return };
        let Ok(text) = std::fs::read_to_string(root.join("db.json")) else { return };
        let list = crate::importer::parse_elements(&text).unwrap();
        let code: Vec<Element> = list.into_iter().map(|l| l.element).filter(needs_conversion).collect();
        let pick = |name: &str| code.iter().find(|e| e.name == name).cloned();
        let (Some(republika), Some(miasto)) = (pick("Republika Lower Third"), pick("MIASTO lewa")) else { return };
        let res = match convert_blocking(&root, &[republika, miasto]) { Ok(r) => r, Err(e) if e.contains("Node.js") || e.contains("playwright") => return, Err(e) => panic!("{e}") };
        let el = res[0].element.as_ref().unwrap_or_else(|| panic!("{:?}", res[0].error));
        // domyślne efekty tekstu z kodu szablonu (operator ich nie wybrał, ale oryginał je odtwarzał)
        assert_eq!(el.animation.text.as_ref().map(|t| t.effect_type.as_str()), Some("clip-up"), "{:?}", res[0].notes);
        assert_eq!(el.animation.text_out.as_ref().map(|t| t.effect_type.as_str()), Some("clip-down"));
        // animacja z onShow (wygaszanie grafiki bocznej) trafia na warstwę o tej klasie, a nie na tło belki
        let side = el.scene.as_ref().unwrap().layers.iter().find(|l| l.name.contains("rep-side-graphic")).expect("warstwa grafiki bocznej");
        assert_eq!(side.motion.motion_in.as_ref().map(|m| m.anim_type.as_str()), Some("fade"));
        assert!((side.motion.motion_in.as_ref().unwrap().delay - 0.15).abs() < 1e-3);
        assert!(!el.js_template.is_empty() && el.js_template.contains("fade"), "kod odtworzony ze sceny zawiera ruch warstwy");
        assert!(res[0].notes.len() >= 3);
        // belka o auto-szerokości: uwaga w notatkach
        assert!(res[1].notes.iter().any(|n| n.contains("dopasowuje szerokość")), "{:?}", res[1].notes);
        let el2 = res[1].element.as_ref().unwrap();
        assert!(el2.scene.as_ref().unwrap().layers.iter().any(|l| l.layer_type == LayerType::Text && l.squash == Some(true)));
    }

    /// Belka BADGE ściskana wg długości: po konwersji warstwa tekstu zachowuje pełną szerokość układu, lewą krawędź
    /// jak w oryginale i wyrównanie do lewej, a ściskanie jest włączone (inaczej tekst wystaje poza tło).
    #[test]
    fn real_db_badge_with_long_title_keeps_squash_geometry() {
        let Some(root) = find_btb_root() else { return };
        let Ok(text) = std::fs::read_to_string(root.join("db.json")) else { return };
        let list = crate::importer::parse_elements(&text).unwrap();
        let Some(mut badge) = list.into_iter().map(|l| l.element).filter(needs_conversion).find(|e| e.elem_type == "BADGE" && e.name.starts_with("MIASTO prawa")) else { return };
        let long = "Warszawa / trubunał konstytucyjny bogdana święczkowskiego";
        badge.title = long.into();
        badge.fields.insert("TITLE".into(), serde_json::Value::String(long.into()));
        let res = match convert_blocking(&root, &[badge]) { Ok(r) => r, Err(e) if e.contains("Node.js") || e.contains("playwright") => return, Err(e) => panic!("{e}") };
        let el = res[0].element.as_ref().unwrap_or_else(|| panic!("{:?}", res[0].error));
        let scene = el.scene.as_ref().unwrap();
        let t = scene.layers.iter().find(|l| l.layer_type == LayerType::Text).expect("warstwa tekstu");
        assert_eq!(t.squash, Some(true));
        assert_eq!(t.font.as_ref().map(|f| f.align.as_str()), Some("left"));
        assert!(t.w > 1000.0, "pełna szerokość układu zamiast szerokości po ściśnięciu: {}", t.w);
        assert!(scene.frame.w >= t.x + t.w - 1.0, "ramka obejmuje warstwę");
    }
}
