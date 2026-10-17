//! Wyrównanie warstw do ramki sceny lub do siebie. Liczone z prostokąta po obrocie (jak kanwa).
use super::heading;
use crate::canvas::layer_frame_bbox;
use crate::i18n::{tr, trf};
use crate::model::*;
use egui::{Color32, RichText, Ui};

type Bbox = (f32, f32, f32, f32);

fn shift(l: &mut Layer, dx: f32, dy: f32) { l.x += dx; l.y += dy; }

pub fn align_single(ui: &mut Ui, l: &mut Layer, frame: (f32, f32)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("align.to_frame")).small().color(Color32::from_gray(150)));
        let (x0, y0, x1, y1) = layer_frame_bbox(l, frame);
        let (w, h) = (x1 - x0, y1 - y0);
        if ui.small_button("L").on_hover_text(tr("align.left")).clicked() { shift(l, -x0, 0.0); }
        if ui.small_button("C").on_hover_text(tr("align.center_h")).clicked() { shift(l, ((frame.0 - w) / 2.0).round() - x0, 0.0); }
        if ui.small_button("R").on_hover_text(tr("align.right")).clicked() { shift(l, frame.0 - w - x0, 0.0); }
        if ui.small_button("T").on_hover_text(tr("align.top")).clicked() { shift(l, 0.0, -y0); }
        if ui.small_button("M").on_hover_text(tr("align.center_v")).clicked() { shift(l, 0.0, ((frame.1 - h) / 2.0).round() - y0); }
        if ui.small_button("B").on_hover_text(tr("align.bottom")).clicked() { shift(l, 0.0, frame.1 - h - y0); }
    });
}

pub fn align_tools(ui: &mut Ui, scene: &mut Scene, selected: &[String]) {
    heading(ui, &trf("align.multi", &[("n", &selected.len())]));
    let frame = (scene.frame.w, scene.frame.h);
    let mut act: Option<u8> = None;
    ui.horizontal(|ui| {
        for (i, (g, t)) in [("L", tr("align.m_left")), ("C", tr("align.m_center_h")), ("R", tr("align.m_right")), ("T", tr("align.m_top")), ("M", tr("align.m_center_v")), ("B", tr("align.m_bottom")), ("H", tr("align.distribute_h")), ("V", tr("align.distribute_v"))].iter().enumerate() {
            if ui.small_button(*g).on_hover_text(*t).clicked() { act = Some(i as u8); }
        }
    });
    let Some(a) = act else { return };
    let mut items: Vec<(usize, Bbox)> = scene.layers.iter().enumerate().filter(|(_, l)| selected.contains(&l.id)).map(|(i, l)| (i, layer_frame_bbox(l, frame))).collect();
    if items.is_empty() { return }
    let (minx, miny, maxx, maxy) = items.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |u, (_, b)| (u.0.min(b.0), u.1.min(b.1), u.2.max(b.2), u.3.max(b.3)));
    if a >= 6 {
        if items.len() < 3 { return }
        let horiz = a == 6;
        let (lo, hi) = if horiz { (minx, maxx) } else { (miny, maxy) };
        let span = |b: &Bbox| if horiz { (b.0, b.2) } else { (b.1, b.3) };
        items.sort_by(|p, q| span(&p.1).0.total_cmp(&span(&q.1).0));
        let total: f32 = items.iter().map(|(_, b)| span(b).1 - span(b).0).sum();
        let gap = ((hi - lo) - total) / (items.len() as f32 - 1.0);
        let mut pos = lo;
        for (i, b) in &items {
            let (s, e) = span(b);
            let d = pos.round() - s;
            if horiz { shift(&mut scene.layers[*i], d, 0.0) } else { shift(&mut scene.layers[*i], 0.0, d) }
            pos += (e - s) + gap;
        }
        return;
    }
    for (i, b) in &items {
        let (w, h) = (b.2 - b.0, b.3 - b.1);
        let l = &mut scene.layers[*i];
        match a {
            0 => shift(l, minx - b.0, 0.0),
            1 => shift(l, ((minx + maxx - w) / 2.0).round() - b.0, 0.0),
            2 => shift(l, maxx - w - b.0, 0.0),
            3 => shift(l, 0.0, miny - b.1),
            4 => shift(l, 0.0, ((miny + maxy - h) / 2.0).round() - b.1),
            _ => shift(l, 0.0, maxy - h - b.1),
        }
    }
}
