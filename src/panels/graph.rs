//! Edytor krzywych (graph editor) wzorowany na After Effects: wykres wartości albo prędkości jednego
//! kanału kluczy, klucze przeciągane w czasie i wartości oraz uchwyty Béziera (poziomo wpływ, pionowo
//! prędkość; Shift blokuje oś czasu). Matematyka uchwytów jest w `btb_motion::Channel`.
use crate::model::*;
use crate::panels::timeline::{channel_mut, prop_label, KeyRef, TimelineUi, PATH};
use btb_motion::{Channel, PathChannel};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

const SNAP: f32 = 1.0 / 60.0;
const PLOT_H: f32 = 240.0;
const HIT: f32 = 16.0;

enum GDrag {
    Handle,
    /// migawka kanału i punkt zaczepienia w przestrzeni (czas, wartość) z początku przeciągania
    Key { snap: Channel, anchor: (f32, f32), value0: f32 },
    /// klucz ścieżki ruchu: przesuwamy tylko w czasie (wartość to długość łuku, wynika z pozycji)
    PathKey { snap: PathChannel, anchor_t: f32 },
}

#[derive(Default)]
pub struct GraphUi {
    speed_view: bool,
    focus: Option<(String, String)>,
    drag: Option<GDrag>,
    /// Pozycje z ostatniej klatki (testy interakcji klikają w te punkty): klucze i uchwyty (indeks, strona wyjścia).
    pub key_pos: Vec<(usize, Pos2)>,
    pub handle_pos: Vec<(usize, bool, Pos2)>,
}

struct View {
    rect: Rect,
    t: (f32, f32),
    v: (f32, f32),
}

impl View {
    fn to_screen(&self, t: f32, v: f32) -> Pos2 {
        let x = self.rect.min.x + (t - self.t.0) / (self.t.1 - self.t.0) * self.rect.width();
        let y = self.rect.max.y - (v - self.v.0) / (self.v.1 - self.v.0) * self.rect.height();
        Pos2::new(x, y)
    }
    fn from_screen(&self, p: Pos2) -> (f32, f32) {
        let t = self.t.0 + (p.x - self.rect.min.x) / self.rect.width() * (self.t.1 - self.t.0);
        let v = self.v.0 + (self.rect.max.y - p.y) / self.rect.height() * (self.v.1 - self.v.0);
        (t, v)
    }
}

/// Zakres osi obejmujący wszystkie wartości z 12% zapasem (płaski wykres dostaje ±1).
fn fit(vals: impl Iterator<Item = f32>) -> (f32, f32) {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for v in vals.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi { return (-1.0, 1.0); }
    if (hi - lo) < 1e-4 { return (lo - 1.0, hi + 1.0); }
    let pad = (hi - lo) * 0.12;
    (lo - pad, hi + pad)
}

pub fn draw_graph(ctx: &egui::Context, el: &mut Element, tl: &mut TimelineUi, g: &mut GraphUi) {
    if !tl.graph_open { return; }
    let mut open = true;
    egui::Window::new("Edytor krzywych").id(egui::Id::new("graph-editor")).open(&mut open).default_size([680.0, 360.0]).resizable(true).show(ctx, |ui| body(ui, el, tl, g));
    tl.graph_open = open;
}

