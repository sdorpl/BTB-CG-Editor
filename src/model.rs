//! Data model mirroring BTB-CG's element + scene JSON (see scene-model.js / element-model.js).
//! Every struct keeps unknown keys in `extra` so import → edit → export is lossless.
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

pub type Extra = Map<String, Value>;

fn de_stringish<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        _ => "700".into(),
    })
}

/// Liczba zapisana jako tekst (`"60"`) w starszych elementach, np. `layout.height` tickerów.
fn de_opt_f32<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f32>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().map(|v| v as f32),
        Value::String(s) => s.trim().trim_end_matches("px").trim().parse().ok(),
        _ => None,
    })
}

fn is_zero(v: &f32) -> bool { *v == 0.0 }
fn is_one(v: &f32) -> bool { *v == 1.0 }
fn t() -> bool { true }

// ── Fill / Stroke / Radius / Shadow ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FillType { #[default] Solid, Gradient, None }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    #[serde(rename = "type", default)]
    pub fill_type: FillType,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub color2: String,
    #[serde(default = "d90")]
    pub angle: f32,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d90() -> f32 { 90.0 }

impl Default for Fill {
    fn default() -> Self {
        Self { fill_type: FillType::Solid, color: "#0047AB".into(), color2: "#000000".into(), angle: 90.0, extra: Extra::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stroke {
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub width: f32,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Default for Stroke {
    fn default() -> Self { Self { color: "#ffffff".into(), width: 0.0, extra: Extra::new() } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Radius { Uniform(f32), Corners(Vec<f32>) }
impl Radius {
    pub fn uniform(&self) -> f32 {
        match self { Radius::Uniform(r) => *r, Radius::Corners(c) => c.first().copied().unwrap_or(0.0) }
    }
    /// [top-left, top-right, bottom-right, bottom-left] like CSS shorthand.
    pub fn corners(&self) -> [f32; 4] {
        match self {
            Radius::Uniform(r) => [*r; 4],
            Radius::Corners(c) => match c.len() {
                0 => [0.0; 4],
                1 => [c[0]; 4],
                2 => [c[0], c[1], c[0], c[1]],
                3 => [c[0], c[1], c[2], c[1]],
                _ => [c[0], c[1], c[2], c[3]],
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Shadow {
    Flag(bool),
    Custom {
        #[serde(default)] x: f32,
        #[serde(default = "d10")] y: f32,
        #[serde(default = "d25")] blur: f32,
        #[serde(default)] color: String,
    },
}
fn d10() -> f32 { 10.0 }
fn d25() -> f32 { 25.0 }
impl Shadow {
    pub fn enabled(&self) -> bool { !matches!(self, Shadow::Flag(false)) }
}

// ── Font ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontStyle {
    #[serde(default = "d_family")]
    pub family: String,
    #[serde(default = "d_size")]
    pub size: f32,
    #[serde(default = "d_weight", deserialize_with = "de_stringish")]
    pub weight: String,
    #[serde(default = "d_white")]
    pub color: String,
    #[serde(default = "d_none")]
    pub transform: String,
    #[serde(default = "d_left")]
    pub align: String,
    #[serde(default = "d_middle")]
    pub valign: String,
    #[serde(default)]
    pub letter_spacing: f32,
    #[serde(default = "d_lh")]
    pub line_height: f32,
    #[serde(default)]
    pub italic: bool,
    #[serde(default = "d_shadow")]
    pub shadow: String,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d_family() -> String { "Bahnschrift".into() }
fn d_size() -> f32 { 36.0 }
fn d_weight() -> String { "700".into() }
fn d_white() -> String { "#ffffff".into() }
fn d_none() -> String { "none".into() }
fn d_left() -> String { "left".into() }
fn d_middle() -> String { "middle".into() }
fn d_lh() -> f32 { 1.2 }
fn d_shadow() -> String { "0 2px 8px rgba(0,0,0,0.7)".into() }

impl Default for FontStyle {
    fn default() -> Self {
        Self {
            family: d_family(), size: 36.0, weight: d_weight(), color: d_white(), transform: d_none(),
            align: d_left(), valign: d_middle(), letter_spacing: 0.0, line_height: 1.2, italic: false,
            shadow: d_shadow(), extra: Extra::new(),
        }
    }
}

// ── Motion ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionSpec {
    #[serde(rename = "type", default)]
    pub anim_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default = "d_dur")]
    pub duration: f32,
    #[serde(default)]
    pub delay: f32,
    #[serde(default = "d_ease")]
    pub ease: String,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d_dur() -> f32 { 0.5 }
fn d_ease() -> String { "power2.out".into() }

impl MotionSpec {
    pub fn new(anim_type: &str, direction: Option<&str>, duration: f32, delay: f32, ease: &str) -> Self {
        Self { anim_type: anim_type.into(), direction: direction.map(String::from), duration, delay, ease: ease.into(), extra: Extra::new() }
    }
}
impl Default for MotionSpec {
    fn default() -> Self { Self::new("fade", Some("left"), 0.5, 0.0, "power2.out") }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEffect {
    #[serde(rename = "type", default)]
    pub effect_type: String,
    #[serde(default = "d_dur")]
    pub duration: f32,
    #[serde(default)]
    pub delay: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger: Option<f32>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl TextEffect {
    pub fn new(effect_type: &str, duration: f32, delay: f32) -> Self {
        Self { effect_type: effect_type.into(), duration, delay, stagger: None, extra: Extra::new() }
    }

    /// Effect the editor can play: own type, or the `fallback` key when the type is unknown here.
    pub fn name(&self) -> &str {
        use crate::anim::{TEXT_IN, TEXT_OUT};
        let t = self.effect_type.as_str();
        if TEXT_IN.contains(&t) || TEXT_OUT.contains(&t) { return t; }
        self.extra.get("fallback").and_then(|v| v.as_str()).unwrap_or(t)
    }

    /// Parametry dla silnika btb-motion (efekt po ewentualnym `fallback`, ease z klucza `ease`).
    pub fn spec(&self) -> btb_motion::TextSpec {
        btb_motion::TextSpec {
            effect: self.name().to_string(),
            duration: self.duration,
            delay: self.delay,
            stagger: self.stagger.unwrap_or(0.0),
            from: self.stagger_from().to_string(),
            ease: self.extra.get("ease").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    }

    /// Stagger origin: "start" (default), "end", "center", "edges" (stored as `staggerFrom`).
    pub fn stagger_from(&self) -> &str {
        self.extra.get("staggerFrom").and_then(|v| v.as_str()).unwrap_or("start")
    }
}

/// Per-layer motion. `text: None` = inherit from the element; `Some(type:"none")` = off.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Motion {
    #[serde(rename = "in", default, skip_serializing_if = "Option::is_none")]
    pub motion_in: Option<MotionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out: Option<MotionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextEffect>,
    #[serde(rename = "textOut", default, skip_serializing_if = "Option::is_none")]
    pub text_out: Option<TextEffect>,
}

// ── Layer ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
#[serde(rename_all = "lowercase")]
pub enum LayerType { Shape, Text, Image, Video, Clock, Ticker }

impl LayerType {
    pub const ALL: [LayerType; 6] = [Self::Shape, Self::Text, Self::Image, Self::Video, Self::Clock, Self::Ticker];
    /// Nazwa jak w JSON (`serde`), używana w testach tłumaczeń.
    #[cfg(test)]
    pub fn key(&self) -> &'static str {
        match self {
            Self::Shape => "shape", Self::Text => "text", Self::Image => "image",
            Self::Video => "video", Self::Clock => "clock", Self::Ticker => "ticker",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Shape => crate::i18n::tr("layertype.shape"), Self::Text => crate::i18n::tr("layertype.text"), Self::Image => crate::i18n::tr("layertype.image"),
            Self::Video => crate::i18n::tr("layertype.video"), Self::Clock => crate::i18n::tr("layertype.clock"), Self::Ticker => crate::i18n::tr("layertype.ticker"),
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Shape => "■", Self::Text => "T", Self::Image => "🖼",
            Self::Video => "▶", Self::Clock => "🕐", Self::Ticker => "↔",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FitText {
    /// `id` warstwy tekstowej z `autoW`.
    pub to: String,
    /// `size` (domyślnie): szerokość rośnie o deltę; `follow`: pozycja przesuwa się o deltę.
    #[serde(default = "d_fit_size")]
    pub mode: String,
}
fn d_fit_size() -> String { "size".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    #[serde(rename = "type")]
    pub layer_type: LayerType,
    #[serde(default)]
    pub name: String,
    #[serde(default = "t")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "d200")]
    pub w: f32,
    #[serde(default = "d60")]
    pub h: f32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub skew: f32,
    #[serde(default = "d1")]
    pub opacity: f32,
    #[serde(default)]
    pub motion: Motion,
    /// Klucze użytkownika (silnik btb-motion): czas Directora (po Stop liczy się czas OUT), wartości
    /// absolutne właściwości z `motion_bridge::KEY_PROPS`; składają się z presetami IN/OUT.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<btb_motion::Channel>,
    /// Ścieżka ruchu warstwy (kanał pozycji ze ścieżką Béziera): przesunięcie względem pozycji warstwy,
    /// dodaje się do kanałów `tx`/`ty` i presetów.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<btb_motion::PathChannel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    // shape
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<Radius>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    // text / clock / ticker
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(rename = "defaultText", default, skip_serializing_if = "Option::is_none")]
    pub default_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squash: Option<bool>,
    /// Auto-szerokość (M2.0): szerokość warstwy tekstowej podąża za tekstem (`w` = szerokość z projektu), z klamrą `minW`/`maxW`.
    #[serde(rename = "autoW", default, skip_serializing_if = "Option::is_none")]
    pub auto_w: Option<bool>,
    #[serde(rename = "minW", default, skip_serializing_if = "Option::is_none")]
    pub min_w: Option<f32>,
    #[serde(rename = "maxW", default, skip_serializing_if = "Option::is_none")]
    pub max_w: Option<f32>,
    /// Warstwa podąża za szerokością tekstu innej warstwy z `autoW` (tło `size`, ikona/kropka `follow`).
    #[serde(rename = "fitText", default, skip_serializing_if = "Option::is_none")]
    pub fit_text: Option<FitText>,
    // image / video
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    #[serde(rename = "maskLayer", default, skip_serializing_if = "Option::is_none")]
    pub mask_layer: Option<String>,
    #[serde(rename = "loop", default, skip_serializing_if = "Option::is_none")]
    pub video_loop: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    // clock
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    // ticker
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d200() -> f32 { 200.0 }
fn d60() -> f32 { 60.0 }
fn d1() -> f32 { 1.0 }

impl Layer {
    pub fn new(layer_type: LayerType) -> Self {
        let id = format!("l_{}", &Uuid::new_v4().simple().to_string()[..6]);
        let mut l = Self {
            id, layer_type, name: layer_type.label().to_owned(), visible: true, locked: false,
            x: 0.0, y: 0.0, w: 200.0, h: 60.0, rotation: 0.0, skew: 0.0, opacity: 1.0,
            motion: Motion::default(), keys: vec![], path: None, group: None,
            fill: None, stroke: None, radius: None, shadow: None, effect: None,
            field: None, text: None, default_text: None, font: None, squash: None, auto_w: None, min_w: None, max_w: None, fit_text: None,
            src: None, fit: None, bind: None, mask_layer: None, video_loop: None, muted: None,
            format: None, speed: None, separator: None, gap: None, extra: Extra::new(),
        };
        match layer_type {
            LayerType::Shape => {
                l.fill = Some(Fill::default());
                l.stroke = Some(Stroke::default());
                l.radius = Some(Radius::Uniform(0.0));
                l.shadow = Some(Shadow::Flag(false));
                l.effect = Some("none".into());
            }
            LayerType::Text => {
                l.field = Some("TITLE".into());
                l.text = Some(String::new());
                l.default_text = Some(crate::i18n::tr("preset.default_text").into());
                l.font = Some(FontStyle::default());
                l.squash = Some(false);
                l.w = 600.0;
            }
            LayerType::Image => {
                l.src = Some(String::new());
                l.bind = Some(String::new());
                l.fit = Some("contain".into());
                l.radius = Some(Radius::Uniform(0.0));
            }
            LayerType::Video => {
                l.src = Some(String::new());
                l.fit = Some("cover".into());
                l.video_loop = Some(true);
                l.muted = Some(true);
                l.radius = Some(Radius::Uniform(0.0));
            }
            LayerType::Clock => {
                l.format = Some("HH:MM:SS".into());
                l.font = Some(FontStyle { size: 28.0, align: "center".into(), ..Default::default() });
            }
            LayerType::Ticker => {
                l.speed = Some(100.0);
                l.separator = Some("•".into());
                l.gap = Some(48.0);
                l.font = Some(FontStyle { size: 30.0, weight: "600".into(), shadow: "none".into(), ..Default::default() });
            }
        }
        l
    }

    pub fn font_or_default(&self) -> FontStyle { self.font.clone().unwrap_or_default() }
}

// ── Scene ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub w: f32,
    pub h: f32,
}
impl Default for Frame {
    fn default() -> Self { Self { w: 1000.0, h: 100.0 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    #[serde(default = "d_ver")]
    pub version: u32,
    #[serde(default)]
    pub detached: bool,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub layers: Vec<Layer>,
    /// Klucze użytkownika całego elementu (cel `"element"` w silniku): ta sama semantyka co `Layer.keys`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<btb_motion::Channel>,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d_ver() -> u32 { 1 }

impl Default for Scene {
    fn default() -> Self {
        Self { version: 1, detached: false, frame: Frame::default(), layers: vec![], keys: vec![], extra: Extra::new() }
    }
}

impl Scene {
    /// Kanały kluczy celu: warstwa (id) albo cały element (`"element"`).
    pub fn key_channels(&self, id: &str) -> Option<&Vec<btb_motion::Channel>> {
        if id == crate::motion_bridge::ELEMENT { Some(&self.keys) } else { self.layers.iter().find(|l| l.id == id).map(|l| &l.keys) }
    }
    pub fn key_channels_mut(&mut self, id: &str) -> Option<&mut Vec<btb_motion::Channel>> {
        if id == crate::motion_bridge::ELEMENT { Some(&mut self.keys) } else { self.layers.iter_mut().find(|l| l.id == id).map(|l| &mut l.keys) }
    }
}

// ── Layout / Animation ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    #[serde(default = "d_side")]
    pub side: String,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_f32")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_f32")]
    pub y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_f32")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_f32")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_opt_f32")]
    pub layer: Option<f32>,
    #[serde(default = "d1")]
    pub scale: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f32,
    /// przezroczystość całego elementu na wizji (mnoży się z animacją i warstwami)
    #[serde(default = "d1", skip_serializing_if = "is_one")]
    pub opacity: f32,
    #[serde(default)]
    pub margin_x: f32,
    #[serde(default)]
    pub margin_y: f32,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d_side() -> String { "custom".into() }

impl Default for Layout {
    fn default() -> Self {
        Self {
            side: "custom".into(), x: Some(100.0), y: Some(800.0), width: None, height: None, layer: Some(1.0),
            scale: 1.0, rotation: 0.0, opacity: 1.0, margin_x: 0.0, margin_y: 0.0, extra: Extra::new(),
        }
    }
}

impl Layout {
    /// Top-left of the element frame on the 1920×1080 output, like BTB-CG's layout engine.
    pub fn frame_origin(&self, fw: f32, fh: f32) -> (f32, f32) {
        let (mx, my) = (self.margin_x, self.margin_y);
        match self.side.as_str() {
            "top-left" => (mx, my),
            "top-right" => (1920.0 - fw - mx, my),
            "bottom-left" => (mx, 1080.0 - fh - my),
            "bottom-right" => (1920.0 - fw - mx, 1080.0 - fh - my),
            "center" => ((1920.0 - fw) / 2.0, (1080.0 - fh) / 2.0),
            _ => (self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
        }
    }

    /// Przesuwa ramkę na wizji o (dx, dy) pikseli pola, zachowując kotwicę: dla narożników zmieniają się marginesy
    /// (z odpowiednim znakiem), dla `custom` współrzędne X/Y. Wyśrodkowanie nie ma marginesów, więc przechodzi na `custom`.
    pub fn move_by(&mut self, dx: f32, dy: f32, fw: f32, fh: f32) {
        match self.side.as_str() {
            "top-left" => { self.margin_x += dx; self.margin_y += dy; }
            "top-right" => { self.margin_x -= dx; self.margin_y += dy; }
            "bottom-left" => { self.margin_x += dx; self.margin_y -= dy; }
            "bottom-right" => { self.margin_x -= dx; self.margin_y -= dy; }
            "center" => {
                let (x, y) = self.frame_origin(fw, fh);
                self.side = "custom".into();
                self.x = Some(x + dx);
                self.y = Some(y + dy);
            }
            _ => { self.x = Some(self.x.unwrap_or(0.0) + dx); self.y = Some(self.y.unwrap_or(0.0) + dy); }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimSpec {
    #[serde(rename = "type", default = "d_none")]
    pub anim_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default = "d_dur")]
    pub duration: f32,
    #[serde(default)]
    pub delay: f32,
    #[serde(default = "d_ease_css")]
    pub ease: String,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d_ease_css() -> String { "ease-out".into() }
impl Default for AnimSpec {
    fn default() -> Self {
        Self { anim_type: "none".into(), direction: None, duration: 0.5, delay: 0.0, ease: d_ease_css(), extra: Extra::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ElementAnimation {
    #[serde(rename = "in", default)]
    pub anim_in: AnimSpec,
    #[serde(default)]
    pub out: AnimSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextEffect>,
    #[serde(rename = "textOut", default, skip_serializing_if = "Option::is_none")]
    pub text_out: Option<TextEffect>,
    #[serde(rename = "loop", default, skip_serializing_if = "Option::is_none")]
    pub loop_spec: Option<LoopSpec>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoopSpec {
    #[serde(rename = "type", default)]
    pub loop_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[serde(flatten)]
    pub extra: Extra,
}

// ── Element ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default = "d_elem_type")]
    pub elem_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<Scene>,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub animation: ElementAnimation,
    #[serde(default)]
    pub html_template: String,
    #[serde(default)]
    pub css_template: String,
    #[serde(default)]
    pub js_template: String,
    #[serde(rename = "ocgInputs", default)]
    pub ocg_inputs: Vec<Value>,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(flatten)]
    pub extra: Extra,
}
fn d_elem_type() -> String { "LOWER_THIRD".into() }

impl Element {
    pub const ELEM_TYPES: [&'static str; 6] = ["LOWER_THIRD", "BADGE", "TICKER", "CLOCK", "IMAGE", "GRAPHIC"];

    pub fn elem_type_label(t: &str) -> &'static str {
        use crate::i18n::tr;
        match t {
            "LOWER_THIRD" => tr("elemtype.LOWER_THIRD"),
            "BADGE" => tr("elemtype.BADGE"),
            "TICKER" => tr("elemtype.TICKER"),
            "CLOCK" => tr("elemtype.CLOCK"),
            "IMAGE" => tr("elemtype.IMAGE"),
            "GRAPHIC" => tr("elemtype.GRAPHIC"),
            _ => tr("elemtype.default"),
        }
    }

    pub fn new_typed(name: &str, elem_type: &str) -> Self {
        let scene = crate::presets::scene_for(elem_type);
        let mut el = Self {
            id: format!("el_{}", &Uuid::new_v4().simple().to_string()[..8]),
            name: name.to_owned(),
            elem_type: elem_type.to_owned(),
            title: String::new(),
            scene: None,
            layout: Layout::default(),
            animation: ElementAnimation::default(),
            html_template: String::new(),
            css_template: String::new(),
            js_template: String::new(),
            ocg_inputs: vec![],
            fields: Map::new(),
            extra: Extra::new(),
        };
        el.animation.anim_in = AnimSpec { anim_type: "fade".into(), duration: 0.4, ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "fade".into(), duration: 0.3, ease: "ease-in".into(), ..Default::default() };
        el.extra.insert("visible".into(), Value::Bool(false));
        el.extra.insert("version".into(), Value::from(1));
        crate::compile::apply_to_element(&mut el, scene);
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_by_shifts_the_frame_origin_for_every_anchor() {
        for side in ["custom", "top-left", "top-right", "bottom-left", "bottom-right", "center"] {
            let mut lo = Layout { side: side.into(), margin_x: 40.0, margin_y: 30.0, ..Layout::default() };
            let (x0, y0) = lo.frame_origin(600.0, 120.0);
            lo.move_by(25.0, -15.0, 600.0, 120.0);
            let (x1, y1) = lo.frame_origin(600.0, 120.0);
            assert!((x1 - x0 - 25.0).abs() < 1e-3 && (y1 - y0 + 15.0).abs() < 1e-3, "{side}: ({x0}, {y0}) -> ({x1}, {y1})");
        }
    }

    #[test]
    fn moving_a_centered_frame_switches_to_custom() {
        let mut lo = Layout { side: "center".into(), ..Layout::default() };
        lo.move_by(10.0, 0.0, 400.0, 100.0);
        assert_eq!(lo.side, "custom");
    }
}

#[cfg(test)]
mod opacity_tests {
    use super::*;

    #[test]
    fn layout_opacity_defaults_to_one_and_is_only_written_when_changed() {
        let lo: Layout = serde_json::from_str(r#"{"side":"custom","x":1,"y":2}"#).unwrap();
        assert_eq!(lo.opacity, 1.0);
        assert!(!serde_json::to_string(&lo).unwrap().contains("opacity"));
        let faded = Layout { opacity: 0.4, ..lo };
        let back: Layout = serde_json::from_str(&serde_json::to_string(&faded).unwrap()).unwrap();
        assert_eq!(back.opacity, 0.4);
    }
}
