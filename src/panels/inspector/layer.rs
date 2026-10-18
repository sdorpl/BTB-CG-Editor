//! Zakładka „Właściwości”: przekształcenie, wygląd zależny od typu i sekcja zaawansowana zaznaczonej warstwy.
use super::{align, edit_opt, edit_or, element, heading, hint};
use crate::i18n::{tr, trf};
use crate::model::*;
use crate::panels::{color_edit, combo, combo_t, num, num_range};
use egui::{Color32, CollapsingHeader, RichText, Ui};

fn section<R>(ui: &mut Ui, id: &str, title: &str, open: bool, body: impl FnOnce(&mut Ui) -> R) {
    CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, |ui| { body(ui); });
}

/// Bez zaznaczonej warstwy (albo przy zaznaczonym „Elemencie”) panel pokazuje właściwości całego elementu w tym samym układzie.
pub fn layer_tab(ui: &mut Ui, el: &mut Element, selected: &[String]) {
    if super::element_selected(selected) { element::element_props(ui, el); return }
    let Some(scene) = el.scene.as_mut() else { hint(ui, tr("layer.no_scene")); return };
    if selected.len() > 1 {
        align::align_tools(ui, scene, selected);
        multi_props(ui, scene, selected);
        return;
    }
    let id = selected[0].clone();
    let others: Vec<(String, String)> = scene.layers.iter().filter(|l| l.layer_type == LayerType::Shape && l.id != id).map(|l| (l.id.clone(), l.name.clone())).collect();
    let auto_texts: Vec<(String, String)> = scene.layers.iter().filter(|l| l.layer_type == LayerType::Text && l.auto_w == Some(true) && l.id != id).map(|l| (l.id.clone(), l.name.clone())).collect();
    let frame = (scene.frame.w, scene.frame.h);
    let Some(layer) = scene.layers.iter_mut().find(|l| l.id == id) else { return };
    align::align_single(ui, layer, frame);

    section(ui, "sec_transform", tr("section.transform"), true, |ui| {
        egui::Grid::new("geom").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label("X"); num(ui, &mut layer.x, 1.0); ui.label("Y"); num(ui, &mut layer.y, 1.0); ui.end_row();
            ui.label("W"); num_range(ui, &mut layer.w, 1.0, 1.0, 10000.0); ui.label("H"); num_range(ui, &mut layer.h, 1.0, 1.0, 10000.0); ui.end_row();
            ui.label(tr("layer.rotation")); num(ui, &mut layer.rotation, 0.5); ui.label(tr("layer.skew")); num(ui, &mut layer.skew, 0.5); ui.end_row();
        });
        if layer.layer_type == LayerType::Text { fit_height(ui, layer); }
        ui.horizontal(|ui| { ui.label(tr("layer.opacity")); ui.add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0)); });
    });

    let mut synced = None;
    section(ui, "sec_appearance", tr("section.appearance"), true, |ui| match layer.layer_type {
        LayerType::Shape => shape_props(ui, layer),
        // Treść domyślna jest też wartością pola: podgląd bierze najpierw `title`/`fields`, więc bez synchronizacji edycja nic nie zmieniała.
        LayerType::Text => synced = text_props(ui, layer),
        LayerType::Image => media_props(ui, layer, false),
        LayerType::Video => media_props(ui, layer, true),
        LayerType::Clock => {
            heading(ui, tr("layer.clock"));
            edit_or(&mut layer.format, "HH:MM:SS".to_string(), |f| { ui.horizontal(|ui| { ui.label(tr("layer.format")); combo(ui, "clk_fmt", f, &["HH:MM:SS", "HH:MM"]); }); });
            edit_opt(&mut layer.font, |f| font_props(ui, f, "clk"));
        }
        LayerType::Ticker => {
            heading(ui, tr("layer.ticker"));
            egui::Grid::new("tk").num_columns(2).show(ui, |ui| {
                ui.label(tr("layer.ticker.speed")); edit_or(&mut layer.speed, 100.0, |v| { num_range(ui, v, 1.0, 5.0, 2000.0); }); ui.end_row();
                ui.label(tr("layer.ticker.gap")); edit_or(&mut layer.gap, 48.0, |v| { num_range(ui, v, 1.0, 0.0, 500.0); }); ui.end_row();
                ui.label(tr("layer.ticker.separator")); edit_or(&mut layer.separator, "•".to_string(), |v| { ui.add(egui::TextEdit::singleline(v).desired_width(60.0)); }); ui.end_row();
            });
            edit_opt(&mut layer.font, |f| font_props(ui, f, "tk"));
        }
    });
    if let Some((fid, text)) = synced {
        if fid == "TITLE" { el.title = text; } else { el.fields.insert(fid, serde_json::Value::String(text)); }
    }
    // zaawansowane: rzadko używane ustawienia warstwy (na końcu, żeby nie wypychały podstawowych)
    let Some(layer) = el.scene.as_mut().and_then(|s| s.layers.iter_mut().find(|l| l.id == id)) else { return };
    section(ui, "sec_advanced", tr("section.advanced"), false, |ui| advanced(ui, layer, &others, &auto_texts));
}

