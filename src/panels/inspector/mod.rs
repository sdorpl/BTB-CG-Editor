//! Prawy panel edycyjny. Zakładki: Właściwości (warstwa), Ruch (element lub warstwa), Element, Kod.
//! Każda dana ma jedno miejsce edycji: layout i ramka w „Element”, ruch w „Ruch”, stan kodu ręcznego w „Kod”.
mod align;
mod code;
mod element;
mod layer;
mod motion;

use crate::i18n::{tr, trf};
use crate::model::*;
use crate::motion_bridge::ELEMENT;
use egui::{Color32, RichText, Ui};

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum InspectorTab { #[default] Layer, Element, Anim, Code }

fn heading(ui: &mut Ui, t: &str) { ui.add_space(4.0); ui.label(RichText::new(t).strong().color(Color32::from_gray(200))); }

fn hint(ui: &mut Ui, t: &str) { ui.label(RichText::new(t).small().color(Color32::from_gray(140))); }

/// Edycja pola opcjonalnego bez efektów ubocznych: `None` zostaje `None`, dopóki użytkownik czegoś nie zmieni
/// (samo narysowanie panelu nie wpisuje wartości domyślnych do modelu).
fn edit_opt<T: Clone + Default + serde::Serialize>(slot: &mut Option<T>, f: impl FnOnce(&mut T)) {
    let mut v = slot.clone().unwrap_or_default();
    let before = serde_json::to_value(&v).ok();
    f(&mut v);
    if slot.is_some() || serde_json::to_value(&v).ok() != before { *slot = Some(v); }
}

/// Jak `edit_opt`, dla wartości porównywalnych z jawnym domyślnym (np. `Some("none")` vs brak).
fn edit_or<T: Clone + PartialEq>(slot: &mut Option<T>, default: T, f: impl FnOnce(&mut T)) {
    let mut v = slot.clone().unwrap_or_else(|| default.clone());
    f(&mut v);
    if v != slot.clone().unwrap_or(default) { *slot = Some(v); }
}

pub fn draw_inspector(ui: &mut Ui, el: &mut Element, selected: &mut Vec<String>, tab: &mut InspectorTab) {
    header(ui, el, selected);
    ui.horizontal(|ui| {
        ui.selectable_value(tab, InspectorTab::Layer, tr("tab.layer"));
        ui.selectable_value(tab, InspectorTab::Anim, tr("tab.anim"));
        ui.selectable_value(tab, InspectorTab::Element, tr("tab.element"));
        ui.selectable_value(tab, InspectorTab::Code, tr("tab.code"));
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match *tab {
        InspectorTab::Layer => layer::layer_tab(ui, el, selected, tab),
        InspectorTab::Element => element::element_tab(ui, el),
        InspectorTab::Anim => motion::motion_tab(ui, el, selected),
        InspectorTab::Code => code::code_tab(ui, el),
    });
}

/// Nagłówek widoczny w każdej zakładce: nazwa, widoczność i blokada zaznaczonej warstwy.
fn header(ui: &mut Ui, el: &mut Element, selected: &[String]) {
    if selected.len() == 1 && selected[0] == ELEMENT {
        ui.label(RichText::new(trf("element.whole", &[("name", &el.name)])).strong());
        ui.separator();
        return;
    }
    let Some(scene) = el.scene.as_mut() else { return };
    if selected.len() > 1 {
        ui.label(RichText::new(trf("inspector.multi", &[("n", &selected.len())])).strong());
        ui.separator();
        return;
    }
    let Some(l) = selected.first().and_then(|id| scene.layers.iter_mut().find(|l| &l.id == id)) else { return };
    ui.horizontal(|ui| {
        ui.label(l.layer_type.icon());
        let w = (ui.available_width() - 64.0).max(60.0);
        ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(w));
        if ui.selectable_label(false, if l.visible { "👁" } else { "–" }).on_hover_text(tr("layers.visibility")).clicked() { l.visible = !l.visible; }
        if ui.selectable_label(false, if l.locked { "🔒" } else { "🔓" }).on_hover_text(tr("layers.lock")).clicked() { l.locked = !l.locked; }
    });
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_helpers_do_not_materialize_defaults() {
        let mut font: Option<FontStyle> = None;
        edit_opt(&mut font, |_| {});
        assert!(font.is_none(), "samo oglądanie nie może wpisać czcionki");
        edit_opt(&mut font, |f| f.size += 1.0);
        assert!(font.is_some());

        let mut fit: Option<String> = None;
        edit_or(&mut fit, "contain".to_string(), |_| {});
        assert!(fit.is_none());
        edit_or(&mut fit, "contain".to_string(), |v| *v = "contain".into());
        assert!(fit.is_none(), "ustawienie wartości domyślnej nie tworzy pola");
        edit_or(&mut fit, "contain".to_string(), |v| *v = "cover".into());
        assert_eq!(fit.as_deref(), Some("cover"));
    }
}
