//! Zakładka „Ruch”: animacja całego elementu albo zaznaczonej warstwy (presety IN/OUT, efekty tekstu, klucze).
use super::{heading, hint};
use crate::anim::{ANIM_TYPES, DIRECTIONAL, DIRECTIONS, EASES, LOOP_TYPES, STAGGER_FROM, TEXT_IN, TEXT_OUT};
use crate::i18n::{opt, tr, trf};
use crate::keyconv::{self, Scope};
use crate::model::*;
use crate::motion_bridge::ELEMENT;
use crate::panels::{combo, combo_t, num_range};
use egui::{Id, Ui};
use serde_json::Value;

/// Wartość listy „dziedzicz” przy efekcie tekstu (brak własnego efektu warstwy).
const INHERIT: &str = "(dziedzicz)";

#[derive(Clone, Copy, PartialEq)]
enum MotionScope { Element, Layer }

pub fn motion_tab(ui: &mut Ui, el: &mut Element, selected: &mut Vec<String>) {
    let sid = Id::new("motion_scope");
    let layer_sel = selected.len() == 1 && selected[0] != ELEMENT;
    let mut scope = ui.data(|d| d.get_temp::<MotionScope>(sid)).unwrap_or(if layer_sel { MotionScope::Layer } else { MotionScope::Element });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut scope, MotionScope::Element, tr("motion.scope.element"));
        ui.selectable_value(&mut scope, MotionScope::Layer, tr("motion.scope.layer"));
    });
    ui.separator();
    match scope {
        MotionScope::Element => element_motion(ui, el),
        MotionScope::Layer => layer_motion(ui, el, selected),
    }
    if let Some(id) = layers_overview(ui, el, selected) { *selected = vec![id]; scope = MotionScope::Layer; }
    ui.data_mut(|d| d.insert_temp(sid, scope));
}

// ── Wspólne wiersze ─────────────────────────────────────────────────────────

/// Pola presetu IN/OUT wspólne dla elementu (`AnimSpec`) i warstwy (`MotionSpec`).
#[allow(clippy::too_many_arguments)]
fn spec_grid(ui: &mut Ui, salt: &str, ty: &mut String, direction: &mut Option<String>, duration: &mut f32, delay: &mut f32, ease: &mut String, extra: &mut Extra, peers: &[(String, String)]) {
    egui::Grid::new(format!("ms_{salt}")).num_columns(2).spacing([6.0, 3.0]).show(ui, |ui| {
        ui.label(tr("common.type")); combo_t(ui, format!("ty_{salt}"), ty, ANIM_TYPES, "opt.anim"); ui.end_row();
        crate::anim::sync_fallback(extra, ty);
        if DIRECTIONAL.contains(&ty.as_str()) {
            let cur = direction.clone().unwrap_or_else(|| "left".into());
            let mut d = cur.clone();
            ui.label(tr("motion.direction")); combo_t(ui, format!("di_{salt}"), &mut d, DIRECTIONS, "opt.dir"); ui.end_row();
            if d != cur || direction.is_some() { *direction = Some(d); }
        }
        ui.label(tr("motion.duration")); num_range(ui, duration, 0.02, 0.05, 10.0); ui.end_row();
        ui.label(tr("motion.delay")); num_range(ui, delay, 0.02, 0.0, 10.0); ui.end_row();
        if !peers.is_empty() { after_row(ui, salt, extra, peers); }
        ui.label(tr("motion.ease")); combo(ui, format!("ea_{salt}"), ease, EASES); ui.end_row();
    });
}

