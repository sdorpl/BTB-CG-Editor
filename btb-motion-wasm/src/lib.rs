//! Powłoka WASM silnika `btb-motion` z czystym C ABI (bez wasm-bindgen): ładuje JSON z Directorem,
//! steruje transportem i zwraca próbki jako JSON w buforze współdzielonym z JS.
//!
//! Użycie z JS: `alloc` -> zapis JSON do pamięci -> `load` -> `start`/`advance`/`sample` ->
//! odczyt `out_ptr`/`out_len`.
use btb_motion::{Director, Fired, TextMode, Transport, Units};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Default)]
struct State {
    root: Option<Director>,
    tr: Transport,
    out: String,
    /// próbki efektów tekstowych (osobny bufor, żeby nie mieszać z transformacjami)
    tout: String,
    units: BTreeMap<String, Units>,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn samples_json(root: &Director, tr: &Transport, at: Option<f32>) -> String {
    let mut s = vec![];
    match at {
        Some(t) => root.sample(t, &mut s),
        None => tr.sample(root, &mut s),
    }
    Value::Array(
        s.into_iter()
            .map(|x| json!({ "target": x.target, "prop": x.prop, "value": x.value }))
            .collect(),
    )
    .to_string()
}

/// JSON z próbkami jednostek tekstu: `[{target, unit, mode, x, y, s, o, b, cl:[l,t,r,b], p}]`.
pub fn texts_json(root: &Director, tr: &Transport, units: &BTreeMap<String, Units>) -> String {
    let mut v = vec![];
    tr.sample_texts(root, units, &mut v);
    Value::Array(
        v.into_iter()
            .map(|u| {
                let p = u.props;
                json!({
                    "target": u.target, "unit": u.unit, "split": u.split,
                    "mode": if u.mode == TextMode::In { "in" } else { "out" },
                    "x": p.x, "y": p.y, "s": p.scale, "o": p.opacity, "b": p.blur, "cl": p.clip, "p": p.progress
                })
            })
            .collect(),
    )
    .to_string()
}

pub fn fired_json(f: &[Fired]) -> String {
    Value::Array(
        f.iter()
            .map(|e| match e {
                Fired::Tag(n) => json!({ "kind": "tag", "name": n }),
                Fired::Stopped(n) => json!({ "kind": "stopped", "name": n }),
                Fired::Paused(n) => json!({ "kind": "paused", "name": n }),
                Fired::End => json!({ "kind": "end" }),
            })
            .collect(),
    )
    .to_string()
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` muszą pochodzić z `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// Ładuje Director z JSON. Zwraca 0 przy sukcesie, 1 przy błędzie.
///
/// # Safety
/// `ptr`/`len` muszą wskazywać poprawny bufor z `alloc`.
#[no_mangle]
pub unsafe extern "C" fn load(ptr: *const u8, len: usize) -> i32 {
    let bytes = std::slice::from_raw_parts(ptr, len);
    match serde_json::from_slice::<Director>(bytes) {
        Ok(d) => with(|s| {
            s.root = Some(d);
            s.tr = Transport::default();
            0
        }),
        Err(_) => 1,
    }
}

#[no_mangle]
pub extern "C" fn start() {
    with(|s| s.tr.start())
}
#[no_mangle]
pub extern "C" fn resume() {
    with(|s| s.tr.resume())
}
#[no_mangle]
pub extern "C" fn stop() {
    with(|s| s.tr.stop())
}
#[no_mangle]
pub extern "C" fn start_reversed_from(t: f32) {
    with(|s| s.tr.start_reversed_from(t))
}

/// Przesuwa transport o `dt` s. Wynik (JSON z zdarzeniami) w buforze wyjściowym.
#[no_mangle]
pub extern "C" fn advance(dt: f32) {
    with(|s| {
        let State { root, tr, out, .. } = s;
        if let Some(r) = root {
            *out = fired_json(&tr.advance(r, dt));
        }
    })
}

/// Próbki w bieżącym stanie transportu (JSON w buforze wyjściowym).
#[no_mangle]
pub extern "C" fn sample() {
    with(|s| {
        if let Some(r) = &s.root {
            s.out = samples_json(r, &s.tr, None);
            s.tout = texts_json(r, &s.tr, &s.units);
        }
    })
}

/// Liczby jednostek tekstu z przeglądarki: JSON `{ "<id warstwy>": {n, cap, plain, num} }`. 0 = ok.
///
/// # Safety
/// `ptr`/`len` muszą wskazywać poprawny bufor z `alloc`.
#[no_mangle]
pub unsafe extern "C" fn set_units(ptr: *const u8, len: usize) -> i32 {
    match serde_json::from_slice::<BTreeMap<String, Units>>(std::slice::from_raw_parts(ptr, len)) {
        Ok(u) => with(|s| {
            s.units = u;
            0
        }),
        Err(_) => 1,
    }
}

/// Easing o nazwie GSAP w punkcie `x` (do testów zgodności z `gsap.parseEase`).
///
/// # Safety
/// `ptr`/`len` muszą wskazywać poprawny bufor z `alloc` z nazwą w UTF-8.
#[no_mangle]
pub unsafe extern "C" fn ease_at(ptr: *const u8, len: usize, x: f32) -> f32 {
    let name = std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("");
    btb_motion::ease::ease(name, x)
}

/// Próbki w zadanym czasie osi (bez zegara swobodnego), niezależnie od transportu.
#[no_mangle]
pub extern "C" fn sample_at(t: f32) {
    with(|s| {
        if let Some(r) = &s.root {
            s.out = samples_json(r, &s.tr, Some(t));
        }
    })
}

#[no_mangle]
pub extern "C" fn time() -> f32 {
    with(|s| s.tr.t)
}
#[no_mangle]
pub extern "C" fn playing() -> i32 {
    with(|s| s.tr.playing as i32)
}
#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    with(|s| s.out.as_ptr())
}
#[no_mangle]
pub extern "C" fn out_len() -> usize {
    with(|s| s.out.len())
}
#[no_mangle]
pub extern "C" fn tout_ptr() -> *const u8 {
    with(|s| s.tout.as_ptr())
}
#[no_mangle]
pub extern "C" fn tout_len() -> usize {
    with(|s| s.tout.len())
}
