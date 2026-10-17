//! Software renderer for scene layers on top of egui meshes: gradients, rounded rects, skew/rotate
//! via a full affine, real text layout (Bahnschrift) with per-glyph effects, images, clock, ticker.
use crate::anim::{self, Transform, UnitFx};
use crate::i18n::{tr, trf};
use crate::model::*;
use egui::epaint::{Vertex, WHITE_UV};
use egui::load::{SizeHint, TexturePoll};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Context, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Shape, TextureId, TextureOptions, Vec2};
use serde_json::Value;
use std::collections::HashMap;

// ── Affine ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct Aff { pub a: f32, pub b: f32, pub c: f32, pub d: f32, pub e: f32, pub f: f32 }

impl Aff {
    pub const ID: Aff = Aff { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };
    pub fn translate(x: f32, y: f32) -> Aff { Aff { e: x, f: y, ..Aff::ID } }
    pub fn scale(x: f32, y: f32) -> Aff { Aff { a: x, d: y, ..Aff::ID } }
    pub fn rotate(deg: f32) -> Aff {
        let (s, c) = deg.to_radians().sin_cos();
        Aff { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }
    pub fn skew_x(deg: f32) -> Aff { Aff { c: deg.to_radians().tan(), ..Aff::ID } }
    /// self ∘ o (o applied first)
    pub fn mul(self, o: Aff) -> Aff {
        Aff {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }
    pub fn apply(&self, p: Pos2) -> Pos2 {
        Pos2::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }
    pub fn inverse(&self) -> Aff {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-9 { return Aff::ID; }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Aff { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) }
    }
}

/// Transform `t` applied about its origin inside a box of size (w,h).
pub fn motion_aff(t: &Transform, w: f32, h: f32) -> Aff {
    let (ox, oy) = (t.origin.0 * w, t.origin.1 * h);
    Aff::translate(t.tx, t.ty)
        .mul(Aff::translate(ox, oy))
        .mul(Aff::rotate(t.rot))
        .mul(Aff::skew_x(t.skew))
        .mul(Aff::scale(t.sx * t.flip_x, t.sy * t.flip_y))
        .mul(Aff::translate(-ox, -oy))
}

/// Layer-local (0..w, 0..h) → field coordinates.
pub fn layer_to_field(l: &Layer, frame_origin: (f32, f32), frame: (f32, f32), elem: &Transform, tr: &Transform) -> Aff {
    let c = (l.w / 2.0, l.h / 2.0);
    let inner = Aff::translate(c.0, c.1).mul(Aff::skew_x(l.skew)).mul(Aff::rotate(l.rotation)).mul(Aff::translate(-c.0, -c.1));
    let local = Aff::translate(l.x, l.y).mul(motion_aff(tr, l.w, l.h)).mul(inner);
    Aff::translate(frame_origin.0, frame_origin.1).mul(motion_aff(elem, frame.0, frame.1)).mul(local)
}

#[derive(Clone, Copy)]
pub struct View { pub origin: Pos2, pub zoom: f32 }
impl View {
    pub fn aff(&self) -> Aff { Aff::translate(self.origin.x, self.origin.y).mul(Aff::scale(self.zoom, self.zoom)) }
    pub fn to_screen(&self, p: Pos2) -> Pos2 { self.origin + p.to_vec2() * self.zoom }
    pub fn to_field(&self, p: Pos2) -> Pos2 { ((p - self.origin) / self.zoom).to_pos2() }
}

// ── Colours ─────────────────────────────────────────────────────────────────

pub fn parse_css_color(s: &str) -> Color32 {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let p = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
        let ex = |c: u8| c * 17;
        match hex.len() {
            3 => if let (Some(r), Some(g), Some(b)) = (p(0, 1), p(1, 1), p(2, 1)) { return Color32::from_rgb(ex(r), ex(g), ex(b)); },
            4 => if let (Some(r), Some(g), Some(b), Some(a)) = (p(0, 1), p(1, 1), p(2, 1), p(3, 1)) { return Color32::from_rgba_unmultiplied(ex(r), ex(g), ex(b), ex(a)); },
            6 => if let (Some(r), Some(g), Some(b)) = (p(0, 2), p(2, 2), p(4, 2)) { return Color32::from_rgb(r, g, b); },
            8 => if let (Some(r), Some(g), Some(b), Some(a)) = (p(0, 2), p(2, 2), p(4, 2), p(6, 2)) { return Color32::from_rgba_unmultiplied(r, g, b, a); },
            _ => {}
        }
        return Color32::GRAY;
    }
    if let Some(inner) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let v: Vec<f32> = inner.split(|c| c == ',' || c == '/' || c == ' ').filter(|x| !x.is_empty()).filter_map(|x| x.trim().trim_end_matches('%').parse().ok()).collect();
        if v.len() >= 3 {
            let a = v.get(3).copied().unwrap_or(1.0).clamp(0.0, 1.0);
            return Color32::from_rgba_unmultiplied(v[0] as u8, v[1] as u8, v[2] as u8, (a * 255.0).round() as u8);
        }
    }
    match s.to_lowercase().as_str() {
        "transparent" | "" => Color32::TRANSPARENT,
        "white" => Color32::WHITE,
        "black" => Color32::BLACK,
        "red" => Color32::from_rgb(255, 0, 0),
        "green" => Color32::from_rgb(0, 128, 0),
        "blue" => Color32::from_rgb(0, 0, 255),
        "yellow" => Color32::from_rgb(255, 255, 0),
        "orange" => Color32::from_rgb(255, 165, 0),
        "gray" | "grey" => Color32::GRAY,
        _ => Color32::GRAY,
    }
}

