//! Zakładka „Element”: nazwa, wartości pól, ramka sceny i położenie na wizji (jedyne miejsce edycji layoutu).
use super::{heading, hint};
use crate::i18n::tr;
use crate::model::*;
use crate::panels::{combo_t, num, num_range};
use egui::{Color32, RichText, Ui};
use serde_json::Value;

/// Rozmiar ramki sceny elementu.
fn frame_section(ui: &mut Ui, scene: &mut Scene) {
    heading(ui, tr("element.frame"));
    ui.horizontal(|ui| {
        ui.label("W"); num_range(ui, &mut scene.frame.w, 1.0, 1.0, 1920.0);
        ui.label("H"); num_range(ui, &mut scene.frame.h, 1.0, 1.0, 1080.0);
    });
}

/// Położenie całego elementu na wizji: kotwica, pozycja/marginesy, obrót, skala i warstwa Z.
fn layout_section(ui: &mut Ui, lo: &mut Layout) {
    heading(ui, tr("element.layout"));
    ui.horizontal(|ui| {
        ui.label(tr("element.anchor"));
        combo_t(ui, "side", &mut lo.side, &["custom", "top-left", "top-right", "bottom-left", "bottom-right", "center"], "opt.side");
    });
    if lo.side == "custom" {
        let (mut x, mut y) = (lo.x.unwrap_or(0.0), lo.y.unwrap_or(0.0));
        ui.horizontal(|ui| { ui.label("X"); num(ui, &mut x, 1.0); ui.label("Y"); num(ui, &mut y, 1.0); });
        if Some(x) != lo.x || Some(y) != lo.y { lo.x = Some(x); lo.y = Some(y); }
    } else {
        ui.horizontal(|ui| { ui.label(tr("element.margin_x")); num(ui, &mut lo.margin_x, 1.0); ui.label("Y"); num(ui, &mut lo.margin_y, 1.0); });
    }
    ui.horizontal(|ui| { ui.label(tr("element.opacity")); ui.add(egui::Slider::new(&mut lo.opacity, 0.0..=1.0).fixed_decimals(2)); });
    ui.horizontal(|ui| { ui.label(tr("element.rotation")); num_range(ui, &mut lo.rotation, 0.5, -360.0, 360.0); });
    ui.horizontal(|ui| {
        ui.label(tr("element.scale")); num_range(ui, &mut lo.scale, 0.01, 0.05, 10.0);
        ui.label(tr("element.layer_z"));
        let cur = lo.layer.unwrap_or(1.0);
        let mut z = cur;
        num_range(ui, &mut z, 0.1, 0.0, 100.0);
        if z != cur || lo.layer.is_some() { lo.layer = Some(z); }
    });
}

pub fn element_tab(ui: &mut Ui, el: &mut Element) {
    ui.horizontal(|ui| { ui.label(tr("common.name")); ui.text_edit_singleline(&mut el.name); });

    heading(ui, tr("element.values"));
    hint(ui, tr("element.values.hint"));
    ui.horizontal(|ui| { ui.label("TITLE"); ui.add(egui::TextEdit::singleline(&mut el.title).desired_width(170.0)); });
    let keys: Vec<String> = el.fields.keys().cloned().collect();
    let mut remove: Option<String> = None;
    for k in keys {
        if let Some(Value::String(s)) = el.fields.get_mut(&k) {
            ui.horizontal(|ui| {
                ui.label(&k);
                ui.add(egui::TextEdit::singleline(s).desired_width(150.0));
                if ui.small_button("×").clicked() { remove = Some(k.clone()); }
            });
        }
    }
    if let Some(k) = remove { el.fields.remove(&k); }

    if let Some(scene) = el.scene.as_mut() { frame_section(ui, scene); }
    layout_section(ui, &mut el.layout);

    ui.separator();
    egui::CollapsingHeader::new(RichText::new(tr("section.advanced")).strong()).id_salt("el_advanced").default_open(false).show(ui, |ui| {
        ui.horizontal(|ui| { ui.label("ID"); ui.add(egui::TextEdit::singleline(&mut el.id).desired_width(150.0).font(egui::TextStyle::Monospace)); });
        ui.horizontal(|ui| {
            ui.label(tr("common.type"));
            egui::ComboBox::from_id_salt("etype").selected_text(Element::elem_type_label(&el.elem_type)).show_ui(ui, |ui| {
                for t in Element::ELEM_TYPES { ui.selectable_value(&mut el.elem_type, t.to_string(), Element::elem_type_label(t)); }
            });
        });
        if el.elem_type == "BADGE" { ui.label(RichText::new(tr("element.badge_note")).small().color(Color32::from_gray(140))); }
        if !el.ocg_inputs.is_empty() {
            heading(ui, tr("element.inputs"));
            for i in &el.ocg_inputs {
                ui.label(RichText::new(format!("{} — {}", i.get("id").and_then(|v| v.as_str()).unwrap_or("?"), i.get("label").and_then(|v| v.as_str()).unwrap_or(""))).small().monospace());
            }
        }
    });
}
