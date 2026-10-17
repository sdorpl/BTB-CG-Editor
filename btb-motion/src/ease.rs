//! Easingi o nazwach GSAP (po normalizacji `CGMotion.normalizeEase`) oraz `cubic-bezier(...)`.
//! Jedno źródło dla edytora (egui), mostu i strony wyjściowej (WASM).
use std::f32::consts::PI;

fn back_out(x: f32, s: f32) -> f32 {
    let t = x - 1.0;
    t * t * ((s + 1.0) * t + s) + 1.0
}
fn back_in(x: f32, s: f32) -> f32 {
    x * x * ((s + 1.0) * x - s)
}
fn bounce_out(x: f32) -> f32 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if x < 1.0 / d1 {
        n1 * x * x
    } else if x < 2.0 / d1 {
        let x = x - 1.5 / d1;
        n1 * x * x + 0.75
    } else if x < 2.5 / d1 {
        let x = x - 2.25 / d1;
        n1 * x * x + 0.9375
    } else {
        let x = x - 2.625 / d1;
        n1 * x * x + 0.984375
    }
}
fn elastic_out(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    2f32.powf(-10.0 * x) * ((x * 10.0 - 0.75) * (2.0 * PI / 3.0)).sin() + 1.0
}
fn pow_out(x: f32, p: i32) -> f32 {
    1.0 - (1.0 - x).powi(p)
}
fn pow_in(x: f32, p: i32) -> f32 {
    x.powi(p)
}
fn pow_inout(x: f32, p: i32) -> f32 {
    if x < 0.5 {
        0.5 * (2.0 * x).powi(p)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - x)).powi(p)
    }
}

/// cubic-bezier(x1,y1,x2,y2) — same two legacy presets as cgmotion (Spring / Bounce → back.*).
fn cubic_bezier(args: &str, x: f32) -> Option<f32> {
    if args.contains("1.56") || args.contains("0.64") {
        return Some(back_out(x, 1.7));
    }
    if args.contains("-0.55") {
        return Some(back_in(x, 1.7));
    }
    let v: Vec<f32> = args
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if v.len() != 4 || !(0.0..=1.0).contains(&v[0]) || !(0.0..=1.0).contains(&v[2]) {
        return None;
    }
    let (x1, y1, x2, y2) = (v[0], v[1], v[2], v[3]);
    let (cx, cy) = (3.0 * x1, 3.0 * y1);
    let (bx, by) = (3.0 * (x2 - x1) - cx, 3.0 * (y2 - y1) - cy);
    let (ax, ay) = (1.0 - cx - bx, 1.0 - cy - by);
    let bx_at = |t: f32| ((ax * t + bx) * t + cx) * t;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let t = (lo + hi) * 0.5;
        if bx_at(t) < x {
            lo = t
        } else {
            hi = t
        }
    }
    let t = (lo + hi) * 0.5;
    Some(((ay * t + by) * t + cy) * t)
}

pub fn ease(name: &str, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if let Some(rest) = name.strip_prefix("cubic-bezier(") {
        if let Some(y) = cubic_bezier(rest.trim_end_matches(')'), x) {
            return y;
        }
        return pow_out(x, 2);
    }
    let name = match name {
        "" => "power2.out",
        "linear" | "none" => return x,
        "ease-in" => "power2.in",
        "ease-out" => "power2.out",
        "ease-in-out" => "power2.inOut",
        "spring" => "back.out(1.4)",
        "spring-soft" => "back.out(0.8)",
        "spring-hard" => "back.out(2.2)",
        "overshoot" => "back.out(1.7)",
        "broadcast" => "power3.out",
        "back-in" => "back.in",
        "back-out" => "back.out",
        "bounce-out" => "bounce.out",
        "elastic-out" => "elastic.out",
        other => other,
    };
    let (kind, arg) = match name.split_once('(') {
        Some((k, a)) => (k, a.trim_end_matches(')').parse::<f32>().ok()),
        None => (name, None),
    };
    match kind {
        "power1.out" | "quad.out" => pow_out(x, 2),
        "power1.in" | "quad.in" => pow_in(x, 2),
        "power1.inOut" | "quad.inOut" => pow_inout(x, 2),
        "sine.out" => (x * PI / 2.0).sin(),
        "sine.in" => 1.0 - (x * PI / 2.0).cos(),
        "sine.inOut" => -((PI * x).cos() - 1.0) / 2.0,
        "power2.out" | "cubic.out" => pow_out(x, 3),
        "power2.in" | "cubic.in" => pow_in(x, 3),
        "power2.inOut" | "cubic.inOut" => pow_inout(x, 3),
        "power3.out" | "quart.out" => pow_out(x, 4),
        "power3.in" | "quart.in" => pow_in(x, 4),
        "power3.inOut" | "quart.inOut" => pow_inout(x, 4),
        "power4.out" | "quint.out" => pow_out(x, 5),
        "power4.in" | "quint.in" => pow_in(x, 5),
        "power4.inOut" | "quint.inOut" => pow_inout(x, 5),
        "back.out" => back_out(x, arg.unwrap_or(1.70158)),
        "back.in" => back_in(x, arg.unwrap_or(1.70158)),
        "bounce.out" => bounce_out(x),
        "bounce.in" => 1.0 - bounce_out(1.0 - x),
        "elastic.out" => elastic_out(x),
        _ => pow_out(x, 3),
    }
}
