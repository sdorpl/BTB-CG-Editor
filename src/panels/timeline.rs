//! Oś czasu: paski presetów (IN / OUT / pętla / tekst) oraz klucze użytkownika (`Layer.keys`) z
//! zaznaczaniem, przeciąganiem, dodawaniem, usuwaniem i edycją interpolacji (Easy Ease, prędkość / wpływ).
use crate::compile;
use crate::i18n::{tr, trf};
use crate::model::*;
use crate::keyconv::{self, Scope};
use crate::motion_bridge::{ELEMENT, KEY_PROPS};
use crate::playback::{self, Phase, Plan, Playback};
use btb_motion::{Channel, Ease, Handle, PathChannel};
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

const ROW_H: f32 = 22.0;
const LABEL_W: f32 = 150.0;
/// Szerokość strefy "hold" (Stop) na osi czasu, w sekundach osi.
const HOLD: f32 = 1.0;
/// Siatka przyciągania czasu kluczy (klatka przy 60 fps); Alt wyłącza.
const SNAP: f32 = 1.0 / 60.0;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyRef {
    pub layer: String,
    pub prop: String,
    pub idx: usize,
}

struct DragState {
    anchor: f32,
    /// (warstwa, właściwość, migawka kanału z początku przeciągania, indeksy zaznaczonych w migawce)
    snaps: Vec<(String, String, Channel, Vec<usize>)>,
    /// to samo dla ścieżki ruchu warstwy (klucze `prop == PATH`)
    path_snaps: Vec<(String, PathChannel, Vec<usize>)>,
}

/// Nazwa "właściwości" kluczy ścieżki ruchu w `KeyRef` (kanały kluczy mają nazwy z `KEY_PROPS`).
pub const PATH: &str = "path";

#[derive(Default)]
pub struct TimelineUi {
    pub sel: Vec<KeyRef>,
    /// Okno edytora krzywych (src/panels/graph.rs).
    pub graph_open: bool,
    /// Komunikat z ostatniej konwersji presetów na klucze (pasek nad osią).
    pub message: Option<String>,
    drag: Option<DragState>,
    /// Środki rombów kluczy z ostatniej klatki (testy interakcji klikają w te pozycje).
    pub key_pos: Vec<(KeyRef, Pos2)>,
}