/// Zaznaczonych kilka warstw: tylko wspólne pola (przesunięcie i krycie dla wszystkich).
fn multi_props(ui: &mut Ui, scene: &mut Scene, selected: &[String]) {
    heading(ui, tr("section.transform"));
    let (mut dx, mut dy) = (0.0f32, 0.0f32);
    ui.horizontal(|ui| {
        ui.label(tr("multi.move"));
        ui.label("X"); num(ui, &mut dx, 1.0); ui.label("Y"); num(ui, &mut dy, 1.0);
    });
    let first = scene.layers.iter().find(|l| selected.contains(&l.id)).map_or(1.0, |l| l.opacity);
    let mut op = first;
    ui.horizontal(|ui| { ui.label(tr("layer.opacity")); ui.add(egui::Slider::new(&mut op, 0.0..=1.0)); });
    for l in scene.layers.iter_mut().filter(|l| selected.contains(&l.id)) {
        l.x += dx; l.y += dy;
        if op != first { l.opacity = op; }
    }
}

/// Wiersz tekstu ma wysokość `rozmiar × interlinia` (tak liczy go strona wyjściowa); przycisk dopasowuje pudełko.
fn fit_height(ui: &mut Ui, l: &mut Layer) {
    let Some(font) = &l.font else { return };
    let want_h = (font.size * font.line_height).round().max(1.0);
    if (l.h - want_h).abs() > 0.5 && ui.small_button(trf("text.fit_h", &[("h", &want_h)])).clicked() { l.h = want_h; }
}