fn mul_alpha(c: Color32, a: f32) -> Color32 { c.gamma_multiply(a.clamp(0.0, 1.0)) }

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

// ── Values bound to layers ──────────────────────────────────────────────────

pub struct Values<'a> {
    pub title: &'a str,
    pub fields: &'a serde_json::Map<String, Value>,
    pub extra: &'a Extra,
    pub base_url: &'a str,
}

pub fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let s = s.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    for c in s.chars() {
        match c { '<' => in_tag = true, '>' => in_tag = false, _ if !in_tag => out.push(c), _ => {} }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
}

impl<'a> Values<'a> {
    pub fn text_for(&self, l: &Layer) -> String {
        match l.field.as_deref().filter(|f| !f.is_empty()) {
            Some(f) => {
                let id = crate::compile::field_id(f);
                if id == "TITLE" && !self.title.is_empty() { return strip_html(self.title); }
                if let Some(Value::String(s)) = self.fields.get(&id) { if !s.is_empty() { return strip_html(s); } }
                strip_html(l.default_text.as_deref().unwrap_or(""))
            }
            None => strip_html(l.text.as_deref().unwrap_or("")),
        }
    }

    pub fn image_uri(&self, l: &Layer) -> Option<String> {
        let mut src = l.src.clone().unwrap_or_default();
        if src.is_empty() {
            if let Some(b) = l.bind.as_deref().filter(|b| !b.is_empty()) {
                let id = crate::compile::field_id(b);
                if let Some(Value::String(s)) = self.fields.get(&id) { src = s.clone(); }
                if src.is_empty() && id == "LOGO_URL" {
                    if let Some(Value::String(s)) = self.extra.get("url") { src = s.clone(); }
                }
            }
        }
        if src.is_empty() { return None; }
        Some(self.resolve(&src))
    }

    fn resolve(&self, src: &str) -> String {
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("file://") || src.starts_with("data:") {
            src.to_string()
        } else if src.starts_with('/') {
            if self.base_url.is_empty() { format!("file://{}", src) } else { format!("{}{}", self.base_url, src) }
        } else if self.base_url.is_empty() {
            format!("file://{}", src)
        } else {
            format!("{}/{}", self.base_url, src)
        }
    }

    pub fn ticker_items(&self) -> Vec<String> {
        let mut out = vec![];
        if let Some(Value::Array(a)) = self.extra.get("items") {
            for i in a {
                match i {
                    Value::String(s) => out.push(strip_html(s)),
                    Value::Object(o) => if let Some(Value::String(s)) = o.get("text") { out.push(strip_html(s)); },
                    _ => {}
                }
            }
        }
        if out.is_empty() { out = vec![tr("render.ticker_sample_1").into(), tr("render.ticker_sample_2").into(), tr("render.ticker_sample_3").into()]; }
        out
    }
}

// ── Per-layer effects passed by the playback engine ─────────────────────────

#[derive(Clone, Debug)]
pub struct TextFx { pub effect: String, pub duration: f32, pub stagger: f32, pub from: String, pub ease: String, pub t: f32, pub out: bool }

#[derive(Clone, Debug, Default)]
pub struct LayerFx { pub tr: Option<Transform>, pub text: Option<TextFx> }

pub struct SceneParams<'a> {
    pub scene: &'a Scene,
    pub frame_origin: (f32, f32),
    pub elem_tr: Transform,
    pub fx: &'a HashMap<String, LayerFx>,
    pub values: Values<'a>,
    pub time: f64,
    /// BADGE: tekst ściskany wg długości (jak `data-squash-mode=length` w BTB-CG), nie do szerokości pudełka.
    pub badge_squash: bool,
}

// ── Geometry helpers ────────────────────────────────────────────────────────

