//! Konwersja presetów IN / OUT (element i warstwy) na klucze użytkownika, żeby dało się je edytować na
//! osi czasu i w edytorze krzywych. Krzywe presetów są dopasowywane kilkoma kluczami z uchwytami Béziera
//! (`Channel::fit`), a nie próbkowane gęsto, więc `power2.out` to dwa klucze, a elastic około dziesięciu.
//!
//! Po konwersji preset jest wyłączony (`none`), a pozycja Stop zostaje taka jak była (`animation.stopAt`),
//! więc wygląd i czas animacji się nie zmieniają. Pętle uwagi i efekty tekstowe zostają presetami.
use crate::anim::{self, Timing, Transform};
use crate::compile;
use crate::model::*;
use crate::motion_bridge::{active, elem, get, OUT_EPS, PROPS};
use crate::playback;
use crate::render::Values;
use btb_motion::{Channel, Handle, Key};

/// Tolerancja dopasowania względem zakresu wartości właściwości.
pub const FIT_EPS: f32 = 0.0015;

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    All,
    Element,
    Layer(String),
}

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// ile kanałów kluczy powstało
    pub channels: usize,
    /// ile kluczy w sumie
    pub keys: usize,
    /// warstwy, którym wyłączono preset
    pub layers: usize,
    /// czy skonwertowano animację całego elementu
    pub element: bool,
    /// właściwości pominięte, bo warstwa ma już własny kanał o tej nazwie
    pub skipped: Vec<String>,
}

/// Bezwzględny limit błędu per właściwość (piksele, stopnie, ułamki): przy dużych zakresach (1920 px)
/// sama tolerancja względna dałaby kilka pikseli.
fn abs_cap(prop: &str) -> f32 {
    match prop {
        "tx" | "ty" => 0.6,
        "rot" | "skew" | "blur" => 0.15,
        _ => 0.004,
    }
}

type Segment = (Channel, f32);

/// Kanał jednej właściwości dla presetu na `[0, total]`; `None`, gdy właściwość się nie zmienia (stała = domyślna).
fn fit_prop(i: usize, total: f32, f: &dyn Fn(f32) -> Transform) -> Option<Channel> {
    let name = PROPS[i];
    let def = get(&Transform::default(), i);
    let v = |t: f32| get(&f(t), i);
    // zakres z gęstego próbkowania; stała równa domyślnej = brak kanału
    let n = 60;
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for k in 0..=n {
        let x = v(total * k as f32 / n as f32);
        lo = lo.min(x);
        hi = hi.max(x);
    }
    if (hi - lo) < 1e-6 && (lo - def).abs() < 1e-6 {
        return None;
    }
    let range = (hi - lo).max(1e-6);
    let eps = FIT_EPS.min(abs_cap(name) / range);
    Some(Channel::fit(name, 0.0, total, eps, v))
}

/// Sklej segmenty (IN, OUT) jednej właściwości w jeden kanał: stan między segmentami trzymany `Hold`em,
/// żeby przeskok stanu początkowego OUT nie był widoczny przed jego startem.
fn merge(prop: &str, def: f32, segs: Vec<Segment>) -> Option<Channel> {
    let mut out = Channel::new(prop);
    for (ch, offset) in segs {
        let mut keys: Vec<Key> = ch.keys;
        for k in &mut keys {
            k.t += offset;
        }
        let Some(first) = keys.first().cloned() else { continue };
        // pokrywające się klucze (koniec IN = początek OUT, ta sama wartość): jeden klucz z wejściem z IN i wyjściem z OUT
        if let Some(last) = out.keys.last_mut() {
            if (last.t - first.t).abs() < 1e-4 && (last.v - first.v).abs() < 1e-4 {
                last.right = first.right;
                out.keys.extend(keys.into_iter().skip(1));
                continue;
            }
        }
        match out.keys.last_mut() {
            Some(last) if (last.v - first.v).abs() > 1e-4 => last.right = Handle::Hold,
            None if first.t > 1e-4 && (first.v - def).abs() > 1e-4 => {
                // kanał zaczyna się dopiero w OUT: przed nim obowiązuje wartość domyślna
                let mut anchor = Key::linear(0.0, def);
                anchor.right = Handle::Hold;
                out.keys.push(anchor);
            }
            _ => {}
        }
        out.keys.extend(keys);
    }
    if out.keys.iter().all(|k| (k.v - def).abs() < 1e-6) { return None; }
    (!out.keys.is_empty()).then_some(out)
}

