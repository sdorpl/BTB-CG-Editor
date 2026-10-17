//! Geometria i trafianie (hit-testing) na kanwie: macierze warstw, uchwyty zaznaczenia, klucze ścieżki.
use crate::anim::Transform;
use crate::model::{Layer, Scene};
use crate::render::{self, Aff, View};
use egui::{Pos2, Vec2};

pub(super) const HANDLES: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];

/// Macierz warstwy -> pole bez animacji (transformy domyślne).
pub(super) fn static_aff(l: &Layer, frame: (f32, f32)) -> Aff {
    let d = Transform::default();
    render::layer_to_field(l, &Aff::ID, frame, &d, &d)
}

/// Prostokąt otaczający warstwę po obrocie we współrzędnych ramki: (min_x, min_y, max_x, max_y).
pub(super) fn frame_bbox(l: &Layer, frame: (f32, f32)) -> (f32, f32, f32, f32) {
    let m = static_aff(l, frame);
    let pts = [Pos2::ZERO, Pos2::new(l.w, 0.0), Pos2::new(l.w, l.h), Pos2::new(0.0, l.h)].map(|q| m.apply(q));
    pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, q| (b.0.min(q.x), b.1.min(q.y), b.2.max(q.x), b.3.max(q.y)))
}

fn contains_local(inv: &Aff, p: Pos2, l: &Layer) -> bool {
    let q = inv.apply(p);
    q.x >= 0.0 && q.y >= 0.0 && q.x <= l.w && q.y <= l.h
}

#[derive(Clone, Copy)]
pub(super) enum ElementHit { Scale, Rotate }

#[derive(Clone, Copy)]
pub(super) enum PathHit { Key(usize), Handle(usize, bool) }

/// Stan widoku i zaznaczenia z początku klatki, potrzebny do przeliczeń ekran <-> ramka i do trafiania.
pub(super) struct Geo {
    pub view: View,
    /// ramka elementu -> pole (położenie, skala i obrót z layoutu)
    pub frame_aff: Aff,
    /// współrzędne ramki -> ekran
    pub view_aff: Aff,
    pub frame: (f32, f32),
    /// skala elementu z layoutu
    pub scale: f32,
    /// zaznaczony jest cały element (a nie jego warstwy)
    pub element_sel: bool,
    /// jedyna zaznaczona warstwa (uchwyty i ścieżka działają tylko dla pojedynczego zaznaczenia)
    pub single: Option<Layer>,
    pub path_sel: Option<usize>,
    pub editing: bool,
    pub playing: bool,
}

impl Geo {
    pub fn to_frame(&self, p: Pos2) -> Pos2 {
        self.frame_aff.inverse().apply(self.view.to_field(p))
    }

    /// Narożniki ramki elementu na ekranie: lewy górny, prawy górny, prawy dolny, lewy dolny.
    pub fn frame_corners(&self) -> [Pos2; 4] {
        let (w, h) = self.frame;
        [Pos2::ZERO, Pos2::new(w, 0.0), Pos2::new(w, h), Pos2::new(0.0, h)].map(|q| self.view_aff.apply(q))
    }

    /// Uchwyty skali zaznaczonego elementu: prawy górny, prawy dolny i lewy dolny róg (lewy górny to punkt obrotu i skali).
    pub fn element_scale_handles(&self) -> [Pos2; 3] {
        let c = self.frame_corners();
        [c[1], c[2], c[3]]
    }

    /// Uchwyt obrotu elementu: 26 px nad środkiem górnej krawędzi (w kierunku „góry" obróconej ramki).
    pub fn element_rotate_handle(&self) -> (Pos2, Pos2) {
        let c = self.frame_corners();
        let mid = c[0] + (c[1] - c[0]) * 0.5;
        let dir = (c[1] - c[0]).normalized();
        let dir = if dir.x.is_nan() { Vec2::new(1.0, 0.0) } else { dir };
        (mid, mid + Vec2::new(dir.y, -dir.x) * 26.0)
    }

    pub fn inside_frame(&self, p: Pos2) -> bool {
        let f = self.to_frame(p);
        f.x >= 0.0 && f.y >= 0.0 && f.x <= self.frame.0 && f.y <= self.frame.1
    }