fn body(ui: &mut Ui, el: &mut Element, tl: &mut TimelineUi, g: &mut GraphUi) {
    g.key_pos.clear();
    g.handle_pos.clear();
    if let Some(s) = tl.sel.first() { g.focus = Some((s.layer.clone(), s.prop.clone())); }
    let Some((layer_id, prop)) = g.focus.clone() else {
        ui.label("Zaznacz klucz na osi czasu, aby edytować jego krzywą.");
        return;
    };
    let Some(scene) = el.scene.as_ref() else { return };
    // cel: warstwa albo cały element (jego klucze są w Scene.keys, bez ścieżki)
    let (layer_name, keys, path) = if layer_id == crate::motion_bridge::ELEMENT {
        ("Element".to_string(), &scene.keys, None)
    } else {
        let Some(layer) = scene.layers.iter().find(|l| l.id == layer_id) else { g.focus = None; return };
        (layer.name.clone(), &layer.keys, layer.path.as_ref())
    };
    let mut props: Vec<String> = keys.iter().map(|c| c.prop.clone()).collect();
    if path.is_some_and(|p| !p.keys.is_empty()) { props.push(PATH.to_string()); }
    let is_path = prop == PATH;
    // ścieżka ruchu: wykres pokazuje kanał odległości (długość łuku w czasie), jego prędkość to prędkość ruchu
    let found = if is_path { path.map(|p| p.distance_channel()) } else { keys.iter().find(|c| c.prop == prop).cloned() };
    let Some(ch) = found else { g.focus = None; return };
    if ch.keys.is_empty() { return; }

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&layer_name).strong());
        let mut cur = prop.clone();
        egui::ComboBox::from_id_salt("graph-chan").selected_text(prop_label(&cur)).show_ui(ui, |ui| {
            for p in &props { ui.selectable_value(&mut cur, p.clone(), prop_label(p)); }
        });
        if cur != prop { g.focus = Some((layer_id.clone(), cur)); tl.sel.clear(); }
        ui.separator();
        ui.selectable_value(&mut g.speed_view, false, "Wartość");
        ui.selectable_value(&mut g.speed_view, true, "Prędkość");
        ui.weak("Shift: blokuje czas uchwytu · Alt: bez przyciągania");
    });

    // zakresy osi
    let (first, last) = (ch.keys.first().map_or(0.0, |k| k.t), ch.keys.last().map_or(1.0, |k| k.t));
    let span = (last - first).max(0.5);
    let t_range = (first - 0.1 * span, last + 0.1 * span);
    let samples: Vec<(f32, f32)> = (0..=160)
        .map(|i| {
            let t = t_range.0 + (t_range.1 - t_range.0) * i as f32 / 160.0;
            (t, if g.speed_view { ch.velocity(t).unwrap_or(0.0) } else { ch.eval(t).unwrap_or(0.0) })
        })
        .collect();
    let mut v_vals: Vec<f32> = samples.iter().map(|s| s.1).collect();
    if g.speed_view { v_vals.push(0.0); } else {
        v_vals.extend(ch.keys.iter().map(|k| k.v));
        for i in 0..ch.keys.len() { for out in [false, true] { if let Some((_, v)) = ch.handle_point(i, out) { v_vals.push(v); } } }
    }
    let view_v = fit(v_vals.into_iter());

    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(300.0), PLOT_H), Sense::hover());
    let view = View { rect, t: t_range, v: view_v };
    let p = ui.painter_at(rect.expand(1.0));
    p.rect_filled(rect, 2.0, Color32::from_gray(20));
    // siatka i opisy osi
    for k in 0..=4 {
        let v = view_v.0 + (view_v.1 - view_v.0) * k as f32 / 4.0;
        let y = view.to_screen(t_range.0, v).y;
        p.hline(rect.x_range(), y, Stroke::new(1.0, Color32::from_gray(if k == 0 { 55 } else { 36 })));
        p.text(Pos2::new(rect.min.x + 3.0, y - 1.0), egui::Align2::LEFT_BOTTOM, format!("{v:.1}"), egui::FontId::monospace(9.0), Color32::from_gray(120));
    }
    let mut tt = (t_range.0 * 2.0).ceil() / 2.0;
    while tt <= t_range.1 {
        let x = view.to_screen(tt, view_v.0).x;
        p.vline(x, rect.y_range(), Stroke::new(1.0, Color32::from_gray(36)));
        p.text(Pos2::new(x + 2.0, rect.max.y - 2.0), egui::Align2::LEFT_BOTTOM, format!("{:.1} s", if tt.abs() < 1e-4 { 0.0 } else { tt }), egui::FontId::monospace(9.0), Color32::from_gray(120));
        tt += 0.5;
    }
    // zero w widoku prędkości
    if g.speed_view { p.hline(rect.x_range(), view.to_screen(t_range.0, 0.0).y, Stroke::new(1.0, Color32::from_gray(80))); }
    // krzywa
    let col = if g.speed_view { Color32::from_rgb(96, 165, 250) } else { Color32::from_rgb(52, 211, 153) };
    let pts: Vec<Pos2> = samples.iter().map(|&(t, v)| view.to_screen(t, v)).collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.8, col)));

    // klucze i uchwyty
    let shift = ui.input(|i| i.modifiers.shift);
    let alt = ui.input(|i| i.modifiers.alt);
    let snap = |t: f32| if alt { t } else { (t / SNAP).round() * SNAP };
    let sel_idx: Vec<usize> = tl.sel.iter().filter(|s| s.layer == layer_id && s.prop == prop).map(|s| s.idx).collect();
    let mut handle_edit: Option<(usize, bool, Pos2)> = None;
    let mut handle_resp: Vec<(usize, bool, egui::Response)> = vec![];
    if !g.speed_view {
        for &i in &sel_idx {
            let Some(k) = ch.keys.get(i) else { continue };
            let kp = view.to_screen(k.t, k.v);
            for out in [false, true] {
                let Some((t, v)) = ch.handle_point(i, out) else { continue };
                let hp = view.to_screen(t, v);
                p.line_segment([kp, hp], Stroke::new(1.0, Color32::from_rgb(250, 204, 21)));
                g.handle_pos.push((i, out, hp));
                let r = ui.interact(Rect::from_center_size(hp, Vec2::splat(HIT)), ui.id().with(("gh", i, out)), Sense::drag());
                p.circle(hp, 4.0, if r.hovered() || r.dragged() { Color32::WHITE } else { Color32::from_rgb(20, 20, 20) }, Stroke::new(1.5, Color32::from_rgb(250, 204, 21)));
                handle_resp.push((i, out, r));
            }
        }
    }
    for (i, k) in ch.keys.iter().enumerate() {
        let yv = if g.speed_view { ch.velocity(k.t).unwrap_or(0.0) } else { k.v };
        let c = view.to_screen(k.t, yv);
        g.key_pos.push((i, c));
        let r = ui.interact(Rect::from_center_size(c, Vec2::splat(HIT)), ui.id().with(("gk", i)), Sense::click_and_drag());
        let selected = sel_idx.contains(&i);
        p.circle(c, 5.0, if selected { Color32::from_rgb(251, 191, 36) } else { Color32::from_rgb(210, 214, 224) }, Stroke::new(1.0, if selected { Color32::WHITE } else { Color32::from_gray(40) }));
        // opis rysowany bez widżetu (podpowiedź przechwytywałaby naciśnięcia)
        if r.hovered() && g.drag.is_none() {
            p.text(c + Vec2::new(0.0, -9.0), egui::Align2::CENTER_BOTTOM, format!("{:.2} @ {:.2} s", k.v, k.t), egui::FontId::monospace(9.5), Color32::from_rgb(250, 230, 160));
        }
        handle_key_response(g, tl, &ch, &layer_id, &prop, i, &r, &view, snap, el.scene.as_mut());
    }

    // uchwyty: przeciąganie ustawia wpływ i prędkość
    for (i, out, r) in handle_resp {
        if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() { handle_edit = Some((i, out, pos)); }
        }
    }
    if let Some((i, out, pos)) = handle_edit {
        let (t, v) = view.from_screen(pos);
        if is_path {
            if let Some(pa) = el.scene.as_mut().and_then(|s| s.layers.iter_mut().find(|l| l.id == layer_id)).and_then(|l| l.path.as_mut()) {
                let mut d = pa.distance_channel();
                d.set_handle_point(i, out, t, v, shift);
                pa.apply_distance_handles(&d);
            }
        } else if let Some(c) = el.scene.as_mut().and_then(|s| channel_mut(s, &layer_id, &prop)) { c.set_handle_point(i, out, t, v, shift); }
        g.drag = Some(GDrag::Handle);
    } else if matches!(g.drag, Some(GDrag::Handle)) && !ui.input(|i| i.pointer.any_down()) {
        g.drag = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_key_response(g: &mut GraphUi, tl: &mut TimelineUi, ch: &Channel, layer: &str, prop: &str, i: usize, r: &egui::Response, view: &View, snap: impl Fn(f32) -> f32, mut scene: Option<&mut Scene>) {
    let kr = KeyRef { layer: layer.to_string(), prop: prop.to_string(), idx: i };
    if r.clicked() { tl.sel = vec![kr.clone()]; }
    if r.drag_started() {
        tl.sel = vec![kr.clone()];
        if let Some(pos) = r.interact_pointer_pos() {
            let anchor = view.from_screen(pos);
            let path_snap = if prop == PATH { scene.as_deref().and_then(|s| s.layers.iter().find(|l| l.id == layer)).and_then(|l| l.path.clone()) } else { None };
            g.drag = Some(match path_snap {
                Some(snap) => GDrag::PathKey { snap, anchor_t: anchor.0 },
                None => GDrag::Key { snap: ch.clone(), anchor, value0: ch.keys[i].v },
            });
        }
    }
    if let (Some(GDrag::PathKey { snap: shot, anchor_t }), true, Some(pos), Some(scene)) = (&g.drag, r.dragged(), r.interact_pointer_pos(), scene.as_deref_mut()) {
        let (t, _) = view.from_screen(pos);
        let (moved, sel) = shot.move_keys(&[i], snap(t - anchor_t));
        if let Some(l) = scene.layers.iter_mut().find(|l| l.id == layer) { l.path = Some(moved); }
        tl.sel = sel.into_iter().map(|idx| KeyRef { layer: layer.to_string(), prop: prop.to_string(), idx }).collect();
        return;
    }
    if r.dragged() {
        if let (Some(GDrag::Key { snap: shot, anchor, value0 }), Some(pos), Some(scene)) = (&g.drag, r.interact_pointer_pos(), scene.as_deref_mut()) {
            if !g.speed_view_active() {
                let (t, v) = view.from_screen(pos);
                let (moved, sel) = shot.move_keys(&[i], snap(t - anchor.0));
                let mut moved = moved;
                if let Some(&ni) = sel.first() { moved.keys[ni].v = value0 + (v - anchor.1); }
                if let Some(c) = channel_mut(scene, layer, prop) { *c = moved; }
                tl.sel = sel.into_iter().map(|idx| KeyRef { layer: layer.to_string(), prop: prop.to_string(), idx }).collect();
            }
        }
    }
    if r.drag_stopped() { g.drag = None; }
}

impl GraphUi {
    /// Przeciąganie kluczy działa tylko w widoku wartości (w widoku prędkości klucz nie ma własnej wartości).
    fn speed_view_active(&self) -> bool { self.speed_view }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Playback;
    use egui::{Event, Modifiers, PointerButton};

    struct H {
        ctx: egui::Context,
        el: Element,
        tl: TimelineUi,
        g: GraphUi,
        time: f64,
        shift: bool,
    }

    impl H {
        fn new() -> Self {
            let mut el = Element::new_typed("t", "BADGE");
            let s = el.scene.as_mut().unwrap();
            let mut ch = Channel::new("tx");
            ch.keys = vec![btb_motion::Key::linear(0.0, 0.0), btb_motion::Key::linear(1.0, 100.0), btb_motion::Key::linear(2.0, 40.0)];
            let id = s.layers[0].id.clone();
            s.layers[0].keys = vec![ch];
            let mut tl = TimelineUi::default();
            tl.graph_open = true;
            tl.sel = vec![KeyRef { layer: id, prop: "tx".into(), idx: 1 }];
            let mut h = Self { ctx: egui::Context::default(), el, tl, g: GraphUi::default(), time: 0.0, shift: false };
            h.frame(vec![]);
            h.frame(vec![]);
            h
        }
        fn frame(&mut self, events: Vec<Event>) {
            self.time += 0.05;
            let mut raw = egui::RawInput::default();
            raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0)));
            raw.time = Some(self.time);
            raw.events = events;
            raw.modifiers.shift = self.shift;
            let Self { el, tl, g, ctx, .. } = self;
            let _ = ctx.run(raw, |ctx| draw_graph(ctx, el, tl, g));
        }
        fn ch(&self) -> &Channel { &self.el.scene.as_ref().unwrap().layers[0].keys[0] }
        fn drag(&mut self, from: Pos2, to: Pos2) {
            let mid = from + (to - from) * 0.5;
            self.frame(vec![Event::PointerMoved(from)]);
            self.frame(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() }]);
            self.frame(vec![Event::PointerMoved(mid)]);
            self.frame(vec![Event::PointerMoved(to)]);
            self.frame(vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() }]);
            self.frame(vec![]);
        }
    }

    #[test]
    fn graph_shows_handles_for_the_selected_key_only() {
        let h = H::new();
        assert_eq!(h.g.key_pos.len(), 3);
        // klucz 1 ma obie strony z sąsiadami; Linear daje uchwyty w 1/3 odcinka
        assert_eq!(h.g.handle_pos.len(), 2);
        assert!(h.g.handle_pos.iter().all(|(i, _, _)| *i == 1));
    }

    #[test]
    fn dragging_a_handle_changes_influence_and_speed() {
        let mut h = H::new();
        let (_, _, hp) = *h.g.handle_pos.iter().find(|(_, out, _)| *out).unwrap();
        let before = h.ch().side_ease(1, true).unwrap();
        h.drag(hp, hp + Vec2::new(60.0, -40.0));
        let after = h.ch().side_ease(1, true).unwrap();
        assert!(after.influence > before.influence, "{before:?} -> {after:?}");
        assert!(after.speed > before.speed, "uchwyt w górę zwiększa prędkość: {before:?} -> {after:?}");
        assert!(matches!(h.ch().keys[1].right, btb_motion::Handle::Ease { .. }));
    }

    #[test]
    fn shift_locks_the_handle_time_axis() {
        let mut h = H::new();
        let (_, _, hp) = *h.g.handle_pos.iter().find(|(_, out, _)| *out).unwrap();
        let before = h.ch().side_ease(1, true).unwrap();
        h.shift = true;
        h.drag(hp, hp + Vec2::new(80.0, -50.0));
        h.shift = false;
        let after = h.ch().side_ease(1, true).unwrap();
        assert!((after.influence - before.influence).abs() < 1e-4, "wpływ bez zmian z Shiftem");
        assert!((after.speed - before.speed).abs() > 0.1, "prędkość się zmieniła");
    }

    #[test]
    fn dragging_a_key_changes_time_and_value() {
        let mut h = H::new();
        let kp = h.g.key_pos.iter().find(|(i, _)| *i == 1).unwrap().1;
        h.drag(kp, kp + Vec2::new(40.0, 30.0));
        let k = &h.ch().keys[1];
        assert!(k.t > 1.0 && k.v < 100.0, "czas w prawo, wartość w dół: t={} v={}", k.t, k.v);
        let on_grid = (k.t / SNAP).round() * SNAP;
        assert!((k.t - on_grid).abs() < 1e-4);
        assert_eq!(h.tl.sel.len(), 1);
    }

    #[test]
    fn clicking_another_key_moves_the_selection_and_handles() {
        let mut h = H::new();
        let kp = h.g.key_pos.iter().find(|(i, _)| *i == 2).unwrap().1;
        h.frame(vec![Event::PointerMoved(kp)]);
        h.frame(vec![Event::PointerButton { pos: kp, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() }]);
        h.frame(vec![Event::PointerButton { pos: kp, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() }]);
        h.frame(vec![]);
        assert_eq!(h.tl.sel[0].idx, 2);
        // ostatni klucz ma tylko stronę wejścia
        assert!(h.g.handle_pos.iter().all(|(i, out, _)| *i == 2 && !*out), "{:?}", h.g.handle_pos);
        let _ = Playback::default();
    }

    #[test]
    fn speed_view_draws_keys_without_handles_and_does_not_edit() {
        let mut h = H::new();
        h.g.speed_view = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert_eq!(h.g.key_pos.len(), 3);
        assert!(h.g.handle_pos.is_empty(), "w widoku prędkości nie ma uchwytów");
        let before = h.ch().clone();
        let kp = h.g.key_pos.iter().find(|(i, _)| *i == 1).unwrap().1;
        h.drag(kp, kp + Vec2::new(50.0, 40.0));
        assert_eq!(h.ch().keys.iter().map(|k| (k.t, k.v)).collect::<Vec<_>>(), before.keys.iter().map(|k| (k.t, k.v)).collect::<Vec<_>>());
    }

    #[test]
    fn view_maps_screen_and_values_both_ways() {
        let v = View { rect: Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(400.0, 200.0)), t: (0.0, 4.0), v: (-10.0, 10.0) };
        let p = v.to_screen(1.0, 5.0);
        let (t, val) = v.from_screen(p);
        assert!((t - 1.0).abs() < 1e-5 && (val - 5.0).abs() < 1e-5);
        assert_eq!(fit([5.0f32, 5.0].into_iter()), (4.0, 6.0));
        assert_eq!(fit(std::iter::empty()), (-1.0, 1.0));
    }

    #[test]
    fn path_focus_edits_speed_handles_and_time_only() {
        let mut h = H::new();
        {
            let l = &mut h.el.scene.as_mut().unwrap().layers[0];
            l.keys.clear();
            l.path = Some(PathChannel { keys: vec![btb_motion::PathKey::new(0.0, 0.0, 0.0), btb_motion::PathKey::new(1.0, 100.0, 0.0), btb_motion::PathKey::new(2.0, 150.0, 0.0)] });
            let id = l.id.clone();
            h.tl.sel = vec![KeyRef { layer: id, prop: PATH.into(), idx: 1 }];
        }
        h.frame(vec![]);
        h.frame(vec![]);
        assert_eq!(h.g.key_pos.len(), 3);
        assert_eq!(h.g.handle_pos.len(), 2, "uchwyty czasowe klucza ścieżki");
        // przeciągnięcie uchwytu wyjścia zmienia prędkość/wpływ klucza ścieżki
        let (_, _, hp) = *h.g.handle_pos.iter().find(|(_, out, _)| *out).unwrap();
        let before = h.el.scene.as_ref().unwrap().layers[0].path.as_ref().unwrap().distance_channel().side_ease(1, true).unwrap();
        h.drag(hp, hp + Vec2::new(30.0, -30.0));
        let path = h.el.scene.as_ref().unwrap().layers[0].path.clone().unwrap();
        let after = path.distance_channel().side_ease(1, true).unwrap();
        assert!(after != before, "{before:?} -> {after:?}");
        assert!(matches!(path.keys[1].right, btb_motion::Handle::Ease { .. }));
        // klucz przeciągnięty w pionie nie zmienia pozycji (wartość to długość łuku), w poziomie zmienia czas
        let kp = h.g.key_pos.iter().find(|(i, _)| *i == 1).unwrap().1;
        h.drag(kp, kp + Vec2::new(40.0, 60.0));
        let moved = h.el.scene.as_ref().unwrap().layers[0].path.clone().unwrap();
        assert!(moved.keys[1].t > 1.0 && (moved.keys[1].x - 100.0).abs() < 1e-4);
    }

    #[test]
    fn element_level_channel_is_editable_in_the_graph() {
        let mut h = H::new();
        {
            let sc = h.el.scene.as_mut().unwrap();
            sc.layers[0].keys.clear();
            let mut ch = Channel::new("opacity");
            ch.keys = vec![btb_motion::Key::linear(0.0, 0.0), btb_motion::Key::linear(1.0, 1.0), btb_motion::Key::linear(2.0, 0.5)];
            sc.keys = vec![ch];
            h.tl.sel = vec![KeyRef { layer: crate::motion_bridge::ELEMENT.into(), prop: "opacity".into(), idx: 1 }];
        }
        h.frame(vec![]);
        h.frame(vec![]);
        assert_eq!(h.g.key_pos.len(), 3);
        let (_, _, hp) = *h.g.handle_pos.iter().find(|(_, out, _)| *out).unwrap();
        let before = h.el.scene.as_ref().unwrap().keys[0].side_ease(1, true).unwrap();
        h.drag(hp, hp + Vec2::new(40.0, -30.0));
        let after = h.el.scene.as_ref().unwrap().keys[0].side_ease(1, true).unwrap();
        assert!(after != before);
    }
}
