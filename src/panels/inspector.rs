use super::{color_edit, combo, combo_t, num, num_range};
use crate::i18n::{opt, tr, trf};
use crate::anim::{STAGGER_FROM, ANIM_TYPES, DIRECTIONAL, DIRECTIONS, EASES, LOOP_TYPES, TEXT_IN, TEXT_OUT};
use crate::model::*;
use egui::{Color32, RichText, Ui};
use serde_json::Value;

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum InspectorTab { #[default] Layer, Element, Anim, Code }

fn heading(ui: &mut Ui, t: &str) { ui.add_space(4.0); ui.label(RichText::new(t).strong().color(Color32::from_gray(200))); }

pub fn draw_inspector(ui: &mut Ui, el: &mut Element, selected: &[String], tab: &mut InspectorTab) {
    ui.horizontal(|ui| {
        ui.selectable_value(tab, InspectorTab::Layer, tr("tab.layer"));
        ui.selectable_value(tab, InspectorTab::Element, tr("tab.element"));
        ui.selectable_value(tab, InspectorTab::Anim, tr("tab.anim"));
        ui.selectable_value(tab, InspectorTab::Code, tr("tab.code"));
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match *tab {
        InspectorTab::Layer => layer_tab(ui, el, selected),
        InspectorTab::Element => element_tab(ui, el),
        InspectorTab::Anim => anim_tab(ui, el),
        InspectorTab::Code => code_tab(ui, el),
    });
}

// ── Layer tab ───────────────────────────────────────────────────────────────

fn layer_tab(ui: &mut Ui, el: &mut Element, selected: &[String]) {
    if selected.len() == 1 && selected[0] == crate::motion_bridge::ELEMENT {
        element_selection(ui, el);
        return;
    }
    let Some(scene) = el.scene.as_mut() else {
        ui.label(RichText::new(tr("layer.no_scene")).color(Color32::from_gray(140)));
        return;
    };
    if selected.is_empty() {
        ui.label(RichText::new(tr("layer.select")).color(Color32::from_gray(140)));
        return;
    }
    if selected.len() > 1 { align_tools(ui, scene, selected); }
    let id = selected[0].clone();
    let peers: Vec<(String, String)> = scene.layers.iter().filter(|l| l.id != id).map(|l| (l.id.clone(), l.name.clone())).collect();
    let others: Vec<(String, String)> = scene.layers.iter().filter(|l| l.layer_type == LayerType::Shape && l.id != id).map(|l| (l.id.clone(), l.name.clone())).collect();
    let frame = (scene.frame.w, scene.frame.h);
    let Some(layer) = scene.layers.iter_mut().find(|l| l.id == id) else { return };
    if selected.len() == 1 { align_single(ui, layer, frame); }

    ui.horizontal(|ui| { ui.label(tr("common.name")); ui.text_edit_singleline(&mut layer.name); });
    heading(ui, tr("layer.position"));
    egui::Grid::new("geom").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label("X"); num(ui, &mut layer.x, 1.0); ui.label("Y"); num(ui, &mut layer.y, 1.0); ui.end_row();
        ui.label("W"); num_range(ui, &mut layer.w, 1.0, 1.0, 10000.0); ui.label("H"); num_range(ui, &mut layer.h, 1.0, 1.0, 10000.0); ui.end_row();
        ui.label(tr("layer.rotation")); num(ui, &mut layer.rotation, 0.5); ui.label(tr("layer.skew")); num(ui, &mut layer.skew, 0.5); ui.end_row();
    });
    ui.horizontal(|ui| { ui.label(tr("layer.opacity")); ui.add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0)); });
    ui.horizontal(|ui| {
        ui.label(tr("layer.group"));
        let mut g = layer.group.clone().unwrap_or_default();
        if ui.add(egui::TextEdit::singleline(&mut g).desired_width(90.0)).changed() { layer.group = if g.is_empty() { None } else { Some(g) }; }
    });

    ui.separator();
    match layer.layer_type {
        LayerType::Shape => shape_props(ui, layer),
        LayerType::Text => {
            // Treść domyślna jest też wartością pola: podgląd bierze najpierw `title`/`fields`, więc bez synchronizacji edycja nic nie zmieniała.
            if let Some((fid, text)) = text_props(ui, layer) {
                if fid == "TITLE" { el.title = text; } else { el.fields.insert(fid, serde_json::Value::String(text)); }
            }
        }
        LayerType::Image => media_props(ui, layer, false, &others),
        LayerType::Video => media_props(ui, layer, true, &others),
        LayerType::Clock => {
            heading(ui, tr("layer.clock"));
            let mut f = layer.format.clone().unwrap_or_else(|| "HH:MM:SS".into());
            ui.horizontal(|ui| { ui.label(tr("layer.format")); combo(ui, "clk_fmt", &mut f, &["HH:MM:SS", "HH:MM"]); });
            layer.format = Some(f);
            let font = layer.font.get_or_insert_with(FontStyle::default);
            font_props(ui, font, "clk");
        }
        LayerType::Ticker => {
            heading(ui, tr("layer.ticker"));
            egui::Grid::new("tk").num_columns(2).show(ui, |ui| {
                let mut sp = layer.speed.unwrap_or(100.0);
                ui.label(tr("layer.ticker.speed")); num_range(ui, &mut sp, 1.0, 5.0, 2000.0); ui.end_row();
                layer.speed = Some(sp);
                let mut gap = layer.gap.unwrap_or(48.0);
                ui.label(tr("layer.ticker.gap")); num_range(ui, &mut gap, 1.0, 0.0, 500.0); ui.end_row();
                layer.gap = Some(gap);
                let mut sep = layer.separator.clone().unwrap_or_else(|| "•".into());
                ui.label(tr("layer.ticker.separator")); ui.add(egui::TextEdit::singleline(&mut sep).desired_width(60.0)); ui.end_row();
                layer.separator = Some(sep);
            });
            let font = layer.font.get_or_insert_with(FontStyle::default);
            font_props(ui, font, "tk");
        }
    }
    ui.separator();
    motion_props(ui, &mut layer.motion, &peers);
}

