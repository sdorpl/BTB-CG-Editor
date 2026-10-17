//! Interactive canvas: view (zoom/pan), selection, move/resize/rotate with handles, snapping, marquee.
use crate::anim::Transform;
use crate::model::{Layer, Scene};
use crate::render::{self, Aff, SceneParams, View};
use egui::{Color32, CursorIcon, Key, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

pub const FIELD_W: f32 = 1920.0;
pub const FIELD_H: f32 = 1080.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bg { Checker, Dark, Green, Gray, White }
impl Bg {
    pub const ALL: [Bg; 5] = [Bg::Checker, Bg::Dark, Bg::Green, Bg::Gray, Bg::White];
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self { Bg::Checker => "bg.checker", Bg::Dark => "bg.dark", Bg::Green => "bg.green", Bg::Gray => "bg.gray", Bg::White => "bg.white" })
    }
}

enum Drag {
    Move { starts: Vec<(String, f32, f32)>, start: Pos2 },
    Resize { id: String, handle: (i8, i8), orig: (f32, f32, f32, f32), rot: f32, start_mouse: Pos2 },
    Rotate { id: String, start_angle: f32, start_rot: f32 },
    Marquee { start: Pos2 },
    PathKey { layer: String, idx: usize },
    PathHandle { layer: String, idx: usize, out: bool },
    Pan,
}

#[derive(Clone, Copy)]
enum PathHit { Key(usize), Handle(usize, bool) }

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
    marquee: Option<Rect>,
    pub editing: Option<String>,
    edit_buf: String,
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
               guides_x: vec![], guides_y: vec![], marquee: None, editing: None, edit_buf: String::new(), path_sel: None, path_picked: None, overlay_keys: vec![], overlay_handles: vec![] }
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
    pub frame_origin: (f32, f32),
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

fn static_aff(l: &Layer, frame: (f32, f32)) -> Aff {
    let d = Transform::default();
    render::layer_to_field(l, (0.0, 0.0), frame, &d, &d)
}

fn contains_local(inv: &Aff, p: Pos2, l: &Layer) -> bool {
    let q = inv.apply(p);
    q.x >= 0.0 && q.y >= 0.0 && q.x <= l.w && q.y <= l.h
}

const HANDLES: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];

fn draw_checker(painter: &egui::Painter, rect: Rect, size: f32) {
    let mut mesh = egui::Mesh::default();
    let (c1, c2) = (Color32::from_gray(52), Color32::from_gray(66));
    let mut y = rect.min.y;
    let mut row = 0;
    while y < rect.max.y {
        let mut x = rect.min.x;
        let mut col = 0;
        while x < rect.max.x {
            let r = Rect::from_min_max(Pos2::new(x, y), Pos2::new((x + size).min(rect.max.x), (y + size).min(rect.max.y)));
            mesh.add_colored_rect(r, if (row + col) % 2 == 0 { c1 } else { c2 });
            x += size;
            col += 1;
        }
        y += size;
        row += 1;
    }
    painter.add(Shape::mesh(mesh));
}

