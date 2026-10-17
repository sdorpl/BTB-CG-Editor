pub mod graph;
pub mod inspector;
pub mod layers;
pub mod timeline;

use egui::{Color32, Ui};

/// Hex/rgba text field + colour picker, editing a CSS colour string in place.
pub fn color_edit(ui: &mut Ui, value: &mut String) {
    let mut c = crate::render::parse_css_color(value);
    let mut rgba = c.to_srgba_unmultiplied();
    if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
        c = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
        *value = if rgba[3] == 255 {
            format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2])
        } else {
            format!("rgba({},{},{},{:.2})", rgba[0], rgba[1], rgba[2], rgba[3] as f32 / 255.0)
        };
    }
    let _ = c;
    ui.add(egui::TextEdit::singleline(value).desired_width(92.0).font(egui::TextStyle::Monospace));
}

pub fn num(ui: &mut Ui, v: &mut f32, speed: f64) -> egui::Response {
    ui.add(egui::DragValue::new(v).speed(speed))
}

pub fn num_range(ui: &mut Ui, v: &mut f32, speed: f64, min: f32, max: f32) -> egui::Response {
    ui.add(egui::DragValue::new(v).speed(speed).range(min..=max))
}

pub fn combo(ui: &mut Ui, id: impl std::hash::Hash, current: &mut String, options: &[&str]) {
    egui::ComboBox::from_id_salt(id).selected_text(current.clone()).width(120.0).show_ui(ui, |ui| {
        for o in options { ui.selectable_value(current, o.to_string(), *o); }
    });
}
