//! Most między modelem aplikacji a silnikiem `btb-motion`: zamienia animacje elementu (IN / OUT /
//! pętla) i warstw na `Director` i odczytuje z niego transformacje.
//!
//! Funkcje z `anim.rs` są wypiekane do kluczy (`Channel::bake`), więc silnik dostaje dane, a nie kod.
//! Efekty tekstowe (stagger per znak/słowo) zostają w `playback::evaluate` (patrz docs/PLAN-btb-motion.md).
use crate::anim::{self, Timing, Transform};
use crate::compile;
use crate::model::*;
use crate::playback::{self, Phase, Playback, Plan};
use crate::render::{LayerFx, Values};
use btb_motion::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// Dopuszczalny błąd wypieku względem zakresu wartości właściwości (0,04%: ~0,8 px na 1920 px).
pub const BAKE_EPS: f32 = 0.0004;
/// Okna aktorów i śladów OUT zaczynają się tuż po `t_in`: w stanie Stop transport stoi dokładnie w `t_in`,
/// a stan początkowy OUT (np. glitch, `none`) nie może być wtedy widoczny.
pub const OUT_EPS: f32 = 1e-4;
/// Właściwości, które użytkownik może animować kluczami: (nazwa kanału, etykieta, wartość neutralna).
pub const KEY_PROPS: [(&str, &str, f32); 6] = [
    ("tx", "Pozycja X", 0.0),
    ("ty", "Pozycja Y", 0.0),
    ("sx", "Skala X", 1.0),
    ("sy", "Skala Y", 1.0),
    ("rot", "Obrót", 0.0),
    ("opacity", "Krycie", 1.0),
];
/// Cel Actora opisującego cały element (reszta to id warstw).
pub const ELEMENT: &str = "element";

pub(crate) const PROPS: [&str; 20] = [
    "tx", "ty", "sx", "sy", "rot", "skew", "opacity", "ox", "oy", "cl", "ct", "cr", "cb", "blur", "fx", "fy", "sh", "dg", "dr", "rgb",
];

pub(crate) fn get(t: &Transform, i: usize) -> f32 {
    match i {
        0 => t.tx,
        1 => t.ty,
        2 => t.sx,
        3 => t.sy,
        4 => t.rot,
        5 => t.skew,
        6 => t.opacity,
        7 => t.origin.0,
        8 => t.origin.1,
        9 => t.clip[0],
        10 => t.clip[1],
        11 => t.clip[2],
        12 => t.clip[3],
        13 => t.blur,
        14 => t.flip_x,
        15 => t.flip_y,
        16 => t.shutter,
        17 => t.diag,
        18 => t.diag_r,
        _ => t.rgb,
    }
}

fn set(t: &mut Transform, prop: &str, v: f32) {
    match prop {
        "tx" => t.tx = v,
        "ty" => t.ty = v,
        "sx" => t.sx = v,
        "sy" => t.sy = v,
        "rot" => t.rot = v,
        "skew" => t.skew = v,
        "opacity" => t.opacity = v,
        "ox" => t.origin.0 = v,
        "oy" => t.origin.1 = v,
        "cl" => t.clip[0] = v,
        "ct" => t.clip[1] = v,
        "cr" => t.clip[2] = v,
        "cb" => t.clip[3] = v,
        "blur" => t.blur = v,
        "fx" => t.flip_x = v,
        "fy" => t.flip_y = v,
        "sh" => t.shutter = v,
        "dg" => t.diag = v,
        "dr" => t.diag_r = v,
        "rgb" => t.rgb = v,
        _ => {}
    }
}

/// Wypieka funkcję `f(t) -> Transform` na `[0, total]` do kanałów (pomija właściwości stałe = domyślne).
fn bake_transform(total: f32, f: impl Fn(f32) -> Transform) -> Vec<Channel> {
    let def = Transform::default();
    PROPS
        .iter()
        .enumerate()
        .map(|(i, name)| Channel::bake(name, 0.0, total, BAKE_EPS, |t| get(&f(t), i)))
        .zip(0..)
        .filter(|(ch, i)| ch.keys.iter().any(|k| (k.v - get(&def, *i)).abs() > 1e-6))
        .map(|(ch, _)| ch)
        .collect()
}

fn actor(target: &str, offset: f32, from: Option<f32>, until: Option<f32>, channels: Vec<Channel>) -> Option<Actor> {
    (!channels.is_empty()).then(|| Actor { target: target.into(), offset, active_from: from, active_until: until, channels, paths: vec![] })
}

/// Transformacja elementu (nie warstwy): domyślny punkt odniesienia to lewy górny róg sceny.
pub(crate) fn elem(mut t: Transform) -> Transform {
    anim::element_origin(&mut t);
    t
}