    pub fn pick_element(&self, p: Pos2) -> Option<ElementHit> {
        if !self.element_sel || self.editing || self.playing { return None; }
        if self.element_rotate_handle().1.distance(p) < 9.0 { return Some(ElementHit::Rotate); }
        self.element_scale_handles().iter().any(|h| h.distance(p) < 8.0).then_some(ElementHit::Scale)
    }

    pub fn layer_to_screen(&self, l: &Layer) -> Aff { self.view_aff.mul(static_aff(l, self.frame)) }

    /// Środek warstwy na ekranie.
    pub fn center_screen(&self, l: &Layer) -> Pos2 { self.layer_to_screen(l).apply(Pos2::new(l.w / 2.0, l.h / 2.0)) }

    /// Prostokąt otaczający (po obrocie) warstwy na ekranie.
    pub fn bbox_screen(&self, l: &Layer) -> egui::Rect {
        let m = self.layer_to_screen(l);
        let pts = [Pos2::ZERO, Pos2::new(l.w, 0.0), Pos2::new(l.w, l.h), Pos2::new(0.0, l.h)].map(|q| m.apply(q));
        let mut bb = egui::Rect::from_min_max(pts[0], pts[0]);
        for q in pts { bb.extend_with(q); }
        bb
    }

    pub fn handle_screen(&self, l: &Layer, h: (i8, i8)) -> Pos2 {
        self.layer_to_screen(l).apply(Pos2::new(l.w * (h.0 as f32 + 1.0) / 2.0, l.h * (h.1 as f32 + 1.0) / 2.0))
    }

    pub fn rotate_handle(&self, l: &Layer) -> Pos2 {
        let m = self.layer_to_screen(l);
        let top = m.apply(Pos2::new(l.w / 2.0, 0.0));
        let ctr = m.apply(Pos2::new(l.w / 2.0, l.h / 2.0));
        let dir = (top - ctr).normalized();
        let dir = if dir.x.is_nan() { Vec2::new(0.0, -1.0) } else { dir };
        top + dir * 26.0
    }

    pub fn pick_handle(&self, p: Pos2) -> Option<(i8, i8)> {
        let l = self.single.as_ref()?;
        if l.locked || self.editing { return None; }
        HANDLES.iter().copied().find(|h| self.handle_screen(l, *h).distance(p) < 8.0)
    }

    pub fn pick_rotate(&self, p: Pos2) -> bool {
        match &self.single { Some(l) if !l.locked => self.rotate_handle(l).distance(p) < 9.0, _ => false }
    }

    /// Punkt (x, y) ścieżki ruchu to przesunięcie od środka warstwy.
    pub fn path_screen(&self, l: &Layer, x: f32, y: f32) -> Pos2 {
        let (cx, cy) = (l.x + l.w / 2.0, l.y + l.h / 2.0);
        self.view_aff.apply(Pos2::new(cx + x, cy + y))
    }

    pub fn pick_path(&self, p: Pos2) -> Option<PathHit> {
        let l = self.single.as_ref()?;
        let path = l.path.as_ref()?;
        if l.locked || self.editing || self.playing { return None; }
        if let Some(i) = self.path_sel.filter(|i| *i < path.keys.len()) {
            let k = &path.keys[i];
            let (tin, tout) = path.tangents(i);
            for (out, t) in [(true, tout), (false, tin)] {
                let has_neighbour = if out { i + 1 < path.keys.len() } else { i > 0 };
                if has_neighbour && self.path_screen(l, k.x + t[0], k.y + t[1]).distance(p) < 7.0 { return Some(PathHit::Handle(i, out)); }
            }
        }
        path.keys.iter().enumerate().map(|(i, k)| (i, self.path_screen(l, k.x, k.y).distance(p))).filter(|(_, d)| *d < 9.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| PathHit::Key(i))
    }

    /// Najwyższa widoczna warstwa pod punktem ekranu.
    pub fn hit_layer(&self, p: Pos2, scene: &Scene) -> Option<String> {
        let fp = self.to_frame(p);
        scene.layers.iter().rev().filter(|l| l.visible).find(|l| contains_local(&static_aff(l, self.frame).inverse(), fp, l)).map(|l| l.id.clone())
    }
}
