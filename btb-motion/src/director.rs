//! Director (oś czasu) -> Actor (element sceny) -> Channel (właściwość), plus Eventy.
use crate::channel::Channel;
use crate::path::PathChannel;
use crate::text::{self, Mode, TextSpec, Units};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Kierunek odtwarzania, w którym zdarzenie jest ważne (Dir w Viz).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    #[default]
    Both,
    Forward,
    Reverse,
}

impl Dir {
    pub fn matches(self, reverse: bool) -> bool {
        matches!(
            (self, reverse),
            (Dir::Both, _) | (Dir::Forward, false) | (Dir::Reverse, true)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum EventKind {
    /// Zatrzymuje całe odtwarzanie do `continue` (Stop w Viz: dotyczy też poddyrektorów).
    Stop,
    /// Jak Stop, ale tylko dla swojego Directora. TODO: wymaga osobnego czasu per Director.
    LocalStop,
    /// Znacznik bez zatrzymania; wołany z zewnątrz po nazwie (np. "pilot1").
    Tag,
    /// Zatrzymuje na `len` sekund.
    Pause { len: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub t: f32,
    #[serde(flatten)]
    pub kind: EventKind,
    #[serde(default)]
    pub dir: Dir,
}

/// Element sceny z kanałami; `target` to id warstwy (Layer.id) albo "element".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Actor {
    pub target: String,
    #[serde(default)]
    pub offset: f32,
    /// Okno aktywności `[active_from, active_until)` w czasie Directora (przed odjęciem `offset`).
    /// Poza oknem aktor nie daje próbek; dzięki temu IN i OUT tego samego elementu nie
    /// konkurują o te same właściwości.
    #[serde(default)]
    pub active_from: Option<f32>,
    #[serde(default)]
    pub active_until: Option<f32>,
    pub channels: Vec<Channel>,
    /// Ścieżki ruchu: każda daje próbki `tx` i `ty` (przesunięcie od pozycji warstwy).
    #[serde(default)]
    pub paths: Vec<PathChannel>,
}

/// Efekt tekstowy elementu `target` (id warstwy tekstowej), liczony przez `text::eval`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextTrack {
    pub target: String,
    pub mode: Mode,
    pub spec: TextSpec,
    /// początek śladu w czasie Directora (przed opóźnieniem efektu)
    #[serde(default)]
    pub start: f32,
    #[serde(default)]
    pub active_from: Option<f32>,
    #[serde(default)]
    pub active_until: Option<f32>,
}

/// Właściwości jednej jednostki tekstu w danej chwili.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitSample {
    pub target: String,
    pub unit: usize,
    pub mode: Mode,
    /// próbka dotyczy jednostki podziału (znak/słowo), a nie całego elementu tekstu
    pub split: bool,
    pub props: text::Props,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Director {
    pub name: String,
    pub length: f32,
    #[serde(default)]
    pub offset: f32,
    /// Zegar swobodny (jak "Use System Time" w Viz): ten Director nie podąża za osią czasu, tylko za
    /// zegarem przekazanym do `sample_ctx`; działa tylko, gdy zegar jest dostępny (np. w stanie Stop).
    #[serde(default)]
    pub free: bool,
    #[serde(default)]
    pub actors: Vec<Actor>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub texts: Vec<TextTrack>,
    #[serde(default)]
    pub children: Vec<Director>,
}

/// Wynik ewaluacji jednego kanału.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub target: String,
    pub prop: String,
    pub value: f32,
}

impl Director {
    pub fn new(name: &str, length: f32) -> Self {
        Self {
            name: name.into(),
            length,
            ..Default::default()
        }
    }

    /// Wszystkie zdarzenia drzewa w czasie bezwzględnym (z uwzględnieniem Offset każdego Directora).
    pub fn events_abs<'a>(&'a self, base: f32, out: &mut Vec<(f32, &'a Event)>) {
        let base = base + self.offset;
        out.extend(self.events.iter().map(|e| (base + e.t, e)));
        for c in &self.children {
            c.events_abs(base, out);
        }
    }

    /// Ewaluacja całego drzewa w czasie `t` (czysta funkcja: ten sam `t` daje ten sam wynik).
    pub fn sample(&self, t: f32, out: &mut Vec<Sample>) {
        self.sample_ctx(t, None, out)
    }

    /// Jak `sample`, ale z zegarem swobodnym dla Directorów z `free = true`
    /// (bez zegara są pomijane razem z poddrzewem).
    pub fn sample_ctx(&self, t: f32, clock: Option<f32>, out: &mut Vec<Sample>) {
        let base = if self.free {
            match clock {
                Some(c) => c,
                None => return,
            }
        } else {
            t
        };
        let local = base - self.offset;
        for a in &self.actors {
            if a.active_from.is_some_and(|f| local < f)
                || a.active_until.is_some_and(|u| local >= u)
            {
                continue;
            }
            for ch in &a.channels {
                if let Some(value) = ch.eval(local - a.offset) {
                    out.push(Sample {
                        target: a.target.clone(),
                        prop: ch.prop.clone(),
                        value,
                    });
                }
            }
            for path in &a.paths {
                if let Some((x, y)) = path.eval(local - a.offset) {
                    for (prop, value) in [("tx", x), ("ty", y)] {
                        out.push(Sample {
                            target: a.target.clone(),
                            prop: prop.into(),
                            value,
                        });
                    }
                }
            }
        }
        for c in &self.children {
            c.sample_ctx(local, clock, out);
        }
    }

    /// Próbkuje wyłącznie Directory swobodne (stan Stop bez animacji osi czasu).
    pub fn sample_free(&self, clock: f32, out: &mut Vec<Sample>) {
        if self.free {
            self.sample_ctx(0.0, Some(clock), out);
        } else {
            for c in &self.children {
                c.sample_free(clock, out);
            }
        }
    }

    /// Efekty tekstowe w czasie `t`: po jednej próbce na jednostkę. `units` podaje liczby jednostek
    /// z przeglądarki (brak wpisu = jeden element).
    pub fn sample_texts(&self, t: f32, units: &BTreeMap<String, Units>, out: &mut Vec<UnitSample>) {
        let local = t - self.offset;
        for tr in &self.texts {
            if tr.active_from.is_some_and(|f| local < f)
                || tr.active_until.is_some_and(|u| local >= u)
            {
                continue;
            }
            let u = units.get(&tr.target).cloned().unwrap_or_default();
            let split = text::applies(text::split_of(&tr.spec.effect), &u);
            let n = if split { u.n.max(1) } else { 1 };
            for i in 0..n {
                out.push(UnitSample {
                    target: tr.target.clone(),
                    unit: i,
                    mode: tr.mode,
                    split,
                    props: text::eval(tr.mode, &tr.spec, &u, i, local - tr.start),
                });
            }
        }
        for c in &self.children {
            c.sample_texts(local, units, out);
        }
    }
}