pub(crate) fn active(anim_type: &str) -> bool { !anim_type.is_empty() && anim_type != "none" }

/// Okres pętli uwagi (sekundy); `None` dla efektów bez ruchu transformacji (glow, shine…).
fn loop_period(kind: &str, dur: Option<f32>) -> Option<f32> {
    Some(match kind {
        "pulse" => 2.0 * dur.unwrap_or(0.8),
        "float" => 2.0 * dur.unwrap_or(2.2),
        "wobble" => 2.0 * dur.unwrap_or(0.9),
        "breathe" => 2.0 * dur.unwrap_or(3.0),
        "flash" => dur.unwrap_or(0.5) * 3.0 + 2.0,
        "shake" => dur.unwrap_or(0.08) * 7.0 + 3.0,
        "heartbeat" => 0.64 + 1.2,
        "bounce" => 0.7 + 2.0,
        _ => return None,
    })
}

/// Buduje Director elementu: `[0, in_total)` IN, Stop "hold" w `in_total`, potem OUT.
/// Pętla uwagi to osobny Director swobodny (działa tylko w stanie Stop).
pub fn build(el: &Element, scene: &Scene, plan: &Plan) -> Director {
    let a = &el.animation;
    let t_in = plan.in_total;
    let mut d = Director::new("Default", t_in + plan.out_total);
    d.events.push(Event { name: "hold".into(), t: t_in, kind: EventKind::Stop, dir: Dir::Forward });

    if active(&a.anim_in.anim_type) {
        let tm = Timing::from(&a.anim_in);
        let total = anim::total(&tm);
        d.actors.extend(actor(ELEMENT, 0.0, Some(0.0), Some(t_in), bake_transform(total, |t| elem(anim::eval_in(&tm, t)))));
    }
    // OUT elementu także dla typu "none" (cgmotion: opacity 0 od razu); startuje po `pre`
    if !a.out.anim_type.is_empty() && !(a.out.anim_type == "none" && !scene.keys.is_empty()) {
        let tm = Timing::from(&a.out);
        let total = anim::total(&tm);
        d.actors.extend(actor(
            ELEMENT,
            t_in + plan.base_out_start,
            Some(t_in + plan.base_out_start + OUT_EPS),
            None,
            bake_transform(total, |t| elem(anim::eval_out(&tm, t))),
        ));
    }
    for l in scene.layers.iter().filter(|l| l.visible) {
        let (g_in, g_out) = compile::group_motion_pair(scene, l);
        let m_in = l.motion.motion_in.clone().or(g_in);
        let m_out = l.motion.out.clone().or(g_out);
        if let Some(m) = m_in.as_ref().filter(|m| active(&m.anim_type)) {
            let tm = Timing::from(m);
            let ch = bake_transform(anim::total(&tm), |t| anim::eval_in(&tm, t));
            d.actors.extend(actor(&l.id, 0.0, Some(0.0), Some(t_in), ch));
        }
        if let Some(m) = m_out.as_ref().filter(|m| active(&m.anim_type)) {
            let tm = Timing::from(m);
            let ch = bake_transform(anim::total(&tm), |t| anim::eval_out(&tm, t));
            d.actors.extend(actor(&l.id, t_in, Some(t_in + OUT_EPS), None, ch));
        }
    }
    // klucze użytkownika elementu (cel "element") i warstw: osobny aktor, bez okna (działają przez IN, hold i OUT)
    let element_keys: Vec<Channel> = scene.keys.iter().filter(|c| !c.keys.is_empty()).cloned().collect();
    if !element_keys.is_empty() {
        d.actors.push(Actor { target: ELEMENT.into(), offset: 0.0, active_from: None, active_until: None, channels: element_keys, paths: vec![] });
    }
    for l in scene.layers.iter().filter(|l| l.visible && (!l.keys.is_empty() || l.path.is_some())) {
        let channels: Vec<Channel> = l.keys.iter().filter(|c| !c.keys.is_empty()).cloned().collect();
        let paths: Vec<PathChannel> = l.path.iter().filter(|p| !p.keys.is_empty()).cloned().collect();
        if !channels.is_empty() || !paths.is_empty() {
            d.actors.push(Actor { target: l.id.clone(), offset: 0.0, active_from: None, active_until: None, channels, paths });
        }
    }
    // efekty tekstowe warstw tekstowych (reguły startu jak w skompilowanym JS: patrz playback::plan)
    let ts = a.extra.get("textSync").and_then(|v| v.as_bool()).unwrap_or(false);
    for l in scene.layers.iter().filter(|l| l.visible && matches!(l.layer_type, LayerType::Text | LayerType::Clock)) {
        if let Some((te, own)) = playback::resolve_text(&l.motion.text, &a.text) {
            let start = if own { 0.0 } else if ts { (plan.base_end - 0.1).max(0.0) } else { 0.0 };
            d.texts.push(TextTrack { target: l.id.clone(), mode: TextMode::In, spec: te.spec(), start, active_from: Some(0.0), active_until: Some(t_in) });
        }
        if let Some((te, own)) = playback::resolve_text(&l.motion.text_out, &a.text_out) {
            let start = t_in + if own { 0.0 } else { plan.pre_dur };
            d.texts.push(TextTrack { target: l.id.clone(), mode: TextMode::Out, spec: te.spec(), start, active_from: Some(t_in + OUT_EPS), active_until: None });
        }
    }
    if let Some(lp) = a.loop_spec.as_ref().filter(|lp| active(&lp.loop_type)) {
        if let Some(period) = loop_period(&lp.loop_type, lp.duration) {
            let mut ch = bake_transform(period, |t| elem(anim::loop_transform(&lp.loop_type, t, lp.duration)));
            for c in &mut ch { c.post = Extend::Loop { count: None }; }
            let mut child = Director::new("loop", period);
            child.free = true;
            child.actors.extend(actor(ELEMENT, 0.0, None, None, ch));
            d.children.push(child);
        }
    }
    d
}

