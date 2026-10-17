//! Port of CGMotion (cgmotion.js) semantics: eases, IN/OUT types, text effects, attention loops.
use crate::model::{AnimSpec, MotionSpec};
use std::f32::consts::PI;

// ── Easing (GSAP names as normalised by CGMotion.normalizeEase) ─────────────

pub use btb_motion::ease::ease;

// ── Layer transform produced by an animation ────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub tx: f32,
    pub ty: f32,
    pub sx: f32,
    pub sy: f32,
    pub rot: f32,   // degrees
    pub skew: f32,  // degrees (skewX)
    pub opacity: f32,
    /// Origin of scale/rotate as fraction of the layer box.
    pub origin: (f32, f32),
    /// Visible region as fractions (left, top, right, bottom) — clip-path inset reveal.
    pub clip: [f32; 4],
    pub blur: f32,
    /// Perspective-like flip about an axis: cos factor applied to scale (0..1).
    pub flip_x: f32,
    pub flip_y: f32,
    /// Animacja ustawia `origin` jawnie (jak `transformOrigin` w cgmotion). Bez tego element-owa
    /// transformacja działa względem lewego górnego rogu sceny (CSS `.cg-scene`), a warstwowa względem środka.
    pub origin_set: bool,
    /// Żaluzja: postęp maski 0..1 (1 = brak maski); w egui przybliżana przezroczystością.
    pub shutter: f32,
    /// Ukośna maska (mask-diag): widoczny postęp 0..1, -1 = brak; `diag_r` = 1 dla kierunku "right".
    pub diag: f32,
    pub diag_r: f32,
    /// Rozdzielenie kanałów RGB: >0 wejście, <0 wyjście (siła 0..1), 0 = brak. W egui pomijane.
    pub rgb: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { tx: 0.0, ty: 0.0, sx: 1.0, sy: 1.0, rot: 0.0, skew: 0.0, opacity: 1.0, origin: (0.5, 0.5),
               clip: [0.0, 0.0, 1.0, 1.0], blur: 0.0, flip_x: 1.0, flip_y: 1.0,
               origin_set: false, shutter: 1.0, diag: -1.0, diag_r: 0.0, rgb: 0.0 }
    }
}

/// Transformacja całego elementu: bez jawnego punktu odniesienia działa od lewego górnego rogu
/// (tak jak na wizji, gdzie `.cg-scene` ma `transform-origin: top left`).
pub fn element_origin(t: &mut Transform) {
    if !t.origin_set { t.origin = (0.0, 0.0); }
}

impl Transform {
    /// Wersja dla podglądu egui: efekty, których nie da się narysować wprost (maska żaluzji, ukośna
    /// maska, rozdzielenie RGB), przybliżone przezroczystością i prostokątnym przycięciem.
    pub fn approx(&self) -> Transform {
        let mut a = *self;
        if self.shutter < 1.0 { a.opacity *= self.shutter; }
        if self.diag >= 0.0 {
            let q = self.diag;
            a.clip = if self.diag_r > 0.5 { [1.0 - q, 0.0, 1.0, 1.0] } else { [0.0, 0.0, q, 1.0] };
        }
        a
    }
}

const SLIDE_X: f32 = 1920.0;
const SLIDE_Y: f32 = 1080.0;

fn clip_inset(dir: &str, amount: f32) -> [f32; 4] {
    // amount 1 = fully hidden, 0 = fully visible; CSS inset(top right bottom left)
    match dir {
        "right" => [amount, 0.0, 1.0, 1.0],
        "top" => [0.0, 0.0, 1.0, 1.0 - amount],
        "bottom" => [0.0, amount, 1.0, 1.0],
        _ => [0.0, 0.0, 1.0 - amount, 1.0], // left
    }
}