/// Kanały z presetów IN i OUT jednego celu (`in_spec` / `out_spec` to `Timing` z offsetem startu OUT).
fn channels_for(in_tm: Option<(&Timing, bool)>, out_tm: Option<(&Timing, f32, bool)>) -> Vec<Channel> {
    let mut by_prop: Vec<Vec<Segment>> = vec![vec![]; PROPS.len()];
    let wrap = |is_elem: bool, t: Transform| if is_elem { elem(t) } else { t };
    if let Some((tm, is_elem)) = in_tm {
        let total = anim::total(tm);
        let f = move |t: f32| wrap(is_elem, anim::eval_in(tm, t));
        for (i, segs) in by_prop.iter_mut().enumerate() {
            if let Some(ch) = fit_prop(i, total, &f) { segs.push((ch, 0.0)); }
        }
    }
    if let Some((tm, offset, is_elem)) = out_tm {
        let total = anim::total(tm);
        let f = move |t: f32| wrap(is_elem, anim::eval_out(tm, t));
        for (i, segs) in by_prop.iter_mut().enumerate() {
            if let Some(ch) = fit_prop(i, total, &f) { segs.push((ch, offset)); }
        }
    }
    let chans: Vec<Channel> = by_prop
        .into_iter()
        .enumerate()
        .filter_map(|(i, segs)| merge(PROPS[i], get(&Transform::default(), i), segs))
        .collect();
    // punkt odniesienia wpływa na wygląd tylko wraz ze skalą, obrotem, skosem albo odwróceniem
    let needs_origin = chans.iter().any(|c| matches!(c.prop.as_str(), "sx" | "sy" | "rot" | "skew" | "fx" | "fy"));
    chans.into_iter().filter(|c| needs_origin || !matches!(c.prop.as_str(), "ox" | "oy")).collect()
}

fn none_spec(prev: Option<&MotionSpec>) -> MotionSpec {
    let mut m = prev.cloned().unwrap_or_default();
    m.anim_type = "none".into();
    m
}

