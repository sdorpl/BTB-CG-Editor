//! Efekty tekstowe (port `applyTextEffect` / `applyTextOutEffect` z cgmotion.js).
//!
//! Efekt działa na „jednostkach”: całym elemencie tekstu albo jego znakach / słowach / liniach.
//! Liczby jednostek zna dopiero przeglądarka (dzielenie DOM, zawijanie linii), więc przychodzą w
//! `Units`; silnik liczy wyłącznie czas i właściwości każdej jednostki (`t -> Props`).
use crate::ease::ease;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    In,
    Out,
}

/// Parametry efektu (jak `config` w cgmotion: type/duration/delay/stagger/staggerFrom/ease).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSpec {
    pub effect: String,
    pub duration: f32,
    #[serde(default)]
    pub delay: f32,
    /// 0 = automatycznie (0,04 s dla znaków, 0,09 s dla reszty)
    #[serde(default)]
    pub stagger: f32,
    /// "start" | "end" | "center" | "edges"
    #[serde(default = "start")]
    pub from: String,
    /// nazwa easingu; pusta = domyślny dla trybu (power2.out dla IN, power2.in dla OUT)
    #[serde(default)]
    pub ease: String,
}
fn start() -> String {
    "start".into()
}
fn one() -> usize {
    1
}

/// Liczby jednostek z przeglądarki dla jednego elementu tekstu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Units {
    /// ile elementów DOM jest animowanych (dla `lines-*` to słowa, GSAP spłaszcza listę linii)
    pub n: usize,
    /// na co element faktycznie podzielono: "chars" | "words" | "" (niepodzielony). Efekt dzielący o
    /// innym trybie niż podział elementu (albo gdy element ma dzieci) animuje cały element jako jedną jednostkę.
    #[serde(default)]
    pub by: String,
    /// liczba linii (używana tylko przez `lines-*` do limitu staggera; w pozostałych limit liczy `n`)
    #[serde(default = "one")]
    pub lines: usize,
    /// element ma tylko tekst (bez elementów potomnych): wymagane przez scramble/counter/caret
    #[serde(default)]
    pub plain: bool,
    /// w tekście jest liczba (wymagane przez counter)
    #[serde(default)]
    pub num: bool,
}
impl Default for Units {
    fn default() -> Self {
        Self {
            n: 1,
            by: String::new(),
            lines: 1,
            plain: true,
            num: true,
        }
    }
}

/// Właściwości jednej jednostki. `clip` = widoczny prostokąt [lewo, góra, prawo, dół] (ułamki).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Props {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: f32,
    pub blur: f32,
    pub clip: [f32; 4],
    /// postęp efektu treści (scramble/counter/caret) 0..1; poza tym 1
    pub progress: f32,
}
impl Default for Props {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            opacity: 1.0,
            blur: 0.0,
            clip: [0.0, 0.0, 1.0, 1.0],
            progress: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Whole,
    Words,
    Chars,
    Lines,
}

/// Sposób dzielenia tekstu dla efektu (`words-*`, `chars-*`, `lines-*`).
pub fn split_of(effect: &str) -> Split {
    if effect.starts_with("words-") {
        Split::Words
    } else if effect.starts_with("lines-") {
        Split::Lines
    } else if effect.starts_with("chars-") {
        Split::Chars
    } else {
        Split::Whole
    }
}

/// Czy efekt dzielący (`split`) działa na jednostkach `u`, czy wraca do animacji całego elementu.
pub fn applies(split: Split, u: &Units) -> bool {
    match split {
        Split::Chars => u.by == "chars",
        Split::Words | Split::Lines => u.by == "words",
        Split::Whole => false,
    }
}

fn is_content(effect: &str) -> bool {
    matches!(effect, "scramble" | "counter" | "typewriter-caret")
}

/// Opóźnienia jednostek wg `gsap.utils.distribute({each, from})` dla siatki jednowymiarowej.
/// Rozrzut jest znormalizowany: najdalsza jednostka dostaje `each * (n - 1)`; `edges` odwraca skalę.
pub fn stagger_delays(n: usize, each: f32, from: &str) -> Vec<f32> {
    if n <= 1 {
        return vec![0.0; n];
    }
    let l = n as f32;
    let ratio = match from {
        "center" | "edges" => 0.5,
        "end" => 1.0,
        _ => 0.0,
    };
    let origin = l * ratio - 0.5;
    let d: Vec<f32> = (0..n).map(|j| (j as f32 - origin).abs()).collect();
    let (min, max) = d
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let span = each * (l - 1.0);
    d.iter()
        .map(|v| {
            let norm = if max > min {
                (v - min) / (max - min)
            } else {
                0.0
            };
            if from == "edges" {
                (1.0 - norm) * span
            } else {
                norm * span
            }
        })
        .collect()
}

fn each_for(sp: &TextSpec, split: Split, u: &Units) -> f32 {
    let per = if sp.stagger > 0.0 {
        sp.stagger
    } else if split == Split::Chars {
        0.04
    } else {
        0.09
    };
    let cap = if split == Split::Lines { u.lines } else { u.n };
    per.min(1.2 / cap.max(1) as f32)
}

fn lerp(a: f32, b: f32, m: f32) -> f32 {
    a + (b - a) * m
}