/// Visible band growing from / shrinking to the centre; left/right = horizontal, top/bottom = vertical.
fn clip_center(dir: &str, amount: f32) -> [f32; 4] {
    let h = (1.0 - amount) * 0.5;
    match dir {
        "top" | "bottom" => [0.0, 0.5 - h, 1.0, 0.5 + h],
        _ => [0.5 - h, 0.0, 0.5 + h, 1.0],
    }
}

fn slide_vec(dir: &str) -> (f32, f32) {
    match dir { "right" => (SLIDE_X, 0.0), "top" => (0.0, -SLIDE_Y), "bottom" => (0.0, SLIDE_Y), _ => (-SLIDE_X, 0.0) }
}

fn opposite(dir: &str) -> &'static str {
    match dir { "left" => "right", "right" => "left", "top" => "bottom", _ => "top" }
}

/// Transform of an IN animation at raw progress `p` in 0..1 (already past `delay`).
pub fn state_in(anim_type: &str, dir: &str, ease_name: &str, p: f32) -> Transform {
    let mut t = Transform::default();
    let e = |name: &str| ease(name, p);
    let main = ease(ease_name, p);
    match anim_type {
        "none" | "" => {}
        "fade" => t.opacity = main,
        "zoom" => { t.opacity = main; let s = 0.8 + 0.2 * main; t.sx = s; t.sy = s; }
        "zoom-in" => { t.opacity = main; let s = 1.15 - 0.15 * main; t.sx = s; t.sy = s; }
        "wipe" => {
            t.origin = (if dir == "right" { 1.0 } else { 0.0 }, 0.5);
            t.origin_set = true;
            t.opacity = main;
            t.sx = main.max(0.0001);
        }
        "wipe-y" => {
            t.origin = (0.5, if dir == "bottom" { 1.0 } else { 0.0 });
            t.origin_set = true;
            t.opacity = main;
            t.sy = main.max(0.0001);
        }
        "flip" => {
            t.origin_set = true;
            t.origin = (0.0, 0.5);
            t.opacity = main;
            t.flip_x = (PI / 2.0 * (1.0 - main)).cos();
        }
        "flip-x" => {
            t.origin_set = true;
            t.origin = (0.5, if dir == "bottom" { 1.0 } else { 0.0 });
            t.opacity = main;
            t.flip_y = (PI / 2.0 * (1.0 - main)).cos();
        }
        "clip" => t.clip = clip_inset(dir, 1.0 - main),
        "blur" => { t.opacity = main; t.blur = 24.0 * (1.0 - main); }
        "pop" => {
            let m = e("back.out(2)");
            t.opacity = m.clamp(0.0, 1.0);
            let s = 0.3 + 0.7 * m;
            t.sx = s; t.sy = s;
        }
        "drop" => {
            let m = e("bounce.out");
            let (dx, dy) = match dir { "bottom" => (0.0, 160.0), "left" => (-160.0, 0.0), "right" => (160.0, 0.0), _ => (0.0, -160.0) };
            t.opacity = m.clamp(0.0, 1.0);
            t.tx = dx * (1.0 - m);
            t.ty = dy * (1.0 - m);
        }
        "rotate" => {
            let r = dir == "right";
            t.origin_set = true;
            t.origin = (if r { 1.0 } else { 0.0 }, 1.0);
            t.opacity = main;
            t.rot = (if r { 90.0 } else { -90.0 }) * (1.0 - main);
        }
        "skew" => {
            let r = dir == "right";
            t.opacity = main;
            t.tx = (if r { SLIDE_X } else { -SLIDE_X }) * (1.0 - main);
            t.skew = (if r { 25.0 } else { -25.0 }) * (1.0 - main);
        }
        "slide-overshoot" => {
            let m = e("back.out(1.2)");
            t.opacity = m.clamp(0.0, 1.0);
            let (dx, dy) = slide_vec(dir);
            t.tx = dx * (1.0 - m);
            t.ty = dy * (1.0 - m);
        }
        "split-reveal" => t.clip = clip_center(dir, 1.0 - main),
        // Dokładne pola (diag / shutter / rgb) rysuje strona wyjściowa; podgląd egui używa `Transform::approx`.
        "mask-diag" => { t.diag = main; t.diag_r = if dir == "right" { 1.0 } else { 0.0 }; }
        "shutter" => t.shutter = main,
        "bar-grow" => { t.origin_set = true; t.sy = (0.04 + 0.96 * main).max(0.0001); }
        "rgb-split" => { t.opacity = main; t.tx = -14.0 * (1.0 - main); t.rgb = 1.0 - main; }
        "glitch" => {
            // discrete steps over 7 slots
            let slot = (p * 7.0).floor() as i32;
            let (o, x) = match slot { 0 => (0.0, 0.0), 1 => (1.0, -12.0), 2 => (0.2, 9.0), 3 => (1.0, -5.0), 4 => (0.4, 4.0), 5 => (0.4, 4.0), _ => (1.0, 0.0) };
            t.opacity = o; t.tx = x;
        }
        _ => { // slide
            t.opacity = main;
            let (dx, dy) = match dir { "right" => (SLIDE_X, 0.0), "top" => (0.0, -SLIDE_Y), "bottom" => (0.0, SLIDE_Y), _ => (-SLIDE_X, 0.0) };
            t.tx = dx * (1.0 - main);
            t.ty = dy * (1.0 - main);
        }
    }
    t
}