/// Zamienia presety IN/OUT na klucze wg `scope`. Zwraca raport; element bez presetów w zakresie zostaje bez zmian.
pub fn convert_presets(el: &mut Element, scope: Scope) -> Report {
    let mut report = Report::default();
    let Some(scene) = el.scene.clone() else { return report };
    let (title, fields, extra) = (el.title.clone(), el.fields.clone(), el.extra.clone());
    let values = Values { title: &title, fields: &fields, extra: &extra, base_url: "" };
    let plan = playback::plan(el, &scene, &values);
    let t_in = plan.in_total;
    let a = el.animation.clone();

    // ── element ──
    if matches!(scope, Scope::All | Scope::Element) {
        let in_tm = active(&a.anim_in.anim_type).then(|| Timing::from(&a.anim_in));
        // OUT "none" też jest animacją (opacity 0 od razu), więc wchodzi do konwersji; element już z kluczami
        // ma wyjście w kluczach, a "none" go wtedy nie chowa (patrz motion_bridge::build)
        let out_tm = (!a.out.anim_type.is_empty() && !(a.out.anim_type == "none" && !scene.keys.is_empty())).then(|| Timing::from(&a.out));
        let chans = channels_for(in_tm.as_ref().map(|t| (t, true)), out_tm.as_ref().map(|t| (t, t_in + plan.base_out_start + OUT_EPS, true)));
        if !chans.is_empty() {
            let Some(sc) = el.scene.as_mut() else { return report };
            for ch in chans {
                if sc.keys.iter().any(|c| c.prop == ch.prop) { report.skipped.push(format!("element: {}", ch.prop)); continue; }
                report.keys += ch.keys.len();
                report.channels += 1;
                sc.keys.push(ch);
            }
            report.element = report.channels > 0;
            if report.element {
                // textSync liczył start tekstu od końca IN elementu: po usunięciu presetu dodajemy to do opóźnienia
                let ts = a.extra.get("textSync").and_then(|v| v.as_bool()).unwrap_or(false);
                if ts && plan.base_end > 0.0 {
                    if let Some(te) = el.animation.text.as_mut() { te.delay += (plan.base_end - 0.1).max(0.0); }
                    el.animation.extra.insert("textSync".into(), serde_json::Value::Bool(false));
                }
                el.animation.anim_in.anim_type = "none".into();
                el.animation.out.anim_type = "none".into();
            }
        }
    }

    // ── warstwy ──
    let ids: Vec<String> = scene
        .layers
        .iter()
        .filter(|l| l.visible)
        .filter(|l| matches!(&scope, Scope::All) || matches!(&scope, Scope::Layer(id) if *id == l.id))
        .map(|l| l.id.clone())
        .collect();
    for id in ids {
        let Some(l0) = scene.layers.iter().find(|l| l.id == id) else { continue };
        let (g_in, g_out) = compile::group_motion_pair(&scene, l0);
        let m_in = l0.motion.motion_in.clone().or(g_in).filter(|m| active(&m.anim_type));
        let m_out = l0.motion.out.clone().or(g_out).filter(|m| active(&m.anim_type));
        if m_in.is_none() && m_out.is_none() { continue; }
        let in_tm = m_in.as_ref().map(Timing::from);
        let out_tm = m_out.as_ref().map(Timing::from);
        let chans = channels_for(in_tm.as_ref().map(|t| (t, false)), out_tm.as_ref().map(|t| (t, t_in + OUT_EPS, false)));
        let Some(sc) = el.scene.as_mut() else { return report };
        let Some(layer) = sc.layers.iter_mut().find(|l| l.id == id) else { continue };
        let mut converted = false;
        for ch in chans {
            if layer.keys.iter().any(|c| c.prop == ch.prop) { report.skipped.push(format!("{}: {}", layer.name, ch.prop)); continue; }
            report.keys += ch.keys.len();
            report.channels += 1;
            layer.keys.push(ch);
            converted = true;
        }
        if converted {
            // jawne "none" blokuje też ruch dziedziczony z grupy
            layer.motion.motion_in = Some(none_spec(m_in.as_ref().or(layer.motion.motion_in.as_ref())));
            layer.motion.out = Some(none_spec(m_out.as_ref().or(layer.motion.out.as_ref())));
            report.layers += 1;
        }
    }

    // OUT elementu startuje po OUT warstw (pre_dur): po usunięciu ich presetów zaczynałby wcześniej, więc
    // opóźniamy go o różnicę (gdy sam element nie został skonwertowany)
    if report.layers > 0 && !report.element && !el.animation.out.anim_type.is_empty() {
        let (title, fields, extra) = (el.title.clone(), el.fields.clone(), el.extra.clone());
        let values = Values { title: &title, fields: &fields, extra: &extra, base_url: "" };
        let Some(scene2) = el.scene.clone() else { return report };
        let plan2 = playback::plan(el, &scene2, &values);
        let shift = plan.base_out_start - plan2.base_out_start;
        if shift > 1e-4 { el.animation.out.delay += shift; }
    }
    if report.channels > 0 {
        // Stop zostaje tam, gdzie był (bez presetów plan skróciłby wejście)
        let old = el.animation.extra.get("stopAt").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.0);
        el.animation.extra.insert("stopAt".into(), serde_json::Value::from(old.max(t_in)));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{ANIM_TYPES, DIRECTIONAL};
    use crate::motion_bridge::{build, sample_phase, ELEMENT};
    use crate::playback::Phase;
    use std::collections::HashMap;

    fn tol(prop: &str) -> f32 {
        match prop {
            "tx" | "ty" => 2.0,
            "rot" | "skew" | "blur" => 0.3,
            _ => 0.012,
        }
    }

    fn setup(el: &Element) -> (Scene, crate::playback::Plan) {
        let scene = el.scene.clone().unwrap();
        let base = String::new();
        let vals = Values { title: &el.title, fields: &el.fields, extra: &el.extra, base_url: &base };
        let plan = playback::plan(el, &scene, &vals);
        (scene, plan)
    }

    /// Transformacje wszystkich celów w chwilach z okna +-2/240 s (położenie skoków jest dokładne z dokładnością
    /// do kroku próbkowania).
    fn timeline(d: &btb_motion::Director, plan: &crate::playback::Plan, phase: Phase, t: f32) -> Vec<HashMap<String, Transform>> {
        (-2..=2).map(|k| sample_phase(d, plan, phase, (t + k as f32 / 240.0).max(0.0), 0.0)).collect()
    }

    fn assert_equivalent(before: &Element, after: &Element, label: &str) {
        let (scene, plan) = setup(before);
        let (scene2, plan2) = setup(after);
        assert!((plan.in_total - plan2.in_total).abs() < 1e-4, "{label}: Stop {} -> {}", plan.in_total, plan2.in_total);
        let (d_before, d_after) = (build(before, &scene, &plan), build(after, &scene2, &plan2));
        for (phase, span) in [(Phase::In, plan.in_total * 0.999), (Phase::Out, plan.out_total + 0.1)] {
            for s in 0..=24 {
                let t = span * s as f32 / 24.0;
                let want = timeline(&d_before, &plan, phase, t);
                let got = timeline(&d_after, &plan2, phase, t);
                let center = &got[2];
                for (id, tr) in center {
                    // punkt odniesienia nie ma wpływu na wygląd, gdy nie ma skali, obrotu, skosu ani odwrócenia
                    let neutral = |t: &Transform| (t.sx - 1.0).abs() < 1e-3 && (t.sy - 1.0).abs() < 1e-3 && t.rot.abs() < 1e-3 && t.skew.abs() < 1e-3 && (t.flip_x - 1.0).abs() < 1e-3 && (t.flip_y - 1.0).abs() < 1e-3;
                    for (i, p) in PROPS.iter().enumerate() {
                        if (*p == "ox" || *p == "oy") && neutral(tr) && want.iter().all(|m| m.get(id).map_or(true, neutral)) { continue; }
                        let vals: Vec<f32> = want.iter().map(|m| m.get(id).map_or(get(&Transform::default(), i), |x| get(x, i))).collect();
                        let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                        let g = get(tr, i);
                        assert!(g >= lo - tol(p) && g <= hi + tol(p), "{label} {phase:?} t={t:.3} {id}.{p}: {g} poza [{lo}, {hi}]");
                    }
                }
                // cele obecne tylko przed konwersją muszą mieć wartości domyślne po niej
                for id in want[2].keys().filter(|k| !center.contains_key(*k)) {
                    for (i, p) in PROPS.iter().enumerate() {
                        let w = get(&want[2][id], i);
                        assert!((w - get(&Transform::default(), i)).abs() <= tol(p) * 3.0, "{label} {phase:?} t={t:.3} {id}.{p}: brak celu po konwersji, a było {w}");
                    }
                }
            }
        }
    }

    fn base_element() -> Element {
        let mut el = Element::new_typed("t", "BADGE");
        el.animation.loop_spec = None;
        el.animation.text = None;
        el.animation.text_out = None;
        if let Some(s) = el.scene.as_mut() { for l in s.layers.iter_mut() { l.motion = Motion::default(); } }
        el
    }

    #[test]
    fn element_presets_convert_to_equivalent_keys_for_every_type_and_ease() {
        let mut checked = 0;
        for ty in ANIM_TYPES.iter().filter(|t| **t != "none") {
            let dirs: &[&str] = if DIRECTIONAL.contains(ty) { &["left", "bottom"] } else { &["left"] };
            for dir in dirs {
                for ease in ["power2.out", "back.out", "elastic-out"] {
                    let mut el = base_element();
                    el.animation.anim_in = AnimSpec { anim_type: (*ty).into(), direction: Some((*dir).into()), duration: 0.6, delay: 0.05, ease: ease.into(), ..Default::default() };
                    el.animation.out = AnimSpec { anim_type: (*ty).into(), direction: Some((*dir).into()), duration: 0.5, ease: "power2.in".into(), ..Default::default() };
                    let mut after = el.clone();
                    let rep = convert_presets(&mut after, Scope::Element);
                    assert!(rep.element && rep.channels > 0, "{ty}/{dir}/{ease}: nic nie skonwertowano");
                    assert_eq!(after.animation.anim_in.anim_type, "none");
                    assert_equivalent(&el, &after, &format!("{ty}/{dir}/{ease}"));
                    checked += 1;
                }
            }
        }
        assert!(checked > 80);
    }

    #[test]
    fn layer_presets_convert_and_group_motion_is_blocked() {
        let mut el = base_element();
        el.animation.anim_in = AnimSpec { anim_type: "none".into(), ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "none".into(), ..Default::default() };
        {
            let s = el.scene.as_mut().unwrap();
            s.layers[0].motion.motion_in = Some(MotionSpec::new("slide-overshoot", Some("left"), 0.6, 0.1, "power2.out"));
            s.layers[0].motion.out = Some(MotionSpec::new("glitch", Some("right"), 0.5, 0.0, "linear"));
            // druga warstwa dziedziczy ruch z grupy
            let gid = "g1".to_string();
            s.layers[1].group = Some(gid.clone());
            s.extra.insert("groups".into(), serde_json::json!({ "g1": { "motion": { "in": { "type": "pop", "direction": "left", "duration": 0.4, "delay": 0.0, "ease": "back.out" }, "out": { "type": "fade", "direction": "left", "duration": 0.3, "delay": 0.0, "ease": "linear" } } } }));
        }
        let mut after = el.clone();
        let rep = convert_presets(&mut after, Scope::All);
        assert_eq!(rep.layers, 2, "{rep:?}");
        let l1 = &after.scene.as_ref().unwrap().layers[1];
        assert_eq!(l1.motion.motion_in.as_ref().unwrap().anim_type, "none", "jawne none blokuje ruch z grupy");
        assert!(!l1.keys.is_empty());
        assert_equivalent(&el, &after, "warstwy + grupa");
        // nie konwertowana warstwa zostaje bez zmian
        let mut only = el.clone();
        let id = only.scene.as_ref().unwrap().layers[0].id.clone();
        let rep = convert_presets(&mut only, Scope::Layer(id));
        assert_eq!(rep.layers, 1);
        assert!(only.scene.as_ref().unwrap().layers[1].keys.is_empty());
        assert_equivalent(&el, &only, "jedna warstwa");
    }

    #[test]
    fn stop_position_and_total_length_are_preserved() {
        let mut el = base_element();
        el.animation.anim_in = AnimSpec { anim_type: "slide".into(), duration: 0.8, ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "fade".into(), duration: 0.4, ..Default::default() };
        let (_, before) = setup(&el);
        let mut after = el.clone();
        convert_presets(&mut after, Scope::All);
        let (_, p) = setup(&after);
        assert!((p.in_total - before.in_total).abs() < 1e-4, "Stop zachowany: {} vs {}", p.in_total, before.in_total);
        assert!(el.animation.extra.get("stopAt").is_none() && after.animation.extra.get("stopAt").is_some());
        assert!((p.out_total - before.out_total).abs() < 0.02, "długość wyjścia: {} vs {}", p.out_total, before.out_total);
    }

    #[test]
    fn element_out_none_stays_hidden_after_start_and_second_conversion_is_a_noop() {
        let mut el = base_element();
        el.animation.anim_in = AnimSpec { anim_type: "fade".into(), duration: 0.4, ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "none".into(), ..Default::default() };
        let mut after = el.clone();
        let rep = convert_presets(&mut after, Scope::Element);
        assert!(rep.element);
        assert_equivalent(&el, &after, "OUT none");
        // po wyjściu element jest niewidoczny, a w hold widoczny
        let (scene, plan) = setup(&after);
        let d = build(&after, &scene, &plan);
        let hold = sample_phase(&d, &plan, Phase::Hold, 0.0, 0.0);
        assert!(hold.get(ELEMENT).map_or(1.0, |t| t.opacity) > 0.99, "hold widoczny: {:?}", hold.get(ELEMENT));
        let out = sample_phase(&d, &plan, Phase::Out, 0.3, 0.0);
        assert!(out[ELEMENT].opacity < 0.01);
        let again = convert_presets(&mut after, Scope::All);
        assert_eq!(again.channels, 0, "druga konwersja nic nie dodaje: {again:?}");
    }

    #[test]
    fn existing_user_channels_are_not_overwritten_and_text_effects_stay() {
        let mut el = base_element();
        el.animation.anim_in = AnimSpec { anim_type: "slide".into(), duration: 0.6, ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "slide".into(), duration: 0.5, ..Default::default() };
        el.animation.text = Some(TextEffect::new("words-up", 0.5, 0.1));
        el.animation.loop_spec = Some(LoopSpec { loop_type: "pulse".into(), duration: None, ..Default::default() });
        let mut mine = Channel::new("tx");
        mine.keys = vec![Key::linear(0.0, 0.0), Key::linear(1.0, 5.0)];
        el.scene.as_mut().unwrap().keys.push(mine.clone());
        let rep = convert_presets(&mut el, Scope::Element);
        assert!(rep.skipped.iter().any(|s| s.contains("tx")), "{rep:?}");
        assert_eq!(el.scene.as_ref().unwrap().keys.iter().find(|c| c.prop == "tx").unwrap().keys.len(), 2, "kanał użytkownika bez zmian");
        assert!(el.animation.text.is_some() && el.animation.loop_spec.is_some(), "tekst i pętla zostają presetami");
    }

    #[test]
    fn nothing_to_convert_changes_nothing() {
        let mut el = base_element();
        el.animation.anim_in = AnimSpec { anim_type: "none".into(), ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "none".into(), ..Default::default() };
        let rep = convert_presets(&mut el, Scope::Layer("x".into()));
        assert_eq!(rep, Report::default());
        assert!(el.scene.as_ref().unwrap().keys.is_empty());
        assert!(el.animation.extra.get("stopAt").is_none());
    }
}