fn rounded_rect(w: f32, h: f32, r: [f32; 4], inflate: f32) -> Vec<Pos2> {
    let (x0, y0, x1, y1) = (-inflate, -inflate, w + inflate, h + inflate);
    let maxr = ((x1 - x0).min(y1 - y0) / 2.0).max(0.0);
    let rr: Vec<f32> = r.iter().map(|v| (v + inflate.max(0.0)).clamp(0.0, maxr)).collect();
    if rr.iter().all(|v| *v < 0.5) {
        return vec![Pos2::new(x0, y0), Pos2::new(x1, y0), Pos2::new(x1, y1), Pos2::new(x0, y1)];
    }
    let mut pts = vec![];
    let seg = 8;
    let arc = |pts: &mut Vec<Pos2>, cx: f32, cy: f32, r: f32, a0: f32| {
        for i in 0..=seg {
            let a = (a0 + 90.0 * i as f32 / seg as f32).to_radians();
            pts.push(Pos2::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    };
    arc(&mut pts, x0 + rr[0], y0 + rr[0], rr[0], 180.0);
    arc(&mut pts, x1 - rr[1], y0 + rr[1], rr[1], 270.0);
    arc(&mut pts, x1 - rr[2], y1 - rr[2], rr[2], 0.0);
    arc(&mut pts, x0 + rr[3], y1 - rr[3], rr[3], 90.0);
    pts
}

fn poly_mesh(pts: &[Pos2], m: &Aff, color_at: &dyn Fn(Pos2) -> Color32, uv_at: Option<&dyn Fn(Pos2) -> Pos2>, tex: TextureId) -> Mesh {
    let mut mesh = Mesh::with_texture(tex);
    let n = pts.len();
    let centroid = Pos2::new(pts.iter().map(|p| p.x).sum::<f32>() / n as f32, pts.iter().map(|p| p.y).sum::<f32>() / n as f32);
    let mk = |p: Pos2| Vertex { pos: m.apply(p), uv: uv_at.map(|f| f(p)).unwrap_or(WHITE_UV), color: color_at(p) };
    mesh.vertices.push(mk(centroid));
    for p in pts { mesh.vertices.push(mk(*p)); }
    for i in 0..n {
        mesh.indices.extend_from_slice(&[0, 1 + i as u32, 1 + ((i + 1) % n) as u32]);
    }
    mesh
}

fn local_rect_to_screen_aabb(m: &Aff, r: Rect) -> Rect {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].map(|p| m.apply(p));
    let mut out = Rect::from_min_max(pts[0], pts[0]);
    for p in pts { out.extend_with(p); }
    out
}

fn css_gradient_t(angle_deg: f32, w: f32, h: f32, p: Pos2) -> f32 {
    let a = angle_deg.to_radians();
    let (s, c) = a.sin_cos();
    let dir = Vec2::new(s, -c);
    let len = (w * s).abs() + (h * c).abs();
    if len < 1e-6 { return 0.0; }
    0.5 + ((p - Pos2::new(w / 2.0, h / 2.0)).dot(dir)) / len
}

// ── Text ────────────────────────────────────────────────────────────────────

struct TextStyleResolved { size: f32, color: Color32, shadow: Option<(f32, f32, f32, Color32)>, bold_passes: usize }

fn parse_text_shadow(s: &str) -> Option<(f32, f32, f32, Color32)> {
    let s = s.trim();
    if s.is_empty() || s == "none" { return None; }
    // "0 2px 8px rgba(0,0,0,0.7)" — split numbers from colour
    let (nums, col) = match s.find(|c: char| c == 'r' || c == '#') { Some(i) => (&s[..i], &s[i..]), None => (s, "rgba(0,0,0,0.5)") };
    let v: Vec<f32> = nums.split_whitespace().filter_map(|t| t.trim_end_matches("px").parse().ok()).collect();
    if v.len() < 2 { return None; }
    Some((v[0], v[1], v.get(2).copied().unwrap_or(0.0), parse_css_color(col)))
}

fn apply_transform(text: &str, how: &str) -> String {
    match how { "uppercase" => text.to_uppercase(), "lowercase" => text.to_lowercase(),
        "capitalize" => text.split(' ').map(|w| { let mut c = w.chars(); match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => String::new() } }).collect::<Vec<_>>().join(" "),
        _ => text.to_string() }
}

/// hhea.ascender / unitsPerEm dla BAHNSCHRIFT.TTF (2036/2048) i LiberationSans (1854/2048).
const BAHN_ASCENT: f32 = 2036.0 / 2048.0;
const LIBERATION_ASCENT: f32 = 1854.0 / 2048.0;

/// Krój do rysowania: Arial/Helvetica/sans-serif to w przeglądarce (Linux) Liberation Sans, metrycznie zgodny z Arialem;
/// reszta zostaje Bahnschriftem (jak `@font-face` na stronie). Zwraca (rodzina, ascent hhea, prawdziwy bold?).
fn resolve_font(family: &str, weight: &str) -> (FontFamily, f32, bool) {
    let bold = weight.parse::<u32>().unwrap_or(400) >= 600 || weight.eq_ignore_ascii_case("bold");
    let f = family.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '\'' || c == '"').to_ascii_lowercase();
    if matches!(f.as_str(), "arial" | "helvetica" | "liberation sans" | "sans-serif" | "arial narrow") {
        (FontFamily::Name(if bold { "LiberationBold" } else { "LiberationRegular" }.into()), LIBERATION_ASCENT, true)
    } else {
        (FontFamily::Name("Bahnschrift".into()), BAHN_ASCENT, false)
    }
}

struct TextBox<'a> {
    text: &'a str,
    font: &'a FontStyle,
    w: f32,
    h: f32,
    squash: bool,
    by_length: bool,
    align: &'a str,
    /// Extra x translation in local px (ticker scrolling).
    dx: f32,
    clip_local: Option<Rect>,
}

