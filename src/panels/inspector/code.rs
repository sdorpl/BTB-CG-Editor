//! Zakładka „Kod”: stan sceny względem kodu (tryb ręczny, odłączenie, kopia) oraz edycja HTML/CSS/JS.
use crate::i18n::tr;
use crate::model::*;
use egui::{Color32, RichText, Ui};
use serde_json::Value;

pub fn code_tab(ui: &mut Ui, el: &mut Element) {
    scene_state(ui, el);
    let editable = el.scene.as_ref().map_or(true, |s| s.detached);
    ui.label(RichText::new(if editable { tr("code.editable") } else { tr("code.readonly") }).small().color(Color32::from_gray(150)));
    for (title, text) in [("HTML", &mut el.html_template), ("CSS", &mut el.css_template), ("JS", &mut el.js_template)] {
        ui.collapsing(title, |ui| {
            ui.add_enabled(editable, egui::TextEdit::multiline(text).code_editor().desired_width(f32::INFINITY).desired_rows(10));
        });
    }
}

/// Element bez sceny (kod ręczny), scena odłączona od kodu albo kopia kodu do przywrócenia.
fn scene_state(ui: &mut Ui, el: &mut Element) {
    if el.scene.is_none() {
        ui.colored_label(Color32::from_rgb(251, 191, 36), tr("element.manual_code"));
        ui.label(RichText::new(tr("element.manual_code.hint")).small());
        if ui.button(tr("canvas.manual.create_scene")).clicked() {
            let backup = serde_json::json!({ "html_template": el.html_template, "css_template": el.css_template, "js_template": el.js_template });
            el.extra.insert("codeBackup".into(), backup);
            let scene = crate::presets::scene_for(&el.elem_type);
            crate::compile::apply_to_element(el, scene);
        }
        ui.separator();
    } else if el.scene.as_ref().map_or(false, |s| s.detached) {
        ui.colored_label(Color32::from_rgb(251, 191, 36), tr("element.detached"));
        if ui.button(tr("element.restore_from_scene")).clicked() { crate::compile::recompile(el); }
        ui.separator();
    } else if el.extra.contains_key("codeBackup") {
        if ui.button(tr("element.restore_backup")).clicked() {
            if let Some(Value::Object(b)) = el.extra.get("codeBackup").cloned() {
                el.html_template = b.get("html_template").and_then(|v| v.as_str()).unwrap_or("").into();
                el.css_template = b.get("css_template").and_then(|v| v.as_str()).unwrap_or("").into();
                el.js_template = b.get("js_template").and_then(|v| v.as_str()).unwrap_or("").into();
                el.scene = None;
            }
        }
        ui.separator();
    }
}
