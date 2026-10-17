//! Narzędzia edycji na kanwie: przesuwanie z przyciąganiem, skalowanie, obrót, ramka zaznaczenia i edycja ścieżki ruchu.
//! `interact` rozpoznaje początek przeciągnięcia (kolejność: ścieżka, uchwyt skalowania, uchwyt obrotu, warstwa, ramka),
//! aktualizuje aktywne narzędzie i obsługuje klik oraz dwuklik.
use super::hit::{frame_bbox, ElementHit, Geo, PathHit};
use super::{CanvasOut, CanvasState};
use crate::model::{Layer, LayerType, Layout, Scene};
use super::{FIELD_H, FIELD_W};
use crate::render::{self, Aff};
use crate::motion_bridge::ELEMENT;
use egui::{CursorIcon, PointerButton, Pos2, Rect, Response, Ui, Vec2};

pub(super) enum Drag {
    Move { starts: Vec<(String, f32, f32)>, start: Pos2 },
    Resize { id: String, handle: (i8, i8), orig: (f32, f32, f32, f32), rot: f32 },
    Rotate { id: String, start_angle: f32, start_rot: f32 },
    Marquee { start: Pos2 },
    PathKey { layer: String, idx: usize },
    PathHandle { layer: String, idx: usize, out: bool },
    /// cały element: przesuwanie ramki na wizji (z migawką layoutu i punktem startu w polu)
    ElementMove { start: Pos2, orig: Layout },
    /// skala wokół lewego górnego rogu ramki: stosunek odległości kursora od punktu obrotu
    ElementScale { start_dist: f32, orig_scale: f32 },
    ElementRotate { start_angle: f32, start_rot: f32 },
}

#[derive(Clone, Copy)]
struct Mods { alt: bool, shift: bool, ctrl: bool }

pub(super) fn interact(ui: &Ui, st: &mut CanvasState, geo: &Geo, scene: &mut Scene, layout: &mut Layout, selected: &mut Vec<String>, response: &Response, space: bool, out: &mut CanvasOut) {
    let ctx = ui.ctx();
    let ptr = response.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.hover_pos()));
    let mods = ui.input(|i| Mods { alt: i.modifiers.alt, shift: i.modifiers.shift, ctrl: i.modifiers.command });

    // kursor nad uchwytami i warstwami
    if let (Some(p), true) = (ui.input(|i| i.pointer.hover_pos()), response.hovered() && st.drag.is_none()) {
        if let Some(h) = geo.pick_element(p) {
            ctx.set_cursor_icon(match h { ElementHit::Scale => CursorIcon::ResizeNwSe, ElementHit::Rotate => CursorIcon::Alias });
        } else if geo.element_sel && geo.inside_frame(p) {
            ctx.set_cursor_icon(CursorIcon::Move);
        } else if let Some(h) = geo.pick_handle(p) {
            ctx.set_cursor_icon(match h { (-1, -1) | (1, 1) => CursorIcon::ResizeNwSe, (1, -1) | (-1, 1) => CursorIcon::ResizeNeSw, (0, _) => CursorIcon::ResizeVertical, _ => CursorIcon::ResizeHorizontal });
        } else if geo.pick_rotate(p) {
            ctx.set_cursor_icon(CursorIcon::Alias);
        } else if geo.hit_layer(p, scene).is_some() {
            ctx.set_cursor_icon(CursorIcon::Move);
        }
    }

    if response.drag_started_by(PointerButton::Primary) && !space {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(ptr) { begin(st, geo, scene, layout, selected, p, mods); }
    }

    if response.dragged_by(PointerButton::Primary) && !space {
        if let (Some(p), Some(drag)) = (ptr, st.drag.take()) {
            out.interacting = true;
            st.guides_x.clear();
            st.guides_y.clear();
            st.field_guides_x.clear();
            st.field_guides_y.clear();
            update(st, geo, scene, layout, selected, &drag, p, mods);
            st.drag = Some(drag);
        }
    }
    if response.drag_stopped() {
        st.drag = None;
        st.marquee = None;
        st.guides_x.clear();
        st.guides_y.clear();
        st.field_guides_x.clear();
        st.field_guides_y.clear();
    }

    if response.clicked_by(PointerButton::Primary) {
        if let Some(p) = response.interact_pointer_pos() { click(st, geo, scene, selected, p, mods); }
    }
    if response.double_clicked_by(PointerButton::Primary) {
        if let Some(p) = response.interact_pointer_pos() { double_click(st, geo, scene, selected, p); }
    }
}