/// Współczynnik ściśnięcia tekstu jak w renderer.js (`applyGlobalSquashing`): domyślnie dopasowanie do szerokości
/// pudełka; w trybie „length” (BADGE) skala = max(0.5, 25/długość) dla tekstów dłuższych niż 25 znaków.
pub fn squash_factor(text_w: f32, box_w: f32, len: usize, by_length: bool) -> f32 {
    if by_length {
        if len > 25 { (25.0 / len as f32).max(0.5) } else { 1.0 }
    } else if text_w > box_w && text_w > 0.0 {
        box_w / text_w
    } else {
        1.0
    }
}

/// Draws text inside the layer box using matrix `m` (local → screen). Returns the unclipped local text width.
fn draw_text(painter: &Painter, ctx: &Context, zoom: f32, m: &Aff, tb: &TextBox, opacity: f32, fx: Option<&TextFx>) -> f32 {
    let font = tb.font;
    let text = apply_transform(tb.text, &font.transform);
    if text.is_empty() { return 0.0; }
    let (family, ascent_em, real_bold) = resolve_font(&font.family, &font.weight);
    let style = TextStyleResolved {
        size: font.size,
        color: parse_css_color(&font.color),
        shadow: parse_text_shadow(&font.shadow),
        bold_passes: if real_bold { 1 } else { match font.weight.parse::<u32>().unwrap_or(400) { w if w >= 800 => 3, w if w >= 600 => 2, _ => 1 } },
    };
    let ppp = ctx.pixels_per_point();
    let size_pts = (style.size * zoom).max(1.0);
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    job.append(&text, 0.0, TextFormat {
        font_id: FontId::new(size_pts, family.clone()),
        extra_letter_spacing: font.letter_spacing * zoom,
        line_height: Some(style.size * font.line_height * zoom),
        color: Color32::WHITE,
        italics: font.italic,
        ..Default::default()
    });
    let galley = ctx.fonts(|f| f.layout_job(job));
    let tw = galley.rect.width() / zoom;
    let th = galley.rect.height() / zoom;
    let sq = if tb.squash { squash_factor(tw, tb.w, text.trim().chars().count(), tb.by_length) } else { 1.0 };
    let eff_w = tw * sq;
    let ox = match tb.align { "center" => (tb.w - eff_w) / 2.0, "right" => tb.w - eff_w, _ => 0.0 } + tb.dx;
    // egui stawia glif u góry wiersza (cały nadmiar line-height ląduje pod nim); CSS dzieli go po połowie (half-leading).
    let natural_h = ctx.fonts(|f| f.row_height(&FontId::new(size_pts, family.clone()))) / zoom;
    let half_leading = (style.size * font.line_height - natural_h) / 2.0;
    // Linia bazowa: egui ma ją na `ascent` własnego wiersza (skalowanego do wysokości ascent+descent), przeglądarka na
    // hhea.ascender; bez tej poprawki tekst Bahnschrift leży ok. 0,17·rozmiar za wysoko względem strony wyjściowej.
    let egui_ascent = galley.rows.first().and_then(|r| r.glyphs.first().map(|g| g.pos.y - r.rect.min.y)).unwrap_or(0.0) / zoom;
    let baseline_fix = style.size * ascent_em - egui_ascent;
    let oy = match font.valign.as_str() { "top" => 0.0, "bottom" => tb.h - th, _ => (tb.h - th) / 2.0 } + half_leading + baseline_fix;

    let [fw, fh] = ctx.fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / fw as f32, 1.0 / fh as f32);

    // unit split
    let split = fx.map(|f| anim::split_of(&f.effect)).unwrap_or(anim::Split::Whole);
    let mut glyph_units: Vec<usize> = vec![];
    let mut unit_count = 0usize;
    let mut in_word = false;
    for row in &galley.rows {
        let row_unit = unit_count;
        if split == anim::Split::Lines { unit_count += 1; }
        for g in &row.glyphs {
            if g.uv_rect.is_nothing() { in_word = false; continue; }
            match split {
                anim::Split::Chars => { glyph_units.push(unit_count); unit_count += 1; }
                anim::Split::Words => {
                    if !in_word { unit_count += 1; in_word = true; }
                    glyph_units.push(unit_count - 1);
                }
                anim::Split::Lines => { glyph_units.push(row_unit); }
                anim::Split::Whole => { glyph_units.push(0); }
            }
        }
        in_word = false;
    }
    if split == anim::Split::Whole { unit_count = 1; }

    let mut unit_fx: Vec<UnitFx> = vec![UnitFx::default(); unit_count.max(1)];
    if let Some(f) = fx {
        for (i, u) in unit_fx.iter_mut().enumerate() {
            *u = if f.out {
                anim::text_out_fx(&f.effect, f.duration, &f.ease, f.stagger, &f.from, i, unit_count.max(1), f.t)
            } else {
                anim::text_in_fx(&f.effect, f.duration, &f.ease, f.stagger, &f.from, i, unit_count.max(1), f.t)
            };
        }
    }

    let build = |dx: f32, dy: f32, color: Color32, alpha: f32| -> Mesh {
        let mut mesh = Mesh::with_texture(TextureId::default());
        let mut gi = 0usize;
        for row in &galley.rows {
            let start = row.visuals.glyph_vertex_range.start;
            let mut vi = start;
            for g in &row.glyphs {
                if g.uv_rect.is_nothing() { continue; }
                let u = unit_fx.get(glyph_units.get(gi).copied().unwrap_or(0)).copied().unwrap_or_default();
                gi += 1;
                let verts = &row.visuals.mesh.vertices[vi..vi + 4];
                vi += 4;
                let gc = Pos2::new(
                    verts.iter().map(|v| v.pos.x).sum::<f32>() / 4.0 / zoom,
                    verts.iter().map(|v| v.pos.y).sum::<f32>() / 4.0 / zoom,
                );
                let base = mesh.vertices.len() as u32;
                for v in verts {
                    let lp = Pos2::new(v.pos.x / zoom, v.pos.y / zoom);
                    let sc = u.scale;
                    let lp = Pos2::new(gc.x + (lp.x - gc.x) * sc, gc.y + (lp.y - gc.y) * sc);
                    let local = Pos2::new(ox + lp.x * sq + u.dx + dx, oy + lp.y + u.dy + dy);
                    mesh.vertices.push(Vertex {
                        pos: m.apply(local),
                        uv: Pos2::new(v.uv.x * uv_norm.x, v.uv.y * uv_norm.y),
                        color: mul_alpha(color, alpha * u.opacity),
                    });
                }
                mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
            }
        }
        mesh
    };

    // reveal clip (typewriter/reveal/clip-up) as screen AABB of the text box fraction
    let mut clip = painter.clip_rect();
    if let Some(f) = unit_fx.first() {
        if f.reveal_x < 0.999 || f.reveal_y < 0.999 {
            let r = Rect::from_min_size(
                Pos2::new(ox + eff_w * 0.0, oy + th * (1.0 - f.reveal_y)),
                Vec2::new(eff_w * f.reveal_x, th * f.reveal_y),
            );
            clip = clip.intersect(local_rect_to_screen_aabb(m, r).expand(1.0));
        }
    }
    if let Some(cl) = tb.clip_local {
        clip = clip.intersect(local_rect_to_screen_aabb(m, cl).expand(0.5));
    }
    let p = painter.with_clip_rect(clip);

    if let Some((sx, sy, blur, sc)) = style.shadow {
        let k = (blur / 3.0).max(0.0);
        if k > 0.1 {
            for (jx, jy) in [(-k, 0.0), (k, 0.0), (0.0, -k), (0.0, k)] {
                let mesh = build(sx + jx, sy + jy, sc, opacity * 0.28);
                p.add(Shape::mesh(mesh));
            }
        } else {
            p.add(Shape::mesh(build(sx, sy, sc, opacity)));
        }
    }
    for i in 0..style.bold_passes {
        let off = i as f32 * style.size * 0.022;
        p.add(Shape::mesh(build(off, 0.0, style.color, opacity)));
    }
    let _ = ppp;
    tw
}