/// Jednostka `idx` w czasie `t` od początku śladu (z opóźnieniem `delay` efektu włącznie).
pub fn eval(mode: Mode, sp: &TextSpec, u: &Units, idx: usize, t: f32) -> Props {
    let eff = sp.effect.as_str();
    let split = split_of(eff);
    let content = is_content(eff);
    // efekty treści bez zastosowania wracają do fade (jak w cgmotion)
    let fallback_fade = content && !(u.plain && (eff != "counter" || u.num));
    let name = if sp.ease.is_empty() {
        if mode == Mode::In {
            "power2.out"
        } else {
            "power2.in"
        }
    } else {
        sp.ease.as_str()
    };

    let delay_i = if applies(split, u) && !content {
        stagger_delays(u.n.max(1), each_for(sp, split, u), &sp.from)
            .get(idx)
            .copied()
            .unwrap_or(0.0)
    } else {
        0.0
    };
    let local = t - sp.delay - delay_i;
    let p = if sp.duration <= 0.0 {
        if local >= 0.0 {
            1.0
        } else {
            0.0
        }
    } else {
        (local / sp.duration).clamp(0.0, 1.0)
    };
    let m = ease(name, p);
    let mut r = Props::default();

    if content && !fallback_fade {
        if mode == Mode::In {
            r.progress = m;
        }
        return r;
    }
    // `k` = stopień ukrycia: IN 1-m (od stanu początkowego do jawnego), OUT m (od jawnego do końcowego)
    let k = if mode == Mode::In { 1.0 - m } else { m };
    let (fade, scale_to, blur_px, dx, dy): (bool, f32, f32, f32, f32);
    let mut clip: Option<[f32; 4]> = None;
    match (mode, eff) {
        _ if content => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
        }
        (_, e) if split != Split::Whole => {
            let variant = e.split('-').nth(1).unwrap_or("");
            let sgn = if mode == Mode::In { 1.0 } else { -1.0 };
            match variant {
                "up" => {
                    fade = true;
                    scale_to = 1.0;
                    blur_px = 0.0;
                    dx = 0.0;
                    dy = 40.0 * sgn
                }
                "down" => {
                    fade = true;
                    scale_to = 1.0;
                    blur_px = 0.0;
                    dx = 0.0;
                    dy = -40.0 * sgn
                }
                "scale" => {
                    fade = true;
                    scale_to = 0.3;
                    blur_px = 0.0;
                    dx = 0.0;
                    dy = 0.0
                }
                "blur" => {
                    fade = true;
                    scale_to = 1.0;
                    blur_px = 14.0;
                    dx = 0.0;
                    dy = 0.0
                }
                "clip" => {
                    fade = false;
                    scale_to = 1.0;
                    blur_px = 0.0;
                    dx = 0.0;
                    dy = 0.0;
                    // IN: inset(100% 0 0 0) -> 0 (od góry); OUT: do inset(0 0 100% 0) (od dołu)
                    clip = Some(if mode == Mode::In {
                        [0.0, k, 1.0, 1.0]
                    } else {
                        [0.0, 0.0, 1.0, 1.0 - k]
                    });
                }
                _ => {
                    fade = true;
                    scale_to = 1.0;
                    blur_px = 0.0;
                    dx = 0.0;
                    dy = 0.0
                } // fade
            }
        }
        (Mode::In, "typewriter" | "reveal") => {
            fade = false;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
            clip = Some([0.0, 0.0, 1.0 - k, 1.0]);
        }
        (Mode::In, "blur" | "blurIn") | (Mode::Out, "blur" | "blurOut") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 20.0;
            dx = 0.0;
            dy = 0.0;
        }
        (_, "fade") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
        }
        (_, "scale") => {
            fade = true;
            scale_to = 0.5;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
        }
        (Mode::In, "slide-up" | "slideReveal") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 50.0;
        }
        (Mode::In, "slide-down") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = -50.0;
        }
        (Mode::In, "slide-left") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 50.0;
            dy = 0.0;
        }
        (Mode::In, "slide-right") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = -50.0;
            dy = 0.0;
        }
        (Mode::Out, "slide-up") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = -50.0;
        }
        (Mode::Out, "slide-down") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 50.0;
        }
        (Mode::Out, "slide-left") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = -50.0;
            dy = 0.0;
        }
        (Mode::Out, "slide-right") => {
            fade = true;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 50.0;
            dy = 0.0;
        }
        (Mode::In, "clip-up") => {
            fade = false;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
            clip = Some([0.0, k, 1.0, 1.0]);
        }
        (Mode::Out, "clip-down") => {
            fade = false;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
            clip = Some([0.0, k, 1.0, 1.0]);
        }
        (Mode::Out, "hide" | "slideHide") => {
            fade = false;
            scale_to = 1.0;
            blur_px = 0.0;
            dx = 0.0;
            dy = 0.0;
            clip = Some([k, 0.0, 1.0, 1.0]);
        }
        _ => return r, // nieznany efekt: brak zmian
    }
    if fade {
        r.opacity = 1.0 - k;
    }
    r.scale = lerp(1.0, scale_to, k);
    r.blur = blur_px * k;
    r.x = dx * k;
    r.y = dy * k;
    if let Some(c) = clip {
        r.clip = c;
    }
    r
}

/// Czas trwania efektu: opóźnienie + czas + najdłuższe opóźnienie staggera.
pub fn total(sp: &TextSpec, u: &Units) -> f32 {
    let split = split_of(&sp.effect);
    if !applies(split, u) || is_content(&sp.effect) {
        return sp.delay + sp.duration;
    }
    let each = each_for(sp, split, u);
    let span = stagger_delays(u.n.max(1), each, &sp.from)
        .into_iter()
        .fold(0.0, f32::max);
    sp.delay + sp.duration + span
}