fn align_single(ui: &mut Ui, l: &mut Layer, frame: (f32, f32)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("align.to_frame")).small().color(Color32::from_gray(150)));
        if ui.small_button("L").on_hover_text(tr("align.left")).clicked() { l.x = 0.0; }
        if ui.small_button("C").on_hover_text(tr("align.center_h")).clicked() { l.x = ((frame.0 - l.w) / 2.0).round(); }
        if ui.small_button("R").on_hover_text(tr("align.right")).clicked() { l.x = frame.0 - l.w; }
        if ui.small_button("T").on_hover_text(tr("align.top")).clicked() { l.y = 0.0; }
        if ui.small_button("M").on_hover_text(tr("align.center_v")).clicked() { l.y = ((frame.1 - l.h) / 2.0).round(); }
        if ui.small_button("B").on_hover_text(tr("align.bottom")).clicked() { l.y = frame.1 - l.h; }
    });
}

fn align_tools(ui: &mut Ui, scene: &mut Scene, selected: &[String]) {
    heading(ui, &trf("align.multi", &[("n", &selected.len())]));
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for l in scene.layers.iter().filter(|l| selected.contains(&l.id)) {
        minx = minx.min(l.x); miny = miny.min(l.y); maxx = maxx.max(l.x + l.w); maxy = maxy.max(l.y + l.h);
    }
    let mut act: Option<u8> = None;
    ui.horizontal(|ui| {
        for (i, (g, t)) in [("L", tr("align.m_left")), ("C", tr("align.m_center_h")), ("R", tr("align.m_right")), ("T", tr("align.m_top")), ("M", tr("align.m_center_v")), ("B", tr("align.m_bottom")), ("H", tr("align.distribute_h")), ("V", tr("align.distribute_v"))].iter().enumerate() {
            if ui.small_button(*g).on_hover_text(*t).clicked() { act = Some(i as u8); }
        }
    });
    let Some(a) = act else { return };
    let mut ids: Vec<(String, f32, f32, f32, f32)> = scene.layers.iter().filter(|l| selected.contains(&l.id)).map(|l| (l.id.clone(), l.x, l.y, l.w, l.h)).collect();
    if a >= 6 && ids.len() >= 3 {
        if a == 6 {
            ids.sort_by(|p, q| p.1.partial_cmp(&q.1).unwrap());
            let total: f32 = ids.iter().map(|i| i.3).sum();
            let gap = ((maxx - minx) - total) / (ids.len() as f32 - 1.0);
            let mut x = minx;
            for (id, _, _, w, _) in &ids { if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) { l.x = x.round(); } x += w + gap; }
        } else {
            ids.sort_by(|p, q| p.2.partial_cmp(&q.2).unwrap());
            let total: f32 = ids.iter().map(|i| i.4).sum();
            let gap = ((maxy - miny) - total) / (ids.len() as f32 - 1.0);
            let mut y = miny;
            for (id, _, _, _, h) in &ids { if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) { l.y = y.round(); } y += h + gap; }
        }
        return;
    }
    for l in scene.layers.iter_mut().filter(|l| selected.contains(&l.id)) {
        match a { 0 => l.x = minx, 1 => l.x = ((minx + maxx - l.w) / 2.0).round(), 2 => l.x = maxx - l.w,
                  3 => l.y = miny, 4 => l.y = ((miny + maxy - l.h) / 2.0).round(), 5 => l.y = maxy - l.h, _ => {} }
    }
}