// ── Layer drawing ───────────────────────────────────────────────────────────

fn clock_text(format: &str) -> String {
    let now = chrono::Local::now();
    if format == "HH:MM" { now.format("%H:%M").to_string() } else { now.format("%H:%M:%S").to_string() }
}

pub fn draw_scene(painter: &Painter, ctx: &Context, view: &View, sp: &SceneParams) {
    let frame = (sp.scene.frame.w, sp.scene.frame.h);
    let zoom = view.zoom;
    let default_tr = Transform::default();
    for l in sp.scene.layers.iter().filter(|l| l.visible) {
        let fx = sp.fx.get(&l.id);
        let tr = fx.and_then(|f| f.tr.as_ref()).unwrap_or(&default_tr);
        let field_m = layer_to_field(l, sp.frame_origin, frame, &sp.elem_tr, tr);
        let m = view.aff().mul(field_m);
        let opacity = l.opacity * tr.opacity * sp.elem_tr.opacity;
        if opacity <= 0.002 { continue; }

        // layer-level reveal clip + element-level clip (AABB approximation)
        let mut clip = painter.clip_rect();
        let apply_clip = |clip: Rect, c: [f32; 4], w: f32, h: f32, mm: &Aff| -> Rect {
            if c == [0.0, 0.0, 1.0, 1.0] { return clip; }
            let r = Rect::from_min_max(Pos2::new(c[0] * w, c[1] * h), Pos2::new(c[2] * w, c[3] * h));
            clip.intersect(local_rect_to_screen_aabb(mm, r))
        };
        clip = apply_clip(clip, tr.clip, l.w, l.h, &m);
        if sp.elem_tr.clip != [0.0, 0.0, 1.0, 1.0] {
            let eb = view.aff().mul(Aff::translate(sp.frame_origin.0, sp.frame_origin.1));
            clip = apply_clip(clip, sp.elem_tr.clip, frame.0, frame.1, &eb);
        }
        let p = painter.with_clip_rect(clip);

        match l.layer_type {
            LayerType::Shape => draw_shape(&p, l, &m, zoom, opacity, sp.time),
            LayerType::Text => {
                let font = l.font_or_default();
                let text = sp.values.text_for(l);
                let tb = TextBox { text: &text, font: &font, w: l.w, h: l.h, squash: l.squash == Some(true), by_length: sp.badge_squash, align: &font.align, dx: 0.0, clip_local: None };
                draw_text(&p, ctx, zoom, &m, &tb, opacity, fx.and_then(|f| f.text.as_ref()));
            }
            LayerType::Clock => {
                let font = l.font_or_default();
                let text = clock_text(l.format.as_deref().unwrap_or("HH:MM:SS"));
                let tb = TextBox { text: &text, font: &font, w: l.w, h: l.h, squash: false, by_length: false, align: &font.align, dx: 0.0, clip_local: None };
                draw_text(&p, ctx, zoom, &m, &tb, opacity, fx.and_then(|f| f.text.as_ref()));
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            LayerType::Ticker => {
                let font = l.font_or_default();
                let sep = l.separator.clone().unwrap_or_else(|| "•".into());
                let gap = l.gap.unwrap_or(48.0);
                let items = sp.values.ticker_items();
                let pad = " ".repeat(((gap / (font.size * 0.3)).round() as usize).max(1));
                let one: String = items.iter().map(|i| format!("{}{}{}{}", i, pad, if sep.is_empty() { String::new() } else { sep.clone() }, pad)).collect();
                let speed = l.speed.unwrap_or(100.0);
                // measure one repetition width
                let mut job = LayoutJob::default();
                job.append(&apply_transform(&one, &font.transform), 0.0, TextFormat { font_id: FontId::new((font.size * zoom).max(1.0), resolve_font(&font.family, &font.weight).0), extra_letter_spacing: font.letter_spacing * zoom, ..Default::default() });
                let one_w = (ctx.fonts(|f| f.layout_job(job)).rect.width() / zoom).max(1.0);
                let off = ((sp.time as f32) * speed) % one_w;
                let mut f2 = font.clone();
                f2.align = "left".into();
                let clip_local = Rect::from_min_size(Pos2::ZERO, Vec2::new(l.w, l.h));
                let mut x = -off;
                while x < l.w {
                    let tb = TextBox { text: &one, font: &f2, w: l.w, h: l.h, squash: false, by_length: false, align: "left", dx: x, clip_local: Some(clip_local) };
                    draw_text(&p, ctx, zoom, &m, &tb, opacity, None);
                    x += one_w;
                }
                ctx.request_repaint();
            }
            LayerType::Image => draw_image(&p, ctx, l, &m, opacity, &sp.values),
            LayerType::Video => {
                let pts = rounded_rect(l.w, l.h, l.radius.as_ref().map(|r| r.corners()).unwrap_or([0.0; 4]), 0.0);
                p.add(Shape::mesh(poly_mesh(&pts, &m, &|_| mul_alpha(Color32::from_rgb(20, 24, 38), opacity), None, TextureId::default())));
                let c = m.apply(Pos2::new(l.w / 2.0, l.h / 2.0));
                p.text(c, egui::Align2::CENTER_CENTER, format!("▶ {}", l.src.as_deref().unwrap_or("wideo")), FontId::proportional(13.0), mul_alpha(Color32::from_gray(170), opacity));
            }
        }
    }
}

fn draw_shape(p: &Painter, l: &Layer, m: &Aff, zoom: f32, opacity: f32, time: f64) {
    let radii = l.radius.as_ref().map(|r| r.corners()).unwrap_or([0.0; 4]);
    let (w, h) = (l.w, l.h);
    let mut op = opacity;
    let effect = l.effect.as_deref().unwrap_or("none");
    if effect == "pulse" {
        let k = 0.5 + 0.5 * ((time as f32) * std::f32::consts::TAU / 1.6).sin();
        op *= 1.0 - k * 0.45;
    }

    // shadow
    if let Some(sh) = &l.shadow {
        if sh.enabled() {
            let (sx, sy, blur, col) = match sh {
                Shadow::Custom { x, y, blur, color } => (*x, *y, *blur, parse_css_color(if color.is_empty() { "rgba(0,0,0,0.6)" } else { color })),
                _ => (0.0, 10.0, 25.0, Color32::from_rgba_unmultiplied(0, 0, 0, 153)),
            };
            let steps = 9;
            for k in 0..steps {
                let f = k as f32 / (steps - 1) as f32;
                let grow = -blur * 0.2 + blur * 0.6 * f;
                let pts = rounded_rect(w, h, radii, grow);
                let sm = m.mul(Aff::translate(sx, sy));
                let a = op * (col.a() as f32 / 255.0) * (0.17 * (1.0 - f * 0.8));
                let c = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 255);
                p.add(Shape::mesh(poly_mesh(&pts, &sm, &|_| mul_alpha(c, a), None, TextureId::default())));
            }
        }
    }

    // fill
    let pts = rounded_rect(w, h, radii, 0.0);
    if let Some(f) = &l.fill {
        match f.fill_type {
            FillType::None => {}
            FillType::Solid => {
                let c = mul_alpha(parse_css_color(&f.color), op);
                p.add(Shape::mesh(poly_mesh(&pts, m, &|_| c, None, TextureId::default())));
            }
            FillType::Gradient => {
                let c1 = parse_css_color(if f.color.is_empty() { "#1e3a8a" } else { &f.color });
                let c2 = parse_css_color(if f.color2.is_empty() { "#3b82f6" } else { &f.color2 });
                let ang = f.angle;
                p.add(Shape::mesh(poly_mesh(&pts, m, &|q| mul_alpha(lerp_color(c1, c2, css_gradient_t(ang, w, h, q)), op), None, TextureId::default())));
            }
        }
    }

    // effects drawn over the fill, clipped to the box
    if matches!(effect, "shimmer" | "scanline") {
        let aabb = local_rect_to_screen_aabb(m, Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h)));
        let pc = p.with_clip_rect(p.clip_rect().intersect(aabb));
        if effect == "shimmer" {
            let ph = ((time as f32) / 3.0) % 1.0;
            let x = -0.2 * w + ph * 1.6 * w;
            let band = [Pos2::new(x, 0.0), Pos2::new(x + 0.6 * w, 0.0), Pos2::new(x + 0.6 * w - 0.2 * h, h), Pos2::new(x - 0.2 * h, h)];
            let col = mul_alpha(Color32::WHITE, 0.2 * op);
            pc.add(Shape::mesh(poly_mesh(&band, m, &|_| col, None, TextureId::default())));
        } else {
            let mut y = 0.0;
            while y < h {
                let line = [Pos2::new(0.0, y), Pos2::new(w, y), Pos2::new(w, y + 1.0), Pos2::new(0.0, y + 1.0)];
                let col = mul_alpha(Color32::BLACK, 0.18 * op);
                pc.add(Shape::mesh(poly_mesh(&line, m, &|_| col, None, TextureId::default())));
                y += 3.0;
            }
        }
    }
    if effect == "glow" {
        let k = 0.5 + 0.5 * ((time as f32) * std::f32::consts::TAU / 2.0).sin();
        let base = l.fill.as_ref().map(|f| parse_css_color(if f.color.is_empty() { "#ffffff" } else { &f.color })).unwrap_or(Color32::WHITE);
        for i in 0..6 {
            let grow = 2.0 + (6.0 + 20.0 * k) * (i as f32 / 5.0);
            let ring = rounded_rect(w, h, radii, grow);
            p.add(Shape::mesh(poly_mesh(&ring, m, &|_| mul_alpha(base, op * 0.06), None, TextureId::default())));
        }
    }

    // stroke (border-box: drawn inside)
    if let Some(s) = &l.stroke {
        if s.width > 0.0 {
            let inset = s.width / 2.0;
            let rr = radii.map(|r| (r - inset).max(0.0));
            let ring = rounded_rect(w - s.width, h - s.width, rr, 0.0);
            let col = mul_alpha(parse_css_color(if s.color.is_empty() { "#ffffff" } else { &s.color }), op);
            let pts: Vec<Pos2> = ring.iter().map(|q| m.apply(Pos2::new(q.x + inset, q.y + inset))).collect();
            p.add(Shape::closed_line(pts, egui::Stroke::new((s.width * zoom * m_scale(m)).max(0.5), col)));
        }
    }
}

