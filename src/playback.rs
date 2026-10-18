//! Playback engine: turns the element's animation settings into per-layer transforms/text effects
//! at a given moment, following the structure of the compiled scene JS (CGMotion.mount + layers).
use crate::anim::{self, Timing, Transform};
use crate::compile;
use crate::model::*;
use crate::render::{LayerFx, TextFx, Values};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase { Idle, In, Hold, Out }

pub struct Playback {
    pub phase: Phase,
    pub t: f32,
    pub playing: bool,
    pub auto_cycle: bool,
    pub hold_secs: f32,
    pub loop_t: f32,
}

impl Default for Playback {
    fn default() -> Self { Self { phase: Phase::Idle, t: 0.0, playing: false, auto_cycle: false, hold_secs: 1.5, loop_t: 0.0 } }
}

impl Playback {
    pub fn play_in(&mut self) { self.phase = Phase::In; self.t = 0.0; self.playing = true; self.loop_t = 0.0; }
    pub fn play_out(&mut self) { self.phase = Phase::Out; self.t = 0.0; self.playing = true; }
    pub fn stop(&mut self) { self.phase = Phase::Idle; self.t = 0.0; self.playing = false; self.loop_t = 0.0; }
    pub fn cycle(&mut self) { self.auto_cycle = true; self.play_in(); }

    pub fn active(&self) -> bool { self.phase != Phase::Idle }

    pub fn advance(&mut self, dt: f32, plan: &Plan) {
        if !self.playing { return; }
        self.t += dt;
        match self.phase {
            Phase::In => if self.t >= plan.in_total {
                self.phase = Phase::Hold; self.t = 0.0; self.loop_t = 0.0;
                if !self.auto_cycle { /* stay on air until OUT */ }
            },
            Phase::Hold => {
                self.loop_t += dt;
                if self.auto_cycle && self.t >= self.hold_secs { self.phase = Phase::Out; self.t = 0.0; }
            }
            Phase::Out => if self.t >= plan.out_total { self.stop(); self.auto_cycle = false; },
            Phase::Idle => {}
        }
    }
}

pub struct Plan {
    pub in_total: f32,
    pub out_total: f32,
    pub base_end: f32,
    pub pre_dur: f32,
    pub base_out_start: f32,
}

/// Liczby jednostek tekstu dla silnika (jak w przeglądarce: `lines-*` animuje słowa, a limit staggera
/// liczy linie).
pub fn units_for(effect: &str, text: &str) -> btb_motion::Units {
    use btb_motion::text::{split_of, Split};
    let words = text.split_whitespace().count().max(1);
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count().max(1);
    let n = match split_of(effect) {
        Split::Chars => text.chars().filter(|c| !c.is_whitespace()).count().max(1),
        Split::Words | Split::Lines => words,
        Split::Whole => 1,
    };
    let by = match split_of(effect) { Split::Chars => "chars", Split::Words | Split::Lines => "words", Split::Whole => "" };
    btb_motion::Units { n, by: by.into(), lines, plain: true, num: text.chars().any(|c| c.is_ascii_digit()) }
}

fn is_active(t: &TextEffect) -> bool { !t.effect_type.is_empty() && t.effect_type != "none" }

/// `eff = own || global` z cgmotion: efekt własny warstwy wygrywa także jako "none" (wyłącza efekt
/// elementu). Zwraca efekt i informację, czy jest własny.
pub fn resolve_text<'a>(own: &'a Option<TextEffect>, global: &'a Option<TextEffect>) -> Option<(&'a TextEffect, bool)> {
    match own {
        Some(o) => is_active(o).then_some((o, true)),
        None => global.as_ref().filter(|g| is_active(g)).map(|g| (g, false)),
    }
}

/// Czas efektu. `in_by`: na co IN podzielił tekst; efekt dzielący w innym trybie animuje cały element.
pub fn text_total(te: &TextEffect, text: &str, in_by: Option<&str>) -> f32 {
    let mut u = units_for(te.name(), text);
    if let Some(b) = in_by { if !u.by.is_empty() && u.by != b { u.by = String::new(); } }
    btb_motion::text::total(&te.spec(), &u)
}