/// „Po warstwie”: ruch startuje po zakończeniu tej samej fazy wskazanej warstwy (opóźnienie liczy się od jej końca).
fn after_row(ui: &mut Ui, salt: &str, extra: &mut Extra, peers: &[(String, String)]) {
    let cur = crate::links::after(extra).map(String::from);
    let name = |id: &str| peers.iter().find(|p| p.0 == id).map(|p| p.1.clone()).unwrap_or_else(|| tr("motion.after.missing").into());
    let mut sel = cur.clone();
    ui.label(tr("motion.after"));
    egui::ComboBox::from_id_salt(format!("af_{salt}")).selected_text(sel.as_deref().map(name).unwrap_or_else(|| tr("motion.after.parallel").into())).width(120.0).show_ui(ui, |ui| {
        ui.selectable_value(&mut sel, None, tr("motion.after.parallel"));
        for (id, n) in peers { ui.selectable_value(&mut sel, Some(id.clone()), n); }
    });
    ui.end_row();
    if sel != cur { crate::links::set_after(extra, sel.as_deref()); }
}

fn optional_text(ui: &mut Ui, label: &str, salt: &str, t: &mut Option<TextEffect>, options: &[&str], default_delay: f32, peers: &[(String, String)]) {
    let prefix = if std::ptr::eq(options, TEXT_OUT) { "opt.text_out" } else { "opt.text_in" };
    let mut cur = t.as_ref().map(|x| x.effect_type.clone()).unwrap_or_else(|| INHERIT.into());
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(salt).selected_text(if cur == INHERIT { tr("text.inherit_short").to_string() } else { opt(prefix, &cur) }).width(120.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut cur, INHERIT.to_string(), tr("text.inherit"));
            for o in options { ui.selectable_value(&mut cur, o.to_string(), opt(prefix, o)); }
        });
    });
    if cur == INHERIT { *t = None; return; }
    let e = t.get_or_insert_with(|| TextEffect::new(&cur, 0.5, default_delay));
    e.effect_type = cur;
    crate::anim::sync_fallback(&mut e.extra, &e.effect_type);
    if e.effect_type != "none" {
        egui::Grid::new(format!("te_{salt}")).num_columns(4).spacing([6.0, 3.0]).show(ui, |ui| {
            ui.label(tr("motion.duration")); num_range(ui, &mut e.duration, 0.02, 0.05, 10.0);
            ui.label(tr("motion.delay_short")); num_range(ui, &mut e.delay, 0.02, 0.0, 10.0); ui.end_row();
            if !peers.is_empty() { after_row(ui, salt, &mut e.extra, peers); }
            let mut st = e.stagger.unwrap_or(0.0);
            ui.label(tr("motion.stagger")); if num_range(ui, &mut st, 0.005, 0.0, 1.0).changed() { e.stagger = if st > 0.0 { Some(st) } else { None }; }
            ui.end_row();
            if crate::anim::split_of(&e.effect_type) != crate::anim::Split::Whole {
                let mut from = e.stagger_from().to_string();
                ui.label(tr("motion.stagger_from")); combo_t(ui, format!("sf_{salt}"), &mut from, STAGGER_FROM, "opt.stagger"); ui.end_row();
                if from == "start" { e.extra.remove("staggerFrom"); } else { e.extra.insert("staggerFrom".into(), Value::from(from)); }
            }
        });
    }
}

fn optional_motion(ui: &mut Ui, label: &str, salt: &str, m: &mut Option<MotionSpec>, peers: &[(String, String)]) {
    let mut on = m.is_some();
    if ui.checkbox(&mut on, label).changed() { *m = if on { Some(MotionSpec::default()) } else { None }; }
    if let Some(s) = m { spec_grid(ui, salt, &mut s.anim_type, &mut s.direction, &mut s.duration, &mut s.delay, &mut s.ease, &mut s.extra, peers); }
}

// ── Element ─────────────────────────────────────────────────────────────────

fn anim_spec(ui: &mut Ui, salt: &str, s: &mut AnimSpec) {
    spec_grid(ui, salt, &mut s.anim_type, &mut s.direction, &mut s.duration, &mut s.delay, &mut s.ease, &mut s.extra, &[]);
}