fn m_scale(m: &Aff) -> f32 {
    // m already includes zoom; return 1 here because stroke width is expressed in field px × zoom elsewhere.
    let _ = m;
    1.0
}

#[derive(Clone)]
enum ImgState { Loading, Ready(String), Failed(String) }
type ImgCache = std::sync::Arc<std::sync::Mutex<HashMap<String, ImgState>>>;

fn sniff_ext(b: &[u8], url: &str) -> &'static str {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) { "png" }
    else if b.starts_with(&[0xFF, 0xD8]) { "jpg" }
    else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" { "webp" }
    else if b.starts_with(b"GIF8") { "gif" }
    else if b.windows(4).take(512).any(|w| w == b"<svg") || url.ends_with(".svg") { "svg" }
    else { "png" }
}

/// Fetches http(s) images on a worker thread (with a User-Agent — hosts like Wikimedia reject
/// requests without one) and hands the bytes to egui under a `bytes://` uri once ready.
fn remote_image_uri(ctx: &Context, url: &str) -> Result<String, Option<String>> {
    let id = egui::Id::new("btb_img_cache");
    let cache: ImgCache = ctx.data_mut(|d| d.get_temp::<ImgCache>(id).unwrap_or_else(|| { let c: ImgCache = Default::default(); d.insert_temp(id, c.clone()); c }));
    let mut map = cache.lock().unwrap();
    match map.get(url).cloned() {
        Some(ImgState::Ready(u)) => Ok(u),
        Some(ImgState::Loading) => Err(None),
        Some(ImgState::Failed(e)) => Err(Some(e)),
        None => {
            map.insert(url.to_string(), ImgState::Loading);
            let (c2, ctx2, url2) = (cache.clone(), ctx.clone(), url.to_string());
            std::thread::spawn(move || {
                let res = ureq::get(&url2).set("User-Agent", "BTB-CGEditor/0.1").timeout(std::time::Duration::from_secs(15)).call()
                    .map_err(|e| e.to_string())
                    .and_then(|r| { let mut buf = vec![]; std::io::Read::read_to_end(&mut r.into_reader(), &mut buf).map(|_| buf).map_err(|e| e.to_string()) });
                let st = match res {
                    Ok(bytes) => {
                        let uri = format!("bytes://{}.{}", url2.replace(|c: char| !c.is_alphanumeric(), "_"), sniff_ext(&bytes, &url2));
                        ctx2.include_bytes(uri.clone(), bytes);
                        ImgState::Ready(uri)
                    }
                    Err(e) => ImgState::Failed(e),
                };
                c2.lock().unwrap().insert(url2, st);
                ctx2.request_repaint();
            });
            Err(None)
        }
    }
}