/// Transform of an OUT animation at progress `p` (0 = settled, 1 = hidden).
pub fn state_out(anim_type: &str, dir: &str, ease_name: &str, p: f32) -> Transform {
    let mut t = Transform::default();
    let main = ease(ease_name, p);
    match anim_type {
        // cgmotion: standardOut("none") = set opacity 0 od razu (po opóźnieniu), niezależnie od czasu trwania
        "none" | "" => { t.opacity = 0.0; }
        "fade" => t.opacity = 1.0 - main,
        "zoom" => { t.opacity = 1.0 - main; let s = 1.0 - 0.2 * main; t.sx = s; t.sy = s; }
        "zoom-in" => { t.opacity = 1.0 - main; let s = 1.0 + 0.15 * main; t.sx = s; t.sy = s; }
        "wipe" => {
            t.origin_set = true;
            t.origin = (if dir == "left" { 0.0 } else { 1.0 }, 0.5);
            t.opacity = 1.0 - main;
            t.sx = (1.0 - main).max(0.0001);
        }
        "wipe-y" => {
            t.origin_set = true;
            t.origin = (0.5, if dir == "top" { 0.0 } else { 1.0 });
            t.opacity = 1.0 - main;
            t.sy = (1.0 - main).max(0.0001);
        }
        "flip" => {
            t.origin_set = true;
            t.origin = (1.0, 0.5);
            t.opacity = 1.0 - main;
            t.flip_x = (PI / 2.0 * main).cos();
        }
        "flip-x" => {
            t.origin_set = true;
            t.origin = (0.5, if dir == "top" { 0.0 } else { 1.0 });
            t.opacity = 1.0 - main;
            t.flip_y = (PI / 2.0 * main).cos();
        }
        "clip" => t.clip = clip_inset(opposite(dir), main),
        "blur" => { t.opacity = 1.0 - main; t.blur = 24.0 * main; }
        "pop" => {
            let m = ease("back.in(2)", p);
            t.opacity = (1.0 - m).clamp(0.0, 1.0);
            let s = 1.0 - 0.7 * m;
            t.sx = s; t.sy = s;
        }
        "drop" => {
            let m = ease("power2.in", p);
            let (dx, dy) = match dir { "top" => (0.0, -120.0), "left" => (-120.0, 0.0), "right" => (120.0, 0.0), _ => (0.0, 120.0) };
            t.opacity = 1.0 - m;
            t.tx = dx * m; t.ty = dy * m;
        }
        "rotate" => {
            let r = dir == "right";
            t.origin_set = true;
            t.origin = (if r { 1.0 } else { 0.0 }, 1.0);
            t.opacity = 1.0 - main;
            t.rot = (if r { 90.0 } else { -90.0 }) * main;
        }
        "skew" => {
            let r = dir == "right";
            t.opacity = 1.0 - main;
            t.tx = (if r { SLIDE_X } else { -SLIDE_X }) * main;
            t.skew = (if r { -25.0 } else { 25.0 }) * main;
        }
        "slide-overshoot" => {
            let m = ease("back.in(1.2)", p);
            t.opacity = (1.0 - m).clamp(0.0, 1.0);
            let (dx, dy) = slide_vec(dir);
            t.tx = dx * m;
            t.ty = dy * m;
        }
        "split-reveal" => t.clip = clip_center(dir, main),
        "mask-diag" => { t.diag = 1.0 - main; t.diag_r = if dir == "right" { 1.0 } else { 0.0 }; }
        "shutter" => t.shutter = 1.0 - main,
        "bar-grow" => { t.origin_set = true; t.sy = (1.0 - 0.96 * main).max(0.0001); }
        "rgb-split" => { t.opacity = 1.0 - main; t.tx = 14.0 * main; t.rgb = -main; }
        "glitch" => {
            let slot = (p * 6.0).floor() as i32;
            let (o, x) = match slot { 0 => (0.3, 10.0), 1 => (1.0, -8.0), 2 => (0.2, 6.0), 3 => (1.0, -3.0), _ => (0.0, 0.0) };
            t.opacity = o; t.tx = x;
        }
        _ => { // slide
            t.opacity = 1.0 - main;
            let (dx, dy) = match dir { "right" => (SLIDE_X, 0.0), "top" => (0.0, -SLIDE_Y), "bottom" => (0.0, SLIDE_Y), _ => (-SLIDE_X, 0.0) };
            t.tx = dx * main;
            t.ty = dy * main;
        }
    }
    t
}