fn shape_props(ui: &mut Ui, l: &mut Layer) {
    heading(ui, tr("shape.fill"));
    let f = l.fill.get_or_insert_with(Fill::default);
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
    heading(ui, tr("shape.stroke"));
    let s = l.stroke.get_or_insert_with(Stroke::default);
    ui.horizontal(|ui| { ui.label(tr("shape.stroke.width")); num_range(ui, &mut s.width, 0.25, 0.0, 100.0); color_edit(ui, &mut s.color); });
    radius_ui(ui, &mut l.radius);
    heading(ui, tr("shadow.title"));
    let mut mode = match &l.shadow { None | Some(Shadow::Flag(false)) => 0, Some(Shadow::Flag(true)) => 1, _ => 2 };
    ui.horizontal(|ui| {
        ui.selectable_value(&mut mode, 0, tr("shadow.none")); ui.selectable_value(&mut mode, 1, tr("shadow.default")); ui.selectable_value(&mut mode, 2, tr("shadow.custom"));
    });
    l.shadow = Some(match mode {
        0 => Shadow::Flag(false),
        1 => Shadow::Flag(true),
        _ => {
            let (mut x, mut y, mut b, mut c) = match &l.shadow { Some(Shadow::Custom { x, y, blur, color }) => (*x, *y, *blur, color.clone()), _ => (0.0, 10.0, 25.0, "rgba(0,0,0,0.6)".to_string()) };
            ui.horizontal(|ui| { ui.label("x"); num(ui, &mut x, 1.0); ui.label("y"); num(ui, &mut y, 1.0); ui.label(tr("shadow.blur")); num_range(ui, &mut b, 1.0, 0.0, 200.0); });
            ui.horizontal(|ui| { ui.label(tr("shadow.color")); color_edit(ui, &mut c); });
            Shadow::Custom { x, y, blur: b, color: c }
        }
    });
    heading(ui, tr("shape.bg_effect"));
    let mut e = l.effect.clone().unwrap_or_else(|| "none".into());
    combo_t(ui, "fx", &mut e, &["none", "shimmer", "pulse", "glow", "scanline"], "opt.fx");
    l.effect = Some(e);
}

