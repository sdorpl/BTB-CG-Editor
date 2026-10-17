//! Interaktywna kanwa: widok (zoom/pan), zaznaczanie, przesuwanie/skalowanie/obrót z uchwytami, przyciąganie,
//! ramka zaznaczenia, nakładka ścieżki ruchu i edycja tekstu w miejscu.
//! Podział: `view` (nawigacja, tła), `hit` (geometria i trafianie), `tools` (narzędzia i maszyna `Drag`),
//! `overlay` (rysowanie nakładek), `text` (edycja tekstu, pasek dodawania). Tu: stan i kolejność klatki.
mod hit;
mod overlay;
mod text;
mod tools;
mod view;

use crate::anim::Transform;
use crate::model::{Layout, Scene};
use crate::motion_bridge::ELEMENT;
use crate::render::{self, SceneParams, View};
use egui::{Pos2, Rect, Sense, Ui, Vec2};
use hit::Geo;
use tools::Drag;
pub use view::Bg;

pub const FIELD_W: f32 = 1920.0;
pub const FIELD_H: f32 = 1080.0;

pub struct CanvasState {
    pub zoom: f32,
    pub pan: Vec2,
    pub bg: Bg,
    pub safe_areas: bool,
    pub snap: bool,
    pub needs_fit: bool,
    pub needs_fit_frame: bool,
    pub last_rect: Rect,
    drag: Option<Drag>,
    guides_x: Vec<f32>,
    guides_y: Vec<f32>,
    /// prowadnice przyciągania elementu do pola (współrzędne pola, nie ramki)
    field_guides_x: Vec<f32>,
    field_guides_y: Vec<f32>,
    marquee: Option<Rect>,
    pub editing: Option<String>,
    edit_buf: String,
    edit_fresh: bool,
    /// Zaznaczony klucz ścieżki ruchu zaznaczonej warstwy (nakładka na kanwie); synchronizowany z osią czasu.
    pub path_sel: Option<usize>,
    /// Ustawiane po kliknięciu klucza ścieżki na kanwie: (warstwa, indeks); aplikacja przenosi to do osi czasu.
    pub path_picked: Option<(String, usize)>,
    /// Pozycje nakładki ścieżki z ostatniej klatki (testy interakcji klikają w te punkty).
    pub overlay_keys: Vec<(usize, Pos2)>,
    pub overlay_handles: Vec<(usize, bool, Pos2)>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self { zoom: 0.5, pan: Vec2::ZERO, bg: Bg::Checker, safe_areas: true, snap: true, needs_fit: true, needs_fit_frame: false, last_rect: Rect::NOTHING, drag: None,
               guides_x: vec![], guides_y: vec![], field_guides_x: vec![], field_guides_y: vec![], marquee: None, editing: None, edit_buf: String::new(), edit_fresh: false, path_sel: None, path_picked: None, overlay_keys: vec![], overlay_handles: vec![] }
    }
}

pub struct CanvasOut {
    /// (layer id, new text) after an inline text edit finished.
    pub edited_text: Option<(String, String)>,
    pub interacting: bool,
}

impl CanvasState {
    pub fn fit_field(&mut self, rect: Rect) {
        let z = (rect.width() / FIELD_W).min(rect.height() / FIELD_H) * 0.96;
        self.zoom = z;
        self.pan = Vec2::new((rect.width() - FIELD_W * z) / 2.0, (rect.height() - FIELD_H * z) / 2.0);
    }
    pub fn fit_frame(&mut self, rect: Rect, frame_origin: (f32, f32), frame: (f32, f32)) {
        let z = ((rect.width() * 0.85) / frame.0.max(1.0)).min((rect.height() * 0.85) / frame.1.max(1.0)).clamp(0.1, 6.0);
        self.zoom = z;
        let cx = frame_origin.0 + frame.0 / 2.0;
        let cy = frame_origin.1 + frame.1 / 2.0;
        self.pan = Vec2::new(rect.width() / 2.0 - cx * z, rect.height() / 2.0 - cy * z);
    }
}

pub struct DrawInputs<'a> {
    pub sp: SceneParamsBuilder<'a>,
}