pub struct Timing<'a> { pub anim_type: &'a str, pub dir: &'a str, pub ease: &'a str, pub duration: f32, pub delay: f32 }

/// Type to evaluate: the spec's own type when the editor knows it, else its `fallback` key (compat with other builds).
fn known_type<'a>(ty: &'a str, extra: &'a crate::model::Extra) -> &'a str {
    if ANIM_TYPES.contains(&ty) { return ty; }
    extra.get("fallback").and_then(|v| v.as_str()).filter(|f| ANIM_TYPES.contains(f)).unwrap_or(ty)
}

impl<'a> From<&'a MotionSpec> for Timing<'a> {
    fn from(m: &'a MotionSpec) -> Self {
        Timing { anim_type: known_type(&m.anim_type, &m.extra), dir: m.direction.as_deref().unwrap_or("left"), ease: &m.ease, duration: m.duration, delay: m.delay }
    }
}
impl<'a> From<&'a AnimSpec> for Timing<'a> {
    fn from(m: &'a AnimSpec) -> Self {
        Timing { anim_type: known_type(&m.anim_type, &m.extra), dir: m.direction.as_deref().unwrap_or("left"), ease: &m.ease, duration: m.duration, delay: m.delay }
    }
}

/// IN transform at `t` seconds since IN started. Before the delay the layer is hidden.
pub fn eval_in(tm: &Timing, t: f32) -> Transform {
    if tm.anim_type == "none" || tm.anim_type.is_empty() {
        return if t >= tm.delay { Transform::default() } else { Transform { opacity: 0.0, ..Default::default() } };
    }
    if t < tm.delay {
        return state_in(tm.anim_type, tm.dir, tm.ease, 0.0);
    }
    let p = if tm.duration <= 0.0 { 1.0 } else { ((t - tm.delay) / tm.duration).clamp(0.0, 1.0) };
    state_in(tm.anim_type, tm.dir, tm.ease, p)
}

/// OUT transform at `t` seconds since OUT started. Before the delay the layer is settled.
pub fn eval_out(tm: &Timing, t: f32) -> Transform {
    if t < tm.delay { return Transform::default(); }
    let p = if tm.duration <= 0.0 { 1.0 } else { ((t - tm.delay) / tm.duration).clamp(0.0, 1.0) };
    state_out(tm.anim_type, tm.dir, tm.ease, p)
}