impl TimelineUi {
    pub fn has_key_selection(&self) -> bool { !self.sel.is_empty() }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Interp { Linear, Easy, EasyIn, EasyOut, Auto, Hold }

enum Act {
    SelectLayer(String),
    AddChannel { layer: String, prop: &'static str, t: f32 },
    AddPath { layer: String, t: f32 },
    Convert(Scope),
    RemoveChannel { layer: String, prop: String },
    InsertKey { layer: String, prop: String, t: f32 },
    Interp { r: KeyRef, kind: Interp },
    Continuous { r: KeyRef, on: bool },
    DeleteSelected,
}

fn bar(p: &egui::Painter, x0: f32, x1: f32, y: f32, col: Color32, label: &str) {
    let r = Rect::from_min_max(Pos2::new(x0, y + 3.0), Pos2::new(x1.max(x0 + 3.0), y + ROW_H - 3.0));
    p.rect_filled(r, 3.0, col);
    if r.width() > 26.0 {
        p.text(r.left_center() + Vec2::new(4.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::monospace(9.0), Color32::BLACK);
    }
}

/// Etykiety wszystkich właściwości, które mogą mieć klucze (także te z konwersji presetów).
pub(crate) fn prop_label(prop: &str) -> &str {
    match prop {
        "tx" => tr("prop.tx"), "ty" => tr("prop.ty"), "sx" => tr("prop.sx"), "sy" => tr("prop.sy"), "rot" => tr("prop.rot"), "opacity" => tr("prop.opacity"),
        "path" => tr("prop.path"),
        "skew" => tr("prop.skew"),
        "blur" => tr("prop.blur"),
        "ox" => tr("prop.ox"),
        "oy" => tr("prop.oy"),
        "cl" => tr("prop.cl"),
        "ct" => tr("prop.ct"),
        "cr" => tr("prop.cr"),
        "cb" => tr("prop.cb"),
        "fx" => tr("prop.fx"),
        "fy" => tr("prop.fy"),
        "sh" => tr("prop.sh"),
        "dg" => tr("prop.dg"),
        "dr" => tr("prop.dr"),
        "rgb" => tr("prop.rgb"),
        _ => prop,
    }
}
fn prop_neutral(prop: &str) -> f32 {
    if let Some((_, _, n)) = KEY_PROPS.iter().find(|(p, _, _)| *p == prop) { return *n; }
    match prop { "ox" | "oy" => 0.5, "cr" | "cb" | "fx" | "fy" | "sh" => 1.0, "dg" => -1.0, _ => 0.0 }
}

fn is_curve(h: Handle) -> bool { !matches!(h, Handle::Linear | Handle::Hold) }

/// Ikona klucza jak w AE: romb (prosty), koło (krzywa / ease), kwadrat (hold).
fn draw_key(p: &egui::Painter, c: Pos2, k: &btb_motion::Key, selected: bool) {
    let fill = if selected { Color32::from_rgb(251, 191, 36) } else { Color32::from_rgb(210, 214, 224) };
    let stroke = Stroke::new(1.0, if selected { Color32::WHITE } else { Color32::from_gray(40) });
    let r = 5.0;
    if k.right == Handle::Hold {
        p.rect(Rect::from_center_size(c, Vec2::splat(r * 1.7)), 1.0, fill, stroke);
    } else if is_curve(k.left) || is_curve(k.right) {
        p.circle(c, r, fill, stroke);
    } else {
        p.add(Shape::convex_polygon(vec![c + Vec2::new(0.0, -r - 1.0), c + Vec2::new(r + 1.0, 0.0), c + Vec2::new(0.0, r + 1.0), c + Vec2::new(-r - 1.0, 0.0)], fill, stroke));
    }
}

fn apply_interp(k: &mut btb_motion::Key, kind: Interp) {
    match kind {
        Interp::Linear => { k.left = Handle::Linear; k.right = Handle::Linear; }
        Interp::Easy => { k.left = Handle::easy(); k.right = Handle::easy(); }
        Interp::EasyIn => k.left = Handle::easy(),
        Interp::EasyOut => k.right = Handle::easy(),
        Interp::Auto => { k.left = Handle::Auto; k.right = Handle::Auto; }
        Interp::Hold => { k.left = Handle::Linear; k.right = Handle::Hold; }
    }
}

pub(crate) fn channel_mut<'a>(scene: &'a mut Scene, layer: &str, prop: &str) -> Option<&'a mut Channel> {
    scene.key_channels_mut(layer)?.iter_mut().find(|c| c.prop == prop)
}

/// Czas osi (sekundy Directora) odpowiadający stanowi odtwarzacza.
fn head_time(pb: &Playback, plan: &Plan) -> Option<f32> {
    match pb.phase {
        Phase::In => Some(pb.t),
        Phase::Hold => Some(plan.in_total),
        Phase::Out => Some(plan.in_total + pb.t),
        Phase::Idle => None,
    }
}

pub fn draw_timeline(ui: &mut Ui, el: &mut Element, plan: &Plan, pb: &mut Playback, selected: &mut Vec<String>, tl: &mut TimelineUi) {
    if el.scene.is_none() {
        ui.label(tr("timeline.no_scene"));
        return;
    }
    let mut acts: Vec<Act> = vec![];
    tl.key_pos.clear();

    // ── pasek szczegółów: Stop i edycja zaznaczonego klucza ──
    detail_strip(ui, el, plan, tl);

    let t_in = plan.in_total;
    let total = t_in + HOLD + plan.out_total;
    let disp = |t: f32| if t <= t_in { t } else { t + HOLD };
    let inv = |u: f32| if u <= t_in { u } else if u < t_in + HOLD { t_in } else { u - HOLD };

    let (n_rows, layer_ids) = {
        let scene = el.scene.as_ref().unwrap();
        (1 + scene.keys.len() + scene.layers.iter().map(|l| 1 + l.keys.len() + usize::from(l.path.is_some())).sum::<usize>(), scene.layers.iter().rev().map(|l| l.id.clone()).collect::<Vec<_>>())
    };
    let height = 18.0 + n_rows as f32 * ROW_H + 4.0;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        let track = Rect::from_min_max(Pos2::new(rect.min.x + LABEL_W, rect.min.y), rect.max);
        // marginesy po bokach, żeby klucze na brzegach nie były przycięte
        let pad = 8.0;
        let sx = |u: f32| track.min.x + pad + u / total * (track.width() - 2.0 * pad);
        let tx = |t: f32| sx(disp(t));
        let time_at = |x: f32| inv(((x - track.min.x - pad) / (track.width() - 2.0 * pad) * total).clamp(0.0, total));
        let snap = |t: f32, alt: bool| if alt { t } else { (t / SNAP).round() * SNAP };
        p.rect_filled(rect, 0.0, Color32::from_gray(22));

        // strefy IN / OUT i znacznik Stop
        p.rect_filled(Rect::from_min_max(Pos2::new(sx(0.0), rect.min.y + 18.0), Pos2::new(sx(t_in), rect.max.y)), 0.0, Color32::from_rgba_unmultiplied(34, 197, 94, 10));
        p.rect_filled(Rect::from_min_max(Pos2::new(sx(t_in + HOLD), rect.min.y + 18.0), Pos2::new(sx(total), rect.max.y)), 0.0, Color32::from_rgba_unmultiplied(239, 68, 68, 10));
        p.rect_filled(Rect::from_min_max(Pos2::new(sx(t_in), rect.min.y + 18.0), Pos2::new(sx(t_in + HOLD), rect.max.y)), 0.0, Color32::from_rgba_unmultiplied(251, 191, 36, 12));
        p.text(Pos2::new(sx(t_in + HOLD / 2.0), rect.min.y + 20.0), egui::Align2::CENTER_TOP, "STOP", egui::FontId::monospace(9.0), Color32::from_rgb(251, 191, 36));

        // linijka
        p.rect_filled(Rect::from_min_size(Pos2::new(track.min.x, rect.min.y), Vec2::new(track.width(), 18.0)), 0.0, Color32::from_gray(36));
        let step = if total > 8.0 { 1.0 } else { 0.5 };
        let mut t = 0.0;
        while t <= total + 0.001 {
            let x = sx(t);
            p.vline(x, (rect.min.y + 12.0)..=rect.max.y, Stroke::new(1.0, Color32::from_gray(44)));
            let label = if t <= t_in + 0.001 { format!("{:.1}", t) } else if t >= t_in + HOLD { format!("{:.1}", t - HOLD) } else { String::new() };
            p.text(Pos2::new(x + 2.0, rect.min.y + 2.0), egui::Align2::LEFT_TOP, label, egui::FontId::monospace(9.0), Color32::from_gray(130));
            t += step;
        }

        let out_off = t_in + HOLD;
        let alt = ui.input(|i| i.modifiers.alt);
        let shift = ui.input(|i| i.modifiers.shift);

        // wiersz elementu
        let y0 = rect.min.y + 18.0;
        p.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x, y0), Vec2::new(LABEL_W, ROW_H)), 0.0, Color32::from_gray(40));
        p.text(Pos2::new(rect.min.x + 6.0, y0 + ROW_H / 2.0), egui::Align2::LEFT_CENTER, tr("timeline.element_row"), egui::FontId::proportional(11.0), Color32::from_rgb(250, 204, 21));
        let a = el.animation.clone();
        {
            let er = Rect::from_min_size(Pos2::new(rect.min.x, y0), Vec2::new(LABEL_W, ROW_H));
            let eresp = ui.interact(er, ui.id().with("tl-element"), Sense::click());
            eresp.context_menu(|ui| {
                ui.label(tr("timeline.element_keys"));
                let at = head_time(pb, plan).unwrap_or(0.0);
                for (prop, _, _) in KEY_PROPS.iter() {
                    if el.scene.as_ref().is_some_and(|s| s.keys.iter().any(|c| c.prop == *prop)) { continue; }
                    if ui.button(trf("timeline.add_channel", &[("name", &prop_label(prop))])).clicked() { acts.push(Act::AddChannel { layer: ELEMENT.to_string(), prop, t: at }); ui.close_menu(); }
                }
                ui.separator();
                if ui.button(tr("timeline.convert_element")).clicked() { acts.push(Act::Convert(Scope::Element)); ui.close_menu(); }
                if ui.button(tr("timeline.convert_all")).clicked() { acts.push(Act::Convert(Scope::All)); ui.close_menu(); }
            });
        }
        if a.anim_in.anim_type != "none" { bar(&p, sx(a.anim_in.delay), sx(a.anim_in.delay + a.anim_in.duration), y0, Color32::from_rgb(34, 197, 94), &a.anim_in.anim_type); }
        if a.out.anim_type != "none" { bar(&p, sx(out_off + plan.base_out_start), sx(out_off + plan.base_out_start + a.out.duration), y0, Color32::from_rgb(239, 68, 68), &a.out.anim_type); }
        if let Some(l) = &a.loop_spec { if l.loop_type != "none" && !l.loop_type.is_empty() { bar(&p, sx(t_in), sx(t_in + HOLD), y0, Color32::from_rgb(168, 85, 247), &l.loop_type); } }

        // wiersze warstw
        let mut y = y0 + ROW_H;
        let scene = el.scene.as_ref().unwrap();
        let mut key_drag_started: Option<(KeyRef, f32)> = None;
        let mut key_dragged_to: Option<f32> = None;
        let mut key_drag_stopped = false;
        macro_rules! channel_rows {
            ($id:expr, $keys:expr, $path:expr) => {{
            // wiersze kanałów kluczy
            for ch in $keys {
                let row = Rect::from_min_size(Pos2::new(track.min.x, y), Vec2::new(track.width(), ROW_H));
                p.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(LABEL_W, ROW_H)), 0.0, Color32::from_gray(28));
                p.rect_filled(row, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 4));
                p.text(Pos2::new(rect.min.x + 22.0, y + ROW_H / 2.0), egui::Align2::LEFT_CENTER, prop_label(&ch.prop), egui::FontId::proportional(10.5), Color32::from_gray(170));
                p.hline(rect.x_range(), y + ROW_H, Stroke::new(1.0, Color32::from_gray(32)));
                let cresp = ui.interact(Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(LABEL_W, ROW_H)), ui.id().with(("tl-chan", $id, &ch.prop)), Sense::click());
                cresp.context_menu(|ui| { if ui.button(tr("timeline.remove_channel")).clicked() { acts.push(Act::RemoveChannel { layer: $id.to_string(), prop: ch.prop.clone() }); ui.close_menu(); } });
                // dwuklik na torze = nowy klucz
                let trow = ui.interact(row, ui.id().with(("tl-row", $id, &ch.prop)), Sense::click_and_drag());
                if trow.double_clicked() {
                    if let Some(pos) = trow.interact_pointer_pos() {
                        acts.push(Act::InsertKey { layer: $id.to_string(), prop: ch.prop.clone(), t: snap(time_at(pos.x), alt) });
                    }
                }
                // linia łącząca klucze o tej samej wartości (bez zmiany)
                for w in ch.keys.windows(2) {
                    if (w[0].v - w[1].v).abs() < 1e-6 { p.hline(tx(w[0].t)..=tx(w[1].t), y + ROW_H / 2.0, Stroke::new(2.0, Color32::from_rgba_unmultiplied(210, 214, 224, 60))); }
                }
                for (i, k) in ch.keys.iter().enumerate() {
                    let c = Pos2::new(tx(k.t), y + ROW_H / 2.0);
                    let kr = KeyRef { layer: $id.to_string(), prop: ch.prop.clone(), idx: i };
                    let is_sel = tl.sel.contains(&kr);
                    draw_key(&p, c, k, is_sel);
                    tl.key_pos.push((kr.clone(), c));
                    let kresp = ui.interact(Rect::from_center_size(c, Vec2::splat(14.0)), ui.id().with(("tl-key", $id, &ch.prop, i)), Sense::click_and_drag());
                    // etykieta rysowana bez widżetu: podpowiedź (tooltip) przechwytywała naciśnięcia na romb
                    if kresp.hovered() && tl.drag.is_none() {
                        p.text(c + Vec2::new(0.0, -9.0), egui::Align2::CENTER_BOTTOM, format!("{:.2} @ {:.2} s", k.v, k.t), egui::FontId::monospace(9.5), Color32::from_rgb(250, 230, 160));
                    }
                    if kresp.clicked() {
                        if shift { if let Some(pos) = tl.sel.iter().position(|s| *s == kr) { tl.sel.remove(pos); } else { tl.sel.push(kr.clone()); } } else { tl.sel = vec![kr.clone()]; }
                    }
                    if kresp.drag_started() {
                        if !is_sel { if shift { tl.sel.push(kr.clone()); } else { tl.sel = vec![kr.clone()]; } }
                        if let Some(pos) = kresp.interact_pointer_pos() { key_drag_started = Some((kr.clone(), time_at(pos.x))); }
                    }
                    if kresp.dragged() { if let Some(pos) = kresp.interact_pointer_pos() { key_dragged_to = Some(time_at(pos.x)); } }
                    if kresp.drag_stopped() { key_drag_stopped = true; }
                    kresp.context_menu(|ui| {
                        if !tl.sel.contains(&kr) { tl.sel = vec![kr.clone()]; }
                        ui.label(tr("timeline.interp"));
                        for (label, kind) in [(tr("interp.linear"), Interp::Linear), (tr("interp.easy"), Interp::Easy), (tr("interp.easy_in"), Interp::EasyIn), (tr("interp.easy_out"), Interp::EasyOut), (tr("interp.auto"), Interp::Auto), (tr("interp.hold"), Interp::Hold)] {
                            if ui.button(label).clicked() { acts.push(Act::Interp { r: kr.clone(), kind }); ui.close_menu(); }
                        }
                        let mut cont = k.continuous;
                        if ui.checkbox(&mut cont, tr("timeline.continuous")).changed() { acts.push(Act::Continuous { r: kr.clone(), on: cont }); ui.close_menu(); }
                        ui.separator();
                        if ui.button(tr("timeline.delete_key")).clicked() { acts.push(Act::DeleteSelected); ui.close_menu(); }
                    });
                }
                y += ROW_H;
            }

            // wiersz ścieżki ruchu
            if let Some(path) = $path {
                let row = Rect::from_min_size(Pos2::new(track.min.x, y), Vec2::new(track.width(), ROW_H));
                p.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(LABEL_W, ROW_H)), 0.0, Color32::from_gray(28));
                p.rect_filled(row, 0.0, Color32::from_rgba_unmultiplied(120, 180, 255, 8));
                p.text(Pos2::new(rect.min.x + 22.0, y + ROW_H / 2.0), egui::Align2::LEFT_CENTER, prop_label(PATH), egui::FontId::proportional(10.5), Color32::from_rgb(147, 197, 253));
                p.hline(rect.x_range(), y + ROW_H, Stroke::new(1.0, Color32::from_gray(32)));
                let cresp = ui.interact(Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(LABEL_W, ROW_H)), ui.id().with(("tl-chan", $id, PATH)), Sense::click());
                cresp.context_menu(|ui| { if ui.button(tr("timeline.remove_path")).clicked() { acts.push(Act::RemoveChannel { layer: $id.to_string(), prop: PATH.to_string() }); ui.close_menu(); } });
                let trow = ui.interact(row, ui.id().with(("tl-row", $id, PATH)), Sense::click_and_drag());
                if trow.double_clicked() {
                    if let Some(pos) = trow.interact_pointer_pos() { acts.push(Act::InsertKey { layer: $id.to_string(), prop: PATH.to_string(), t: snap(time_at(pos.x), alt) }); }
                }
                for (i, k) in path.keys.iter().enumerate() {
                    let c = Pos2::new(tx(k.t), y + ROW_H / 2.0);
                    let kr = KeyRef { layer: $id.to_string(), prop: PATH.to_string(), idx: i };
                    let is_sel = tl.sel.contains(&kr);
                    let icon = btb_motion::Key { t: k.t, v: 0.0, left: k.left, right: k.right, continuous: k.continuous };
                    draw_key(&p, c, &icon, is_sel);
                    if k.roving { p.circle_stroke(c, 8.0, Stroke::new(1.0, Color32::from_rgb(147, 197, 253))); }
                    tl.key_pos.push((kr.clone(), c));
                    let kresp = ui.interact(Rect::from_center_size(c, Vec2::splat(14.0)), ui.id().with(("tl-key", $id, PATH, i)), Sense::click_and_drag());
                    if kresp.hovered() && tl.drag.is_none() {
                        p.text(c + Vec2::new(0.0, -9.0), egui::Align2::CENTER_BOTTOM, format!("({:.0}, {:.0}) @ {:.2} s", k.x, k.y, k.t), egui::FontId::monospace(9.5), Color32::from_rgb(250, 230, 160));
                    }
                    if kresp.clicked() {
                        if shift { if let Some(pos) = tl.sel.iter().position(|s| *s == kr) { tl.sel.remove(pos); } else { tl.sel.push(kr.clone()); } } else { tl.sel = vec![kr.clone()]; }
                    }
                    if kresp.drag_started() {
                        if !is_sel { if shift { tl.sel.push(kr.clone()); } else { tl.sel = vec![kr.clone()]; } }
                        if let Some(pos) = kresp.interact_pointer_pos() { key_drag_started = Some((kr.clone(), time_at(pos.x))); }
                    }
                    if kresp.dragged() { if let Some(pos) = kresp.interact_pointer_pos() { key_dragged_to = Some(time_at(pos.x)); } }
                    if kresp.drag_stopped() { key_drag_stopped = true; }
                    kresp.context_menu(|ui| {
                        if !tl.sel.contains(&kr) { tl.sel = vec![kr.clone()]; }
                        ui.label(tr("timeline.interp_time"));
                        for (label, kind) in [(tr("interp.linear_const"), Interp::Linear), (tr("interp.easy"), Interp::Easy), (tr("interp.easy_in"), Interp::EasyIn), (tr("interp.easy_out"), Interp::EasyOut), (tr("interp.hold"), Interp::Hold)] {
                            if ui.button(label).clicked() { acts.push(Act::Interp { r: kr.clone(), kind }); ui.close_menu(); }
                        }
                        ui.separator();
                        if ui.button(tr("timeline.delete_key")).clicked() { acts.push(Act::DeleteSelected); ui.close_menu(); }
                    });
                }
                y += ROW_H;
            }
            }};
        }

        // klucze całego elementu (cel "element") pod wierszem elementu
        let el_id = ELEMENT.to_string();
        channel_rows!(&el_id, &scene.keys, &None::<PathChannel>);
        for lid in &layer_ids {
            let Some(l) = scene.layers.iter().find(|l| &l.id == lid) else { continue };
            let sel = selected.contains(&l.id);
            let lr = Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(LABEL_W, ROW_H));
            p.rect_filled(lr, 0.0, if sel { Color32::from_rgb(40, 60, 100) } else { Color32::from_gray(32) });
            p.text(lr.left_center() + Vec2::new(6.0, 0.0), egui::Align2::LEFT_CENTER, format!("{} {}", l.layer_type.icon(), l.name), egui::FontId::proportional(11.0), if l.visible { Color32::from_gray(210) } else { Color32::from_gray(100) });
            p.hline(rect.x_range(), y + ROW_H, Stroke::new(1.0, Color32::from_gray(36)));
            let lresp = ui.interact(lr, ui.id().with(("tl-layer", &l.id)), Sense::click());
            if lresp.clicked() { acts.push(Act::SelectLayer(l.id.clone())); }
            lresp.context_menu(|ui| {
                ui.label(tr("timeline.add_channel_title"));
                let at = head_time(pb, plan).unwrap_or(0.0);
                for (prop, _, _) in KEY_PROPS.iter() {
                    if l.keys.iter().any(|c| c.prop == *prop) { continue; }
                    if ui.button(prop_label(prop)).clicked() { acts.push(Act::AddChannel { layer: l.id.clone(), prop, t: at }); ui.close_menu(); }
                }
                ui.separator();
                if ui.button(tr("timeline.convert_layer")).clicked() { acts.push(Act::Convert(Scope::Layer(l.id.clone()))); ui.close_menu(); }
                if l.path.is_none() {
                    if ui.button(tr("timeline.add_path")).clicked() { acts.push(Act::AddPath { layer: l.id.clone(), t: at }); ui.close_menu(); }
                }
            });

            let (g_in, g_out) = compile::group_motion_pair(scene, l);
            let m_in = l.motion.motion_in.clone().or(g_in);
            let m_out = l.motion.out.clone().or(g_out);
            if let Some(m) = m_in.filter(|m| m.anim_type != "none") { bar(&p, sx(m.delay), sx(m.delay + m.duration), y, Color32::from_rgb(34, 197, 94), &m.anim_type); }
            if matches!(l.layer_type, LayerType::Text | LayerType::Clock) {
                let text = l.text.clone().filter(|t| !t.is_empty()).or(l.default_text.clone()).unwrap_or_default();
                if let Some((te, own)) = playback::resolve_text(&l.motion.text, &a.text) {
                    let start = if own { 0.0 } else if a.extra.get("textSync").and_then(|v| v.as_bool()).unwrap_or(false) { (plan.base_end - 0.1).max(0.0) } else { 0.0 };
                    let end = start + playback::text_total(te, &text, None);
                    bar(&p, sx(start + te.delay), sx(end), y, Color32::from_rgb(45, 212, 191), &te.effect_type);
                }
                if let Some((te, own)) = playback::resolve_text(&l.motion.text_out, &a.text_out) {
                    let start = out_off + if own { 0.0 } else { plan.pre_dur };
                    bar(&p, sx(start + te.delay), sx(start + playback::text_total(te, &text, None)), y, Color32::from_rgb(251, 146, 60), &te.effect_type);
                }
            }
            if let Some(m) = m_out.filter(|m| m.anim_type != "none") { bar(&p, sx(out_off + m.delay), sx(out_off + m.delay + m.duration), y, Color32::from_rgb(239, 68, 68), &m.anim_type); }
            y += ROW_H;

            channel_rows!(&l.id, &l.keys, &l.path);
        }

        // przeciąganie kluczy: liczone z migawki z początku przeciągania
        if let Some((kr, anchor)) = key_drag_started {
            let mut snaps = vec![];
            for (layer, prop) in tl.sel.iter().filter(|s| s.prop != PATH).map(|s| (s.layer.clone(), s.prop.clone())).collect::<std::collections::BTreeSet<_>>() {
                if let Some(ch) = scene.key_channels(&layer).and_then(|ks| ks.iter().find(|c| c.prop == prop)) {
                    let idx: Vec<usize> = tl.sel.iter().filter(|s| s.layer == layer && s.prop == prop).map(|s| s.idx).collect();
                    snaps.push((layer, prop, ch.clone(), idx));
                }
            }
            let _ = kr;
            let mut path_snaps = vec![];
            for layer in tl.sel.iter().filter(|s| s.prop == PATH).map(|s| s.layer.clone()).collect::<std::collections::BTreeSet<_>>() {
                if let Some(path) = scene.layers.iter().find(|l| l.id == layer).and_then(|l| l.path.as_ref()) {
                    let idx: Vec<usize> = tl.sel.iter().filter(|s| s.layer == layer && s.prop == PATH).map(|s| s.idx).collect();
                    path_snaps.push((layer, path.clone(), idx));
                }
            }
            tl.drag = Some(DragState { anchor, snaps, path_snaps });
        }
        let drag_update = match (&tl.drag, key_dragged_to) {
            (Some(d), Some(cur)) => Some((d.anchor, cur)),
            _ => None,
        };
        if key_drag_stopped { tl.drag = None; }

        // playhead
        if let Some(h) = head_time(pb, plan).map(|t| if pb.phase == Phase::Hold { t_in + pb.t.min(HOLD) } else { disp(t) }) {
            p.vline(sx(h), rect.min.y..=rect.max.y, Stroke::new(1.5, Color32::from_rgb(251, 191, 36)));
        }

        // scrub po tle (przeciąganie klucza ma pierwszeństwo)
        if tl.drag.is_none() && drag_update.is_none() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if resp.dragged() && pos.x >= track.min.x {
                    let u = ((pos.x - track.min.x - pad) / (track.width() - 2.0 * pad) * total).clamp(0.0, total);
                    pb.playing = false;
                    pb.auto_cycle = false;
                    if u <= t_in { pb.phase = Phase::In; pb.t = u; }
                    else if u <= out_off { pb.phase = Phase::Hold; pb.t = u - t_in; pb.loop_t = pb.t; }
                    else { pb.phase = Phase::Out; pb.t = (u - out_off).min(plan.out_total); }
                }
                if resp.clicked() && pos.x >= track.min.x && !shift { tl.sel.clear(); }
            }
        }

        if let Some((anchor, cur)) = drag_update {
            let dt = snap(cur - anchor, alt);
            if let Some(d) = &tl.drag {
                let snaps: Vec<_> = d.snaps.iter().map(|(l, pr, ch, idx)| (l.clone(), pr.clone(), ch.clone(), idx.clone())).collect();
                let path_snaps: Vec<_> = d.path_snaps.iter().map(|(l, p, idx)| (l.clone(), p.clone(), idx.clone())).collect();
                let scene = el.scene.as_mut().unwrap();
                let mut new_sel: Vec<KeyRef> = tl.sel.iter().filter(|s| if s.prop == PATH { !path_snaps.iter().any(|(l, _, _)| *l == s.layer) } else { !snaps.iter().any(|(l, pr, _, _)| *l == s.layer && *pr == s.prop) }).cloned().collect();
                for (layer, snapshot, idx) in path_snaps {
                    let (moved, sel_idx) = snapshot.move_keys(&idx, dt);
                    if let Some(l) = scene.layers.iter_mut().find(|l| l.id == layer) { l.path = Some(moved); }
                    new_sel.extend(sel_idx.into_iter().map(|i| KeyRef { layer: layer.clone(), prop: PATH.to_string(), idx: i }));
                }
                for (layer, prop, snapshot, idx) in snaps {
                    let (moved, sel_idx) = snapshot.move_keys(&idx, dt);
                    if let Some(ch) = channel_mut(scene, &layer, &prop) { *ch = moved; }
                    new_sel.extend(sel_idx.into_iter().map(|i| KeyRef { layer: layer.clone(), prop: prop.clone(), idx: i }));
                }
                tl.sel = new_sel;
            }
        }
    });

    // ── skutki akcji z menu i kliknięć ──
    let delete_pressed = ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete_pressed && tl.has_key_selection() { acts.push(Act::DeleteSelected); }
    // konwersje presetów zmieniają cały element (nie tylko scenę), więc przed pożyczeniem sceny
    let (converts, acts): (Vec<Act>, Vec<Act>) = acts.into_iter().partition(|a| matches!(a, Act::Convert(_)));
    for act in converts {
        if let Act::Convert(scope) = act {
            let r = keyconv::convert_presets(el, scope);
            tl.message = Some(if r.channels == 0 { tr("timeline.nothing_scope").to_string() } else {
                format!("{}{}{}", trf("timeline.converted", &[("channels", &r.channels), ("keys", &r.keys)]), if r.layers > 0 { trf("timeline.converted_layers", &[("n", &r.layers)]) } else { String::new() },
                    if r.skipped.is_empty() { String::new() } else { trf("timeline.converted_skipped", &[("names", &r.skipped.join(", "))]) })
            });
            tl.sel.clear();
        }
    }
    let scene = el.scene.as_mut().unwrap();
    for act in acts {
        match act {
            Act::SelectLayer(id) => *selected = vec![id],
            Act::AddChannel { layer, prop, t } => {
                if let Some(keys) = scene.key_channels_mut(&layer) {
                    if !keys.iter().any(|c| c.prop == prop) {
                        let mut c = Channel::new(prop);
                        c.insert_key(t, prop_neutral(prop));
                        keys.push(c);
                    }
                }
            }
            Act::Convert(_) => {}
            Act::AddPath { layer, t } => {
                if let Some(l) = scene.layers.iter_mut().find(|l| l.id == layer) {
                    if l.path.is_none() {
                        let mut p = PathChannel::default();
                        p.insert_key(t, Some((0.0, 0.0)));
                        p.insert_key(t + 1.0, Some((160.0, 0.0)));
                        l.path = Some(p);
                    }
                }
            }
            Act::RemoveChannel { layer, prop } => {
                if prop == PATH {
                    if let Some(l) = scene.layers.iter_mut().find(|l| l.id == layer) { l.path = None; }
                } else if let Some(keys) = scene.key_channels_mut(&layer) { keys.retain(|c| c.prop != prop); }
                tl.sel.retain(|s| !(s.layer == layer && s.prop == prop));
            }
            Act::InsertKey { layer, prop, t } if prop == PATH => {
                if let Some(path) = scene.layers.iter_mut().find(|l| l.id == layer).and_then(|l| l.path.as_mut()) {
                    let i = path.insert_key(t, None);
                    tl.sel = vec![KeyRef { layer, prop, idx: i }];
                }
            }
            Act::InsertKey { layer, prop, t } => {
                if let Some(ch) = channel_mut(scene, &layer, &prop) {
                    let v = ch.eval(t).unwrap_or_else(|| prop_neutral(&prop));
                    let i = ch.insert_key(t, v);
                    tl.sel = vec![KeyRef { layer, prop, idx: i }];
                }
            }
            Act::Interp { r, kind } => {
                for s in tl.sel.clone().into_iter().chain(std::iter::once(r)) {
                    if s.prop == PATH {
                        if let Some(k) = scene.layers.iter_mut().find(|l| l.id == s.layer).and_then(|l| l.path.as_mut()).and_then(|p| p.keys.get_mut(s.idx)) {
                            let mut tmp = btb_motion::Key { t: k.t, v: 0.0, left: k.left, right: k.right, continuous: k.continuous };
                            apply_interp(&mut tmp, kind);
                            (k.left, k.right) = (tmp.left, tmp.right);
                        }
                        continue;
                    }
                    if let Some(k) = channel_mut(scene, &s.layer, &s.prop).and_then(|c| c.keys.get_mut(s.idx)) { apply_interp(k, kind); }
                }
            }
            Act::Continuous { r, on } => {
                for s in tl.sel.clone().into_iter().chain(std::iter::once(r)) {
                    if s.prop == PATH { continue; }
                    if let Some(k) = channel_mut(scene, &s.layer, &s.prop).and_then(|c| c.keys.get_mut(s.idx)) { k.continuous = on; }
                }
            }
            Act::DeleteSelected => {
                let sel = std::mem::take(&mut tl.sel);
                let mut groups: std::collections::BTreeMap<(String, String), Vec<usize>> = Default::default();
                for s in sel { groups.entry((s.layer, s.prop)).or_default().push(s.idx); }
                for ((layer, prop), idx) in groups {
                    if prop == PATH {
                        if let Some(l) = scene.layers.iter_mut().find(|l| l.id == layer) { if let Some(p) = l.path.as_mut() { p.remove_keys(&idx); } }
                    } else if let Some(ch) = channel_mut(scene, &layer, &prop) { ch.remove_keys(&idx); }
                }
                scene.keys.retain(|c| !c.keys.is_empty());
                for l in scene.layers.iter_mut() {
                    l.keys.retain(|c| !c.keys.is_empty());
                    if l.path.as_ref().is_some_and(|p| p.keys.is_empty()) { l.path = None; }
                }
            }
        }
    }
}

