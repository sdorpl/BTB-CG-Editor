//! Zakładka „Element”: nazwa, wartości pól, ramka sceny i położenie na wizji (jedyne miejsce edycji layoutu).
use super::{heading, hint};
use crate::i18n::tr;
use crate::model::*;
use crate::panels::{combo_t, num, num_range};
use egui::{RichText, Ui};
use serde_json::Value;

fn section<R>(ui: &mut Ui, id: &str, title: &str, open: bool, body: impl FnOnce(&mut Ui) -> R) {
    egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, |ui| { body(ui); });
}

/// Zakładka „Właściwości” przy zaznaczonym całym elemencie: ten sam układ co dla warstwy (Przekształcenie, Zaawansowane),
/// z parametrami elementu: kotwica i położenie na wizji, ramka sceny (W/H), obrót, skala, krycie i warstwa Z.
pub fn element_props(ui: &mut Ui, el: &mut Element) {
    section(ui, "el_transform", tr("section.transform"), true, |ui| {
        let frame = el.scene.as_mut().map(|s| &mut s.frame);
        let lo = &mut el.layout;
        egui::Grid::new("el_geom").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label(tr("element.anchor"));
            combo_t(ui, "side", &mut lo.side, &["custom", "top-left", "top-right", "bottom-left", "bottom-right", "center"], "opt.side");
            ui.end_row();
            if lo.side == "custom" {
                let (cx, cy) = (lo.x.unwrap_or(0.0), lo.y.unwrap_or(0.0));
                let (mut x, mut y) = (cx, cy);
                ui.label("X"); num(ui, &mut x, 1.0); ui.label("Y"); num(ui, &mut y, 1.0); ui.end_row();
                if x != cx || y != cy || lo.x.is_some() { lo.x = Some(x); lo.y = Some(y); }
            } else {
                ui.label(tr("element.margin_x")); num(ui, &mut lo.margin_x, 1.0); ui.label(tr("element.margin_y")); num(ui, &mut lo.margin_y, 1.0); ui.end_row();
            }
            if let Some(f) = frame {
                ui.label("W"); num_range(ui, &mut f.w, 1.0, 1.0, 1920.0); ui.label("H"); num_range(ui, &mut f.h, 1.0, 1.0, 1080.0); ui.end_row();
            }
            ui.label(tr("layer.rotation")); num_range(ui, &mut lo.rotation, 0.5, -360.0, 360.0);
            ui.label(tr("element.scale")); num_range(ui, &mut lo.scale, 0.01, 0.05, 10.0); ui.end_row();
            ui.label(tr("element.layer_z"));
            let cur = lo.layer.unwrap_or(1.0);
            let mut z = cur;
            num_range(ui, &mut z, 0.1, 0.0, 100.0);
            if z != cur || lo.layer.is_some() { lo.layer = Some(z); }
            ui.end_row();
        });
        ui.horizontal(|ui| { ui.label(tr("layer.opacity")); ui.add(egui::Slider::new(&mut el.layout.opacity, 0.0..=1.0)); });
    });
    section(ui, "el_advanced", tr("section.advanced"), false, |ui| {
        ui.horizontal(|ui| { ui.label("ID"); ui.add(egui::TextEdit::singleline(&mut el.id).desired_width(150.0).font(egui::TextStyle::Monospace)); });
        ui.horizontal(|ui| {
            ui.label(tr("common.type"));
            egui::ComboBox::from_id_salt("etype").selected_text(Element::elem_type_label(&el.elem_type)).show_ui(ui, |ui| {
                for t in Element::ELEM_TYPES { ui.selectable_value(&mut el.elem_type, t.to_string(), Element::elem_type_label(t)); }
            });
        });
        if el.elem_type == "BADGE" { hint(ui, tr("element.badge_note")); }
    });
}

/// Zakładka „Element”: dane elementu (nazwa, wartości pól, pola wejściowe). Geometria jest we „Właściwościach”, ruch w „Ruchu”.
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

    if !el.ocg_inputs.is_empty() {
        heading(ui, tr("element.inputs"));
        for i in &el.ocg_inputs {
            ui.label(RichText::new(format!("{} — {}", i.get("id").and_then(|v| v.as_str()).unwrap_or("?"), i.get("label").and_then(|v| v.as_str()).unwrap_or(""))).small().monospace());
        }
    }
}