/// Tryb podziału wybrany przez efekt IN warstwy (kolejne efekty muszą mieć ten sam, inaczej animują całość).
fn in_split_by(l: &Layer, a: &ElementAnimation, text: &str) -> Option<&'static str> {
    resolve_text(&l.motion.text, &a.text).map(|(te, _)| units_for(te.name(), text).by).filter(|b| !b.is_empty()).map(|b| if b == "chars" { "chars" } else { "words" })
}

pub fn layer_text(l: &Layer, values: &Values) -> String {
    match l.layer_type {
        LayerType::Text => values.text_for(l),
        LayerType::Clock => "00:00:00".into(),
        _ => String::new(),
    }
}

fn is_text_layer(l: &Layer) -> bool { matches!(l.layer_type, LayerType::Text | LayerType::Clock) }

pub fn plan(el: &Element, scene: &Scene, values: &Values) -> Plan {
    let a = &el.animation;
    let elem_in = Timing::from(&a.anim_in);
    let base_end = if a.anim_in.anim_type == "none" { 0.0 } else { anim::total(&elem_in) };
    let mut in_total = base_end;
    let mut pre_dur = 0.0f32;
    let mut par_dur = 0.0f32;
    let ts = a.extra.get("textSync").and_then(|v| v.as_bool()).unwrap_or(false);
    let tos = a.extra.get("textOutSync").and_then(|v| v.as_bool()).unwrap_or(false);

    for l in scene.layers.iter().filter(|l| l.visible) {
        let (m_in, m_out) = compile::group_motion_pair(scene, l);
        let m_in = l.motion.motion_in.clone().or(m_in);
        let m_out = l.motion.out.clone().or(m_out);
        if let Some(m) = &m_in { if m.anim_type != "none" { in_total = in_total.max(m.delay + m.duration); } }
        if let Some(m) = &m_out { if m.anim_type != "none" { pre_dur = pre_dur.max(m.delay + m.duration); } }
        if is_text_layer(l) {
            let txt = layer_text(l, values);
            if let Some((te, own)) = resolve_text(&l.motion.text, &a.text) {
                let start = if own { 0.0 } else if ts { (base_end - 0.1).max(0.0) } else { 0.0 };
                in_total = in_total.max(start + text_total(te, &txt, None));
            }
            match resolve_text(&l.motion.text_out, &a.text_out) {
                Some((te, true)) => pre_dur = pre_dur.max(text_total(te, &txt, in_split_by(l, a, &txt))),
                Some((te, false)) => par_dur = par_dur.max(text_total(te, &txt, in_split_by(l, a, &txt))),
                None => {}
            }
        }
    }
    // pozycja Stop ustawiona ręcznie na osi czasu (klucze użytkownika mogą wymagać dłuższego wejścia)
    if let Some(stop) = a.extra.get("stopAt").and_then(|v| v.as_f64()) { in_total = in_total.max(stop as f32); }
    // Local Stop (M2.6): pośrednie zatrzymania leżą w oknie wejścia, które musi je obejmować
    if let Some(last) = a.local_stops().last() { in_total = in_total.max(*last + 0.05); }
    // klucze po Stop należą do wyjścia: oś kończy się na ostatnim z nich
    let key_end = scene.layers.iter().filter(|l| l.visible).flat_map(|l| l.keys.iter().filter_map(|c| c.keys.last().map(|k| k.t)).chain(l.path.iter().filter_map(|p| p.keys.last().map(|k| k.t)))).chain(scene.keys.iter().filter_map(|c| c.keys.last().map(|k| k.t))).fold(0.0f32, f32::max);
    let elem_out = Timing::from(&a.out);
    let elem_out_total = if a.out.anim_type == "none" { 0.0 } else { anim::total(&elem_out) };
    let base_out_start = if tos { (pre_dur + par_dur - 0.1).max(pre_dur) } else { pre_dur };
    let out_total = (base_out_start + elem_out_total).max(pre_dur + par_dur).max(key_end - in_total.max(0.05)).max(0.05);
    Plan { in_total: in_total.max(0.05), out_total, base_end, pre_dur, base_out_start }
}

