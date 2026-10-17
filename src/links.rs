//! Powiązania ruchu warstw: IN/OUT (i efekt tekstu) warstwy może startować „po warstwie X”
//! (`extra["after"]` = id warstwy, ta sama faza). Powiązanie zamieniamy na zwykłe opóźnienie
//! w kopii sceny, więc podgląd, `compile`, `playback` i silnik nie muszą znać pojęcia powiązania.
use crate::model::*;
use crate::playback;
use crate::render::Values;
use std::collections::HashMap;

/// Id warstwy, po której startuje dany ruch/efekt.
pub fn after(extra: &Extra) -> Option<&str> {
    extra.get("after").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

pub fn set_after(extra: &mut Extra, id: Option<&str>) {
    match id { Some(id) => { extra.insert("after".into(), id.into()); } None => { extra.remove("after"); } }
}

fn motion_slots(m: &Motion) -> [(bool, Option<&Extra>); 4] {
    [(false, m.motion_in.as_ref().map(|s| &s.extra)), (true, m.out.as_ref().map(|s| &s.extra)),
     (false, m.text.as_ref().map(|s| &s.extra)), (true, m.text_out.as_ref().map(|s| &s.extra))]
}

pub fn has_links(scene: &Scene) -> bool {
    scene.layers.iter().any(|l| motion_slots(&l.motion).iter().any(|(_, e)| e.map_or(false, |e| after(e).is_some())))
}

struct Ctx<'a> {
    scene: &'a Scene,
    values: &'a Values<'a>,
    memo: HashMap<(usize, bool), f32>,
    stack: Vec<(usize, bool)>,
}

impl Ctx<'_> {
    fn offset(&mut self, extra: &Extra, out: bool) -> f32 {
        let Some(id) = after(extra) else { return 0.0 };
        match self.scene.layers.iter().position(|l| l.id == id) { Some(j) => self.end(j, out), None => 0.0 }
    }

    /// Koniec ruchu warstwy w danej fazie: ruch warstwy i jej własny efekt tekstu, z opóźnieniami po rozwiązaniu powiązań.
    fn end(&mut self, i: usize, out: bool) -> f32 {
        if let Some(v) = self.memo.get(&(i, out)) { return *v; }
        if self.stack.contains(&(i, out)) { return 0.0 }
        let scene = self.scene;
        let l = &scene.layers[i];
        if !l.visible { return 0.0 }
        self.stack.push((i, out));
        let mut e = 0.0f32;
        let m = if out { &l.motion.out } else { &l.motion.motion_in };
        if let Some(m) = m.as_ref().filter(|m| crate::motion_bridge::active(&m.anim_type)) {
            e = e.max(m.delay + self.offset(&m.extra, out) + m.duration);
        }
        if matches!(l.layer_type, LayerType::Text | LayerType::Clock) {
            let te = if out { &l.motion.text_out } else { &l.motion.text };
            if let Some(te) = te.as_ref().filter(|t| !t.effect_type.is_empty() && t.effect_type != "none") {
                let txt = playback::layer_text(l, self.values);
                e = e.max(te.delay + self.offset(&te.extra, out) + playback::text_total(te, &txt, None));
            }
        }
        self.stack.pop();
        self.memo.insert((i, out), e);
        e
    }
}

/// Kopia sceny z powiązaniami zamienionymi na opóźnienia (znacznik `after` jest usuwany, więc wynik jest idempotentny).
pub fn resolve(scene: &Scene, values: &Values) -> Scene {
    let mut out = scene.clone();
    if !has_links(scene) { return out }
    let mut cx = Ctx { scene, values, memo: HashMap::new(), stack: vec![] };
    for (i, l) in out.layers.iter_mut().enumerate() {
        let shift = |cx: &mut Ctx, extra: &mut Extra, is_out: bool| -> f32 {
            let d = cx.offset(extra, is_out);
            extra.remove("after");
            d
        };
        if let Some(s) = &mut l.motion.motion_in { s.delay += shift(&mut cx, &mut s.extra, false); }
        if let Some(s) = &mut l.motion.out { s.delay += shift(&mut cx, &mut s.extra, true); }
        if let Some(s) = &mut l.motion.text { s.delay += shift(&mut cx, &mut s.extra, false); }
        if let Some(s) = &mut l.motion.text_out { s.delay += shift(&mut cx, &mut s.extra, true); }
        let _ = i;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(id: &str, t: LayerType) -> Layer {
        let mut l: Layer = serde_json::from_value(serde_json::json!({ "id": id, "type": match t { LayerType::Text => "text", _ => "shape" } })).unwrap();
        l.visible = true;
        l
    }

    #[test]
    fn bg_out_waits_for_text_out() {
        let mut text = layer("t", LayerType::Text);
        text.default_text = Some("Tytuł".into());
        text.motion.out = Some(MotionSpec::new("fade", None, 0.3, 0.1, "power2.in"));
        let mut bg = layer("bg", LayerType::Shape);
        let mut out = MotionSpec::new("fade", None, 0.4, 0.05, "power2.in");
        set_after(&mut out.extra, Some("t"));
        bg.motion.out = Some(out);
        let scene = Scene { layers: vec![bg, text], ..Default::default() };
        assert!(has_links(&scene));
        let empty = serde_json::Map::new();
        let ex = Extra::new();
        let v = Values { title: "", fields: &empty, extra: &ex, base_url: "" };
        let r = resolve(&scene, &v);
        let d = r.layers[0].motion.out.as_ref().unwrap();
        assert!((d.delay - (0.05 + 0.1 + 0.3)).abs() < 1e-5, "{}", d.delay);
        assert!(after(&d.extra).is_none());
        assert!(!has_links(&r));
    }

    #[test]
    fn cycle_does_not_hang() {
        let mut a = layer("a", LayerType::Shape);
        let mut b = layer("b", LayerType::Shape);
        let mut sa = MotionSpec::default(); set_after(&mut sa.extra, Some("b")); a.motion.out = Some(sa);
        let mut sb = MotionSpec::default(); set_after(&mut sb.extra, Some("a")); b.motion.out = Some(sb);
        let scene = Scene { layers: vec![a, b], ..Default::default() };
        let empty = serde_json::Map::new();
        let ex = Extra::new();
        let v = Values { title: "", fields: &empty, extra: &ex, base_url: "" };
        let _ = resolve(&scene, &v);
    }
}