fn advanced(ui: &mut Ui, l: &mut Layer, shapes: &[(String, String)], auto_texts: &[(String, String)]) {
    ui.horizontal(|ui| {
        ui.label(tr("layer.group"));
        let mut g = l.group.clone().unwrap_or_default();
        if ui.add(egui::TextEdit::singleline(&mut g).desired_width(90.0)).changed() { l.group = if g.is_empty() { None } else { Some(g) }; }
    });
    match l.layer_type {
        LayerType::Shape => {
            heading(ui, tr("shape.bg_effect"));
            edit_or(&mut l.effect, "none".to_string(), |e| combo_t(ui, "fx", e, &["none", "shimmer", "pulse", "glow", "scanline"], "opt.fx"));
        }
        LayerType::Text => {
            let mut sq = l.squash.unwrap_or(false);
            if ui.checkbox(&mut sq, tr("text.squash")).changed() { l.squash = Some(sq); }
            let mut aw = l.auto_w.unwrap_or(false);
            if ui.checkbox(&mut aw, tr("text.auto_w")).on_hover_text(tr("text.auto_w.hover")).changed() { l.auto_w = if aw { Some(true) } else { None }; }
            if aw {
                ui.horizontal(|ui| {
                    ui.label(tr("text.min_w")); let mut v = l.min_w.unwrap_or(0.0);
                    if num_range(ui, &mut v, 1.0, 0.0, 10000.0).changed() { l.min_w = if v > 0.0 { Some(v) } else { None }; }
                    ui.label(tr("text.max_w")); let mut v = l.max_w.unwrap_or(0.0);
                    if num_range(ui, &mut v, 1.0, 0.0, 10000.0).changed() { l.max_w = if v > 0.0 { Some(v) } else { None }; }
                });
            }
        }
        LayerType::Image | LayerType::Video => {
            let cur = l.mask_layer.clone().unwrap_or_default();
            let cur_name = shapes.iter().find(|(id, _)| *id == cur).map(|(_, n)| n.clone()).unwrap_or_else(|| tr("common.none").into());
            ui.horizontal(|ui| {
                ui.label(tr("media.mask"));
                egui::ComboBox::from_id_salt("mask").selected_text(cur_name).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_empty(), tr("common.none")).clicked() { l.mask_layer = None; }
                    for (id, n) in shapes { if ui.selectable_label(*id == cur, n).clicked() { l.mask_layer = Some(id.clone()); } }
                });
            });
        }
        _ => {}
    }
    if l.layer_type != LayerType::Text && !auto_texts.is_empty() {
        heading(ui, tr("fit.title"));
        let cur = l.fit_text.clone();
        let cur_name = cur.as_ref().and_then(|f| auto_texts.iter().find(|(id, _)| *id == f.to)).map(|(_, n)| n.clone()).unwrap_or_else(|| tr("common.none").into());
        ui.horizontal(|ui| {
            ui.label(tr("fit.to"));
            egui::ComboBox::from_id_salt("fit_to").selected_text(cur_name).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), tr("common.none")).clicked() { l.fit_text = None; }
                for (id, n) in auto_texts {
                    if ui.selectable_label(cur.as_ref().is_some_and(|f| f.to == *id), n).clicked() {
                        l.fit_text = Some(FitText { to: id.clone(), mode: cur.as_ref().map(|f| f.mode.clone()).unwrap_or_else(|| "size".into()) });
                    }
                }
            });
        });
        if let Some(f) = l.fit_text.as_mut() {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut f.mode, "size".to_string(), tr("fit.mode.size")).on_hover_text(tr("fit.mode.size.hover"));
                ui.selectable_value(&mut f.mode, "follow".to_string(), tr("fit.mode.follow")).on_hover_text(tr("fit.mode.follow.hover"));
            });
        }
    }
}

fn shape_props(ui: &mut Ui, l: &mut Layer) {
    heading(ui, tr("shape.fill"));
    edit_opt(&mut l.fill, |f| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut f.fill_type, FillType::Solid, tr("shape.fill.solid"));
            ui.selectable_value(&mut f.fill_type, FillType::Gradient, tr("shape.fill.gradient"));
            ui.selectable_value(&mut f.fill_type, FillType::None, tr("shape.fill.none"));
        });
        if f.fill_type != FillType::None { ui.horizontal(|ui| { ui.label(tr("shape.color1")); color_edit(ui, &mut f.color); }); }
        if f.fill_type == FillType::Gradient {
            ui.horizontal(|ui| { ui.label(tr("shape.color2")); color_edit(ui, &mut f.color2); });
            ui.horizontal(|ui| { ui.label(tr("shape.angle")); num_range(ui, &mut f.angle, 1.0, -360.0, 360.0); });
        }
    });
    heading(ui, tr("shape.stroke"));
    edit_opt(&mut l.stroke, |s| { ui.horizontal(|ui| { ui.label(tr("shape.stroke.width")); num_range(ui, &mut s.width, 0.25, 0.0, 100.0); color_edit(ui, &mut s.color); }); });
    radius_ui(ui, &mut l.radius);
    heading(ui, tr("shadow.title"));
    let cur = l.shadow.clone();
    let mut mode = match &cur { None | Some(Shadow::Flag(false)) => 0, Some(Shadow::Flag(true)) => 1, _ => 2 };
    ui.horizontal(|ui| {
        ui.selectable_value(&mut mode, 0, tr("shadow.none")); ui.selectable_value(&mut mode, 1, tr("shadow.default")); ui.selectable_value(&mut mode, 2, tr("shadow.custom"));
    });
    let new = match mode {
        0 => Shadow::Flag(false),
        1 => Shadow::Flag(true),
        _ => {
            let (mut x, mut y, mut b, mut c) = match &cur { Some(Shadow::Custom { x, y, blur, color }) => (*x, *y, *blur, color.clone()), _ => (0.0, 10.0, 25.0, "rgba(0,0,0,0.6)".to_string()) };
            ui.horizontal(|ui| { ui.label("x"); num(ui, &mut x, 1.0); ui.label("y"); num(ui, &mut y, 1.0); ui.label(tr("shadow.blur")); num_range(ui, &mut b, 1.0, 0.0, 200.0); });
            ui.horizontal(|ui| { ui.label(tr("shadow.color")); color_edit(ui, &mut c); });
            Shadow::Custom { x, y, blur: b, color: c }
        }
    };
    if cur.clone().unwrap_or(Shadow::Flag(false)) != new { l.shadow = Some(new); }
}