pub fn total(tm: &Timing) -> f32 { tm.delay + tm.duration }

// ── Text effects (per whole / word / char) ──────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct UnitFx {
    pub opacity: f32,
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    /// Fraction of the unit visible (clip reveal), left→right = `reveal_x`, bottom→top = `reveal_y`.
    pub reveal_x: f32,
    pub reveal_y: f32,
}
impl Default for UnitFx {
    fn default() -> Self { Self { opacity: 1.0, dx: 0.0, dy: 0.0, scale: 1.0, reveal_x: 1.0, reveal_y: 1.0 } }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Split { Whole, Words, Chars, Lines }

pub fn split_of(effect: &str) -> Split {
    if effect.starts_with("words-") { Split::Words } else if effect.starts_with("lines-") { Split::Lines } else if effect.starts_with("chars-") { Split::Chars } else { Split::Whole }
}

/// Stagger order of unit `idx` for `from`: "start" (default), "end", "center", "edges".
fn stagger_order(from: &str, idx: usize, count: usize) -> f32 {
    let last = count.saturating_sub(1) as f32;
    match from {
        "end" => last - idx as f32,
        "center" => (idx as f32 - last / 2.0).abs(),
        "edges" => (last / 2.0 - (idx as f32 - last / 2.0).abs()).max(0.0),
        _ => idx as f32,
    }
}

/// Effect of text-IN for unit `idx` of `count` at time `t` since the effect's start (after delay).
pub fn text_in_fx(effect: &str, duration: f32, ease_name: &str, stagger_cfg: f32, from: &str, idx: usize, count: usize, t: f32) -> UnitFx {
    let mut fx = UnitFx::default();
    let split = split_of(effect);
    let per = if stagger_cfg > 0.0 { stagger_cfg } else if split == Split::Chars { 0.04 } else { 0.09 };
    let stagger = if split == Split::Whole { 0.0 } else { per.min(1.2 / count.max(1) as f32) };
    let local = t - stagger * stagger_order(from, idx, count);
    let p = if duration <= 0.0 { 1.0 } else { (local / duration).clamp(0.0, 1.0) };
    let m = ease(ease_name, p);
    let variant = effect.split('-').nth(1).unwrap_or("");
    match effect {
        "none" | "" => {}
        "typewriter" | "reveal" => fx.reveal_x = m,
        "blur" | "blurIn" => fx.opacity = m,
        "fade" => fx.opacity = m,
        "scale" => { fx.opacity = m; fx.scale = 0.5 + 0.5 * m; }
        "slide-up" | "slideReveal" => { fx.opacity = m; fx.dy = 50.0 * (1.0 - m); }
        "slide-down" => { fx.opacity = m; fx.dy = -50.0 * (1.0 - m); }
        "slide-left" => { fx.opacity = m; fx.dx = 50.0 * (1.0 - m); }
        "slide-right" => { fx.opacity = m; fx.dx = -50.0 * (1.0 - m); }
        "clip-up" => fx.reveal_y = m,
        // przybliżenia podglądu: scramble/counter → fade, typewriter-caret → odsłanianie
        "scramble" | "counter" => fx.opacity = m,
        "typewriter-caret" => fx.reveal_x = m,
        _ if split != Split::Whole => match variant {
            "up" | "clip" => { fx.opacity = m; fx.dy = 40.0 * (1.0 - m); }
            "down" => { fx.opacity = m; fx.dy = -40.0 * (1.0 - m); }
            "scale" => { fx.opacity = m; fx.scale = 0.3 + 0.7 * m; }
            _ => fx.opacity = m, // fade / blur
        },
        _ => {}
    }
    fx
}

pub fn text_out_fx(effect: &str, duration: f32, ease_name: &str, stagger_cfg: f32, from: &str, idx: usize, count: usize, t: f32) -> UnitFx {
    let mut fx = UnitFx::default();
    let split = split_of(effect);
    let per = if stagger_cfg > 0.0 { stagger_cfg } else if split == Split::Chars { 0.04 } else { 0.09 };
    let stagger = if split == Split::Whole { 0.0 } else { per.min(1.2 / count.max(1) as f32) };
    let local = t - stagger * stagger_order(from, idx, count);
    let p = if duration <= 0.0 { 1.0 } else { (local / duration).clamp(0.0, 1.0) };
    let m = ease(ease_name, p);
    let variant = effect.split('-').nth(1).unwrap_or("");
    match effect {
        "none" | "" => {}
        "fade" | "blur" | "blurOut" => fx.opacity = 1.0 - m,
        "scale" => { fx.opacity = 1.0 - m; fx.scale = 1.0 - 0.5 * m; }
        "hide" | "slideHide" => fx.reveal_x = 1.0 - m, // approximated: shrinks from the right edge
        "slide-up" => { fx.opacity = 1.0 - m; fx.dy = -50.0 * m; }
        "slide-down" => { fx.opacity = 1.0 - m; fx.dy = 50.0 * m; }
        "slide-left" => { fx.opacity = 1.0 - m; fx.dx = -50.0 * m; }
        "slide-right" => { fx.opacity = 1.0 - m; fx.dx = 50.0 * m; }
        "clip-down" => fx.reveal_y = 1.0 - m,
        _ if split != Split::Whole => match variant {
            "up" | "clip" => { fx.opacity = 1.0 - m; fx.dy = -40.0 * m; }
            "down" => { fx.opacity = 1.0 - m; fx.dy = 40.0 * m; }
            "scale" => { fx.opacity = 1.0 - m; fx.scale = 1.0 - 0.7 * m; }
            _ => fx.opacity = 1.0 - m,
        },
        _ => {}
    }
    fx
}

/// Compat key written to exports so runtimes that lack a new type degrade to a known one (cgmotion reads `fallback`).
pub fn fallback_for(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "slide-overshoot" => "slide",
        "split-reveal" => "clip",
        "lines-up" => "slide-up",
        "lines-down" => "slide-down",
        "lines-fade" => "fade",
        "lines-scale" => "scale",
        "mask-diag" => "clip",
        "shutter" => "fade",
        "bar-grow" => "wipe-y",
        "rgb-split" => "glitch",
        "scramble" | "counter" => "fade",
        "typewriter-caret" => "typewriter",
        "chars-clip" => "chars-up",
        "words-clip" => "words-up",
        "lines-clip" => "lines-up",
        _ => return None,
    })
}

