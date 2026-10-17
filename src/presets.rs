//! Starter scenes for each BTB-CG element type.
use crate::i18n::tr;
use crate::model::*;

fn bg_shape(name: &str, x: f32, y: f32, w: f32, h: f32, fill: Fill) -> Layer {
    let mut l = Layer::new(LayerType::Shape);
    l.name = name.into();
    l.x = x; l.y = y; l.w = w; l.h = h;
    l.fill = Some(fill);
    l
}

fn solid(c: &str) -> Fill { Fill { fill_type: FillType::Solid, color: c.into(), ..Default::default() } }
fn gradient(c1: &str, c2: &str, angle: f32) -> Fill {
    Fill { fill_type: FillType::Gradient, color: c1.into(), color2: c2.into(), angle, extra: Extra::new() }
}

fn text(name: &str, field: Option<&str>, default: &str, x: f32, y: f32, w: f32, h: f32, size: f32, weight: &str) -> Layer {
    let mut l = Layer::new(LayerType::Text);
    l.name = name.into();
    l.field = field.map(String::from);
    l.default_text = Some(default.into());
    l.x = x; l.y = y; l.w = w; l.h = h;
    let f = l.font.as_mut().unwrap();
    f.size = size;
    f.weight = weight.into();
    l
}

pub fn scene_for(elem_type: &str) -> Scene {
    match elem_type {
        "BADGE" => badge(),
        "TICKER" => ticker(),
        "CLOCK" => clock(),
        "IMAGE" => image(),
        "GRAPHIC" => graphic(),
        _ => lower_third(),
    }
}

fn scene(w: f32, h: f32, layers: Vec<Layer>) -> Scene {
    Scene { frame: Frame { w, h }, layers, ..Default::default() }
}

fn lower_third() -> Scene {
    let mut accent = bg_shape(tr("preset.accent"), 0.0, 0.0, 14.0, 80.0, solid("#ef4444"));
    accent.motion.motion_in = Some(MotionSpec::new("wipe", Some("top"), 0.4, 0.0, "power2.out"));
    let mut bg = bg_shape(tr("preset.bg"), 14.0, 0.0, 860.0, 80.0, gradient("#0b1220", "#1e293b", 90.0));
    bg.motion.motion_in = Some(MotionSpec::new("wipe", Some("left"), 0.5, 0.1, "power2.out"));
    bg.shadow = Some(Shadow::Flag(true));
    let mut title = text(tr("preset.title"), Some("TITLE"), tr("preset.sample_name"), 40.0, 0.0, 820.0, 80.0, 44.0, "700");
    title.font.as_mut().unwrap().transform = "uppercase".into();
    title.squash = Some(true);
    title.motion.text = Some(TextEffect::new("reveal", 0.5, 0.35));
    scene(900.0, 80.0, vec![accent, bg, title])
}

fn badge() -> Scene {
    let mut bg = bg_shape(tr("preset.bg"), 0.0, 0.0, 260.0, 56.0, solid("#dc2626"));
    bg.radius = Some(Radius::Uniform(28.0));
    bg.motion.motion_in = Some(MotionSpec::new("pop", None, 0.45, 0.0, "spring"));
    let mut t = text(tr("preset.text"), Some("TITLE"), tr("preset.sample_live"), 0.0, 0.0, 260.0, 56.0, 26.0, "800");
    {
        let f = t.font.as_mut().unwrap();
        f.align = "center".into();
        f.transform = "uppercase".into();
        f.letter_spacing = 1.0;
    }
    t.squash = Some(true);
    scene(260.0, 56.0, vec![bg, t])
}

fn ticker() -> Scene {
    let mut bar = bg_shape(tr("preset.bar"), 0.0, 0.0, 1920.0, 60.0, solid("#0f172a"));
    bar.motion.motion_in = Some(MotionSpec::new("wipe", Some("left"), 0.5, 0.0, "power2.out"));
    let mut wiper = bg_shape(tr("preset.wiper"), 0.0, 0.0, 240.0, 60.0, solid("#b91c1c"));
    wiper.skew = -10.0;
    wiper.motion.motion_in = Some(MotionSpec::new("slide", Some("left"), 0.5, 0.15, "power2.out"));
    let mut label = text(tr("preset.label"), Some("INTRO_TEXT"), tr("preset.sample_news"), 0.0, 0.0, 230.0, 60.0, 24.0, "800");
    {
        let f = label.font.as_mut().unwrap();
        f.align = "center".into();
        f.transform = "uppercase".into();
    }
    let mut tk = Layer::new(LayerType::Ticker);
    tk.x = 270.0; tk.y = 0.0; tk.w = 1630.0; tk.h = 60.0;
    scene(1920.0, 60.0, vec![bar, wiper, label, tk])
}

fn clock() -> Scene {
    let mut bg = bg_shape(tr("preset.bg"), 0.0, 0.0, 152.0, 40.0, solid("#111827"));
    bg.radius = Some(Radius::Uniform(6.0));
    bg.stroke = Some(Stroke { color: "#334155".into(), width: 1.0, extra: Extra::new() });
    bg.motion.motion_in = Some(MotionSpec::new("fade", None, 0.4, 0.0, "power2.out"));
    let mut c = Layer::new(LayerType::Clock);
    c.x = 0.0; c.y = 0.0; c.w = 152.0; c.h = 40.0;
    c.font.as_mut().unwrap().size = 26.0;
    scene(152.0, 40.0, vec![bg, c])
}

fn image() -> Scene {
    let mut img = Layer::new(LayerType::Image);
    img.w = 240.0; img.h = 120.0;
    img.motion.motion_in = Some(MotionSpec::new("zoom", None, 0.5, 0.0, "power2.out"));
    scene(240.0, 120.0, vec![img])
}

fn graphic() -> Scene {
    let mut bg = bg_shape(tr("preset.bg"), 0.0, 0.0, 800.0, 480.0, gradient("#1e293b", "#0f172a", 135.0));
    bg.radius = Some(Radius::Uniform(16.0));
    bg.shadow = Some(Shadow::Flag(true));
    bg.motion.motion_in = Some(MotionSpec::new("zoom", None, 0.5, 0.0, "power2.out"));
    let mut img = Layer::new(LayerType::Image);
    img.x = 30.0; img.y = 30.0; img.w = 300.0; img.h = 420.0;
    img.radius = Some(Radius::Uniform(10.0));
    img.fit = Some("cover".into());
    let t = text(tr("preset.heading"), Some("TITLE"), tr("preset.sample_heading"), 360.0, 40.0, 410.0, 90.0, 40.0, "800");
    let mut body = text(tr("preset.body"), Some("BODY"), tr("preset.sample_body"), 360.0, 140.0, 410.0, 120.0, 24.0, "400");
    body.font.as_mut().unwrap().color = "#cbd5e1".into();
    body.font.as_mut().unwrap().valign = "top".into();
    scene(800.0, 480.0, vec![bg, img, t, body])
}