fn radius_ui(ui: &mut Ui, r: &mut Option<Radius>) {
    let cur = r.clone().unwrap_or(Radius::Uniform(0.0));
    let mut per_corner = matches!(cur, Radius::Corners(_));
    ui.horizontal(|ui| {
        ui.label(tr("shape.radius"));
        ui.checkbox(&mut per_corner, tr("shape.radius.per_corner"));
    });
    let new = if per_corner {
        let mut c = cur.corners();
        ui.horizontal(|ui| { for v in c.iter_mut() { num_range(ui, v, 0.5, 0.0, 1000.0); } });
        Radius::Corners(c.to_vec())
    } else {
        let mut u = cur.uniform();
        ui.horizontal(|ui| { num_range(ui, &mut u, 0.5, 0.0, 1000.0); });
        Radius::Uniform(u)
    };
    if new != cur { *r = Some(new); }
}

/// Zwraca (id pola, nowa treść), gdy zmieniono tekst domyślny pola.
fn text_props(ui: &mut Ui, l: &mut Layer) -> Option<(String, String)> {
    let mut synced = None;
    heading(ui, tr("text.content"));
    let mut field = l.field.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(tr("text.field"));
        if ui.add(egui::TextEdit::singleline(&mut field).desired_width(110.0).hint_text(tr("text.field.hint"))).changed() {
            l.field = if field.is_empty() { None } else { Some(field.clone()) };
        }
        if ui.small_button("TITLE").clicked() { l.field = Some("TITLE".into()); }
        if ui.small_button(tr("text.fixed")).clicked() { l.field = None; }
    });
    if l.field.is_some() {
        ui.label(tr("text.default"));
        let mut changed = false;
        edit_or(&mut l.default_text, String::new(), |t| changed = ui.text_edit_multiline(t).changed());
        if changed {
            let fid = crate::compile::field_id(l.field.as_deref().unwrap_or("TITLE"));
            synced = Some((if fid.is_empty() { "TITLE".to_string() } else { fid }, l.default_text.clone().unwrap_or_default()));
        }
    } else {
        ui.label(tr("text.static"));
        edit_or(&mut l.text, String::new(), |t| { ui.text_edit_multiline(t); });
    }
    edit_opt(&mut l.font, |f| font_props(ui, f, "txt"));
    synced
}