fn begin(st: &mut CanvasState, geo: &Geo, scene: &Scene, layout: &Layout, selected: &mut Vec<String>, p: Pos2, mods: Mods) {
    if geo.element_sel {
        let pivot = geo.frame_corners()[0];
        match geo.pick_element(p) {
            Some(ElementHit::Scale) => { st.drag = Some(Drag::ElementScale { start_dist: (p - pivot).length(), orig_scale: layout.scale }); return; }
            Some(ElementHit::Rotate) => { st.drag = Some(Drag::ElementRotate { start_angle: (p - pivot).angle().to_degrees(), start_rot: layout.rotation }); return; }
            None if geo.inside_frame(p) => { st.drag = Some(Drag::ElementMove { start: geo.view.to_field(p), orig: layout.clone() }); return; }
            None => {}
        }
    }
    if let (Some(hit), Some(l)) = (geo.pick_path(p), geo.single.as_ref()) {
        match hit {
            PathHit::Key(i) => { st.path_sel = Some(i); st.path_picked = Some((l.id.clone(), i)); st.drag = Some(Drag::PathKey { layer: l.id.clone(), idx: i }); }
            PathHit::Handle(i, out) => st.drag = Some(Drag::PathHandle { layer: l.id.clone(), idx: i, out }),
        }
    } else if let (Some(h), Some(l)) = (geo.pick_handle(p), geo.single.as_ref()) {
        st.drag = Some(Drag::Resize { id: l.id.clone(), handle: h, orig: (l.x, l.y, l.w, l.h), rot: l.rotation });
    } else if let (true, Some(l)) = (geo.pick_rotate(p), geo.single.as_ref()) {
        let a = (p - geo.center_screen(l)).angle().to_degrees();
        st.drag = Some(Drag::Rotate { id: l.id.clone(), start_angle: a, start_rot: l.rotation });
    } else if let Some(id) = geo.hit_layer(p, scene) {
        if !selected.contains(&id) {
            if mods.shift || mods.ctrl { selected.push(id.clone()); } else { *selected = vec![id.clone()]; }
        }
        let starts = scene.layers.iter().filter(|l| selected.contains(&l.id) && !l.locked).map(|l| (l.id.clone(), l.x, l.y)).collect();
        st.drag = Some(Drag::Move { starts, start: geo.to_frame(p) });
    } else {
        if !(mods.shift || mods.ctrl) { selected.clear(); }
        st.drag = Some(Drag::Marquee { start: p });
    }
}

fn update(st: &mut CanvasState, geo: &Geo, scene: &mut Scene, layout: &mut Layout, selected: &mut Vec<String>, drag: &Drag, p: Pos2, mods: Mods) {
    match drag {
        Drag::ElementMove { start, orig } => move_element(st, geo, layout, orig, geo.view.to_field(p) - *start, mods),
        Drag::ElementScale { start_dist, orig_scale } => {
            let pivot = geo.frame_corners()[0];
            layout.scale = (((orig_scale * (p - pivot).length() / start_dist.max(1.0)).clamp(0.05, 10.0)) * 1000.0).round() / 1000.0;
        }
        Drag::ElementRotate { start_angle, start_rot } => {
            let pivot = geo.frame_corners()[0];
            let mut rot = start_rot + ((p - pivot).angle().to_degrees() - start_angle);
            if mods.shift { rot = (rot / 15.0).round() * 15.0; }
            layout.rotation = (rot * 10.0).round() / 10.0;
        }
        Drag::Move { starts, start } => move_layers(st, geo, scene, starts, geo.to_frame(p) - *start, mods),
        Drag::Resize { id, handle, orig, rot } => {
            if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) { resize_layer(l, *handle, *orig, *rot, geo.to_frame(p), mods.shift); }
        }
        Drag::Rotate { id, start_angle, start_rot } => {
            if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) {
                let a = (p - geo.center_screen(l)).angle().to_degrees();
                let mut rot = start_rot + (a - start_angle);
                if mods.shift { rot = (rot / 15.0).round() * 15.0; }
                l.rotation = (rot * 10.0).round() / 10.0;
            }
        }
        Drag::Marquee { start } => {
            let r = Rect::from_two_pos(*start, p);
            st.marquee = Some(r);
            *selected = scene.layers.iter().filter(|l| l.visible && r.intersects(geo.bbox_screen(l))).map(|l| l.id.clone()).collect();
        }
        Drag::PathKey { layer, idx } => {
            let f = geo.to_frame(p);
            if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == layer) {
                let c = (l.x + l.w / 2.0, l.y + l.h / 2.0);
                if let Some(k) = l.path.as_mut().and_then(|pa| pa.keys.get_mut(*idx)) {
                    let (nx, ny) = (((f.x - c.0) * 2.0).round() / 2.0, ((f.y - c.1) * 2.0).round() / 2.0);
                    if mods.shift { if (nx - k.x).abs() > (ny - k.y).abs() { k.x = nx } else { k.y = ny } } else { k.x = nx; k.y = ny; }
                }
            }
        }
        Drag::PathHandle { layer, idx, out } => {
            let f = geo.to_frame(p);
            if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == layer) {
                let c = (l.x + l.w / 2.0, l.y + l.h / 2.0);
                // gładki węzeł: drugi uchwyt symetryczny, Alt rozdziela
                if let Some(pa) = l.path.as_mut() { pa.set_tangent(*idx, *out, f.x - c.0, f.y - c.1, !mods.alt); }
            }
        }
    }
}