/// Pasek nad osią: pozycja Stop oraz edycja zaznaczonego klucza (czas, wartość, prędkość i wpływ obu stron).
fn detail_strip(ui: &mut Ui, el: &mut Element, plan: &Plan, tl: &mut TimelineUi) {
    ui.horizontal_wrapped(|ui| {
        let mut stop = el.animation.extra.get("stopAt").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(plan.in_total);
        let before = stop;
        ui.label(tr("timeline.stop"));
        ui.add(egui::DragValue::new(&mut stop).speed(0.02).range(0.05..=60.0).suffix(" s")).on_hover_text(tr("timeline.stop.hover"));
        if (stop - before).abs() > 1e-6 { el.animation.extra.insert("stopAt".into(), serde_json::Value::from(stop)); }
        if el.animation.extra.contains_key("stopAt") && ui.small_button(tr("timeline.auto")).on_hover_text(tr("timeline.auto.hover")).clicked() { el.animation.extra.remove("stopAt"); }
        if ui.button(tr("timeline.presets_to_keys")).on_hover_text(tr("timeline.presets_to_keys.hover")).clicked() {
            let r = keyconv::convert_presets(el, Scope::All);
            tl.message = Some(if r.channels == 0 { tr("timeline.nothing").to_string() } else { format!("{}{}", trf("timeline.converted", &[("channels", &r.channels), ("keys", &r.keys)]), trf("timeline.converted_layers", &[("n", &r.layers)])) });
            tl.sel.clear();
        }
        if let Some(m) = &tl.message { ui.weak(m.clone()); }
        ui.checkbox(&mut tl.graph_open, tr("timeline.graph")).on_hover_text(tr("timeline.graph.hover"));
        ui.separator();
        let Some(r) = (tl.sel.len() == 1).then(|| tl.sel[0].clone()) else {
            ui.weak(if tl.sel.is_empty() { tr("timeline.hint") } else { tr("timeline.hint_multi") });
            return;
        };
        let Some(scene) = el.scene.as_mut() else { return };
        if r.prop == PATH {
            path_detail(ui, scene, tl, &r);
            return;
        }
        let Some(ch) = channel_mut(scene, &r.layer, &r.prop) else { return };
        let Some(k) = ch.keys.get(r.idx).cloned() else { return };
        ui.label(egui::RichText::new(prop_label(&r.prop)).strong());
        let (mut t, mut v) = (k.t, k.v);
        ui.add(egui::DragValue::new(&mut t).speed(0.01).range(0.0..=120.0).prefix("t ").suffix(" s"));
        ui.add(egui::DragValue::new(&mut v).speed(if r.prop.starts_with('s') || r.prop == "opacity" { 0.01 } else { 0.5 }).prefix(tr("timeline.value_prefix")));
        if (t - k.t).abs() > 1e-6 {
            let (moved, sel) = ch.move_keys(&[r.idx], t - k.t);
            *ch = moved;
            tl.sel = sel.into_iter().map(|i| KeyRef { layer: r.layer.clone(), prop: r.prop.clone(), idx: i }).collect();
            return;
        }
        if (v - k.v).abs() > 1e-9 { ch.keys[r.idx].v = v; }
        for (out, label) in [(false, tr("timeline.in_short")), (true, tr("timeline.out_short"))] {
            match ch.side_ease(r.idx, out) {
                Some(e) => {
                    let mut e = e;
                    let before = e;
                    ui.label(label);
                    ui.add(egui::DragValue::new(&mut e.speed).speed(1.0).suffix(" /s")).on_hover_text(tr("timeline.speed.hover"));
                    let mut pct = e.influence * 100.0;
                    ui.add(egui::DragValue::new(&mut pct).speed(0.5).range(0.0..=100.0).suffix(" %")).on_hover_text(tr("timeline.influence.hover"));
                    e.influence = pct / 100.0;
                    if e != before { ch.set_ease(r.idx, out, Ease { speed: e.speed, influence: e.influence }); }
                }
                None => { ui.weak(trf("timeline.hold_side", &[("side", &label)])); }
            }
        }
        let mut cont = k.continuous;
        if ui.checkbox(&mut cont, tr("timeline.continuous_short")).on_hover_text(tr("timeline.continuous_short.hover")).changed() { ch.keys[r.idx].continuous = cont; }
    });
}