fn element_motion(ui: &mut Ui, el: &mut Element) {
    hint(ui, tr("anim.intro"));
    heading(ui, tr("motion.in"));
    anim_spec(ui, "ein", &mut el.animation.anim_in);
    heading(ui, tr("motion.out"));
    anim_spec(ui, "eout", &mut el.animation.out);
    heading(ui, tr("anim.text"));
    optional_text(ui, tr("anim.text_in"), "e_t_in", &mut el.animation.text, TEXT_IN, 0.2, &[]);
    optional_text(ui, tr("anim.text_out"), "e_t_out", &mut el.animation.text_out, TEXT_OUT, 0.0, &[]);
    for (key, label) in [("textSync", tr("anim.text_sync")), ("textOutSync", tr("anim.text_out_sync"))] {
        let mut v = el.animation.extra.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        if ui.checkbox(&mut v, label).changed() { el.animation.extra.insert(key.into(), Value::Bool(v)); }
    }
    heading(ui, tr("anim.loop"));
    let mut lp = el.animation.loop_spec.clone().unwrap_or_default();
    let mut t = if lp.loop_type.is_empty() { "none".to_string() } else { lp.loop_type.clone() };
    ui.horizontal(|ui| { ui.label(tr("common.type")); combo_t(ui, "loop", &mut t, LOOP_TYPES, "opt.loop"); });
    lp.loop_type = t.clone();
    if t != "none" {
        let mut d = lp.duration.unwrap_or(0.0);
        ui.horizontal(|ui| { ui.label(tr("anim.loop.duration")); num_range(ui, &mut d, 0.05, 0.0, 20.0); });
        lp.duration = if d > 0.0 { Some(d) } else { None };
        el.animation.loop_spec = Some(lp);
    } else {
        el.animation.loop_spec = None;
    }
    keys_section(ui, el, Scope::Element);
}

// ── Warstwa ─────────────────────────────────────────────────────────────────

fn layer_motion(ui: &mut Ui, el: &mut Element, selected: &[String]) {
    let Some(scene) = el.scene.as_mut() else { hint(ui, tr("layer.no_scene")); return };
    let [id] = selected else { hint(ui, tr("motion.pick_layer")); return };
    if id == ELEMENT { hint(ui, tr("motion.pick_layer")); return }
    let peers: Vec<(String, String)> = scene.layers.iter().filter(|l| &l.id != id).map(|l| (l.id.clone(), l.name.clone())).collect();
    let Some(layer) = scene.layers.iter_mut().find(|l| &l.id == id) else { return };
    heading(ui, tr("motion.title"));
    let m = &mut layer.motion;
    optional_motion(ui, tr("motion.in"), "m_in", &mut m.motion_in, &peers);
    optional_motion(ui, tr("motion.out"), "m_out", &mut m.out, &peers);
    optional_text(ui, tr("motion.text_in"), "t_in", &mut m.text, TEXT_IN, 0.2, &peers);
    optional_text(ui, tr("motion.text_out"), "t_out", &mut m.text_out, TEXT_OUT, 0.0, &peers);
    keys_section(ui, el, Scope::Layer(id.clone()));

    // kopiowanie ruchu do innej warstwy
    let mut to = String::new();
    ui.horizontal(|ui| {
        ui.label(tr("anim.copy_motion"));
        egui::ComboBox::from_id_salt("dup").selected_text(tr("anim.choose")).width(120.0).show_ui(ui, |ui| {
            for (pid, n) in &peers { ui.selectable_value(&mut to, pid.clone(), n); }
        });
    });
    if !to.is_empty() { if let Some(scene) = el.scene.as_mut() { copy_motion(scene, id, &to); } }
}

fn copy_motion(scene: &mut Scene, from: &str, to: &str) {
    let Some(mut m) = scene.layers.iter().find(|l| l.id == from).map(|l| l.motion.clone()) else { return };
    // powiązanie warstwy ze sobą nie ma sensu
    for e in [m.motion_in.as_mut().map(|s| &mut s.extra), m.out.as_mut().map(|s| &mut s.extra), m.text.as_mut().map(|s| &mut s.extra), m.text_out.as_mut().map(|s| &mut s.extra)].into_iter().flatten() {
        if crate::links::after(e) == Some(to) { crate::links::set_after(e, None); }
    }
    if let Some(dst) = scene.layers.iter_mut().find(|l| l.id == to) { dst.motion = m; }
}