/// Przesuwa zaznaczone warstwy o `d` (współrzędne ramki). Shift blokuje oś, a przyciąganie (bez Alt) dosuwa
/// krawędzie i środek pudełka grupy do krawędzi/środków ramki i innych widocznych warstw, zapisując prowadnice.
fn move_layers(st: &mut CanvasState, geo: &Geo, scene: &mut Scene, starts: &[(String, f32, f32)], mut d: Vec2, mods: Mods) {
    if mods.shift { if d.x.abs() > d.y.abs() { d.y = 0.0 } else { d.x = 0.0 } }
    if st.snap && !mods.alt {
        // pudełko grupy po obrocie warstw, przesunięte o `d` względem pozycji startowych
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (id, sx, sy) in starts {
            if let Some(l) = scene.layers.iter().find(|l| &l.id == id) {
                let (bx0, by0, bx1, by1) = frame_bbox(l, geo.frame);
                let (ox, oy) = (sx + d.x - l.x, sy + d.y - l.y);
                minx = minx.min(bx0 + ox); miny = miny.min(by0 + oy);
                maxx = maxx.max(bx1 + ox); maxy = maxy.max(by1 + oy);
            }
        }
        let thr = 6.0 / st.zoom;
        let mut xs = vec![0.0, geo.frame.0 / 2.0, geo.frame.0];
        let mut ys = vec![0.0, geo.frame.1 / 2.0, geo.frame.1];
        for l in scene.layers.iter().filter(|l| l.visible && !starts.iter().any(|(id, _, _)| *id == l.id)) {
            let (bx0, by0, bx1, by1) = frame_bbox(l, geo.frame);
            xs.extend([bx0, (bx0 + bx1) / 2.0, bx1]);
            ys.extend([by0, (by0 + by1) / 2.0, by1]);
        }
        if let Some((diff, line)) = nearest_snap([minx, (minx + maxx) / 2.0, maxx], &xs, thr) { d.x += diff; st.guides_x.push(line); }
        if let Some((diff, line)) = nearest_snap([miny, (miny + maxy) / 2.0, maxy], &ys, thr) { d.y += diff; st.guides_y.push(line); }
    }
    for (id, sx, sy) in starts {
        if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) {
            l.x = (sx + d.x).round();
            l.y = (sy + d.y).round();
        }
    }
}