fn radius_ui(ui: &mut Ui, r: &mut Option<Radius>) {
    let cur = r.clone().unwrap_or(Radius::Uniform(0.0));
    let mut per_corner = matches!(cur, Radius::Corners(_));
    ui.horizontal(|ui| {
        ui.label(tr("shape.radius"));
        ui.checkbox(&mut per_corner, tr("shape.radius.per_corner"));
    });
    if per_corner {
        let mut c = cur.corners();
        ui.horizontal(|ui| { for v in c.iter_mut() { num_range(ui, v, 0.5, 0.0, 1000.0); } });
        *r = Some(Radius::Corners(c.to_vec()));
    } else {
        let mut u = cur.uniform();
        ui.horizontal(|ui| { num_range(ui, &mut u, 0.5, 0.0, 1000.0); });
        *r = Some(Radius::Uniform(u));
    }
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
        let t = l.default_text.get_or_insert_with(String::new);
        ui.label(tr("text.default"));
        if ui.text_edit_multiline(t).changed() {
            let fid = crate::compile::field_id(l.field.as_deref().unwrap_or("TITLE"));
            synced = Some((if fid.is_empty() { "TITLE".to_string() } else { fid }, l.default_text.clone().unwrap_or_default()));
        }
    } else {
        let t = l.text.get_or_insert_with(String::new);
        ui.label(tr("text.static"));
        ui.text_edit_multiline(t);
    }
    let mut sq = l.squash.unwrap_or(false);
    if ui.checkbox(&mut sq, tr("text.squash")).changed() { l.squash = Some(sq); }
    let font = l.font.get_or_insert_with(FontStyle::default);
    font_props(ui, font, "txt");
    // Pudełko tekstu: wiersz o wysokości `rozmiar × interlinia` (tak liczy go strona wyjściowa).
    let want_h = (font.size * font.line_height).round().max(1.0);
    if (l.h - want_h).abs() > 0.5 && ui.small_button(trf("text.fit_h", &[("h", &want_h)])).clicked() { l.h = want_h; }
    synced
}