pub fn draw_canvas(
    ui: &mut Ui,
    st: &mut CanvasState,
    scene: &mut Scene,
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
    let fo = inputs.frame_origin;
    let mut out = CanvasOut { edited_text: None, interacting: false };

    if st.needs_fit && rect.width() > 50.0 {
        st.fit_field(rect);
        st.needs_fit = false;
    }
    if st.needs_fit_frame && rect.width() > 50.0 {
        st.fit_frame(rect, fo, frame);
        st.needs_fit_frame = false;
    }

    // ── view navigation ──
    let hovered = response.hovered();
    if hovered {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y * 0.0);
        if scroll != 0.0 {
            if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
                let old = st.zoom;
                let new = (old * (1.0 + scroll * 0.0016)).clamp(0.08, 8.0);
                let rel = mp - rect.min - st.pan;
                st.pan = mp - rect.min - rel * (new / old);
                st.zoom = new;
            }
        }
    }
    let space = ui.input(|i| i.key_down(Key::Space));
    if response.dragged_by(PointerButton::Middle) || response.dragged_by(PointerButton::Secondary) || (space && response.dragged_by(PointerButton::Primary)) {
        st.pan += response.drag_delta();
        ctx.set_cursor_icon(CursorIcon::Grabbing);
    }
    let view = View { origin: rect.min + st.pan, zoom: st.zoom };
    let to_frame = |p: Pos2| -> Pos2 { let f = view.to_field(p); Pos2::new(f.x - fo.0, f.y - fo.1) };
    let view_aff = view.aff().mul(Aff::translate(fo.0, fo.1));

    // ── interaction ──
    let ptr = response.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.hover_pos()));
    let alt = ui.input(|i| i.modifiers.alt);
    let shift = ui.input(|i| i.modifiers.shift);
    let ctrl = ui.input(|i| i.modifiers.command);

    let single = if selected.len() == 1 { scene.layers.iter().find(|l| l.id == selected[0]).cloned() } else { None };
    let handle_screen = |l: &Layer, h: (i8, i8)| -> Pos2 {
        let m = view_aff.mul(static_aff(l, frame));
        m.apply(Pos2::new(l.w * (h.0 as f32 + 1.0) / 2.0, l.h * (h.1 as f32 + 1.0) / 2.0))
    };
    let rotate_handle = |l: &Layer| -> Pos2 {
        let m = view_aff.mul(static_aff(l, frame));
        let top = m.apply(Pos2::new(l.w / 2.0, 0.0));
        let ctr = m.apply(Pos2::new(l.w / 2.0, l.h / 2.0));
        let dir = (top - ctr).normalized();
        let dir = if dir.x.is_nan() { Vec2::new(0.0, -1.0) } else { dir };
        top + dir * 26.0
    };
    let pick_handle = |p: Pos2| -> Option<(i8, i8)> {
        let l = single.as_ref()?;
        if l.locked || st_editing_none(&st.editing) == false { return None; }
        HANDLES.iter().copied().find(|h| handle_screen(l, *h).distance(p) < 8.0)
    };
    let pick_rotate = |p: Pos2| -> bool {
        match &single { Some(l) if !l.locked => rotate_handle(l).distance(p) < 9.0, _ => false }
    };
    // ścieżka ruchu zaznaczonej warstwy: punkt (x, y) ścieżki to przesunięcie od środka warstwy
    let path_center = |l: &Layer| (l.x + l.w / 2.0, l.y + l.h / 2.0);
    let path_screen = |l: &Layer, x: f32, y: f32| -> Pos2 {
        let c = path_center(l);
        view.to_screen(Pos2::new(fo.0 + c.0 + x, fo.1 + c.1 + y))
    };
    let path_sel = st.path_sel;
    let pick_path = |p: Pos2| -> Option<PathHit> {
        let l = single.as_ref()?;
        let path = l.path.as_ref()?;
        if l.locked || !st_editing_none(&st.editing) || inputs.sp.playing { return None; }
        if let Some(i) = path_sel.filter(|i| *i < path.keys.len()) {
            let k = &path.keys[i];
            let (tin, tout) = path.tangents(i);
            for (out, t) in [(true, tout), (false, tin)] {
                let has_neighbour = if out { i + 1 < path.keys.len() } else { i > 0 };
                if has_neighbour && path_screen(l, k.x + t[0], k.y + t[1]).distance(p) < 7.0 { return Some(PathHit::Handle(i, out)); }
            }
        }
        path.keys.iter().enumerate().map(|(i, k)| (i, path_screen(l, k.x, k.y).distance(p))).filter(|(_, d)| *d < 9.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| PathHit::Key(i))
    };
    let hit_layer = |p: Pos2, scene: &Scene| -> Option<String> {
        let fp = to_frame(p);
        scene.layers.iter().rev().filter(|l| l.visible).find(|l| contains_local(&static_aff(l, frame).inverse(), fp, l)).map(|l| l.id.clone())
    };

    if st.editing.is_none() && !inputs.sp.playing {
        // hover cursor
        if let (Some(p), true) = (ui.input(|i| i.pointer.hover_pos()), hovered && st.drag.is_none()) {
            if let Some(h) = pick_handle(p) {
                ctx.set_cursor_icon(match h { (-1, -1) | (1, 1) => CursorIcon::ResizeNwSe, (1, -1) | (-1, 1) => CursorIcon::ResizeNeSw, (0, _) => CursorIcon::ResizeVertical, _ => CursorIcon::ResizeHorizontal });
            } else if pick_rotate(p) {
                ctx.set_cursor_icon(CursorIcon::Alias);
            } else if hit_layer(p, scene).is_some() {
                ctx.set_cursor_icon(CursorIcon::Move);
            }
        }

        if response.drag_started_by(PointerButton::Primary) && !space {
            let press = ui.input(|i| i.pointer.press_origin()).or(ptr);
            if let Some(p) = press {
                if let (Some(hit), Some(l)) = (pick_path(p), single.as_ref()) {
                    match hit {
                        PathHit::Key(i) => { st.path_sel = Some(i); st.path_picked = Some((l.id.clone(), i)); st.drag = Some(Drag::PathKey { layer: l.id.clone(), idx: i }); }
                        PathHit::Handle(i, out) => st.drag = Some(Drag::PathHandle { layer: l.id.clone(), idx: i, out }),
                    }
                } else if let Some(h) = pick_handle(p) {
                    let l = single.as_ref().unwrap();
                    st.drag = Some(Drag::Resize { id: l.id.clone(), handle: h, orig: (l.x, l.y, l.w, l.h), rot: l.rotation, start_mouse: to_frame(p) });
                } else if pick_rotate(p) {
                    let l = single.as_ref().unwrap();
                    let ctr = (view_aff.mul(static_aff(l, frame))).apply(Pos2::new(l.w / 2.0, l.h / 2.0));
                    let a = (p - ctr).angle().to_degrees();
                    st.drag = Some(Drag::Rotate { id: l.id.clone(), start_angle: a, start_rot: l.rotation });
                } else if let Some(id) = hit_layer(p, scene) {
                    if !selected.contains(&id) {
                        if shift || ctrl { selected.push(id.clone()); } else { *selected = vec![id.clone()]; }
                    }
                    let starts = scene.layers.iter().filter(|l| selected.contains(&l.id) && !l.locked).map(|l| (l.id.clone(), l.x, l.y)).collect();
                    st.drag = Some(Drag::Move { starts, start: to_frame(p) });
                } else {
                    if !(shift || ctrl) { selected.clear(); }
                    st.drag = Some(Drag::Marquee { start: p });
                }
            }
        }

        if response.dragged_by(PointerButton::Primary) && !space {
            if let (Some(p), Some(drag)) = (ptr, st.drag.take()) {
                out.interacting = true;
                st.guides_x.clear();
                st.guides_y.clear();
                match &drag {
                    Drag::Move { starts, start } => {
                        let mut d = to_frame(p) - *start;
                        if shift { if d.x.abs() > d.y.abs() { d.y = 0.0 } else { d.x = 0.0 } }
                        if st.snap && !alt {
                            let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                            for (id, sx, sy) in starts {
                                if let Some(l) = scene.layers.iter().find(|l| &l.id == id) {
                                    minx = minx.min(sx + d.x); miny = miny.min(sy + d.y);
                                    maxx = maxx.max(sx + d.x + l.w); maxy = maxy.max(sy + d.y + l.h);
                                }
                            }
                            let thr = 6.0 / st.zoom;
                            let mut xs = vec![0.0, frame.0 / 2.0, frame.0];
                            let mut ys = vec![0.0, frame.1 / 2.0, frame.1];
                            for l in scene.layers.iter().filter(|l| l.visible && !starts.iter().any(|(id, _, _)| *id == l.id)) {
                                xs.extend([l.x, l.x + l.w / 2.0, l.x + l.w]);
                                ys.extend([l.y, l.y + l.h / 2.0, l.y + l.h]);
                            }
                            let best = |edges: [f32; 3], cands: &[f32]| -> Option<(f32, f32)> {
                                let mut b: Option<(f32, f32)> = None;
                                for e in edges { for c in cands { let diff = c - e; if diff.abs() < thr && b.map_or(true, |(bd, _)| diff.abs() < bd.abs()) { b = Some((diff, *c)); } } }
                                b
                            };
                            if let Some((diff, line)) = best([minx, (minx + maxx) / 2.0, maxx], &xs) { d.x += diff; st.guides_x.push(line); }
                            if let Some((diff, line)) = best([miny, (miny + maxy) / 2.0, maxy], &ys) { d.y += diff; st.guides_y.push(line); }
                        }
                        for (id, sx, sy) in starts {
                            if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) {
                                l.x = (sx + d.x).round();
                                l.y = (sy + d.y).round();
                            }
                        }
                    }
                    Drag::Resize { id, handle, orig, rot, start_mouse } => {
                        let _ = start_mouse;
                        if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) {
                            let (ox, oy, ow, oh) = *orig;
                            let c0 = Pos2::new(ox + ow / 2.0, oy + oh / 2.0);
                            let r = Aff::rotate(*rot);
                            let rinv = Aff::rotate(-*rot);
                            let m_local = rinv.apply((to_frame(p) - c0).to_pos2());
                            let (sx, sy) = (handle.0 as f32, handle.1 as f32);
                            let (hw, hh) = (ow / 2.0, oh / 2.0);
                            let ax = -sx * hw;
                            let ay = -sy * hh;
                            let mut nw = if handle.0 != 0 { ((m_local.x - ax) * sx).max(1.0) } else { ow };
                            let mut nh = if handle.1 != 0 { ((m_local.y - ay) * sy).max(1.0) } else { oh };
                            if shift && handle.0 != 0 && handle.1 != 0 {
                                let ratio = ow / oh.max(0.001);
                                if nw / nh > ratio { nh = nw / ratio } else { nw = nh * ratio }
                            }
                            let off = Pos2::new(ax + if handle.0 != 0 { sx * nw / 2.0 } else { 0.0 }, ay + if handle.1 != 0 { sy * nh / 2.0 } else { 0.0 });
                            let wc = r.apply(off) + c0.to_vec2();
                            l.w = nw.round();
                            l.h = nh.round();
                            l.x = (wc.x - l.w / 2.0).round();
                            l.y = (wc.y - l.h / 2.0).round();
                        }
                    }
                    Drag::Rotate { id, start_angle, start_rot } => {
                        if let Some(l) = scene.layers.iter().find(|l| &l.id == id).cloned() {
                            let ctr = view_aff.mul(static_aff(&l, frame)).apply(Pos2::new(l.w / 2.0, l.h / 2.0));
                            let a = (p - ctr).angle().to_degrees();
                            let mut rot = start_rot + (a - start_angle);
                            if shift { rot = (rot / 15.0).round() * 15.0; }
                            if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) { l.rotation = (rot * 10.0).round() / 10.0; }
                        }
                    }
                    Drag::Marquee { start } => {
                        let r = Rect::from_two_pos(*start, p);
                        st.marquee = Some(r);
                        let mut hits = vec![];
                        for l in scene.layers.iter().filter(|l| l.visible) {
                            let m = view_aff.mul(static_aff(l, frame));
                            let pts = [Pos2::ZERO, Pos2::new(l.w, 0.0), Pos2::new(l.w, l.h), Pos2::new(0.0, l.h)].map(|q| m.apply(q));
                            let mut bb = Rect::from_min_max(pts[0], pts[0]);
                            for q in pts { bb.extend_with(q); }
                            if r.intersects(bb) { hits.push(l.id.clone()); }
                        }
                        *selected = hits;
                    }
                    Drag::PathKey { layer, idx } => {
                        let f = to_frame(p);
                        if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == layer) {
                            let c = (l.x + l.w / 2.0, l.y + l.h / 2.0);
                            if let Some(k) = l.path.as_mut().and_then(|pa| pa.keys.get_mut(*idx)) {
                                let (nx, ny) = (((f.x - c.0) * 2.0).round() / 2.0, ((f.y - c.1) * 2.0).round() / 2.0);
                                if shift { if (nx - k.x).abs() > (ny - k.y).abs() { k.x = nx } else { k.y = ny } } else { k.x = nx; k.y = ny; }
                            }
                        }
                    }
                    Drag::PathHandle { layer, idx, out } => {
                        let f = to_frame(p);
                        if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == layer) {
                            let c = (l.x + l.w / 2.0, l.y + l.h / 2.0);
                            // gładki węzeł: drugi uchwyt symetryczny, Alt rozdziela
                            if let Some(pa) = l.path.as_mut() { pa.set_tangent(*idx, *out, f.x - c.0, f.y - c.1, !alt); }
                        }
                    }
                    Drag::Pan => {}
                }
                st.drag = Some(drag);
            }
        }
        if response.drag_stopped() {
            st.drag = None;
            st.marquee = None;
            st.guides_x.clear();
            st.guides_y.clear();
        }

        if response.clicked_by(PointerButton::Primary) {
            if let Some(p) = response.interact_pointer_pos() {
                let path_hit = pick_path(p);
                if let (Some(PathHit::Key(i)), Some(l)) = (path_hit, single.as_ref()) {
                    st.path_sel = Some(i);
                    st.path_picked = Some((l.id.clone(), i));
                } else if path_hit.is_some() {
                    // uchwyt tangenta: bez zmiany zaznaczenia
                } else {
                match hit_layer(p, scene) {
                    Some(id) => {
                        if shift || ctrl { if let Some(i) = selected.iter().position(|s| *s == id) { selected.remove(i); } else { selected.push(id); } }
                        else { *selected = vec![id]; }
                    }
                    None => if !(shift || ctrl) { selected.clear(); },
                }
                }
            }
        }
        if response.double_clicked_by(PointerButton::Primary) {
            if let Some(p) = response.interact_pointer_pos() {
                if let Some(id) = hit_layer(p, scene) {
                    if let Some(l) = scene.layers.iter().find(|l| l.id == id) {
                        if l.layer_type == crate::model::LayerType::Text && !l.locked {
                            st.edit_buf = l.default_text.clone().unwrap_or_default();
                            st.editing = Some(id.clone());
                            *selected = vec![id];
                        }
                    }
                }
            }
        }
    }

    // ── draw ──
    let field_rect = Rect::from_min_size(view.origin, Vec2::new(FIELD_W, FIELD_H) * st.zoom);
    painter.rect_filled(rect, 0.0, Color32::from_gray(20));
    let fr = field_rect.intersect(rect);
    match st.bg {
        Bg::Checker => draw_checker(&painter.with_clip_rect(fr), field_rect, (40.0 * st.zoom).max(6.0)),
        Bg::Dark => { painter.rect_filled(fr, 0.0, Color32::from_gray(14)); }
        Bg::Green => { painter.rect_filled(fr, 0.0, Color32::from_rgb(0, 177, 64)); }
        Bg::Gray => { painter.rect_filled(fr, 0.0, Color32::from_gray(120)); }
        Bg::White => { painter.rect_filled(fr, 0.0, Color32::WHITE); }
    }
    let fpaint = painter.with_clip_rect(fr);
    if st.safe_areas {
        for (pct, a) in [(0.9f32, 90u8), (0.8f32, 60u8)] {
            let r = Rect::from_center_size(field_rect.center(), field_rect.size() * pct);
            fpaint.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 140, 255, a)));
        }
    }
    painter.rect_stroke(field_rect, 0.0, Stroke::new(1.0, Color32::from_gray(90)));

    // element frame outline
    let frame_screen = Rect::from_min_size(view.to_screen(Pos2::new(fo.0, fo.1)), Vec2::new(frame.0, frame.1) * st.zoom);
    painter.add(Shape::dashed_line(&[frame_screen.left_top(), frame_screen.right_top(), frame_screen.right_bottom(), frame_screen.left_bottom(), frame_screen.left_top()],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(250, 204, 21, 140)), 6.0, 4.0));

    let sp = SceneParams {
        scene,
        frame_origin: fo,
        elem_tr: inputs.sp.elem_tr,
        fx: inputs.sp.fx,
        values: render::Values { title: inputs.sp.values.title, fields: inputs.sp.values.fields, extra: inputs.sp.values.extra, base_url: inputs.sp.values.base_url },
        time: inputs.sp.time,
        glow: 0.0,
        badge_squash: inputs.sp.badge_squash,
    };
    render::draw_scene(&painter.with_clip_rect(rect), &ctx, &view, &sp);

    // guides
    for gx in &st.guides_x {
        let x = view.to_screen(Pos2::new(fo.0 + gx, 0.0)).x;
        painter.vline(x, rect.y_range(), Stroke::new(1.0, Color32::from_rgb(255, 0, 170)));
    }
    for gy in &st.guides_y {
        let y = view.to_screen(Pos2::new(0.0, fo.1 + gy)).y;
        painter.hline(rect.x_range(), y, Stroke::new(1.0, Color32::from_rgb(255, 0, 170)));
    }

    // selection overlay
    if !inputs.sp.playing {
        for l in scene.layers.iter().filter(|l| selected.contains(&l.id)) {
            let m = view_aff.mul(static_aff(l, frame));
            let pts: Vec<Pos2> = [Pos2::ZERO, Pos2::new(l.w, 0.0), Pos2::new(l.w, l.h), Pos2::new(0.0, l.h)].iter().map(|q| m.apply(*q)).collect();
            painter.add(Shape::closed_line(pts, Stroke::new(1.5, Color32::from_rgb(59, 130, 246))));
            if l.locked {
                painter.text(m.apply(Pos2::new(l.w, 0.0)), egui::Align2::LEFT_BOTTOM, "🔒", egui::FontId::proportional(12.0), Color32::from_rgb(250, 204, 21));
            }
        }
        if let Some(l) = &single {
            if let Some(l) = scene.layers.iter().find(|x| x.id == l.id) {
                if !l.locked && st.editing.is_none() {
                    for h in HANDLES {
                        let c = handle_screen(l, h);
                        let r = Rect::from_center_size(c, Vec2::splat(8.0));
                        painter.rect_filled(r, 1.0, Color32::WHITE);
                        painter.rect_stroke(r, 1.0, Stroke::new(1.5, Color32::from_rgb(59, 130, 246)));
                    }
                    let rh = rotate_handle(l);
                    let top = handle_screen(l, (0, -1));
                    painter.line_segment([top, rh], Stroke::new(1.0, Color32::from_rgb(59, 130, 246)));
                    painter.circle(rh, 5.0, Color32::WHITE, Stroke::new(1.5, Color32::from_rgb(59, 130, 246)));
                    let lab = format!("{}×{}  ({}, {})", l.w.round(), l.h.round(), l.x.round(), l.y.round());
                    let bottom = handle_screen(l, (0, 1)) + Vec2::new(0.0, 16.0);
                    painter.text(bottom, egui::Align2::CENTER_TOP, lab, egui::FontId::monospace(10.5), Color32::from_rgb(147, 197, 253));
                }
            }
        }
    }
    // nakładka ścieżki ruchu zaznaczonej warstwy
    if !inputs.sp.playing && st.editing.is_none() {
        if let Some(l) = single.as_ref().and_then(|s| scene.layers.iter().find(|x| x.id == s.id)) {
            if let Some(path) = l.path.as_ref().filter(|p| !p.keys.is_empty()) {
                let col = Color32::from_rgb(147, 197, 253);
                let pts: Vec<Pos2> = path.polyline(24).iter().map(|q| path_screen(l, q[0], q[1])).collect();
                if pts.len() > 1 { painter.add(Shape::line(pts, Stroke::new(1.6, col))); }
                let sel_i = st.path_sel.filter(|i| *i < path.keys.len());
                for (i, k) in path.keys.iter().enumerate() {
                    let c = path_screen(l, k.x, k.y);
                    let r = Rect::from_center_size(c, Vec2::splat(9.0));
                    let selected_key = sel_i == Some(i);
                    st.overlay_keys.push((i, c));
                    painter.rect(r, 1.0, if selected_key { Color32::from_rgb(251, 191, 36) } else { Color32::WHITE }, Stroke::new(1.5, col));
                    painter.text(c + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, format!("{:.2} s", k.t), egui::FontId::monospace(9.5), Color32::from_rgb(190, 215, 255));
                }
                if let Some(i) = sel_i {
                    let k = &path.keys[i];
                    let (tin, tout) = path.tangents(i);
                    let kc = path_screen(l, k.x, k.y);
                    for (out, t) in [(true, tout), (false, tin)] {
                        let has_neighbour = if out { i + 1 < path.keys.len() } else { i > 0 };
                        if !has_neighbour { continue; }
                        let hp = path_screen(l, k.x + t[0], k.y + t[1]);
                        st.overlay_handles.push((i, out, hp));
                        painter.line_segment([kc, hp], Stroke::new(1.0, Color32::from_rgb(250, 204, 21)));
                        painter.circle(hp, 4.0, Color32::from_rgb(20, 20, 20), Stroke::new(1.5, Color32::from_rgb(250, 204, 21)));
                    }
                }
            }
        }
    }
    if let Some(r) = st.marquee {
        painter.rect(r, 0.0, Color32::from_rgba_unmultiplied(59, 130, 246, 30), Stroke::new(1.0, Color32::from_rgb(59, 130, 246)));
    }

    // inline text editing overlay
    if let Some(id) = st.editing.clone() {
        if let Some(l) = scene.layers.iter().find(|l| l.id == id) {
            let m = view_aff.mul(static_aff(l, frame));
            let bb = {
                let pts = [Pos2::ZERO, Pos2::new(l.w, 0.0), Pos2::new(l.w, l.h), Pos2::new(0.0, l.h)].map(|q| m.apply(q));
                let mut r = Rect::from_min_max(pts[0], pts[0]);
                for q in pts { r.extend_with(q); }
                r
            };
            let er = Rect::from_min_size(bb.min, Vec2::new(bb.width().max(140.0), bb.height().max(26.0)));
            let resp = ui.put(er, egui::TextEdit::singleline(&mut st.edit_buf).font(egui::FontId::proportional(15.0)));
            resp.request_focus();
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            if enter || resp.lost_focus() && !esc {
                out.edited_text = Some((id, st.edit_buf.clone()));
                st.editing = None;
            } else if esc {
                st.editing = None;
            }
        } else {
            st.editing = None;
        }
    }

    // labels
    painter.text(field_rect.left_top() + Vec2::new(2.0, -3.0), egui::Align2::LEFT_BOTTOM, "1920 × 1080", egui::FontId::monospace(10.0), Color32::from_gray(110));
    painter.text(frame_screen.left_top() + Vec2::new(2.0, -3.0), egui::Align2::LEFT_BOTTOM, crate::i18n::trf("canvas.frame_label", &[("w", &frame.0.round()), ("h", &frame.1.round())]), egui::FontId::monospace(10.0), Color32::from_rgb(250, 204, 21));

    out
}

fn st_editing_none(e: &Option<String>) -> bool { e.is_none() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Element, LayerType};
    use egui::{Event, Modifiers};

    struct C {
        ctx: egui::Context,
        st: CanvasState,
        el: Element,
        selected: Vec<String>,
        time: f64,
        alt: bool,
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
            let mut c = Self { ctx: egui::Context::default(), st, el, selected: vec![id], time: 0.0, alt: false };
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
            let Self { ctx, st, el, selected, .. } = self;
            let fx = std::collections::HashMap::new();
            let title = el.title.clone();
            let fields = el.fields.clone();
            let extra = el.extra.clone();
            let fo = el.layout.frame_origin(el.scene.as_ref().unwrap().frame.w, el.scene.as_ref().unwrap().frame.h);
            let scene = el.scene.as_mut().unwrap();
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let values = render::Values { title: &title, fields: &fields, extra: &extra, base_url: "" };
                    draw_canvas(ui, st, scene, selected, DrawInputs { frame_origin: fo, sp: SceneParamsBuilder { elem_tr: Transform::default(), fx: &fx, values, time: 0.0, playing: false, badge_squash: false } });
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
}