/// Przegląd ruchu wszystkich warstw w jednym miejscu; kliknięcie wybiera warstwę do edycji. Zwraca jej id.
fn layers_overview(ui: &mut Ui, el: &Element, selected: &[String]) -> Option<String> {
    let scene = el.scene.as_ref()?;
    let mut pick = None;
    ui.separator();
    egui::CollapsingHeader::new(egui::RichText::new(tr("anim.layers")).strong()).id_salt("motion_overview").default_open(false).show(ui, |ui| {
        hint(ui, tr("anim.layers.hint"));
        for l in scene.layers.iter().rev() {
            let m = &l.motion;
            let mut tags: Vec<&str> = [("IN", m.motion_in.is_some()), ("OUT", m.out.is_some()), ("T-IN", m.text.is_some()), ("T-OUT", m.text_out.is_some())].iter().filter(|x| x.1).map(|x| x.0).collect();
            if !l.keys.is_empty() || l.path.is_some() { tags.push("⏱"); }
            let title = format!("{} {}  {}", l.layer_type.icon(), l.name, if tags.is_empty() { String::new() } else { format!("[{}]", tags.join(" ")) });
            if ui.selectable_label(selected.contains(&l.id), title).clicked() { pick = Some(l.id.clone()); }
        }
    });
    pick
}

// ── Klucze i ścieżka ────────────────────────────────────────────────────────

/// Stan kluczy użytkownika: ile kanałów/kluczy ma element lub warstwa, konwersja presetów i usuwanie.
/// Klucze są edytowane na osi czasu; tu widać, że istnieją (przełączają podgląd na silnik kluczy).
fn keys_section(ui: &mut Ui, el: &mut Element, scope: Scope) {
    let Some(scene) = el.scene.as_ref() else { return };
    let (channels, keys, path_keys) = match &scope {
        Scope::Layer(id) => scene.layers.iter().find(|l| &l.id == id).map_or((0, 0, 0), |l| {
            (l.keys.iter().filter(|c| !c.keys.is_empty()).count(), l.keys.iter().map(|c| c.keys.len()).sum::<usize>(), l.path.as_ref().map_or(0, |p| p.keys.len()))
        }),
        _ => (scene.keys.iter().filter(|c| !c.keys.is_empty()).count(), scene.keys.iter().map(|c| c.keys.len()).sum::<usize>(), 0),
    };
    let mid = Id::new(("keys_msg", format!("{scope:?}")));
    ui.separator();
    heading(ui, tr("motion.keys"));
    if channels == 0 && path_keys == 0 {
        hint(ui, tr("motion.keys.none"));
    } else {
        ui.label(trf("motion.keys.info", &[("channels", &channels), ("keys", &keys)]));
        if path_keys > 0 { ui.label(trf("motion.keys.path", &[("n", &path_keys)])); }
        hint(ui, tr("motion.keys.engine"));
    }
    ui.horizontal(|ui| {
        if ui.button(tr("timeline.presets_to_keys")).clicked() {
            let r = keyconv::convert_presets(el, scope.clone());
            let msg = if r.channels == 0 { tr("timeline.nothing_scope").to_string() } else { trf("timeline.converted", &[("channels", &r.channels), ("keys", &r.keys)]) };
            ui.data_mut(|d| d.insert_temp(mid, msg));
        }
        if (channels > 0 || path_keys > 0) && ui.button(tr("motion.keys.clear")).clicked() {
            if let Some(scene) = el.scene.as_mut() {
                match &scope {
                    Scope::Layer(id) => if let Some(l) = scene.layers.iter_mut().find(|l| &l.id == id) { l.keys.clear(); l.path = None; },
                    _ => scene.keys.clear(),
                }
            }
            ui.data_mut(|d| d.remove::<String>(mid));
        }
    });
    if let Some(m) = ui.data(|d| d.get_temp::<String>(mid)) { ui.weak(m); }
}
