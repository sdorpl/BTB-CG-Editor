//! Edycja tekstu w miejscu (pole + pasek formatowania) i pasek dodawania warstw.
use super::hit::Geo;
use super::{CanvasOut, CanvasState};
use crate::model::{FontStyle, LayerType, Scene};
use egui::{Key, Pos2, Rect, Ui, Vec2};

/// Pole tekstu nad edytowaną warstwą z paskiem formatowania. Enter albo klik poza zatwierdza, Esc anuluje.
pub(super) fn edit_overlay(ui: &mut Ui, st: &mut CanvasState, scene: &mut Scene, geo: &Geo, rect: Rect, out: &mut CanvasOut) {
    let Some(id) = st.editing.clone() else { return };
    let ctx = ui.ctx().clone();
    let Some(idx) = scene.layers.iter().position(|l| l.id == id) else {
        st.editing = None;
        return;
    };
    let bb = geo.bbox_screen(&scene.layers[idx]);
    let er = Rect::from_min_size(bb.min, Vec2::new(bb.width().max(140.0), bb.height().max(26.0)));
    let before = st.edit_buf.clone();
    let resp = ui.put(er, egui::TextEdit::singleline(&mut st.edit_buf).font(egui::FontId::proportional(15.0)));
    if st.edit_fresh { resp.request_focus(); st.edit_fresh = false; }

    let tb_pos = Pos2::new(er.min.x.clamp(rect.min.x + 4.0, (rect.max.x - 420.0).max(rect.min.x + 4.0)), (er.min.y - 38.0).max(rect.min.y + 4.0));
    let tb = egui::Area::new(egui::Id::new("text_format_bar")).order(egui::Order::Foreground).fixed_pos(tb_pos).show(&ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(font) = scene.layers[idx].font.as_mut() { format_bar(ui, font); }
            });
        });
    });

    if st.edit_buf != before { out.edited_text = Some((id.clone(), st.edit_buf.clone())); }
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let esc = ui.input(|i| i.key_pressed(Key::Escape));
    let popup_open = ctx.memory(|m| m.any_popup_open());
    let outside = ui.input(|i| i.pointer.any_pressed()) && !popup_open
        && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !er.contains(p) && !tb.response.rect.contains(p));
    if enter || esc || outside {
        if enter || outside { out.edited_text = Some((id, st.edit_buf.clone())); }
        st.editing = None;
    }
}

/// Pasek dodawania warstw (jak w Canvie): jedno kliknięcie = nowa warstwa na środku ramki.
pub(super) fn add_bar(ui: &Ui, st: &mut CanvasState, scene: &mut Scene, selected: &mut Vec<String>, rect: Rect, out: &mut CanvasOut) {
    let mut add = None;
    egui::Area::new(egui::Id::new("canvas_add_bar")).order(egui::Order::Foreground).fixed_pos(rect.min + Vec2::new(8.0, 8.0)).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                for lt in LayerType::ALL {
                    if ui.button(format!("{} {}", lt.icon(), lt.label())).clicked() { add = Some(lt); }
                }
            });
        });
    });
    if let Some(lt) = add {
        let id = crate::panels::layers::add_layer(scene, lt, selected);
        if lt == LayerType::Text {
            st.edit_buf = scene.layers.iter().find(|l| l.id == id).and_then(|l| l.text.clone()).unwrap_or_default();
            st.editing = Some(id);
            st.edit_fresh = true;
        }
        out.interacting = true;
    }
}

/// Pasek formatowania tekstu (rodzina, rozmiar, B/I, wyrównanie, kolor) nad edytowanym polem.
fn format_bar(ui: &mut Ui, f: &mut FontStyle) {
    egui::ComboBox::from_id_salt("fb_family").selected_text(f.family.clone()).width(110.0).show_ui(ui, |ui| {
        for fam in ["Bahnschrift", "Arial", "Helvetica", "Liberation Sans"] { if ui.selectable_label(f.family.eq_ignore_ascii_case(fam), fam).clicked() { f.family = fam.into(); } }
    });
    ui.add(egui::DragValue::new(&mut f.size).range(4.0..=500.0).speed(0.5));
    let mut bold = f.weight.parse::<u32>().unwrap_or(400) >= 600;
    if ui.toggle_value(&mut bold, egui::RichText::new("B").strong()).changed() { f.weight = if bold { "700" } else { "400" }.into(); }
    ui.toggle_value(&mut f.italic, egui::RichText::new("I").italics());
    for (v, g) in [("left", "⬅"), ("center", "↔"), ("right", "➡")] { ui.selectable_value(&mut f.align, v.to_string(), g); }
    let mut rgba = crate::render::parse_css_color(&f.color).to_srgba_unmultiplied();
    if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
        f.color = if rgba[3] == 255 { format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2]) } else { format!("rgba({},{},{},{:.2})", rgba[0], rgba[1], rgba[2], rgba[3] as f32 / 255.0) };
    }
}