/// Keep `extra["fallback"]` in sync with the chosen type (set for new types, removed otherwise).
pub fn sync_fallback(extra: &mut crate::model::Extra, ty: &str) {
    match fallback_for(ty) {
        Some(f) => { extra.insert("fallback".into(), serde_json::Value::from(f)); }
        None => { extra.remove("fallback"); }
    }
}

// ── Attention loops (CGMotion.ATTENTION_PRESETS) ────────────────────────────

fn yoyo(t: f32, dur: f32) -> f32 {
    // 0→1→0 with sine.inOut easing, period 2*dur
    let ph = (t / dur) % 2.0;
    let lin = if ph <= 1.0 { ph } else { 2.0 - ph };
    0.5 - 0.5 * (PI * lin).cos()
}

pub fn loop_transform(kind: &str, t: f32, dur_override: Option<f32>) -> Transform {
    let mut o = Transform::default();
    match kind {
        "pulse" => { let s = 1.0 + 0.04 * yoyo(t, dur_override.unwrap_or(0.8)); o.sx = s; o.sy = s; }
        "float" => o.ty = -6.0 * yoyo(t, dur_override.unwrap_or(2.2)),
        "wobble" => o.rot = -1.2 + 2.4 * yoyo(t, dur_override.unwrap_or(0.9)),
        "breathe" => o.opacity = 1.0 - 0.4 * yoyo(t, dur_override.unwrap_or(3.0)),
        "glow" | "shine" | "underline-sweep" => {} // drop-shadow filter — shown as a halo by the renderer via `glow_amount`
        "flash" => {
            let d = dur_override.unwrap_or(0.5);
            let cycle = d * 3.0 + 2.0;
            let c = t % cycle;
            if c < d * 3.0 { let k = (c % d) / d; o.opacity = if k < 0.5 { 1.0 - k * 2.0 } else { (k - 0.5) * 2.0 }; }
        }
        "shake" => {
            let steps = [-1.0, 1.0, -1.0, 1.0, -0.5, 0.5, 0.0];
            let d = dur_override.unwrap_or(0.08);
            let cycle = d * 7.0 + 3.0;
            let c = t % cycle;
            if c < d * 7.0 { o.tx = steps[((c / d) as usize).min(6)] * 5.0; }
        }
        "heartbeat" => {
            let c = t % (0.64 + 1.2);
            let s = 1.06f32;
            let k = if c < 0.12 { 1.0 + (s - 1.0) * (c / 0.12) }
                    else if c < 0.24 { s - (s - 1.0) * ((c - 0.12) / 0.12) }
                    else if c < 0.34 { 1.0 + (s * 0.99 - 1.0) * ((c - 0.24) / 0.10) }
                    else if c < 0.64 { s * 0.99 - (s * 0.99 - 1.0) * ((c - 0.34) / 0.30) }
                    else { 1.0 };
            o.sx = k; o.sy = k;
        }
        "bounce" => {
            let c = t % (0.7 + 2.0);
            if c < 0.25 { o.ty = -10.0 * ease("power2.out", c / 0.25); }
            else if c < 0.7 { o.ty = -10.0 * (1.0 - ease("bounce.out", (c - 0.25) / 0.45)); }
        }
        _ => {}
    }
    o
}

