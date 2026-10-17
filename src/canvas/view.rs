//! Widok kanwy: tła, zoom kółkiem względem kursora i przesuwanie (pan).
use super::CanvasState;
use egui::{Color32, CursorIcon, Key, PointerButton, Pos2, Rect, Response, Shape, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bg { Checker, Dark, Green, Gray, White }
impl Bg {
    pub const ALL: [Bg; 5] = [Bg::Checker, Bg::Dark, Bg::Green, Bg::Gray, Bg::White];
    pub fn label(self) -> &'static str {
        crate::i18n::tr(match self { Bg::Checker => "bg.checker", Bg::Dark => "bg.dark", Bg::Green => "bg.green", Bg::Gray => "bg.gray", Bg::White => "bg.white" })
    }
}

pub(super) fn draw_checker(painter: &egui::Painter, rect: Rect, size: f32) {
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

/// Zoom kółkiem (punkt pod kursorem stoi w miejscu) i pan środkowym/prawym przyciskiem albo Spacja + LPM.
/// Zwraca, czy Spacja jest wciśnięta (wtedy LPM przesuwa widok zamiast edytować warstwy).
pub(super) fn navigate(ui: &Ui, st: &mut CanvasState, rect: Rect, response: &Response) -> bool {
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
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
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }
    space
}