/// Jak wiele źródeł (preset IN/OUT i klucze użytkownika) składa się na tej samej właściwości:
/// przesunięcia, obroty i rozmycia sumują się, skale i krycie mnożą się, resztę ustawia ostatnie źródło.
fn combine(old: f32, v: f32, prop: &str) -> f32 {
    match prop {
        "tx" | "ty" | "rot" | "skew" | "blur" => old + v,
        "sx" | "sy" | "opacity" | "fx" | "fy" | "sh" => old * v,
        _ => v,
    }
}

/// Składa próbki z powrotem w `Transform` per cel (id warstwy albo `ELEMENT`).
pub fn to_transforms(samples: &[Sample]) -> HashMap<String, Transform> {
    let mut m: HashMap<String, Transform> = HashMap::new();
    let mut seen: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
    for s in samples {
        let t = m.entry(s.target.clone()).or_default();
        let v = match PROPS.iter().position(|p| *p == s.prop) {
            Some(i) if !seen.insert((s.target.as_str(), s.prop.as_str())) => combine(get(t, i), s.value, &s.prop),
            _ => s.value,
        };
        set(t, &s.prop, v);
    }
    m
}

/// Transformacje w danej fazie odtwarzania (te same wejścia co `playback::evaluate`).
pub fn sample_phase(d: &Director, plan: &Plan, phase: Phase, t: f32, loop_t: f32) -> HashMap<String, Transform> {
    let mut s = vec![];
    match phase {
        // okno IN jest prawostronnie otwarte; tuż przed Stop trzymamy stan końcowy IN
        Phase::In => d.sample(t.min(plan.in_total - 1e-4), &mut s),
        // t=0 to początek OUT w odtwarzaczu edytora; silnik widzi go dopiero po OUT_EPS
        Phase::Out => d.sample(plan.in_total + t.max(2.0 * OUT_EPS), &mut s),
        // w hold aktywni są tylko aktorzy kluczy użytkownika (okna IN/OUT są wtedy zamknięte) i pętle
        Phase::Hold => {
            d.sample(plan.in_total, &mut s);
            d.sample_free(loop_t, &mut s);
        }
        Phase::Idle => {}
    }
    to_transforms(&s)
}

struct Cached { key: String, dir: Director }
thread_local! { static CACHE: RefCell<Option<Cached>> = const { RefCell::new(None) }; }

/// Odpowiednik `playback::evaluate` liczący transformacje silnikiem `btb-motion`. Efekty tekstowe
/// i to, które warstwy w ogóle mają animację w danej fazie, pochodzą z `playback::evaluate`.
pub fn evaluate(pb: &Playback, el: &Element, scene: &Scene, values: &Values, plan: &Plan) -> (Transform, HashMap<String, LayerFx>) {
    let (mut elem_tr, mut fx) = playback::evaluate(pb, el, scene, values, plan);
    if !pb.active() { return (elem_tr, fx); }
    let key = format!(
        "{}|{}|{}|{}|{}",
        serde_json::to_string(&el.animation).unwrap_or_default(),
        serde_json::to_string(scene).unwrap_or_default(),
        plan.in_total, plan.base_out_start, plan.out_total
    );
    let tr = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().map(|x| x.key != key).unwrap_or(true) {
            *c = Some(Cached { key, dir: build(el, scene, plan) });
        }
        sample_phase(&c.as_ref().unwrap().dir, plan, pb.phase, pb.t, pb.loop_t)
    });
    // bez próbek elementu (brak animacji) obowiązuje ta sama zasada punktu odniesienia co w playback::evaluate
    elem_tr = tr.get(ELEMENT).copied().unwrap_or_else(|| elem(Transform::default()));
    for (id, lf) in fx.iter_mut() {
        if lf.tr.is_some() { lf.tr = Some(tr.get(id).copied().unwrap_or_default()); }
    }
    // warstwy animowane wyłącznie kluczami użytkownika nie mają wpisu z playback::evaluate
    for (id, t) in &tr {
        if id != ELEMENT { fx.entry(id.clone()).or_default().tr = Some(*t); }
    }
    (elem_tr, fx)
}