fn combine(a: &mut Transform, b: &Transform) {
    a.tx += b.tx; a.ty += b.ty;
    a.sx *= b.sx; a.sy *= b.sy;
    a.rot += b.rot;
    a.opacity *= b.opacity;
}

/// Computes (element-level transform, per-layer effects) for the current playback state.
pub fn evaluate(pb: &Playback, el: &Element, scene: &Scene, values: &Values, plan: &Plan) -> (Transform, HashMap<String, LayerFx>) {
    let mut fx: HashMap<String, LayerFx> = HashMap::new();
    let mut elem_tr = Transform::default();
    if !pb.active() { return (elem_tr, fx); }
    let a = &el.animation;
    let ts = a.extra.get("textSync").and_then(|v| v.as_bool()).unwrap_or(false);

    match pb.phase {
        Phase::In => {
            let tm = Timing::from(&a.anim_in);
            if a.anim_in.anim_type != "none" { elem_tr = anim::eval_in(&tm, pb.t); }
        }
        Phase::Hold => {
            if let Some(lp) = &a.loop_spec {
                if !lp.loop_type.is_empty() && lp.loop_type != "none" {
                    let lt = anim::loop_transform(&lp.loop_type, pb.loop_t, lp.duration);
                    combine(&mut elem_tr, &lt);
                }
            }
        }
        Phase::Out => {
            let tm = Timing::from(&a.out);
            // OUT elementu startuje dopiero po `pre` (OUT warstw i ich tekstów); wcześniej nie ma wpływu
            if pb.t >= plan.base_out_start { elem_tr = anim::eval_out(&tm, pb.t - plan.base_out_start); }
        }
        Phase::Idle => {}
    }
    anim::element_origin(&mut elem_tr);

    for l in scene.layers.iter().filter(|l| l.visible) {
        let (g_in, g_out) = compile::group_motion_pair(scene, l);
        let m_in = l.motion.motion_in.clone().or(g_in);
        let m_out = l.motion.out.clone().or(g_out);
        let mut lf = LayerFx::default();
        let txt = if is_text_layer(l) { layer_text(l, values) } else { String::new() };
        match pb.phase {
            Phase::In => {
                if let Some(m) = m_in.as_ref().filter(|m| m.anim_type != "none") {
                    lf.tr = Some(anim::eval_in(&Timing::from(m), pb.t));
                }
                if is_text_layer(l) {
                    if let Some((te, own)) = resolve_text(&l.motion.text, &a.text) {
                        let start = if own { 0.0 } else if ts { (plan.base_end - 0.1).max(0.0) } else { 0.0 };
                        lf.text = Some(TextFx {
                            effect: te.name().to_string(), duration: te.duration, stagger: te.stagger.unwrap_or(0.0), from: te.stagger_from().to_string(),
                            ease: "power2.out".into(), t: pb.t - start - te.delay, out: false,
                        });
                    }
                }
            }
            Phase::Out => {
                if let Some(m) = m_out.as_ref().filter(|m| m.anim_type != "none") {
                    lf.tr = Some(anim::eval_out(&Timing::from(m), pb.t));
                }
                if is_text_layer(l) {
                    if let Some((te, own)) = resolve_text(&l.motion.text_out, &a.text_out) {
                        let start = if own { 0.0 } else { plan.pre_dur };
                        lf.text = Some(TextFx { effect: te.name().to_string(), duration: te.duration, stagger: te.stagger.unwrap_or(0.0), from: te.stagger_from().to_string(), ease: "power2.in".into(), t: pb.t - start - te.delay, out: true });
                    }
                }
            }
            _ => {}
        }
        let _ = &txt;
        if lf.tr.is_some() || lf.text.is_some() { fx.insert(l.id.clone(), lf); }
    }
    (elem_tr, fx)
}