fn font_props(ui: &mut Ui, f: &mut FontStyle, salt: &str) {
    heading(ui, tr("font.title"));
    ui.horizontal(|ui| {
        ui.label(tr("font.family"));
        ui.add(egui::TextEdit::singleline(&mut f.family).desired_width(120.0));
        egui::ComboBox::from_id_salt(format!("fam_{salt}")).selected_text("▾").width(20.0).show_ui(ui, |ui| {
            for fam in ["Bahnschrift", "Arial", "Helvetica", "Liberation Sans"] { if ui.selectable_label(f.family.eq_ignore_ascii_case(fam), fam).clicked() { f.family = fam.into(); } }
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

fn media_props(ui: &mut Ui, l: &mut Layer, video: bool, shapes: &[(String, String)]) {
    heading(ui, if video { tr("media.video") } else { tr("media.image") });
    let src = l.src.get_or_insert_with(String::new);
    ui.horizontal(|ui| { ui.label(tr("media.source")); ui.add(egui::TextEdit::singleline(src).desired_width(150.0)); });
    if !video {
        let b = l.bind.get_or_insert_with(String::new);
        ui.horizontal(|ui| { ui.label(tr("media.bind")); ui.add(egui::TextEdit::singleline(b).desired_width(110.0).hint_text(tr("media.bind.hint"))); });
    }
    let mut fit = l.fit.clone().unwrap_or_else(|| "contain".into());
    ui.horizontal(|ui| { ui.label(tr("media.fit")); combo_t(ui, "fit", &mut fit, &["contain", "cover", "fill", "none", "scale-down"], "opt.fit"); });
    l.fit = Some(fit);
    if video {
        let mut lp = l.video_loop.unwrap_or(true);
        let mut mu = l.muted.unwrap_or(true);
        ui.horizontal(|ui| { ui.checkbox(&mut lp, tr("media.loop")); ui.checkbox(&mut mu, tr("media.muted")); });
        l.video_loop = Some(lp); l.muted = Some(mu);
    }
    radius_ui(ui, &mut l.radius);
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

// ── Motion ──────────────────────────────────────────────────────────────────

pub fn motion_props(ui: &mut Ui, m: &mut Motion, peers: &[(String, String)]) {
    heading(ui, tr("motion.title"));
    optional_motion(ui, tr("motion.in"), "m_in", &mut m.motion_in, peers);
    optional_motion(ui, tr("motion.out"), "m_out", &mut m.out, peers);
    optional_text(ui, tr("motion.text_in"), "t_in", &mut m.text, TEXT_IN, 0.2, peers);
    optional_text(ui, tr("motion.text_out"), "t_out", &mut m.text_out, TEXT_OUT, 0.0, peers);
}

/// „Po warstwie”: ruch startuje po zakończeniu tej samej fazy wskazanej warstwy (opóźnienie liczy się od jej końca).
fn after_row(ui: &mut Ui, salt: &str, extra: &mut Extra, peers: &[(String, String)]) {
    let cur = crate::links::after(extra).map(String::from);
    let name = |id: &str| peers.iter().find(|p| p.0 == id).map(|p| p.1.clone()).unwrap_or_else(|| tr("motion.after.missing").into());
    let mut sel = cur.clone();
    ui.label(tr("motion.after"));
    egui::ComboBox::from_id_salt(format!("af_{salt}")).selected_text(sel.as_deref().map(name).unwrap_or_else(|| tr("motion.after.parallel").into())).width(120.0).show_ui(ui, |ui| {
        ui.selectable_value(&mut sel, None, tr("motion.after.parallel"));
        for (id, n) in peers { ui.selectable_value(&mut sel, Some(id.clone()), n); }
    });
    ui.end_row();
    if sel != cur { crate::links::set_after(extra, sel.as_deref()); }
}

fn optional_motion(ui: &mut Ui, label: &str, salt: &str, m: &mut Option<MotionSpec>, peers: &[(String, String)]) {
    let mut on = m.is_some();
    if ui.checkbox(&mut on, label).changed() { *m = if on { Some(MotionSpec::default()) } else { None }; }
    if let Some(spec) = m { motion_spec(ui, salt, spec, peers); }
}

fn motion_spec(ui: &mut Ui, salt: &str, s: &mut MotionSpec, peers: &[(String, String)]) {
    egui::Grid::new(format!("ms_{salt}")).num_columns(2).spacing([6.0, 3.0]).show(ui, |ui| {
        ui.label(tr("common.type")); combo_t(ui, format!("ty_{salt}"), &mut s.anim_type, ANIM_TYPES, "opt.anim"); ui.end_row();
        crate::anim::sync_fallback(&mut s.extra, &s.anim_type);
        if DIRECTIONAL.contains(&s.anim_type.as_str()) {
            let mut d = s.direction.clone().unwrap_or_else(|| "left".into());
            ui.label(tr("motion.direction")); combo_t(ui, format!("di_{salt}"), &mut d, DIRECTIONS, "opt.dir"); ui.end_row();
            s.direction = Some(d);
        }
        ui.label(tr("motion.duration")); num_range(ui, &mut s.duration, 0.02, 0.05, 10.0); ui.end_row();
        ui.label(tr("motion.delay")); num_range(ui, &mut s.delay, 0.02, 0.0, 10.0); ui.end_row();
        if !peers.is_empty() { after_row(ui, salt, &mut s.extra, peers); }
        ui.label(tr("motion.ease")); combo(ui, format!("ea_{salt}"), &mut s.ease, EASES); ui.end_row();
    });
}

fn optional_text(ui: &mut Ui, label: &str, salt: &str, t: &mut Option<TextEffect>, options: &[&str], default_delay: f32, peers: &[(String, String)]) {
    let prefix = if std::ptr::eq(options, TEXT_OUT) { "opt.text_out" } else { "opt.text_in" };
    let mut cur = t.as_ref().map(|x| x.effect_type.clone()).unwrap_or_else(|| "(dziedzicz)".into());
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(salt).selected_text(if cur == "(dziedzicz)" { tr("text.inherit_short").to_string() } else { opt(prefix, &cur) }).width(120.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut cur, "(dziedzicz)".to_string(), tr("text.inherit"));
            for o in options { ui.selectable_value(&mut cur, o.to_string(), opt(prefix, o)); }
        });
    });
    if cur == "(dziedzicz)" { *t = None; return; }
    let e = t.get_or_insert_with(|| TextEffect::new(&cur, 0.5, default_delay));
    e.effect_type = cur;
    crate::anim::sync_fallback(&mut e.extra, &e.effect_type);
    if e.effect_type != "none" {
        egui::Grid::new(format!("te_{salt}")).num_columns(4).spacing([6.0, 3.0]).show(ui, |ui| {
            ui.label(tr("motion.duration")); num_range(ui, &mut e.duration, 0.02, 0.05, 10.0);
            ui.label(tr("motion.delay_short")); num_range(ui, &mut e.delay, 0.02, 0.0, 10.0); ui.end_row();
            if !peers.is_empty() { after_row(ui, salt, &mut e.extra, peers); }
            let mut st = e.stagger.unwrap_or(0.0);
            ui.label(tr("motion.stagger")); if num_range(ui, &mut st, 0.005, 0.0, 1.0).changed() { e.stagger = if st > 0.0 { Some(st) } else { None }; }
            ui.end_row();
            if crate::anim::split_of(&e.effect_type) != crate::anim::Split::Whole {
                let mut from = e.stagger_from().to_string();
                ui.label(tr("motion.stagger_from")); combo_t(ui, format!("sf_{salt}"), &mut from, STAGGER_FROM, "opt.stagger"); ui.end_row();
                if from == "start" { e.extra.remove("staggerFrom"); } else { e.extra.insert("staggerFrom".into(), Value::from(from)); }
            }
        });
    }
}

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
        lo.x = Some(x); lo.y = Some(y);
    } else {
        ui.horizontal(|ui| { ui.label(tr("element.margin_x")); num(ui, &mut lo.margin_x, 1.0); ui.label("Y"); num(ui, &mut lo.margin_y, 1.0); });
    }
    ui.horizontal(|ui| { ui.label(tr("element.opacity")); ui.add(egui::Slider::new(&mut lo.opacity, 0.0..=1.0).fixed_decimals(2)); });
    ui.horizontal(|ui| { ui.label(tr("element.rotation")); num_range(ui, &mut lo.rotation, 0.5, -360.0, 360.0); });
    ui.horizontal(|ui| { ui.label(tr("element.scale")); num_range(ui, &mut lo.scale, 0.01, 0.05, 10.0); ui.label(tr("element.layer_z")); let mut z = lo.layer.unwrap_or(1.0); num_range(ui, &mut z, 0.1, 0.0, 100.0); lo.layer = Some(z); });
}