/// Przesuwa cały element o `d` (piksele pola) względem migawki layoutu `orig`. Shift blokuje oś; przyciąganie (bez Alt)
/// dosuwa krawędzie i środek ramki (po skali i obrocie) do krawędzi i środka pola 1920×1080.
fn move_element(st: &mut CanvasState, geo: &Geo, layout: &mut Layout, orig: &Layout, mut d: Vec2, mods: Mods) {
    if mods.shift { if d.x.abs() > d.y.abs() { d.y = 0.0 } else { d.x = 0.0 } }
    if st.snap && !mods.alt {
        let m = render::frame_aff(orig.frame_origin(geo.frame.0, geo.frame.1), orig.scale, orig.rotation);
        let (w, h) = geo.frame;
        let pts = [Pos2::ZERO, Pos2::new(w, 0.0), Pos2::new(w, h), Pos2::new(0.0, h)].map(|q| m.apply(q));
        let b = pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, q| (b.0.min(q.x), b.1.min(q.y), b.2.max(q.x), b.3.max(q.y)));
        let thr = 6.0 / st.zoom;
        if let Some((diff, line)) = nearest_snap([b.0 + d.x, (b.0 + b.2) / 2.0 + d.x, b.2 + d.x], &[0.0, FIELD_W / 2.0, FIELD_W], thr) { d.x += diff; st.field_guides_x.push(line); }
        if let Some((diff, line)) = nearest_snap([b.1 + d.y, (b.1 + b.3) / 2.0 + d.y, b.3 + d.y], &[0.0, FIELD_H / 2.0, FIELD_H], thr) { d.y += diff; st.field_guides_y.push(line); }
    }
    *layout = orig.clone();
    layout.move_by(d.x.round(), d.y.round(), geo.frame.0, geo.frame.1);
}

/// Najbliższa para (krawędź, kandydat) w granicy `thr`: zwraca poprawkę do dodania i linię prowadnicy.
fn nearest_snap(edges: [f32; 3], cands: &[f32], thr: f32) -> Option<(f32, f32)> {
    let mut b: Option<(f32, f32)> = None;
    for e in edges { for c in cands { let diff = c - e; if diff.abs() < thr && b.map_or(true, |(bd, _)| diff.abs() < bd.abs()) { b = Some((diff, *c)); } } }
    b
}

/// Skalowanie uchwytem `handle` obróconej warstwy: przeciwległy róg/krawędź stoi w miejscu, Shift zachowuje
/// proporcje na rogach. `orig` = (x, y, w, h) z początku przeciągania, `mouse` we współrzędnych ramki.
fn resize_layer(l: &mut Layer, handle: (i8, i8), orig: (f32, f32, f32, f32), rot: f32, mouse: Pos2, shift: bool) {
    let (ox, oy, ow, oh) = orig;
    let c0 = Pos2::new(ox + ow / 2.0, oy + oh / 2.0);
    let r = Aff::rotate(rot);
    let m_local = Aff::rotate(-rot).apply((mouse - c0).to_pos2());
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

fn click(st: &mut CanvasState, geo: &Geo, scene: &Scene, selected: &mut Vec<String>, p: Pos2, mods: Mods) {
    // Alt+klik wybiera cały element, także gdy warstwy zakrywają ramkę
    if mods.alt && geo.inside_frame(p) { *selected = vec![ELEMENT.to_string()]; return; }
    let path_hit = geo.pick_path(p);
    if let (Some(PathHit::Key(i)), Some(l)) = (path_hit, geo.single.as_ref()) {
        st.path_sel = Some(i);
        st.path_picked = Some((l.id.clone(), i));
    } else if path_hit.is_some() {
        // uchwyt tangenta: bez zmiany zaznaczenia
    } else {
        match geo.hit_layer(p, scene) {
            Some(id) => {
                if mods.shift || mods.ctrl { if let Some(i) = selected.iter().position(|s| *s == id) { selected.remove(i); } else { selected.push(id); } }
                else { *selected = vec![id]; }
            }
            // puste miejsce w ramce zaznacza cały element, poza ramką czyści zaznaczenie
            None => if !(mods.shift || mods.ctrl) {
                if geo.inside_frame(p) { *selected = vec![ELEMENT.to_string()]; } else { selected.clear(); }
            },
        }
    }
}

/// Dwuklik na niezablokowanej warstwie tekstowej włącza edycję w miejscu.
fn double_click(st: &mut CanvasState, geo: &Geo, scene: &Scene, selected: &mut Vec<String>, p: Pos2) {
    let Some(id) = geo.hit_layer(p, scene) else { return };
    let Some(l) = scene.layers.iter().find(|l| l.id == id) else { return };
    if l.layer_type == LayerType::Text && !l.locked {
        st.edit_buf = if l.field.as_deref().is_some_and(|f| !f.is_empty()) { l.default_text.clone() } else { l.text.clone() }.unwrap_or_default();
        st.editing = Some(id.clone());
        st.edit_fresh = true;
        *selected = vec![id];
    }
}
