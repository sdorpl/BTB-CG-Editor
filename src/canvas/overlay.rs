//! Rysowanie nakładek kanwy: tło pola, ramka elementu, prowadnice, zaznaczenie z uchwytami, ścieżka ruchu, etykiety.
use super::hit::{Geo, HANDLES};
use super::view::draw_checker;
use super::{Bg, CanvasState, FIELD_H, FIELD_W};
use crate::model::Scene;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

const BLUE: Color32 = Color32::from_rgb(59, 130, 246);
const YELLOW: Color32 = Color32::from_rgb(250, 204, 21);

/// Tło całego widoku i pola 1920×1080 z polami bezpiecznymi; zwraca prostokąt pola na ekranie.
pub(super) fn draw_field(painter: &Painter, st: &CanvasState, rect: Rect, geo: &Geo) -> Rect {
    let field_rect = Rect::from_min_size(geo.view.origin, Vec2::new(FIELD_W, FIELD_H) * st.zoom);
    painter.rect_filled(rect, 0.0, Color32::from_gray(20));
    let fr = field_rect.intersect(rect);
    match st.bg {
        Bg::Checker => draw_checker(&painter.with_clip_rect(fr), field_rect, (40.0 * st.zoom).max(6.0)),
        Bg::Dark => { painter.rect_filled(fr, 0.0, Color32::from_gray(14)); }
        Bg::Green => { painter.rect_filled(fr, 0.0, Color32::from_rgb(0, 177, 64)); }
        Bg::Gray => { painter.rect_filled(fr, 0.0, Color32::from_gray(120)); }
        Bg::White => { painter.rect_filled(fr, 0.0, Color32::WHITE); }
    }
    if st.safe_areas {
        let fpaint = painter.with_clip_rect(fr);
        for (pct, a) in [(0.9f32, 90u8), (0.8f32, 60u8)] {
            let r = Rect::from_center_size(field_rect.center(), field_rect.size() * pct);
            fpaint.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 140, 255, a)));
        }
    }
    painter.rect_stroke(field_rect, 0.0, Stroke::new(1.0, Color32::from_gray(90)));
    field_rect
}

/// Przerywany obrys ramki elementu (z jego skalą i obrotem); zwraca lewy górny róg na ekranie.
pub(super) fn draw_frame_outline(painter: &Painter, geo: &Geo) -> Pos2 {
    let c = geo.frame_corners();
    painter.add(Shape::dashed_line(&[c[0], c[1], c[2], c[3], c[0]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(250, 204, 21, 140)), 6.0, 4.0));
    c[0]
}

pub(super) fn draw_guides(painter: &Painter, st: &CanvasState, rect: Rect, geo: &Geo) {
    let col = Color32::from_rgb(255, 0, 170);
    let line = |a: Pos2, b: Pos2| painter.line_segment([geo.view_aff.apply(a), geo.view_aff.apply(b)], Stroke::new(1.0, col));
    // długie odcinki w układzie ramki wypełniają widok także przy skali i obrocie elementu
    let far = (rect.width() + rect.height()) / st.zoom.max(0.01) * 4.0 / geo.scale.max(0.01);
    // prowadnice przyciągania elementu do pola: współrzędne pola, rysowane przez cały widok
    for gx in &st.field_guides_x { painter.vline(geo.view.to_screen(Pos2::new(*gx, 0.0)).x, rect.y_range(), Stroke::new(1.0, col)); }
    for gy in &st.field_guides_y { painter.hline(rect.x_range(), geo.view.to_screen(Pos2::new(0.0, *gy)).y, Stroke::new(1.0, col)); }
    for gx in &st.guides_x { line(Pos2::new(*gx, -far), Pos2::new(*gx, far)); }
    for gy in &st.guides_y { line(Pos2::new(-far, *gy), Pos2::new(far, *gy)); }
}

/// Obrysy zaznaczonych warstw oraz uchwyty skalowania, uchwyt obrotu i etykieta rozmiaru dla pojedynczej warstwy.
pub(super) fn draw_selection(painter: &Painter, geo: &Geo, scene: &Scene, selected: &[String], editing: bool) {
    for l in scene.layers.iter().filter(|l| selected.contains(&l.id)) {
        let m = geo.layer_to_screen(l);
        let pts: Vec<Pos2> = [Pos2::ZERO, Pos2::new(l.w, 0.0), Pos2::new(l.w, l.h), Pos2::new(0.0, l.h)].iter().map(|q| m.apply(*q)).collect();
        painter.add(Shape::closed_line(pts, Stroke::new(1.5, BLUE)));
        if l.locked {
            painter.text(m.apply(Pos2::new(l.w, 0.0)), egui::Align2::LEFT_BOTTOM, "🔒", egui::FontId::proportional(12.0), YELLOW);
        }
    }
    let Some(l) = geo.single.as_ref().and_then(|s| scene.layers.iter().find(|x| x.id == s.id)) else { return };
    if l.locked || editing { return; }
    for h in HANDLES {
        let r = Rect::from_center_size(geo.handle_screen(l, h), Vec2::splat(8.0));
        painter.rect_filled(r, 1.0, Color32::WHITE);
        painter.rect_stroke(r, 1.0, Stroke::new(1.5, BLUE));
    }
    let rh = geo.rotate_handle(l);
    painter.line_segment([geo.handle_screen(l, (0, -1)), rh], Stroke::new(1.0, BLUE));
    painter.circle(rh, 5.0, Color32::WHITE, Stroke::new(1.5, BLUE));
    let lab = format!("{}×{}  ({}, {})", l.w.round(), l.h.round(), l.x.round(), l.y.round());
    painter.text(geo.handle_screen(l, (0, 1)) + Vec2::new(0.0, 16.0), egui::Align2::CENTER_TOP, lab, egui::FontId::monospace(10.5), Color32::from_rgb(147, 197, 253));
}