/// Zaznaczony cały element (na kanwie lub w panelu warstw): jego rozmiar i położenie na wizji.
fn element_selection(ui: &mut Ui, el: &mut Element) {
    heading(ui, &trf("element.whole", &[("name", &el.name)]));
    if let Some(scene) = el.scene.as_mut() { frame_section(ui, scene); }
    layout_section(ui, &mut el.layout);
}

// ── Element tab ─────────────────────────────────────────────────────────────

fn element_tab(ui: &mut Ui, el: &mut Element) {
    heading(ui, tr("tab.element"));
    ui.horizontal(|ui| { ui.label(tr("common.name")); ui.text_edit_singleline(&mut el.name); });
    ui.horizontal(|ui| { ui.label("ID"); ui.add(egui::TextEdit::singleline(&mut el.id).desired_width(150.0).font(egui::TextStyle::Monospace)); });
    ui.horizontal(|ui| {
        ui.label(tr("common.type"));
        egui::ComboBox::from_id_salt("etype").selected_text(Element::elem_type_label(&el.elem_type)).show_ui(ui, |ui| {
            for t in Element::ELEM_TYPES { ui.selectable_value(&mut el.elem_type, t.to_string(), Element::elem_type_label(t)); }
        });
    });
    if el.elem_type == "BADGE" {
        ui.label(RichText::new(tr("element.badge_note")).small().color(Color32::from_gray(140)));
    }

    if let Some(scene) = el.scene.as_mut() { frame_section(ui, scene); }

    layout_section(ui, &mut el.layout);

    heading(ui, tr("element.values"));
    ui.label(RichText::new(tr("element.values.hint")).small().color(Color32::from_gray(140)));
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

    ui.separator();
    if el.scene.is_none() {
        ui.colored_label(Color32::from_rgb(251, 191, 36), tr("element.manual_code"));
        ui.label(RichText::new(tr("element.manual_code.hint")).small());
        if ui.button(tr("canvas.manual.create_scene")).clicked() {
            let backup = serde_json::json!({ "html_template": el.html_template, "css_template": el.css_template, "js_template": el.js_template });
            el.extra.insert("codeBackup".into(), backup);
            let scene = crate::presets::scene_for(&el.elem_type);
            crate::compile::apply_to_element(el, scene);
        }
    } else if el.scene.as_ref().map_or(false, |s| s.detached) {
        ui.colored_label(Color32::from_rgb(251, 191, 36), tr("element.detached"));
        if ui.button(tr("element.restore_from_scene")).clicked() { crate::compile::recompile(el); }
    } else if el.extra.contains_key("codeBackup") && ui.button(tr("element.restore_backup")).clicked() {
        if let Some(Value::Object(b)) = el.extra.get("codeBackup").cloned() {
            el.html_template = b.get("html_template").and_then(|v| v.as_str()).unwrap_or("").into();
            el.css_template = b.get("css_template").and_then(|v| v.as_str()).unwrap_or("").into();
            el.js_template = b.get("js_template").and_then(|v| v.as_str()).unwrap_or("").into();
            el.scene = None;
        }
    }
}