/// Czy element ma klucze użytkownika (wtedy podgląd musi używać silnika, bo `playback::evaluate` ich nie zna).
pub fn uses_keys(el: &Element) -> bool {
    el.scene.as_ref().is_some_and(|s| s.keys.iter().any(|c| !c.keys.is_empty()) || s.layers.iter().any(|l| l.visible && (l.keys.iter().any(|c| !c.keys.is_empty()) || l.path.as_ref().is_some_and(|p| !p.keys.is_empty()))))
}

/// Czy aplikacja ma używać silnika `btb-motion` (`BTB_ENGINE=motion`).
pub fn enabled() -> bool {
    thread_local! { static ON: bool = std::env::var("BTB_ENGINE").is_ok_and(|v| v == "motion"); }
    ON.with(|v| *v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{ANIM_TYPES, DIRECTIONS, EASES, LOOP_TYPES};

    /// Tolerancje (błąd bezwzględny) per właściwość: piksele, ułamki, stopnie.
    fn tol(prop: &str) -> f32 {
        match prop {
            "tx" | "ty" => 1.0,
            "rot" | "skew" | "blur" => 0.1,
            _ => 0.004,
        }
    }

    /// Wartość pasuje, jeśli zgadza się z oryginałem w `t` albo w sąsiedztwie ±2 próbek 240 Hz
    /// (położenie skoków w wypieku jest dokładne z dokładnością do kroku próbkowania).
    fn close(prop: &str, i: usize, got: f32, orig: impl Fn(f32) -> Transform, t: f32) -> bool {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for k in -40..=40 {
            let v = get(&orig(t + k as f32 / 2400.0), i);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        got >= lo - tol(prop) && got <= hi + tol(prop)
    }

    fn spec(ty: &str, dir: &str, ease: &str, dur: f32, delay: f32) -> MotionSpec {
        MotionSpec::new(ty, Some(dir), dur, delay, ease)
    }

    #[test]
    fn element_in_out_matrix_matches_anim() {
        let mut checked = 0;
        for ty in ANIM_TYPES.iter().filter(|t| **t != "none") {
            for dir in DIRECTIONS {
                for (ei, ease) in EASES.iter().enumerate() {
                    let (dur, delay) = (0.4 + 0.1 * (ei % 4) as f32, 0.05 * (ei % 3) as f32);
                    let m = spec(ty, dir, ease, dur, delay);
                    let tm = Timing::from(&m);
                    let total = anim::total(&tm);
                    for (name, f) in [
                        ("in", Box::new(|t| anim::eval_in(&tm, t)) as Box<dyn Fn(f32) -> Transform>),
                        ("out", Box::new(|t| anim::eval_out(&tm, t))),
                    ] {
                        let chans = bake_transform(total, &f);
                        for step in 0..=60 {
                            let t = total * 1.1 * step as f32 / 60.0;
                            let mut got = Transform::default();
                            for c in &chans { set(&mut got, &c.prop, c.eval(t).unwrap()); }
                            for (i, prop) in PROPS.iter().enumerate() {
                                assert!(close(prop, i, get(&got, i), &f, t), "{name} {ty}/{dir}/{ease} t={t:.3} {prop}: got {} want {}", get(&got, i), get(&f(t), i));
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 10_000);
    }

    #[test]
    fn attention_loops_match_anim_over_many_periods() {
        for kind in LOOP_TYPES.iter().filter(|k| **k != "none") {
            let lp = LoopSpec { loop_type: kind.to_string(), duration: None, ..Default::default() };
            let Some(period) = loop_period(kind, lp.duration) else { continue };
            let f = |t: f32| anim::loop_transform(kind, t, None);
            let mut ch = bake_transform(period, f);
            for c in &mut ch { c.post = Extend::Loop { count: None }; }
            for step in 0..400 {
                let t = period * 3.7 * step as f32 / 400.0;
                let mut got = Transform::default();
                for c in &ch { set(&mut got, &c.prop, c.eval(t).unwrap()); }
                for (i, prop) in PROPS.iter().enumerate() {
                    assert!(close(prop, i, get(&got, i), f, t), "loop {kind} t={t:.3} {prop}: got {} want {}", get(&got, i), get(&f(t), i));
                }
            }
        }
    }

    fn values_for<'a>(el: &'a Element, base: &'a str) -> Values<'a> {
        Values { title: &el.title, fields: &el.fields, extra: &el.extra, base_url: base }
    }

    fn fx_eq(a: &Transform, b: &Transform) -> Result<(), String> {
        for (i, p) in PROPS.iter().enumerate() {
            if (get(a, i) - get(b, i)).abs() > tol(p) { return Err(format!("{p}: {} vs {}", get(a, i), get(b, i))); }
        }
        Ok(())
    }

    /// Czy `b` mieści się (z tolerancją) w zakresie wartości `window` dla każdej właściwości.
    fn in_window(window: &[Transform], b: &Transform) -> bool {
        PROPS.iter().enumerate().all(|(i, p)| {
            let lo = window.iter().map(|w| get(w, i)).fold(f32::MAX, f32::min);
            let hi = window.iter().map(|w| get(w, i)).fold(f32::MIN, f32::max);
            get(b, i) >= lo - tol(p) && get(b, i) <= hi + tol(p)
        })
    }

    /// Cały odtwarzacz: dla każdego typu elementu i par animacji IN/OUT/pętla porównuje
    /// `motion_bridge::evaluate` z `playback::evaluate` w fazach In, Hold i Out.
    #[test]
    fn player_matches_playback_on_presets() {
        let mut compared = 0;
        let pairs = ANIM_TYPES.len();
        for (ei, ety) in Element::ELEM_TYPES.iter().enumerate() {
            for k in 0..pairs {
                let (tin, tout) = (ANIM_TYPES[k], ANIM_TYPES[(k + 3 + ei) % pairs]);
                let mut el = Element::new_typed("t", ety);
                el.animation.anim_in = AnimSpec { anim_type: tin.into(), direction: Some(DIRECTIONS[k % 4].into()), duration: 0.5 + 0.05 * (k % 5) as f32, delay: 0.0, ease: "power2.out".into(), ..Default::default() };
                el.animation.out = AnimSpec { anim_type: tout.into(), direction: Some(DIRECTIONS[(k + 1) % 4].into()), duration: 0.4, delay: 0.0, ease: "power2.in".into(), ..Default::default() };
                el.animation.loop_spec = Some(LoopSpec { loop_type: LOOP_TYPES[k % LOOP_TYPES.len()].into(), duration: None, ..Default::default() });
                // animacje warstw: część warstw dostaje własne ruchy
                if let Some(scene) = el.scene.as_mut() {
                    for (li, l) in scene.layers.iter_mut().enumerate() {
                        if (li + k) % 2 == 0 {
                            l.motion.motion_in = Some(spec(ANIM_TYPES[(k + li) % pairs], "left", "back.out", 0.5, 0.1 * li as f32));
                            l.motion.out = Some(spec(ANIM_TYPES[(k + li + 5) % pairs], "right", "power2.in", 0.3, 0.05 * li as f32));
                        }
                    }
                }
                let scene = el.scene.clone().unwrap();
                let base = String::new();
                let vals = values_for(&el, &base);
                let plan = playback::plan(&el, &scene, &vals);
                let pbat = |phase: Phase, t: f32| {
                    let t = t.max(0.0);
                    Playback { phase, playing: true, t, loop_t: t, ..Default::default() }
                };
                for (phase, span) in [(Phase::In, plan.in_total), (Phase::Hold, 2.7 * 3.0), (Phase::Out, plan.out_total)] {
                    for s in 0..=20 {
                        let t = span * s as f32 / 20.0;
                        let pb = pbat(phase, t);
                        let (e0, f0) = playback::evaluate(&pb, &el, &scene, &vals, &plan);
                        let (e1, f1) = evaluate(&pb, &el, &scene, &vals, &plan);
                        // wartości w sąsiedztwie skoku: akceptujemy zgodność z oryginałem w t±1/240
                        let near = |tt: f32| playback::evaluate(&pbat(phase, tt), &el, &scene, &vals, &plan).0;
                        if let Err(m) = fx_eq(&e0, &e1) {
                            let window: Vec<_> = (-2..=2).map(|k| near(t + k as f32 / 240.0)).collect();
                            let ok = in_window(&window, &e1);
                            assert!(ok, "{ety} {tin}->{tout} {phase:?} t={t:.3} element: {m}");
                        }
                        for (id, l0) in &f0 {
                            let l1 = f1.get(id).unwrap_or_else(|| panic!("brak warstwy {id}"));
                            assert_eq!(l0.tr.is_some(), l1.tr.is_some(), "{ety} {phase:?} {id}");
                            if let (Some(a), Some(b)) = (&l0.tr, &l1.tr) {
                                if let Err(m) = fx_eq(a, b) {
                                    let window: Vec<_> = (-2..=2)
                                        .filter_map(|k| {
                                            let pn = pbat(phase, t + k as f32 / 240.0);
                                            let (_, ff) = playback::evaluate(&pn, &el, &scene, &vals, &plan);
                                            ff.get(id).and_then(|x| x.tr)
                                        })
                                        .collect();
                                    let ok = in_window(&window, b);
                                    let lay = scene.layers.iter().find(|l| &l.id == id).unwrap();
                                    assert!(ok, "{ety} {tin}->{tout} {phase:?} t={t:.3} {id}: {m}\n layer motion: {:?}\n group pair: {:?}", lay.motion, compile::group_motion_pair(&scene, lay));
                                }
                            }
                        }
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 1000, "{compared}");
    }

    /// Efekty tekstowe: ślady w Directorze muszą odpowiadać `TextFx` z `playback::evaluate` (które warstwy,
    /// w których fazach, od kiedy), z regułą cgmotion, że efekt własny "none" wyłącza efekt elementu.
    #[test]
    fn text_tracks_match_playback_text_fx() {
        let mut el = Element::new_typed("t", "LOWER_THIRD");
        el.animation.anim_in = AnimSpec { anim_type: "none".into(), ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "fade".into(), duration: 0.4, ..Default::default() };
        el.animation.loop_spec = None;
        el.animation.text = Some(TextEffect::new("words-up", 0.5, 0.1));
        el.animation.text_out = Some(TextEffect::new("fade", 0.3, 0.0));
        let text_ids: Vec<String> = el.scene.as_ref().unwrap().layers.iter().filter(|l| l.layer_type == LayerType::Text).map(|l| l.id.clone()).collect();
        assert!(!text_ids.is_empty());
        // pierwsza warstwa tekstu: własny efekt OUT "none" (wyłącza), IN dziedziczy po elemencie
        if let Some(s) = el.scene.as_mut() {
            for l in s.layers.iter_mut().filter(|l| l.layer_type == LayerType::Text).take(1) {
                l.motion.text_out = Some(TextEffect::new("none", 0.3, 0.0));
            }
        }
        let scene = el.scene.clone().unwrap();
        let base = String::new();
        let vals = values_for(&el, &base);
        let plan = playback::plan(&el, &scene, &vals);
        let d = build(&el, &scene, &plan);
        let units = std::collections::BTreeMap::new();
        for (phase, t) in [(Phase::In, 0.2), (Phase::In, plan.in_total * 0.9), (Phase::Out, 0.1), (Phase::Out, plan.pre_dur + 0.2)] {
            let pb = Playback { phase, playing: true, t, loop_t: t, ..Default::default() };
            let (_, fx) = playback::evaluate(&pb, &el, &scene, &vals, &plan);
            let mut got = vec![];
            let time = if phase == Phase::In { t } else { plan.in_total + t };
            d.sample_texts(time, &units, &mut got);
            for id in &text_ids {
                let eg = fx.get(id).is_some_and(|f| f.text.is_some());
                let mine = got.iter().any(|u| &u.target == id);
                assert_eq!(eg, mine, "{phase:?} t={t} {id}: playback {eg}, silnik {mine}");
            }
        }
        // własny "none" wyłącza efekt elementu: pierwsza warstwa nie ma śladu OUT, pozostałe mają
        let out_tracks: Vec<_> = d.texts.iter().filter(|t| t.mode == TextMode::Out).map(|t| t.target.clone()).collect();
        assert!(!out_tracks.contains(&text_ids[0]));
        if text_ids.len() > 1 { assert!(out_tracks.contains(&text_ids[1])); }
        // ślad IN: start 0, okno [0, in_total)
        let tr = d.texts.iter().find(|t| t.mode == TextMode::In).unwrap();
        assert_eq!((tr.start, tr.active_from), (0.0, Some(0.0)));
        assert!((tr.active_until.unwrap() - plan.in_total).abs() < 1e-6);
    }

    fn keyed_element() -> (Element, String) {
        let mut el = Element::new_typed("t", "BADGE");
        el.animation.anim_in = AnimSpec { anim_type: "fade".into(), duration: 0.4, ease: "linear".into(), ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "fade".into(), duration: 0.4, ease: "linear".into(), ..Default::default() };
        el.animation.loop_spec = None;
        el.animation.text = None;
        el.animation.text_out = None;
        let s = el.scene.as_mut().unwrap();
        for l in s.layers.iter_mut() { l.motion = Motion::default(); }
        let id = s.layers[0].id.clone();
        let mut tx = btb_motion::Channel::new("tx");
        tx.keys = vec![btb_motion::Key::linear(0.0, 0.0), btb_motion::Key::linear(1.0, 100.0)];
        let mut op = btb_motion::Channel::new("opacity");
        op.keys = vec![btb_motion::Key::linear(0.0, 0.5)];
        s.layers[0].keys = vec![tx, op];
        (el, id)
    }

    /// Klucze użytkownika: warstwa bez presetów dostaje transformację z kluczy w IN, hold i OUT; stan hold
    /// nie dostaje stanu początkowego OUT (regresja: glitch/none w hold).
    #[test]
    fn user_keys_drive_layers_in_every_phase() {
        let (mut el, id) = keyed_element();
        // preset OUT glitch na tej samej warstwie: jego stan początkowy (opacity 0,3, x=10) nie może wyciec do hold
        el.scene.as_mut().unwrap().layers[0].motion.out = Some(spec("glitch", "left", "linear", 0.5, 0.0));
        assert!(uses_keys(&el));
        let scene = el.scene.clone().unwrap();
        let base = String::new();
        let vals = values_for(&el, &base);
        let plan = playback::plan(&el, &scene, &vals);
        let d = build(&el, &scene, &plan);
        // IN: tx z kluczy (100 * t), krycie 0,5
        let tr = sample_phase(&d, &plan, Phase::In, 0.2, 0.0);
        assert!((tr[&id].tx - 20.0).abs() < 0.5 && (tr[&id].opacity - 0.5).abs() < 1e-3, "{:?}", tr[&id]);
        // hold: wartości kluczy w t_in, bez glitcha OUT
        let tr = sample_phase(&d, &plan, Phase::Hold, 0.0, 0.0);
        assert!((tr[&id].tx - 100.0 * plan.in_total.min(1.0)).abs() < 0.5, "hold {:?}", tr[&id]);
        assert!((tr[&id].opacity - 0.5).abs() < 1e-3, "hold bez stanu OUT: {:?}", tr[&id]);
        // OUT: preset glitch (slot 0: opacity 0,3, x=10) składa się z kluczami: x = 10 + tx(klucze), krycie 0,3 * 0,5
        let tr = sample_phase(&d, &plan, Phase::Out, 0.05, 0.0);
        assert!((tr[&id].opacity - 0.15).abs() < 1e-3 && tr[&id].tx > 10.0, "out {:?}", tr[&id]);
        // evaluate (ścieżka silnika) zwraca warstwę także bez presetu IN
        let pb = Playback { phase: Phase::Hold, playing: true, t: 0.0, loop_t: 0.0, ..Default::default() };
        let (_, fx) = evaluate(&pb, &el, &scene, &vals, &plan);
        assert!(fx.get(&id).is_some_and(|f| f.tr.is_some()));
    }

    /// Ścieżka ruchu warstwy: pozycja z krzywej (tx, ty), stała po ostatnim kluczu, zakres planu i roundtrip JSON.
    #[test]
    fn layer_path_drives_position_and_extends_the_plan() {
        let (mut el, id) = keyed_element();
        let s = el.scene.as_mut().unwrap();
        s.layers[0].keys.clear();
        s.layers[0].path = Some(btb_motion::PathChannel { keys: vec![btb_motion::PathKey::new(0.0, 0.0, 0.0), btb_motion::PathKey::new(2.0, 100.0, 60.0)] });
        assert!(uses_keys(&el));
        let scene = el.scene.clone().unwrap();
        let base = String::new();
        let vals = values_for(&el, &base);
        let plan = playback::plan(&el, &scene, &vals);
        assert!(plan.in_total + plan.out_total >= 2.0 - 1e-4, "ścieżka do t=2 s wydłuża oś");
        let d = build(&el, &scene, &plan);
        // dwa klucze = prosta, stała prędkość: w t=1 s połowa drogi
        let tr = to_transforms(&{ let mut v = vec![]; d.sample(1.0, &mut v); v });
        assert!((tr[&id].tx - 50.0).abs() < 0.5 && (tr[&id].ty - 30.0).abs() < 0.5, "{:?}", tr[&id]);
        // hold: pozycja w t_in
        let h = sample_phase(&d, &plan, Phase::Hold, 0.0, 0.0);
        // 100 px w 2 s po prostej = 50 px/s, więc w t_in pozycja to 50 * t_in
        assert!((h[&id].tx - 50.0 * plan.in_total).abs() < 0.6 && (h[&id].ty - 30.0 * plan.in_total).abs() < 0.6, "hold {:?} (t_in={})", h[&id], plan.in_total);
        // po ostatnim kluczu pozycja stała
        let late = to_transforms(&{ let mut v = vec![]; d.sample(5.0, &mut v); v });
        assert!((late[&id].tx - 100.0).abs() < 0.1 && (late[&id].ty - 60.0).abs() < 0.1);
        // roundtrip JSON warstwy i brak pola przy braku ścieżki
        let back: Layer = serde_json::from_value(serde_json::to_value(&scene.layers[0]).unwrap()).unwrap();
        assert_eq!(back.path.unwrap().keys.len(), 2);
        assert!(serde_json::to_value(Layer::new(LayerType::Shape)).unwrap().get("path").is_none());
    }

    #[test]
    fn keys_extend_plan_and_stop_position() {
        let (mut el, _) = keyed_element();
        let mut c = btb_motion::Channel::new("ty");
        c.keys = vec![btb_motion::Key::linear(0.0, 0.0), btb_motion::Key::linear(3.0, 40.0)];
        el.scene.as_mut().unwrap().layers[0].keys.push(c);
        let scene = el.scene.clone().unwrap();
        let base = String::new();
        let before = playback::plan(&el, &scene, &values_for(&el, &base));
        // klucz w t=3 s leży po Stop, więc wydłuża wyjście, a nie wejście
        assert!(before.in_total < 1.0 && (before.in_total + before.out_total) >= 3.0 - 1e-3);
        // ręczna pozycja Stop przesuwa granicę IN/OUT i skraca wyjście o tyle samo
        el.animation.extra.insert("stopAt".into(), serde_json::Value::from(2.0));
        let after = playback::plan(&el, &scene, &values_for(&el, &base));
        assert!((after.in_total - 2.0).abs() < 1e-6 && (after.in_total + after.out_total) >= 3.0 - 1e-3);
        let d = build(&el, &scene, &after);
        let stop = d.events.iter().find(|e| e.kind == EventKind::Stop).unwrap();
        assert!((stop.t - 2.0).abs() < 1e-6);
    }

    #[test]
    fn sources_compose_additively_and_multiplicatively() {
        let samples = vec![
            Sample { target: "a".into(), prop: "tx".into(), value: 10.0 },
            Sample { target: "a".into(), prop: "tx".into(), value: 5.0 },
            Sample { target: "a".into(), prop: "opacity".into(), value: 0.5 },
            Sample { target: "a".into(), prop: "opacity".into(), value: 0.5 },
            Sample { target: "a".into(), prop: "ox".into(), value: 0.2 },
            Sample { target: "a".into(), prop: "ox".into(), value: 0.9 },
            Sample { target: "b".into(), prop: "tx".into(), value: 3.0 },
        ];
        let m = to_transforms(&samples);
        assert_eq!(m["a"].tx, 15.0);
        assert_eq!(m["a"].opacity, 0.25);
        assert_eq!(m["a"].origin.0, 0.9); // reszta: ostatnie źródło
        assert_eq!(m["b"].tx, 3.0);
    }

    #[test]
    fn layers_without_keys_serialize_as_before() {
        let (el, _) = keyed_element();
        let json = serde_json::to_string(&el).unwrap();
        let back: Element = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scene.as_ref().unwrap().layers[0].keys.len(), 2);
        // brak pola `keys` w starych plikach i w warstwach bez kluczy
        let plain = serde_json::to_value(Layer::new(LayerType::Shape)).unwrap();
        assert!(plain.get("keys").is_none());
        let old: Layer = serde_json::from_value(plain).unwrap();
        assert!(old.keys.is_empty());
    }

    #[test]
    fn director_roundtrips_through_json_and_exports_wasm_fixtures() {
        let mut el = Element::new_typed("t", "LOWER_THIRD");
        el.animation.anim_in = AnimSpec { anim_type: "slide-overshoot".into(), direction: Some("left".into()), duration: 0.6, ..Default::default() };
        el.animation.out = AnimSpec { anim_type: "glitch".into(), duration: 0.5, ..Default::default() };
        el.animation.loop_spec = Some(LoopSpec { loop_type: "pulse".into(), duration: None, ..Default::default() });
        let scene = el.scene.clone().unwrap();
        let base = String::new();
        let plan = playback::plan(&el, &scene, &values_for(&el, &base));
        let d = build(&el, &scene, &plan);
        let json = serde_json::to_string(&d).unwrap();
        let back: Director = serde_json::from_str(&json).unwrap();
        let (mut a, mut b) = (vec![], vec![]);
        d.sample(0.3, &mut a);
        back.sample(0.3, &mut b);
        assert_eq!(a, b);
        assert!(!a.is_empty());
        // fixture dla tools/wasm-parity.js (zgodność WASM <-> natywnie na prawdziwych danych aplikacji)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(target) = exe.ancestors().nth(3) {
                let dir = target.join("parity-extra");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("app_lower_third.json"), json).unwrap();
            }
        }
    }
}
