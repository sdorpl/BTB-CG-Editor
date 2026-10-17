use crate::model::{Layer, LayerType, Scene};
use crate::i18n::tr;
use egui::{Color32, RichText, Ui};

pub fn draw_layers_panel(ui: &mut Ui, scene: &mut Scene, selected: &mut Vec<String>, last_clicked: &mut Option<String>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("layers.title")).size(11.0).strong().color(Color32::from_gray(170)));
    });
    ui.horizontal_wrapped(|ui| {
        for lt in LayerType::ALL {
            if ui.small_button(format!("{} {}", lt.icon(), lt.label())).on_hover_text(tr("layers.add")).clicked() {
                let mut l = Layer::new(lt);
                l.x = ((scene.frame.w - l.w) / 2.0).max(0.0).round();
                l.y = ((scene.frame.h - l.h) / 2.0).max(0.0).round();
                let id = l.id.clone();
                scene.layers.push(l);
                *selected = vec![id];
            }
        }
    });
    ui.separator();

    let mut delete: Vec<String> = vec![];
    let mut dup: Vec<String> = vec![];
    let mut swap: Option<(usize, usize)> = None;
    let order: Vec<String> = scene.layers.iter().rev().map(|l| l.id.clone()).collect();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let n = scene.layers.len();
        for idx in (0..n).rev() {
            let id = scene.layers[idx].id.clone();
            let is_sel = selected.contains(&id);
            let fill = if is_sel { Color32::from_rgba_unmultiplied(59, 130, 246, 55) } else { Color32::TRANSPARENT };
            egui::Frame::none().fill(fill).inner_margin(egui::Margin::symmetric(3.0, 1.0)).rounding(3.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let l = &mut scene.layers[idx];
                    if ui.selectable_label(false, if l.visible { "👁" } else { "–" }).on_hover_text(tr("layers.visibility")).clicked() { l.visible = !l.visible; }
                    if ui.selectable_label(false, if l.locked { "🔒" } else { "🔓" }).on_hover_text(tr("layers.lock")).clicked() { l.locked = !l.locked; }
                    let name_w = (ui.available_width() - 4.0 * 26.0).max(40.0);
                    let label = format!("{} {}", l.layer_type.icon(), if l.name.is_empty() { l.layer_type.label() } else { &l.name });
                    let clicked = ui
                        .allocate_ui_with_layout(egui::vec2(name_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            ui.add(egui::SelectableLabel::new(is_sel, RichText::new(label).color(if l.visible { Color32::from_gray(225) } else { Color32::from_gray(110) })))
                        })
                        .inner
                        .clicked();
                    if clicked {
                        let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                        if shift && last_clicked.is_some() {
                            let a = order.iter().position(|x| Some(x) == last_clicked.as_ref()).unwrap_or(0);
                            let b = order.iter().position(|x| *x == id).unwrap_or(0);
                            *selected = order[a.min(b)..=a.max(b)].to_vec();
                        } else if ctrl {
                            if let Some(p) = selected.iter().position(|x| *x == id) { selected.remove(p); } else { selected.push(id.clone()); }
                            *last_clicked = Some(id.clone());
                        } else {
                            *selected = vec![id.clone()];
                            *last_clicked = Some(id.clone());
                        }
                    }
                    if arrow_button(ui, true).on_hover_text(tr("layers.up")).clicked() && idx + 1 < n { swap = Some((idx, idx + 1)); }
                    if arrow_button(ui, false).on_hover_text(tr("layers.down")).clicked() && idx > 0 { swap = Some((idx, idx - 1)); }
                    if ui.small_button("+").on_hover_text(tr("layers.duplicate")).clicked() { dup.push(id.clone()); }
                    if ui.small_button("×").on_hover_text(tr("layers.delete")).clicked() { delete.push(id.clone()); }
                });
            });
        }
    });

    if let Some((a, b)) = swap { scene.layers.swap(a, b); }
    for id in dup { duplicate_layer(scene, &id, selected); }
    if !delete.is_empty() {
        scene.layers.retain(|l| !delete.contains(&l.id));
        selected.retain(|s| !delete.contains(s));
    }
}

pub fn duplicate_layer(scene: &mut Scene, id: &str, selected: &mut Vec<String>) {
    if let Some(pos) = scene.layers.iter().position(|l| l.id == id) {
        let mut c = scene.layers[pos].clone();
        c.id = format!("l_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
        c.name = crate::i18n::trf("layers.copy_suffix", &[("name", &c.name)]);
        c.x += 20.0;
        c.y += 20.0;
        let nid = c.id.clone();
        scene.layers.insert(pos + 1, c);
        *selected = vec![nid];
    }
}

fn arrow_button(ui: &mut Ui, up: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 18.0), egui::Sense::click());
    let v = ui.style().interact(&resp);
    ui.painter().rect_filled(rect, 3.0, v.weak_bg_fill);
    let c = rect.center();
    let (a, b) = if up { (-3.0, 3.0) } else { (3.0, -3.0) };
    ui.painter().add(egui::Shape::convex_polygon(
        vec![egui::pos2(c.x, c.y + a), egui::pos2(c.x - 4.5, c.y + b), egui::pos2(c.x + 4.5, c.y + b)],
        v.fg_stroke.color,
        egui::Stroke::NONE,
    ));
    resp
}