// ── Element animation tab ───────────────────────────────────────────────────

fn anim_spec(ui: &mut Ui, salt: &str, s: &mut AnimSpec) {
    egui::Grid::new(format!("as_{salt}")).num_columns(2).spacing([6.0, 3.0]).show(ui, |ui| {
        ui.label(tr("common.type")); combo_t(ui, format!("ty_{salt}"), &mut s.anim_type, ANIM_TYPES, "opt.anim"); ui.end_row();
        crate::anim::sync_fallback(&mut s.extra, &s.anim_type);
        if DIRECTIONAL.contains(&s.anim_type.as_str()) {
            let mut d = s.direction.clone().unwrap_or_else(|| "left".into());
            ui.label(tr("motion.direction")); combo_t(ui, format!("di_{salt}"), &mut d, DIRECTIONS, "opt.dir"); ui.end_row();
            s.direction = Some(d);
        }
        ui.label(tr("motion.duration")); num_range(ui, &mut s.duration, 0.02, 0.05, 10.0); ui.end_row();
        ui.label(tr("motion.delay")); num_range(ui, &mut s.delay, 0.02, 0.0, 10.0); ui.end_row();
        ui.label(tr("motion.ease")); combo(ui, format!("ea_{salt}"), &mut s.ease, EASES); ui.end_row();
    });
}

fn anim_tab(ui: &mut Ui, el: &mut Element) {
    ui.label(RichText::new(tr("anim.intro")).small().color(Color32::from_gray(140)));
    heading(ui, tr("motion.in"));
    anim_spec(ui, "ein", &mut el.animation.anim_in);
    heading(ui, tr("motion.out"));
    anim_spec(ui, "eout", &mut el.animation.out);
    heading(ui, tr("anim.text"));
    optional_text(ui, tr("anim.text_in"), "e_t_in", &mut el.animation.text, TEXT_IN, 0.2, &[]);
    optional_text(ui, tr("anim.text_out"), "e_t_out", &mut el.animation.text_out, TEXT_OUT, 0.0, &[]);
    for (key, label) in [("textSync", tr("anim.text_sync")), ("textOutSync", tr("anim.text_out_sync"))] {
        let mut v = el.animation.extra.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        if ui.checkbox(&mut v, label).changed() { el.animation.extra.insert(key.into(), Value::Bool(v)); }
    }
    heading(ui, tr("anim.loop"));
    let mut lp = el.animation.loop_spec.clone().unwrap_or_default();
    let mut t = if lp.loop_type.is_empty() { "none".to_string() } else { lp.loop_type.clone() };
    ui.horizontal(|ui| { ui.label(tr("common.type")); combo_t(ui, "loop", &mut t, LOOP_TYPES, "opt.loop"); });
    lp.loop_type = t.clone();
    if t != "none" {
        let mut d = lp.duration.unwrap_or(0.0);
        ui.horizontal(|ui| { ui.label(tr("anim.loop.duration")); num_range(ui, &mut d, 0.05, 0.0, 20.0); });
        lp.duration = if d > 0.0 { Some(d) } else { None };
        el.animation.loop_spec = Some(lp);
    } else {
        el.animation.loop_spec = None;
    }
    layers_motion(ui, el);
}