fn draw_image(p: &Painter, ctx: &Context, l: &Layer, m: &Aff, opacity: f32, values: &Values) {
    let radii = l.radius.as_ref().map(|r| r.corners()).unwrap_or([0.0; 4]);
    let (w, h) = (l.w, l.h);
    let placeholder = |msg: &str| {
        let pts = rounded_rect(w, h, radii, 0.0);
        let col = mul_alpha(Color32::from_rgb(38, 44, 66), opacity);
        p.add(Shape::mesh(poly_mesh(&pts, m, &|_| col, None, TextureId::default())));
        let border: Vec<Pos2> = pts.iter().map(|q| m.apply(*q)).collect();
        p.add(Shape::closed_line(border, egui::Stroke::new(1.0, mul_alpha(Color32::from_gray(90), opacity))));
        p.text(m.apply(Pos2::new(w / 2.0, h / 2.0)), egui::Align2::CENTER_CENTER, msg, FontId::proportional(12.0), mul_alpha(Color32::from_gray(160), opacity));
    };
    let Some(mut uri) = values.image_uri(l) else { return placeholder(tr("render.no_source")); };
    if uri.starts_with("http://") || uri.starts_with("https://") {
        match remote_image_uri(ctx, &uri) {
            Ok(u) => uri = u,
            Err(None) => return placeholder(tr("render.loading")),
            Err(Some(e)) => return placeholder(&trf("render.fetch_error", &[("err", &e.chars().take(60).collect::<String>())])),
        }
    }
    match ctx.try_load_texture(&uri, TextureOptions::LINEAR, SizeHint::default()) {
        Ok(TexturePoll::Ready { texture }) => {
            let (iw, ih) = (texture.size.x.max(1.0), texture.size.y.max(1.0));
            let fit = l.fit.as_deref().unwrap_or("contain");
            let (bx, by, bw, bh) = match fit {
                "cover" => { let s = (w / iw).max(h / ih); (0.0 + (w - iw * s) / 2.0, (h - ih * s) / 2.0, iw * s, ih * s) }
                "fill" => (0.0, 0.0, w, h),
                "none" => ((w - iw) / 2.0, (h - ih) / 2.0, iw, ih),
                "scale-down" => { let s = (w / iw).min(h / ih).min(1.0); ((w - iw * s) / 2.0, (h - ih * s) / 2.0, iw * s, ih * s) }
                _ => { let s = (w / iw).min(h / ih); ((w - iw * s) / 2.0, (h - ih * s) / 2.0, iw * s, ih * s) }
            };
            let uv = |q: Pos2| Pos2::new((q.x - bx) / bw, (q.y - by) / bh);
            let tint = mul_alpha(Color32::WHITE, opacity);
            let pts = if fit == "cover" || fit == "fill" {
                rounded_rect(w, h, radii, 0.0)
            } else {
                vec![Pos2::new(bx, by), Pos2::new(bx + bw, by), Pos2::new(bx + bw, by + bh), Pos2::new(bx, by + bh)]
            };
            let aabb = local_rect_to_screen_aabb(m, Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h)));
            let pc = p.with_clip_rect(p.clip_rect().intersect(aabb));
            pc.add(Shape::mesh(poly_mesh(&pts, m, &|_| tint, Some(&uv), texture.id)));
        }
        Ok(TexturePoll::Pending { .. }) => { placeholder(tr("render.loading")); ctx.request_repaint(); }
        Err(e) => placeholder(&trf("render.image_error", &[("err", &e.to_string().chars().take(40).collect::<String>())])),
    }
}

#[cfg(test)]
mod squash_tests {
    use super::squash_factor;

    #[test]
    fn fit_mode_shrinks_only_on_overflow() {
        assert_eq!(squash_factor(100.0, 200.0, 5, false), 1.0);
        assert!((squash_factor(400.0, 200.0, 5, false) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn length_mode_matches_renderer_rule() {
        assert_eq!(squash_factor(1000.0, 1000.0, 25, true), 1.0);
        assert!((squash_factor(0.0, 0.0, 50, true) - 0.5).abs() < 1e-6);
        assert!((squash_factor(0.0, 0.0, 30, true) - 25.0 / 30.0).abs() < 1e-6);
        assert!((squash_factor(0.0, 0.0, 200, true) - 0.5).abs() < 1e-6);
    }
}