pub const ANIM_TYPES: &[&str] = &["none", "slide", "fade", "zoom", "zoom-in", "pop", "wipe", "wipe-y", "clip", "blur", "flip", "flip-x", "rotate", "skew", "drop", "glitch", "slide-overshoot", "split-reveal", "mask-diag", "shutter", "bar-grow", "rgb-split"];
pub const DIRECTIONS: &[&str] = &["left", "right", "top", "bottom"];
pub const EASES: &[&str] = &["power2.out", "power2.in", "power2.inOut", "power3.out", "linear", "spring", "overshoot", "broadcast", "back-out", "bounce-out", "elastic-out", "ease-out", "ease-in", "ease-in-out"];
pub const LOOP_TYPES: &[&str] = &["none", "pulse", "breathe", "heartbeat", "float", "bounce", "wobble", "shake", "flash", "glow", "shine", "underline-sweep"];
pub const TEXT_IN: &[&str] = &["none", "reveal", "typewriter", "clip-up", "fade", "blur", "scale", "slide-up", "slide-down", "slide-left", "slide-right", "words-up", "words-fade", "words-blur", "words-scale", "words-down", "chars-up", "chars-fade", "chars-scale", "chars-blur", "chars-down", "lines-up", "lines-down", "lines-fade", "lines-scale", "chars-clip", "words-clip", "lines-clip", "scramble", "counter", "typewriter-caret"];
pub const TEXT_OUT: &[&str] = &["none", "fade", "blur", "scale", "slide-up", "slide-down", "slide-left", "slide-right", "hide", "clip-down", "words-up", "words-fade", "words-scale", "chars-up", "chars-fade", "chars-scale", "lines-up", "lines-down", "lines-fade", "lines-scale"];
pub const DIRECTIONAL: &[&str] = &["mask-diag", "slide-overshoot", "split-reveal", "slide", "wipe", "wipe-y", "clip", "flip-x", "rotate", "skew", "drop"];

pub const STAGGER_FROM: &[&str] = &["start", "end", "center", "edges"];