/// Ruch wszystkich warstw w jednym miejscu: powiązania „po warstwie” (np. OUT tła po OUT tekstu) i kopiowanie ruchu.
fn layers_motion(ui: &mut Ui, el: &mut Element) {
    let Some(scene) = el.scene.as_mut() else { return };
    ui.separator();
    heading(ui, tr("anim.layers"));
    ui.label(RichText::new(tr("anim.layers.hint")).small().color(Color32::from_gray(140)));
    let all: Vec<(String, String)> = scene.layers.iter().map(|l| (l.id.clone(), l.name.clone())).collect();
    let mut copy: Option<(String, String)> = None;
    for l in scene.layers.iter_mut().rev() {
        let peers: Vec<(String, String)> = all.iter().filter(|p| p.0 != l.id).cloned().collect();
        let m = &l.motion;
        let summary: Vec<&str> = [("IN", m.motion_in.is_some()), ("OUT", m.out.is_some()), ("T-IN", m.text.is_some()), ("T-OUT", m.text_out.is_some())].iter().filter(|x| x.1).map(|x| x.0).collect();
        let title = format!("{} {}  {}", l.layer_type.icon(), l.name, if summary.is_empty() { String::new() } else { format!("[{}]", summary.join(" "))});
        egui::CollapsingHeader::new(title).id_salt(format!("lm_{}", l.id)).show(ui, |ui| {
            ui.push_id(&l.id, |ui| {
                motion_props(ui, &mut l.motion, &peers);
                ui.horizontal(|ui| {
                    ui.label(tr("anim.copy_motion"));
                    let mut to = String::new();
                    egui::ComboBox::from_id_salt("dup").selected_text(tr("anim.choose")).width(120.0).show_ui(ui, |ui| {
                        for (id, n) in &peers { ui.selectable_value(&mut to, id.clone(), n); }
                    });
                    if !to.is_empty() { copy = Some((l.id.clone(), to)); }
                });
            });
        });
    }
    if let Some((from, to)) = copy {
        if let Some(mut m) = scene.layers.iter().find(|l| l.id == from).map(|l| l.motion.clone()) {
            // powiązanie warstwy ze sobą nie ma sensu
            for e in [m.motion_in.as_mut().map(|s| &mut s.extra), m.out.as_mut().map(|s| &mut s.extra), m.text.as_mut().map(|s| &mut s.extra), m.text_out.as_mut().map(|s| &mut s.extra)].into_iter().flatten() {
                if crate::links::after(e) == Some(to.as_str()) { crate::links::set_after(e, None); }
            }
            if let Some(dst) = scene.layers.iter_mut().find(|l| l.id == to) { dst.motion = m; }
        }
    }
}

// ── Code tab ────────────────────────────────────────────────────────────────

fn code_tab(ui: &mut Ui, el: &mut Element) {
    let editable = el.scene.as_ref().map_or(true, |s| s.detached);
    ui.label(RichText::new(if editable { tr("code.editable") } else { tr("code.readonly") }).small().color(Color32::from_gray(150)));
    for (title, text) in [("HTML", &mut el.html_template), ("CSS", &mut el.css_template), ("JS", &mut el.js_template)] {
        ui.collapsing(title, |ui| {
            ui.add_enabled(editable, egui::TextEdit::multiline(text).code_editor().desired_width(f32::INFINITY).desired_rows(10));
        });
    }
}