/// Everything in SceneParams except `scene` (borrowed after interaction mutates it).
pub struct SceneParamsBuilder<'a> {
    pub elem_tr: Transform,
    pub fx: &'a std::collections::HashMap<String, render::LayerFx>,
    pub values: render::Values<'a>,
    pub time: f64,
    pub playing: bool,
    pub badge_squash: bool,
}

/// Jedna klatka kanwy: nawigacja, interakcja (gdy nie trwa edycja tekstu ani odtwarzanie), potem rysowanie warstwami
/// od spodu: pole, ramka, scena, prowadnice, zaznaczenie, ścieżka, ramka zaznaczenia, edycja tekstu, pasek, etykiety.
pub fn draw_canvas(
    ui: &mut Ui,
    st: &mut CanvasState,
    scene: &mut Scene,
    layout: &mut Layout,
    selected: &mut Vec<String>,
    inputs: DrawInputs,
) -> CanvasOut {
    let ctx = ui.ctx().clone();
    let avail = ui.available_rect_before_wrap();
    let (rect, response) = ui.allocate_exact_size(avail.size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    st.last_rect = rect;
    st.overlay_keys.clear();
    st.overlay_handles.clear();
    let frame = (scene.frame.w, scene.frame.h);
    let fo = layout.frame_origin(frame.0, frame.1);
    let mut out = CanvasOut { edited_text: None, interacting: false };

    if st.needs_fit && rect.width() > 50.0 {
        st.fit_field(rect);
        st.needs_fit = false;
    }
    if st.needs_fit_frame && rect.width() > 50.0 {
        st.fit_frame(rect, fo, frame);
        st.needs_fit_frame = false;
    }

    let space = view::navigate(ui, st, rect, &response);
    let view = View { origin: rect.min + st.pan, zoom: st.zoom };
    let single = if selected.len() == 1 { scene.layers.iter().find(|l| l.id == selected[0]).cloned() } else { None };
    let frame_aff = render::frame_aff(fo, layout.scale, layout.rotation);
    let geo = Geo { view, frame_aff, view_aff: view.aff().mul(frame_aff), scale: layout.scale, element_sel: selected.len() == 1 && selected[0] == ELEMENT, frame, single, path_sel: st.path_sel, editing: st.editing.is_some(), playing: inputs.sp.playing };

    if st.editing.is_none() && !inputs.sp.playing {
        tools::interact(ui, st, &geo, scene, layout, selected, &response, space, &mut out);
    }

    // ── rysowanie ──
    let field_rect = overlay::draw_field(&painter, st, rect, &geo);
    let frame_tl = overlay::draw_frame_outline(&painter, &geo);
    let sp = SceneParams {
        scene,
        frame_aff,
        elem_tr: { let mut t = inputs.sp.elem_tr; t.opacity *= layout.opacity; t },
        fx: inputs.sp.fx,
        values: render::Values { title: inputs.sp.values.title, fields: inputs.sp.values.fields, extra: inputs.sp.values.extra, base_url: inputs.sp.values.base_url },
        time: inputs.sp.time,
        badge_squash: inputs.sp.badge_squash,
    };
    render::draw_scene(&painter.with_clip_rect(rect), &ctx, &view, &sp);
    overlay::draw_guides(&painter, st, rect, &geo);
    if !inputs.sp.playing {
        overlay::draw_element_selection(&painter, &geo, layout);
        overlay::draw_selection(&painter, &geo, scene, selected, st.editing.is_some());
        if st.editing.is_none() { overlay::draw_path(&painter, st, &geo, scene); }
    }
    overlay::draw_marquee(&painter, st);
    text::edit_overlay(ui, st, scene, &geo, rect, &mut out);
    if !inputs.sp.playing && st.editing.is_none() { text::add_bar(ui, st, scene, selected, rect, &mut out); }
    overlay::draw_labels(&painter, field_rect, frame_tl, frame);
    // cały element wyklucza zaznaczenie warstw (Shift/Ctrl na warstwie dokłada tylko warstwy)
    if selected.len() > 1 { selected.retain(|s| s != ELEMENT); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Element, Layer, LayerType};
    use egui::{Event, Modifiers, PointerButton};

    struct C {
        ctx: egui::Context,
        st: CanvasState,
        el: Element,
        selected: Vec<String>,
        time: f64,
        alt: bool,
        shift: bool,
    }

    impl C {
        fn new() -> Self {
            let mut el = Element::new_typed("t", "BADGE");
            let id = {
                let l = &mut el.scene.as_mut().unwrap().layers[0];
                l.path = Some(btb_motion::PathChannel { keys: vec![btb_motion::PathKey::new(0.0, 0.0, 0.0), btb_motion::PathKey::new(1.0, 120.0, 60.0), btb_motion::PathKey::new(2.0, 240.0, 0.0)] });
                l.id.clone()
            };
            let mut st = CanvasState::default();
            st.zoom = 0.6;
            st.needs_fit = false;
            st.path_sel = Some(1);
            let mut c = Self { ctx: egui::Context::default(), st, el, selected: vec![id], time: 0.0, alt: false, shift: false };
            crate::app::setup_fonts(&c.ctx);
            c.frame(vec![]);
            c.frame(vec![]);
            c
        }
        fn frame(&mut self, events: Vec<Event>) {
            self.time += 0.05;
            let mut raw = egui::RawInput::default();
            raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0)));
            raw.time = Some(self.time);
            raw.events = events;
            raw.modifiers.alt = self.alt;
            raw.modifiers.shift = self.shift;
            let Self { ctx, st, el, selected, .. } = self;
            let fx = std::collections::HashMap::new();
            let title = el.title.clone();
            let fields = el.fields.clone();
            let extra = el.extra.clone();
            let layout = &mut el.layout;
            let scene = el.scene.as_mut().unwrap();
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let values = render::Values { title: &title, fields: &fields, extra: &extra, base_url: "" };
                    draw_canvas(ui, st, scene, layout, selected, DrawInputs { sp: SceneParamsBuilder { elem_tr: Transform::default(), fx: &fx, values, time: 0.0, playing: false, badge_squash: false } });
                });
            });
        }
        fn path(&self) -> btb_motion::PathChannel { self.el.scene.as_ref().unwrap().layers[0].path.clone().unwrap() }
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
    fn overlay_shows_keys_and_handles_of_the_selected_key() {
        let c = C::new();
        assert_eq!(c.st.overlay_keys.len(), 3);
        assert_eq!(c.st.overlay_handles.len(), 2, "klucz środkowy ma oba tangenty");
        assert!(c.st.overlay_handles.iter().all(|(i, _, _)| *i == 1));
        let _ = LayerType::Shape;
    }

    #[test]
    fn dragging_a_path_key_moves_it_and_reports_the_pick() {
        let mut c = C::new();
        let kp = c.st.overlay_keys.iter().find(|(i, _)| *i == 2).unwrap().1;
        let before = c.path().keys[2].clone();
        let layer_before = (c.el.scene.as_ref().unwrap().layers[0].x, c.el.scene.as_ref().unwrap().layers[0].y);
        c.drag(kp, kp + Vec2::new(60.0, -30.0));
        let k = &c.path().keys[2];
        assert!(k.x > before.x + 20.0 && k.y < before.y - 10.0, "klucz przesunięty w polu: ({}, {}) -> ({}, {})", before.x, before.y, k.x, k.y);
        // przeciągnięcie klucza ścieżki nie rusza samej warstwy
        let l = &c.el.scene.as_ref().unwrap().layers[0];
        assert_eq!((l.x, l.y), layer_before);
        assert_eq!(c.st.path_sel, Some(2));
        assert_eq!(c.st.path_picked.as_ref().map(|p| p.1), Some(2));
    }

    #[test]
    fn dragging_a_handle_turns_off_auto_and_mirrors_unless_alt() {
        let mut c = C::new();
        let (_, _, hp) = *c.st.overlay_handles.iter().find(|(_, out, _)| *out).unwrap();
        assert!(c.path().keys[1].auto);
        c.drag(hp, hp + Vec2::new(30.0, 40.0));
        let k = c.path().keys[1].clone();
        assert!(!k.auto, "ręczny tangent wyłącza auto");
        assert!((k.tin[0] + k.tout[0]).abs() < 1e-3 && (k.tin[1] + k.tout[1]).abs() < 1e-3, "gładki węzeł: tangenty symetryczne {:?} {:?}", k.tin, k.tout);
        // z Alt drugi uchwyt zostaje
        let tin_before = k.tin;
        let (_, _, hp2) = *c.st.overlay_handles.iter().find(|(_, out, _)| *out).unwrap();
        c.alt = true;
        c.drag(hp2, hp2 + Vec2::new(-20.0, 25.0));
        c.alt = false;
        let k2 = c.path().keys[1].clone();
        assert_eq!(k2.tin, tin_before, "Alt: drugi tangent bez zmian");
        assert!(k2.tout != k.tout);
    }

    #[test]
    fn clicking_a_path_key_selects_it_without_selecting_other_layers() {
        let mut c = C::new();
        let kp = c.st.overlay_keys.iter().find(|(i, _)| *i == 0).unwrap().1;
        c.frame(vec![Event::PointerMoved(kp)]);
        c.frame(vec![Event::PointerButton { pos: kp, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() }]);
        c.frame(vec![Event::PointerButton { pos: kp, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() }]);
        assert_eq!(c.st.path_sel, Some(0));
        assert_eq!(c.selected.len(), 1, "zaznaczenie warstw bez zmian");
    }

    // ── testy zachowania narzędzi na warstwie bez ścieżki (zabezpieczają refaktor) ──

    impl C {
        /// Jedna warstwa bez ścieżki, zaznaczona; przyciąganie wyłączone dla deterministycznych wyników.
        fn plain() -> Self {
            let mut c = Self::new();
            {
                let sc = c.el.scene.as_mut().unwrap();
                sc.layers.truncate(1);
                sc.layers[0].path = None;
            }
            c.st.snap = false;
            c.st.path_sel = None;
            c.frame(vec![]);
            c.frame(vec![]);
            c
        }
        fn layer(&self) -> Layer { self.el.scene.as_ref().unwrap().layers[0].clone() }
        fn fo(&self) -> (f32, f32) {
            let f = &self.el.scene.as_ref().unwrap().frame;
            self.el.layout.frame_origin(f.w, f.h)
        }
        /// Punkt we współrzędnych ramki -> ekran.
        fn screen(&self, x: f32, y: f32) -> Pos2 {
            let fo = self.fo();
            self.st.last_rect.min + self.st.pan + Vec2::new(fo.0 + x, fo.1 + y) * self.st.zoom
        }
        fn press_release(&mut self, p: Pos2) {
            self.frame(vec![Event::PointerMoved(p)]);
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() }]);
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() }]);
            self.frame(vec![]);
        }
    }

    #[test]
    fn dragging_a_layer_moves_it_by_the_pointer_delta() {
        let mut c = C::plain();
        let l0 = c.layer();
        let from = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        let d = Vec2::new(60.0, 30.0);
        c.drag(from, from + d);
        let l = c.layer();
        assert!((l.x - l0.x - (d.x / c.st.zoom).round()).abs() <= 1.0, "x: {} -> {}", l0.x, l.x);
        assert!((l.y - l0.y - (d.y / c.st.zoom).round()).abs() <= 1.0, "y: {} -> {}", l0.y, l.y);
        assert_eq!((l.w, l.h), (l0.w, l0.h));
    }

    #[test]
    fn shift_locks_the_move_axis() {
        let mut c = C::plain();
        let l0 = c.layer();
        let from = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        c.shift = true;
        c.drag(from, from + Vec2::new(80.0, 20.0));
        c.shift = false;
        let l = c.layer();
        assert!(l.x > l0.x + 20.0);
        assert_eq!(l.y, l0.y, "dominująca oś X, Y bez zmiany");
    }

    #[test]
    fn snapping_pulls_the_layer_to_frame_edges() {
        let mut c = C::plain();
        c.st.snap = true;
        let l0 = c.layer();
        let from = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        // lewa krawędź warstwy kilka pikseli od 0 ramki: przyciągnięcie do 0
        let target_x = 3.0;
        let d = Vec2::new((target_x - l0.x) * c.st.zoom, 0.0);
        c.drag(from, from + d);
        assert_eq!(c.layer().x, 0.0);
    }

    #[test]
    fn snapping_uses_the_rotated_bounding_box() {
        let mut c = C::plain();
        c.st.snap = true;
        {
            let l = &mut c.el.scene.as_mut().unwrap().layers[0];
            l.w = 260.0;
            l.h = 56.0;
            l.rotation = 90.0;
        }
        c.frame(vec![]);
        let l0 = c.layer();
        let from = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        // pudełko po obrocie ma szerokość 56 i środek w środku warstwy; jego lewa krawędź ma trafić w 0
        let (bx0, _, _, _) = hit::frame_bbox(&l0, (c.el.scene.as_ref().unwrap().frame.w, c.el.scene.as_ref().unwrap().frame.h));
        let d = Vec2::new((3.0 - bx0) * c.st.zoom, 0.0);
        c.drag(from, from + d);
        let l = c.layer();
        let (nx0, _, _, _) = hit::frame_bbox(&l, (c.el.scene.as_ref().unwrap().frame.w, c.el.scene.as_ref().unwrap().frame.h));
        assert!(nx0.abs() < 1.0, "lewa krawędź obróconego pudełka: {nx0}");
    }

    #[test]
    fn layout_scale_and_rotation_apply_to_hit_testing_and_moves() {
        let mut c = C::plain();
        c.el.layout.scale = 2.0;
        c.frame(vec![]);
        let l0 = c.layer();
        let fo = c.fo();
        let to_screen = |c: &C, x: f32, y: f32| c.st.last_rect.min + c.st.pan + Vec2::new(fo.0 + 2.0 * x, fo.1 + 2.0 * y) * c.st.zoom;
        let from = to_screen(&c, l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        let d = Vec2::new(80.0, 40.0);
        c.drag(from, from + d);
        let l = c.layer();
        // ekran -> pole dzieli przez zoom, pole -> ramka przez skalę 2
        assert!((l.x - l0.x - (d.x / c.st.zoom / 2.0).round()).abs() <= 1.0, "x: {} -> {}", l0.x, l.x);
        assert!((l.y - l0.y - (d.y / c.st.zoom / 2.0).round()).abs() <= 1.0, "y: {} -> {}", l0.y, l.y);
    }

    /// Mała warstwa w rogu ramki, żeby reszta ramki była pusta; zaznaczony cały element.
    fn element_selected() -> C {
        let mut c = C::plain();
        {
            let l = &mut c.el.scene.as_mut().unwrap().layers[0];
            l.x = 0.0;
            l.y = 0.0;
            l.w = 20.0;
            l.h = 10.0;
        }
        c.el.layout.side = "custom".into();
        c.el.layout.x = Some(100.0);
        c.el.layout.y = Some(300.0);
        c.selected = vec![ELEMENT.to_string()];
        c.frame(vec![]);
        c.frame(vec![]);
        c
    }

    fn frame_size(c: &C) -> (f32, f32) { let f = &c.el.scene.as_ref().unwrap().frame; (f.w, f.h) }

    #[test]
    fn clicking_empty_space_in_the_frame_selects_the_whole_element() {
        let mut c = C::plain();
        {
            let l = &mut c.el.scene.as_mut().unwrap().layers[0];
            l.w = 20.0;
            l.h = 10.0;
        }
        c.frame(vec![]);
        let (fw, fh) = frame_size(&c);
        c.press_release(c.screen(fw - 10.0, fh - 10.0));
        assert_eq!(c.selected, vec![ELEMENT.to_string()]);
        // poza ramką zaznaczenie jest czyszczone
        c.press_release(c.st.last_rect.min + c.st.pan + Vec2::new(4.0, 4.0));
        assert!(c.selected.is_empty());
    }

    #[test]
    fn dragging_the_selected_element_moves_its_layout_not_the_layers() {
        let mut c = element_selected();
        let (fw, fh) = frame_size(&c);
        let layer0 = (c.layer().x, c.layer().y);
        let from = c.screen(fw - 10.0, fh - 10.0);
        let d = Vec2::new(60.0, 30.0);
        c.drag(from, from + d);
        let lo = &c.el.layout;
        assert!((lo.x.unwrap() - 100.0 - (d.x / c.st.zoom).round()).abs() <= 1.0, "x = {:?}", lo.x);
        assert!((lo.y.unwrap() - 300.0 - (d.y / c.st.zoom).round()).abs() <= 1.0, "y = {:?}", lo.y);
        assert_eq!((c.layer().x, c.layer().y), layer0);
        assert_eq!(c.selected, vec![ELEMENT.to_string()], "zaznaczenie zostaje przy elemencie");
    }

    #[test]
    fn moving_an_anchored_element_changes_margins() {
        let mut c = element_selected();
        c.el.layout.side = "bottom-right".into();
        c.el.layout.margin_x = 50.0;
        c.el.layout.margin_y = 50.0;
        c.frame(vec![]);
        let (fw, fh) = frame_size(&c);
        let from = c.screen(fw - 10.0, fh - 10.0);
        let o0 = c.el.layout.frame_origin(fw, fh);
        c.drag(from, from + Vec2::new(-80.0, -40.0));
        assert_eq!(c.el.layout.side, "bottom-right");
        let o1 = c.el.layout.frame_origin(fw, fh);
        assert!(o1.0 < o0.0 && o1.1 < o0.1, "{o0:?} -> {o1:?}");
    }

    #[test]
    fn the_corner_handle_scales_the_element_around_its_top_left() {
        let mut c = element_selected();
        let (fw, fh) = frame_size(&c);
        let origin = c.el.layout.frame_origin(fw, fh);
        let handle = c.screen(fw, fh);
        c.drag(handle, handle + Vec2::new(60.0, 30.0));
        let lo = &c.el.layout;
        assert!(lo.scale > 1.05, "scale = {}", lo.scale);
        assert_eq!(lo.frame_origin(fw, fh), origin, "lewy górny róg stoi w miejscu");
    }

    #[test]
    fn the_rotate_handle_rotates_the_element_around_its_top_left() {
        let mut c = element_selected();
        let (fw, _) = frame_size(&c);
        let pivot = c.screen(0.0, 0.0);
        let handle = c.screen(fw / 2.0, 0.0) + Vec2::new(0.0, -26.0);
        let start = (handle - pivot).angle().to_degrees();
        // kursor pionowo pod punktem obrotu = kąt 90°
        c.drag(handle, pivot + Vec2::new(0.0, 200.0));
        let r = c.el.layout.rotation;
        assert!((r - (90.0 - start)).abs() < 1.0, "rotation = {r}, start = {start}");
    }

    #[test]
    fn element_move_snaps_to_the_field_edge() {
        let mut c = element_selected();
        c.st.snap = true;
        let (fw, fh) = frame_size(&c);
        let from = c.screen(fw - 10.0, fh - 10.0);
        // lewa krawędź ramki 3 px od krawędzi pola
        c.drag(from, from + Vec2::new(-97.0 * c.st.zoom, 0.0));
        assert_eq!(c.el.layout.x, Some(0.0));
    }

    #[test]
    fn alt_click_selects_the_element_even_over_a_layer() {
        let mut c = C::plain();
        let l0 = c.layer();
        c.alt = true;
        c.press_release(c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0));
        c.alt = false;
        assert_eq!(c.selected, vec![ELEMENT.to_string()]);
    }

    #[test]
    fn clicking_a_layer_while_the_element_is_selected_selects_the_layer() {
        let mut c = element_selected();
        let l0 = c.layer();
        c.press_release(c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0));
        assert_eq!(c.selected, vec![l0.id.clone()]);
    }

    #[test]
    fn resizing_from_the_corner_handle_grows_the_layer() {
        let mut c = C::plain();
        let l0 = c.layer();
        let corner = c.screen(l0.x + l0.w, l0.y + l0.h);
        let d = Vec2::new(30.0, 24.0);
        c.drag(corner, corner + d);
        let l = c.layer();
        assert!((l.w - l0.w - (d.x / c.st.zoom).round()).abs() <= 1.0, "w: {} -> {}", l0.w, l.w);
        assert!((l.h - l0.h - (d.y / c.st.zoom).round()).abs() <= 1.0, "h: {} -> {}", l0.h, l.h);
        assert_eq!((l.x, l.y), (l0.x, l0.y), "przeciwległy róg stoi w miejscu");
    }

    #[test]
    fn shift_keeps_the_aspect_ratio_on_corner_resize() {
        let mut c = C::plain();
        let l0 = c.layer();
        let corner = c.screen(l0.x + l0.w, l0.y + l0.h);
        c.shift = true;
        c.drag(corner, corner + Vec2::new(60.0, 5.0));
        c.shift = false;
        let l = c.layer();
        assert!((l.w / l.h - l0.w / l0.h).abs() < 0.1, "{}x{} vs {}x{}", l.w, l.h, l0.w, l0.h);
    }

    #[test]
    fn the_rotate_handle_turns_the_layer_around_its_center() {
        let mut c = C::plain();
        let l0 = c.layer();
        let ctr = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        let top = c.screen(l0.x + l0.w / 2.0, l0.y);
        let handle = top + Vec2::new(0.0, -26.0);
        // z góry na prawo od środka = +90°
        c.drag(handle, ctr + Vec2::new(150.0, 0.0));
        let r = c.layer().rotation;
        assert!((r - 90.0).abs() < 1.0, "rotation = {r}");
        let l = c.layer();
        assert_eq!((l.x, l.y, l.w, l.h), (l0.x, l0.y, l0.w, l0.h));
    }

    #[test]
    fn marquee_selects_intersecting_layers_and_clears_when_empty() {
        let mut c = C::plain();
        let l0 = c.layer();
        c.selected.clear();
        // start w pustym miejscu pola, koniec wewnątrz warstwy
        let start = c.st.last_rect.min + c.st.pan + Vec2::new(4.0, 4.0);
        let inside = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        c.drag(start, inside);
        assert_eq!(c.selected, vec![l0.id.clone()]);
        // mały prostokąt w pustym rogu nic nie zaznacza
        let a = c.st.last_rect.min + c.st.pan + Vec2::new(4.0, 4.0);
        c.drag(a, a + Vec2::new(10.0, 10.0));
        assert!(c.selected.is_empty());
    }

    #[test]
    fn clicking_selects_a_layer_and_empty_space_clears() {
        let mut c = C::plain();
        let l0 = c.layer();
        c.selected.clear();
        c.press_release(c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0));
        assert_eq!(c.selected, vec![l0.id.clone()]);
        c.press_release(c.st.last_rect.min + c.st.pan + Vec2::new(4.0, 4.0));
        assert!(c.selected.is_empty());
    }

    #[test]
    fn locked_layers_do_not_move() {
        let mut c = C::plain();
        c.el.scene.as_mut().unwrap().layers[0].locked = true;
        let l0 = c.layer();
        let from = c.screen(l0.x + l0.w / 2.0, l0.y + l0.h / 2.0);
        c.drag(from, from + Vec2::new(60.0, 30.0));
        let l = c.layer();
        assert_eq!((l.x, l.y), (l0.x, l0.y));
    }

    #[test]
    fn wheel_zoom_keeps_the_point_under_the_cursor() {
        let mut c = C::plain();
        let p = c.st.last_rect.center();
        let field_before = (p - c.st.last_rect.min - c.st.pan) / c.st.zoom;
        let z0 = c.st.zoom;
        c.frame(vec![Event::PointerMoved(p)]);
        for _ in 0..6 {
            c.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: Vec2::new(0.0, 40.0), modifiers: Modifiers::default() }]);
        }
        for _ in 0..10 { c.frame(vec![]); }
        assert!(c.st.zoom > z0, "zoom {} -> {}", z0, c.st.zoom);
        let field_after = (p - c.st.last_rect.min - c.st.pan) / c.st.zoom;
        assert!((field_after - field_before).length() < 1.0, "{field_before:?} vs {field_after:?}");
    }

    #[test]
    fn fit_field_and_fit_frame_center_the_view() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 600.0));
        let mut st = CanvasState::default();
        st.fit_field(rect);
        assert!((st.zoom - 1000.0 / FIELD_W * 0.96).abs() < 1e-4);
        let center = Pos2::ZERO + st.pan + Vec2::new(FIELD_W, FIELD_H) * st.zoom / 2.0;
        assert!((center - rect.center()).length() < 1.0);
        st.fit_frame(rect, (100.0, 100.0), (400.0, 200.0));
        let c2 = Pos2::ZERO + st.pan + Vec2::new(300.0, 200.0) * st.zoom;
        assert!((c2 - rect.center()).length() < 1.0);
    }
}