/// Edycja zaznaczonego klucza ścieżki ruchu: czas, pozycja, auto-Bézier, roving, prędkość i wpływ obu stron.
fn path_detail(ui: &mut Ui, scene: &mut Scene, tl: &mut TimelineUi, r: &KeyRef) {
    let Some(path) = scene.layers.iter_mut().find(|l| l.id == r.layer).and_then(|l| l.path.as_mut()) else { return };
    let Some(k) = path.keys.get(r.idx).cloned() else { return };
    ui.label(egui::RichText::new(prop_label(PATH)).strong());
    let (mut t, mut x, mut y) = (k.t, k.x, k.y);
    ui.add(egui::DragValue::new(&mut t).speed(0.01).range(0.0..=120.0).prefix("t ").suffix(" s"));
    ui.add(egui::DragValue::new(&mut x).speed(0.5).prefix("x "));
    ui.add(egui::DragValue::new(&mut y).speed(0.5).prefix("y "));
    if (t - k.t).abs() > 1e-6 {
        let (moved, sel) = path.move_keys(&[r.idx], t - k.t);
        *path = moved;
        tl.sel = sel.into_iter().map(|i| KeyRef { layer: r.layer.clone(), prop: PATH.to_string(), idx: i }).collect();
        return;
    }
    if (x - k.x).abs() > 1e-9 || (y - k.y).abs() > 1e-9 {
        path.keys[r.idx].x = x;
        path.keys[r.idx].y = y;
    }
    let mut auto = k.auto;
    if ui.checkbox(&mut auto, tr("timeline.auto_bezier")).on_hover_text(tr("timeline.auto_bezier.hover")).changed() {
        if auto { path.keys[r.idx].auto = true; } else {
            let (tin, tout) = path.tangents(r.idx);
            let key = &mut path.keys[r.idx];
            key.auto = false;
            key.tin = tin;
            key.tout = tout;
        }
    }
    let mut rove = k.roving;
    if ui.checkbox(&mut rove, "roving").on_hover_text(tr("timeline.roving.hover")).changed() {
        path.keys[r.idx].roving = rove;
        path.retime_roving();
    }
    // prędkość wzdłuż ścieżki: uchwyty czasowe edytujemy na kanale odległości i przepisujemy do kluczy
    let mut dist = path.distance_channel();
    let mut changed = false;
    for (out, label) in [(false, tr("timeline.in_short")), (true, tr("timeline.out_short"))] {
        match dist.side_ease(r.idx, out) {
            Some(e) => {
                let mut e = e;
                let before = e;
                ui.label(label);
                ui.add(egui::DragValue::new(&mut e.speed).speed(1.0).suffix(" px/s"));
                let mut pct = e.influence * 100.0;
                ui.add(egui::DragValue::new(&mut pct).speed(0.5).range(0.0..=100.0).suffix(" %"));
                e.influence = pct / 100.0;
                if e != before { dist.set_ease(r.idx, out, Ease { speed: e.speed, influence: e.influence }); changed = true; }
            }
            None => { ui.weak(trf("timeline.hold_side", &[("side", &label)])); }
        }
    }
    if changed { path.apply_distance_handles(&dist); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton};

    struct Harness {
        ctx: egui::Context,
        el: Element,
        pb: Playback,
        sel: Vec<String>,
        tl: TimelineUi,
        time: f64,
        shift: bool,
    }

    impl Harness {
        fn new() -> Self {
            let mut el = Element::new_typed("t", "BADGE");
            el.animation.anim_in = AnimSpec { anim_type: "fade".into(), duration: 0.5, ..Default::default() };
            el.animation.out = AnimSpec { anim_type: "fade".into(), duration: 0.5, ..Default::default() };
            el.animation.loop_spec = None;
            el.animation.text = None;
            el.animation.text_out = None;
            let s = el.scene.as_mut().unwrap();
            for l in s.layers.iter_mut() { l.motion = Motion::default(); }
            let mut ch = Channel::new("tx");
            ch.keys = vec![btb_motion::Key::linear(0.2, 0.0), btb_motion::Key::linear(0.9, 80.0)];
            s.layers[0].keys = vec![ch];
            let mut h = Self { ctx: egui::Context::default(), el, pb: Playback::default(), sel: vec![], tl: TimelineUi::default(), time: 0.0, shift: false };
            h.frame(vec![]);
            h.frame(vec![]);
            h
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 0.05;
            let mut raw = egui::RawInput::default();
            raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 520.0)));
            raw.time = Some(self.time);
            raw.events = events;
            raw.modifiers.shift = self.shift;
            let scene = self.el.scene.clone().unwrap();
            let base = String::new();
            let vals = crate::render::Values { title: &self.el.title, fields: &self.el.fields, extra: &self.el.extra, base_url: &base };
            let plan = playback::plan(&self.el, &scene, &vals);
            let Self { el, pb, sel, tl, ctx, .. } = self;
            let _ = ctx.run(raw, |ctx| { egui::CentralPanel::default().show(ctx, |ui| draw_timeline(ui, el, &plan, pb, sel, tl)); });
        }

        fn keys(&self) -> Vec<f32> { self.el.scene.as_ref().unwrap().layers[0].keys.first().map(|c| c.keys.iter().map(|k| k.t).collect()).unwrap_or_default() }
        fn pos(&self, idx: usize) -> Pos2 { self.tl.key_pos.iter().find(|(r, _)| r.idx == idx).map(|(_, p)| *p).expect("klucz widoczny") }
        fn mv(&mut self, p: Pos2) { self.frame(vec![Event::PointerMoved(p)]); }
        fn press(&mut self, p: Pos2, shift: bool) {
            self.shift = shift;
            let m = Modifiers { shift, ..Default::default() };
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m }]);
        }
        fn release(&mut self, p: Pos2) {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() }]);
        }
        fn click(&mut self, p: Pos2) { self.mv(p); self.press(p, false); self.release(p); }
        fn key(&mut self, key: egui::Key) {
            for pressed in [true, false] {
                self.frame(vec![Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: Modifiers::default() }]);
            }
        }
    }

    #[test]
    fn click_selects_and_drag_moves_keys_with_snapping() {
        let mut h = Harness::new();
        let p0 = h.pos(0);
        h.click(p0);
        assert_eq!(h.tl.sel.len(), 1);
        // przeciągnięcie klucza 0 w prawo
        h.mv(p0);
        h.press(p0, false);
        h.mv(p0 + Vec2::new(30.0, 0.0));
        h.mv(p0 + Vec2::new(90.0, 0.0));
        h.release(p0 + Vec2::new(90.0, 0.0));
        let t = h.keys();
        assert_eq!(t.len(), 2);
        assert!(t[0] > 0.25 && t[0] < t[1], "klucz przesunięty w prawo, kolejność zachowana: {t:?}");
        let on_grid = (t[0] / SNAP).round() * SNAP;
        assert!((t[0] - on_grid).abs() < 1e-4, "czas przyciągnięty do klatki: {}", t[0]);
        assert_eq!(h.tl.sel.len(), 1);
        assert!(h.tl.drag.is_none(), "przeciąganie zakończone");
    }

    #[test]
    fn drag_past_a_neighbour_keeps_selection_on_the_moved_key() {
        let mut h = Harness::new();
        let p0 = h.pos(0);
        h.mv(p0);
        h.press(p0, false);
        h.mv(p0 + Vec2::new(60.0, 0.0));
        h.mv(p0 + Vec2::new(700.0, 0.0));
        h.release(p0 + Vec2::new(700.0, 0.0));
        let t = h.keys();
        assert_eq!(t.len(), 2);
        assert!(t[0] < t[1] && (t[1] - 0.2).abs() > 0.3, "klucz minął sąsiada: {t:?}");
        // zaznaczenie wskazuje przesunięty klucz (po zmianie kolejności ma indeks 1), wartość 0 (jego)
        assert_eq!(h.tl.sel.len(), 1);
        let i = h.tl.sel[0].idx;
        assert_eq!(h.el.scene.as_ref().unwrap().layers[0].keys[0].keys[i].v, 0.0);
    }

    #[test]
    fn double_click_on_track_inserts_a_key() {
        let mut h = Harness::new();
        let (a, b) = (h.pos(0), h.pos(1));
        let mid = Pos2::new((a.x + b.x) / 2.0, a.y);
        h.mv(mid);
        for _ in 0..2 {
            h.press(mid, false);
            h.release(mid);
        }
        let t = h.keys();
        assert_eq!(t.len(), 3, "dwuklik dodał klucz: {t:?}");
        assert!(t[1] > t[0] && t[1] < t[2]);
        // wartość nowego klucza leży na krzywej między sąsiadami
        let v = h.el.scene.as_ref().unwrap().layers[0].keys[0].keys[1].v;
        assert!(v > 5.0 && v < 75.0, "wartość z krzywej: {v}");
    }

    #[test]
    fn delete_removes_selected_keys_then_the_empty_channel() {
        let mut h = Harness::new();
        let p0 = h.pos(0);
        h.click(p0);
        h.key(egui::Key::Delete);
        assert_eq!(h.keys().len(), 1);
        assert!(h.tl.sel.is_empty());
        let p = h.pos(0);
        h.click(p);
        h.key(egui::Key::Delete);
        assert!(h.el.scene.as_ref().unwrap().layers[0].keys.is_empty(), "pusty kanał usunięty");
    }

    #[test]
    fn shift_click_extends_selection_and_drag_moves_both() {
        let mut h = Harness::new();
        let (p0, p1) = (h.pos(0), h.pos(1));
        h.click(p0);
        h.mv(p1);
        h.press(p1, true);
        h.release(p1);
        h.shift = false;
        assert_eq!(h.tl.sel.len(), 2);
        let before = h.keys();
        h.mv(p1);
        h.press(p1, false);
        h.mv(p1 + Vec2::new(25.0, 0.0));
        h.mv(p1 + Vec2::new(60.0, 0.0));
        h.release(p1 + Vec2::new(60.0, 0.0));
        let after = h.keys();
        let (d0, d1) = (after[0] - before[0], after[1] - before[1]);
        assert!(d0 > 0.0 && (d0 - d1).abs() < 1e-3, "oba klucze przesunięte o to samo: {before:?} -> {after:?}");
    }

    #[test]
    fn interpolation_presets_set_both_sides() {
        let mut k = btb_motion::Key::linear(0.0, 1.0);
        apply_interp(&mut k, Interp::Easy);
        assert_eq!((k.left, k.right), (Handle::easy(), Handle::easy()));
        apply_interp(&mut k, Interp::Hold);
        assert_eq!((k.left, k.right), (Handle::Linear, Handle::Hold));
        apply_interp(&mut k, Interp::EasyIn);
        assert_eq!((k.left, k.right), (Handle::easy(), Handle::Hold));
        apply_interp(&mut k, Interp::Auto);
        assert_eq!((k.left, k.right), (Handle::Auto, Handle::Auto));
        apply_interp(&mut k, Interp::Linear);
        assert_eq!((k.left, k.right), (Handle::Linear, Handle::Linear));
    }

    #[test]
    fn path_row_keys_select_drag_insert_and_delete() {
        let mut h = Harness::new();
        {
            let l = &mut h.el.scene.as_mut().unwrap().layers[0];
            l.keys.clear();
            l.path = Some(PathChannel { keys: vec![btb_motion::PathKey::new(0.2, 0.0, 0.0), btb_motion::PathKey::new(0.9, 80.0, 40.0)] });
        }
        h.frame(vec![]);
        h.frame(vec![]);
        let pk: Vec<_> = h.tl.key_pos.iter().filter(|(r, _)| r.prop == PATH).cloned().collect();
        assert_eq!(pk.len(), 2, "klucze ścieżki widoczne na osi");
        // przeciąganie klucza ścieżki w czasie
        let p0 = pk[0].1;
        h.mv(p0);
        h.press(p0, false);
        h.mv(p0 + Vec2::new(30.0, 0.0));
        h.mv(p0 + Vec2::new(80.0, 0.0));
        h.release(p0 + Vec2::new(80.0, 0.0));
        let t: Vec<f32> = h.el.scene.as_ref().unwrap().layers[0].path.as_ref().unwrap().keys.iter().map(|k| k.t).collect();
        assert!(t[0] > 0.25 && t[0] < t[1], "klucz przesunięty: {t:?}");
        assert!(h.tl.sel.iter().all(|s| s.prop == PATH));
        // dwuklik w wierszu ścieżki wstawia klucz leżący na krzywej
        let (a, b) = {
            let pk: Vec<_> = h.tl.key_pos.iter().filter(|(r, _)| r.prop == PATH).cloned().collect();
            (pk[0].1, pk[1].1)
        };
        let mid = Pos2::new((a.x + b.x) / 2.0, a.y);
        h.mv(mid);
        for _ in 0..2 { h.press(mid, false); h.release(mid); }
        let path = h.el.scene.as_ref().unwrap().layers[0].path.clone().unwrap();
        assert_eq!(path.keys.len(), 3);
        let k = &path.keys[1];
        assert!(k.x > 5.0 && k.x < 75.0 && (k.y / k.x - 0.5).abs() < 0.05, "nowy klucz na prostej: ({}, {})", k.x, k.y);
        // Delete usuwa zaznaczone klucze, a po usunięciu wszystkich znika cała ścieżka
        for _ in 0..3 {
            let pos = h.tl.key_pos.iter().find(|(r, _)| r.prop == PATH).map(|(_, p)| *p);
            let Some(pos) = pos else { break };
            h.click(pos);
            h.key(egui::Key::Delete);
        }
        assert!(h.el.scene.as_ref().unwrap().layers[0].path.is_none(), "pusta ścieżka usunięta");
    }

    #[test]
    fn element_level_keys_have_their_own_rows_and_support_editing() {
        let mut h = Harness::new();
        {
            let sc = h.el.scene.as_mut().unwrap();
            sc.layers[0].keys.clear();
            let mut ch = Channel::new("opacity");
            ch.keys = vec![btb_motion::Key::linear(0.2, 0.0), btb_motion::Key::linear(0.9, 1.0)];
            sc.keys = vec![ch];
        }
        h.frame(vec![]);
        h.frame(vec![]);
        let ek: Vec<_> = h.tl.key_pos.iter().filter(|(r, _)| r.layer == ELEMENT).cloned().collect();
        assert_eq!(ek.len(), 2, "klucze elementu widoczne na osi");
        // przeciąganie klucza elementu przesuwa go w czasie
        let p0 = ek[0].1;
        h.mv(p0);
        h.press(p0, false);
        h.mv(p0 + Vec2::new(30.0, 0.0));
        h.mv(p0 + Vec2::new(80.0, 0.0));
        h.release(p0 + Vec2::new(80.0, 0.0));
        let t: Vec<f32> = h.el.scene.as_ref().unwrap().keys[0].keys.iter().map(|k| k.t).collect();
        assert!(t[0] > 0.25 && t[0] < t[1], "{t:?}");
        // Delete usuwa klucz, a po ostatnim znika kanał elementu
        for _ in 0..2 {
            let pos = h.tl.key_pos.iter().find(|(r, _)| r.layer == ELEMENT).map(|(_, p)| *p);
            let Some(pos) = pos else { break };
            h.click(pos);
            h.key(egui::Key::Delete);
        }
        assert!(h.el.scene.as_ref().unwrap().keys.is_empty());
    }

    #[test]
    fn conversion_through_the_timeline_creates_visible_keys() {
        let mut h = Harness::new();
        h.el.animation.anim_in = AnimSpec { anim_type: "slide".into(), duration: 0.6, ..Default::default() };
        let r = keyconv::convert_presets(&mut h.el, Scope::All);
        assert!(r.element && r.channels > 0);
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(h.tl.key_pos.iter().any(|(r, _)| r.layer == ELEMENT && r.prop == "tx"), "klucze skonwertowanego presetu widać na osi");
    }
}
