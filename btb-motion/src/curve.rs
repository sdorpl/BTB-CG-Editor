//! Segment krzywej między dwoma kluczami: sześcienna krzywa Béziera w przestrzeni (czas, wartość),
//! sterowana uchwytami kluczy (jak Spline Editor w Viz).
use serde::{Deserialize, Serialize};

/// Ease jednej strony klucza w stylu After Effects: prędkość (jednostki wartości na sekundę) i wpływ
/// (influence, ułamek czasu sąsiedniego odcinka, 0..1). Uchwyt krzywej leży `influence * czas_odcinka`
/// od klucza, a jego nachylenie to `speed`. Wzorzec: crates/keyframe z EffectCraft (MIT OR Apache-2.0),
/// zaadaptowany do kanału `f32` w sekundach.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ease {
    pub speed: f32,
    pub influence: f32,
}

impl Ease {
    /// Easy Ease: zerowa prędkość, wpływ 33,33%.
    pub const EASY: Ease = Ease {
        speed: 0.0,
        influence: 1.0 / 3.0,
    };
    /// Wpływ uchwytu prostego odcinka i "Smooth" (jedna trzecia odcinka).
    pub const THIRD: f32 = 1.0 / 3.0;
    /// Wpływ auto-Béziera (jedna szósta odcinka, jak w AE).
    pub const AUTO_INFLUENCE: f32 = 1.0 / 6.0;
}

impl Default for Ease {
    fn default() -> Self {
        Ease {
            speed: 0.0,
            influence: Ease::AUTO_INFLUENCE,
        }
    }
}

/// Uchwyt klucza. `Free` to wektor względem klucza (dt, dv) zapisany wprost: dla prawego uchwytu
/// dt >= 0, dla lewego dt <= 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Handle {
    /// Prosta do sąsiedniego klucza.
    Linear,
    /// Styczna wyliczona z sąsiadów (Catmull-Rom); na końcach pozioma, czyli łagodne wejście/wyjście.
    #[default]
    Smooth,
    Free {
        dt: f32,
        dv: f32,
    },
    /// Prawy uchwyt: wartość klucza trzymana do następnego klucza (skok). Jak kanały bez interpolacji,
    /// ale per klucz; używane do wypieków animacji schodkowych (np. glitch).
    Hold,
    /// Prędkość i wpływ (After Effects, graf prędkości). Nie zależy od długości odcinka, więc zachowuje
    /// charakter ruchu po przesunięciu kluczy w czasie.
    Ease {
        speed: f32,
        influence: f32,
    },
    /// Auto-Bézier z AE: prędkość z sąsiadów (Catmull-Rom), wpływ 1/6; na końcach zerowa prędkość.
    Auto,
}

impl Handle {
    /// Easy Ease (prędkość 0, wpływ 33,33%).
    pub fn easy() -> Handle {
        Handle::Ease {
            speed: Ease::EASY.speed,
            influence: Ease::EASY.influence,
        }
    }
    pub fn from_ease(e: Ease) -> Handle {
        Handle::Ease {
            speed: e.speed,
            influence: e.influence,
        }
    }
}

/// Wektor uchwytu `h` klucza w punkcie `p`, dla odcinka o długości `seg` = (dt, dv).
/// `slope` to nachylenie dla `Smooth` i `Auto`. `right` wybiera stronę.
pub(crate) fn handle_vec(h: Handle, seg: (f32, f32), slope: f32, right: bool) -> (f32, f32) {
    let sign = if right { 1.0 } else { -1.0 };
    match h {
        // lewy Hold kształtuje odcinek przychodzący jak prosta (prawy Hold jest rozstrzygany wcześniej)
        Handle::Linear | Handle::Hold => (sign * seg.0 * Ease::THIRD, sign * seg.1 * Ease::THIRD),
        Handle::Smooth => (
            sign * seg.0 * Ease::THIRD,
            sign * slope * seg.0 * Ease::THIRD,
        ),
        Handle::Auto => (
            sign * seg.0 * Ease::AUTO_INFLUENCE,
            sign * slope * seg.0 * Ease::AUTO_INFLUENCE,
        ),
        Handle::Free { dt, dv } => (dt, dv),
        Handle::Ease { speed, influence } => {
            let dt = influence.clamp(0.0, 1.0) * seg.0;
            (sign * dt, sign * speed * dt)
        }
    }
}

/// Wartość krzywej w czasie `t` na odcinku (t0,v0)->(t1,v1) z wektorami uchwytów `out0` i `in1`
/// (oba względem swoich kluczy). Wektory są przycinane tak, by krzywa pozostała funkcją czasu.
pub(crate) fn eval_segment(
    p0: (f32, f32),
    p3: (f32, f32),
    out0: (f32, f32),
    in1: (f32, f32),
    t: f32,
) -> f32 {
    let span = p3.0 - p0.0;
    if span <= 0.0 {
        return p3.1;
    }
    let c1 = (p0.0 + out0.0.clamp(0.0, span), p0.1 + out0.1);
    let c2 = (p3.0 + in1.0.clamp(-span, 0.0), p3.1 + in1.1);
    let bez = |a: f32, b: f32, c: f32, d: f32, u: f32| {
        let m = 1.0 - u;
        m * m * m * a + 3.0 * m * m * u * b + 3.0 * m * u * u * c + u * u * u * d
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        if bez(p0.0, c1.0, c2.0, p3.0, mid) < t {
            lo = mid
        } else {
            hi = mid
        }
    }
    bez(p0.1, c1.1, c2.1, p3.1, 0.5 * (lo + hi))
}