fn font_props(ui: &mut Ui, f: &mut FontStyle, salt: &str) {
    heading(ui, tr("font.title"));
    ui.horizontal(|ui| {
        ui.label(tr("font.family"));
        ui.add(egui::TextEdit::singleline(&mut f.family).desired_width(120.0));
        egui::ComboBox::from_id_salt(format!("fam_{salt}")).selected_text("▾").width(20.0).show_ui(ui, |ui| {
            // lista z pakietu fontów (manifest): Bahnschrift (lokalny) → rodziny OFL z pakietu → systemowe odpowiedniki (Arial → Liberation Sans)
            let reg = crate::fonts::registry();
            let mut pick = |ui: &mut Ui, fam: &str| { if ui.selectable_label(f.family.eq_ignore_ascii_case(fam), fam).clicked() { f.family = fam.into(); } };
            ui.weak(tr("font.group.local"));
            pick(ui, "Bahnschrift");
            let pack: Vec<String> = reg.family_names().into_iter().filter(|(_, p)| *p).map(|(n, _)| n).collect();
            if !pack.is_empty() { ui.separator(); ui.weak(tr("font.group.pack")); }
            for fam in &pack { pick(ui, fam); }
            ui.separator(); ui.weak(tr("font.group.system"));
            for fam in ["Arial", "Helvetica", "Liberation Sans"] { pick(ui, fam); }
        });
    });
    egui::Grid::new(format!("font_{salt}")).num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label(tr("font.size")); num_range(ui, &mut f.size, 0.5, 4.0, 500.0);
        ui.label(tr("font.weight")); combo(ui, format!("w_{salt}"), &mut f.weight, &["300", "400", "500", "600", "700", "800", "900"]); ui.end_row();
        ui.label(tr("font.letter_spacing")); num(ui, &mut f.letter_spacing, 0.1);
        ui.label(tr("font.line_height")); num_range(ui, &mut f.line_height, 0.02, 0.5, 4.0); ui.end_row();
    });
    ui.horizontal(|ui| { ui.label(tr("font.color")); color_edit(ui, &mut f.color); });
    ui.horizontal(|ui| {
        ui.label(tr("font.transform")); combo_t(ui, format!("tr_{salt}"), &mut f.transform, &["none", "uppercase", "lowercase", "capitalize"], "opt.transform");
    });
    ui.horizontal(|ui| {
        ui.label(tr("font.align_h"));
        for (v, g) in [("left", "⬅"), ("center", "↔"), ("right", "➡")] { ui.selectable_value(&mut f.align, v.to_string(), g); }
        ui.label(tr("font.align_v"));
        for (v, g) in [("top", "⬆"), ("middle", "↕"), ("bottom", "⬇")] { ui.selectable_value(&mut f.valign, v.to_string(), g); }
    });
    ui.checkbox(&mut f.italic, tr("font.italic"));
    ui.horizontal(|ui| { ui.label(tr("font.shadow")); ui.add(egui::TextEdit::singleline(&mut f.shadow).desired_width(150.0).font(egui::TextStyle::Monospace)); });
}

/// Przycisk „Wybierz plik…”: osadza obraz w warstwie (błąd i ostrzeżenie o rozmiarze zostają pod przyciskiem).
fn pick_file_ui(ui: &mut Ui, l: &mut Layer) {
    let err_id = egui::Id::new(("media_err", &l.id));
    ui.horizontal(|ui| {
        if ui.button(tr("media.pick")).clicked() {
            if let Some(p) = crate::media::pick_image() {
                match crate::media::load_file(&p) {
                    Ok(a) => {
                        crate::media::apply_to_layer(l, &a);
                        let msg = if a.is_large() { trf("media.big_warning", &[("mb", &(crate::media::WARN_BYTES / 1024 / 1024))]) } else { String::new() };
                        ui.data_mut(|d| d.insert_temp(err_id, msg));
                    }
                    Err(e) => ui.data_mut(|d| d.insert_temp(err_id, e)),
                }
            }
        }
        if let Some(n) = l.extra.get("srcName").and_then(|v| v.as_str()).filter(|_| l.src.as_deref().is_some_and(|s| s.starts_with("data:"))) {
            ui.label(RichText::new(trf("media.file", &[("name", &n)])).small().color(Color32::from_gray(150)));
        }
    });
    if let Some(msg) = ui.data(|d| d.get_temp::<String>(err_id)).filter(|m| !m.is_empty()) {
        ui.label(RichText::new(msg).small().color(Color32::from_rgb(230, 170, 70)));
    }
}

fn media_props(ui: &mut Ui, l: &mut Layer, video: bool) {
    heading(ui, if video { tr("media.video") } else { tr("media.image") });
    if !video { pick_file_ui(ui, l); }
    edit_or(&mut l.src, String::new(), |src| { ui.horizontal(|ui| { ui.label(tr("media.source")); ui.add(egui::TextEdit::singleline(src).desired_width(150.0)); }); });
    if !video {
        edit_or(&mut l.bind, String::new(), |b| { ui.horizontal(|ui| { ui.label(tr("media.bind")); ui.add(egui::TextEdit::singleline(b).desired_width(110.0).hint_text(tr("media.bind.hint"))); }); });
    }
    edit_or(&mut l.fit, "contain".to_string(), |fit| { ui.horizontal(|ui| { ui.label(tr("media.fit")); combo_t(ui, "fit", fit, &["contain", "cover", "fill", "none", "scale-down"], "opt.fit"); }); });
    if video {
        edit_or(&mut l.video_loop, true, |lp| { ui.checkbox(lp, tr("media.loop")); });
        edit_or(&mut l.muted, true, |mu| { ui.checkbox(mu, tr("media.muted")); });
    }
    radius_ui(ui, &mut l.radius);
}