/// Nakładka ścieżki ruchu zaznaczonej warstwy: krzywa, klucze z czasem i tangenty zaznaczonego klucza.
/// Zapisuje pozycje w `overlay_keys` / `overlay_handles` (testy klikają w te punkty).
pub(super) fn draw_path(painter: &Painter, st: &mut CanvasState, geo: &Geo, scene: &Scene) {
    let Some(l) = geo.single.as_ref().and_then(|s| scene.layers.iter().find(|x| x.id == s.id)) else { return };
    let Some(path) = l.path.as_ref().filter(|p| !p.keys.is_empty()) else { return };
    let col = Color32::from_rgb(147, 197, 253);
    let pts: Vec<Pos2> = path.polyline(24).iter().map(|q| geo.path_screen(l, q[0], q[1])).collect();
    if pts.len() > 1 { painter.add(Shape::line(pts, Stroke::new(1.6, col))); }
    let sel_i = st.path_sel.filter(|i| *i < path.keys.len());
    for (i, k) in path.keys.iter().enumerate() {
        let c = geo.path_screen(l, k.x, k.y);
        st.overlay_keys.push((i, c));
        painter.rect(Rect::from_center_size(c, Vec2::splat(9.0)), 1.0, if sel_i == Some(i) { Color32::from_rgb(251, 191, 36) } else { Color32::WHITE }, Stroke::new(1.5, col));
        painter.text(c + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, format!("{:.2} s", k.t), egui::FontId::monospace(9.5), Color32::from_rgb(190, 215, 255));
    }
    if let Some(i) = sel_i {
        let k = &path.keys[i];
        let (tin, tout) = path.tangents(i);
        let kc = geo.path_screen(l, k.x, k.y);
        for (out, t) in [(true, tout), (false, tin)] {
            let has_neighbour = if out { i + 1 < path.keys.len() } else { i > 0 };
            if !has_neighbour { continue; }
            let hp = geo.path_screen(l, k.x + t[0], k.y + t[1]);
            st.overlay_handles.push((i, out, hp));
            painter.line_segment([kc, hp], Stroke::new(1.0, YELLOW));
            painter.circle(hp, 4.0, Color32::from_rgb(20, 20, 20), Stroke::new(1.5, YELLOW));
        }
    }
}

pub(super) fn draw_marquee(painter: &Painter, st: &CanvasState) {
    if let Some(r) = st.marquee {
        painter.rect(r, 0.0, Color32::from_rgba_unmultiplied(59, 130, 246, 30), Stroke::new(1.0, BLUE));
    }
}

pub(super) fn draw_labels(painter: &Painter, field_rect: Rect, frame_top_left: Pos2, frame: (f32, f32)) {
    painter.text(field_rect.left_top() + Vec2::new(2.0, -3.0), egui::Align2::LEFT_BOTTOM, "1920 × 1080", egui::FontId::monospace(10.0), Color32::from_gray(110));
    painter.text(frame_top_left + Vec2::new(2.0, -3.0), egui::Align2::LEFT_BOTTOM, crate::i18n::trf("canvas.frame_label", &[("w", &frame.0.round()), ("h", &frame.1.round())]), egui::FontId::monospace(10.0), YELLOW);
}

/// Zaznaczony cały element: obrys ramki (po skali i obrocie), uchwyty skali na trzech narożnikach, uchwyt obrotu nad
/// górną krawędzią (lewy górny róg to punkt obrotu i skali) oraz etykieta ze skalą, obrotem i położeniem.
pub(super) fn draw_element_selection(painter: &Painter, geo: &Geo, layout: &crate::model::Layout) {
    if !geo.element_sel { return; }
    let c = geo.frame_corners();
    painter.add(Shape::closed_line(c.to_vec(), Stroke::new(1.5, YELLOW)));
    for h in geo.element_scale_handles() {
        let r = Rect::from_center_size(h, Vec2::splat(9.0));
        painter.rect_filled(r, 1.0, Color32::WHITE);
        painter.rect_stroke(r, 1.0, Stroke::new(1.5, YELLOW));
    }
    painter.circle(c[0], 4.0, YELLOW, Stroke::new(1.0, Color32::from_gray(30)));
    let (mid, rh) = geo.element_rotate_handle();
    painter.line_segment([mid, rh], Stroke::new(1.0, YELLOW));
    painter.circle(rh, 5.0, Color32::WHITE, Stroke::new(1.5, YELLOW));
    let (x, y) = layout.frame_origin(geo.frame.0, geo.frame.1);
    let lab = format!("{:.0}%  {:.1}°  ({}, {})", layout.scale * 100.0, layout.rotation, x.round(), y.round());
    painter.text(c[3] + (c[2] - c[3]) * 0.5 + Vec2::new(0.0, 16.0), egui::Align2::CENTER_TOP, lab, egui::FontId::monospace(10.5), YELLOW);
}
