//! Transport: odtwarzanie Directora w czasie rzeczywistym (Start / Stop / Continue / Reverse)
//! z obsługą zdarzeń. Zastępuje maszynę stanów `Phase { In, Hold, Out }` z playback.rs:
//! wejście i wyjście to jedna oś czasu rozdzielona zdarzeniem Stop.
use crate::director::{Director, EventKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Fired {
    Tag(String),
    Stopped(String),
    Paused(String),
    /// Doszliśmy do końca (albo początku przy reverse) osi czasu.
    End,
}

#[derive(Debug, Clone, Default)]
pub struct Transport {
    pub t: f32,
    pub playing: bool,
    pub reverse: bool,
    pause_left: f32,
    holding: bool,
    clock: f32,
}

impl Transport {
    pub fn start(&mut self) {
        self.t = 0.0;
        self.reverse = false;
        self.playing = true;
        self.pause_left = 0.0;
        self.holding = false;
        self.clock = 0.0;
    }
    /// Odtwarzanie wstecz od `t` (np. wyjście jako odwrócone wejście).
    pub fn start_reversed_from(&mut self, t: f32) {
        self.t = t;
        self.reverse = true;
        self.playing = true;
        self.pause_left = 0.0;
        self.holding = false;
        self.clock = 0.0;
    }
    pub fn stop(&mut self) {
        self.playing = false;
    }
    /// Continue Animation: wznawia od miejsca zatrzymania.
    pub fn resume(&mut self) {
        self.playing = true;
        self.holding = false;
        self.clock = 0.0;
    }
    pub fn goto(&mut self, t: f32) {
        self.t = t;
        self.pause_left = 0.0;
    }

    /// Przesuwa czas o `dt` sekund. Zwraca zdarzenia, które zadziałały. Zdarzenie Stop/Pause
    /// zatrzymuje transport dokładnie na swoim czasie (reszta `dt` jest pomijana).
    pub fn advance(&mut self, root: &Director, mut dt: f32) -> Vec<Fired> {
        let mut fired = vec![];
        if !self.playing {
            if self.holding {
                self.clock += dt;
            }
            return fired;
        }
        if self.pause_left > 0.0 {
            let used = dt.min(self.pause_left);
            self.pause_left -= used;
            dt -= used;
            if dt <= 0.0 {
                return fired;
            }
        }
        let prev = self.t;
        let target = if self.reverse {
            (prev - dt).max(0.0)
        } else {
            (prev + dt).min(root.length)
        };
        let mut evs = vec![];
        root.events_abs(0.0, &mut evs);
        evs.retain(|(et, e)| {
            e.dir.matches(self.reverse)
                && if self.reverse {
                    *et < prev && *et >= target
                } else {
                    *et > prev && *et <= target
                }
        });
        evs.sort_by(|a, b| (a.0 - prev).abs().total_cmp(&(b.0 - prev).abs()));
        for (et, e) in evs {
            match &e.kind {
                EventKind::Tag => fired.push(Fired::Tag(e.name.clone())),
                EventKind::Stop | EventKind::LocalStop => {
                    self.t = et;
                    self.playing = false;
                    self.holding = true;
                    self.clock = 0.0;
                    fired.push(Fired::Stopped(e.name.clone()));
                    return fired;
                }
                EventKind::Pause { len } => {
                    self.t = et;
                    self.pause_left = *len;
                    fired.push(Fired::Paused(e.name.clone()));
                    return fired;
                }
            }
        }
        self.t = target;
        if (!self.reverse && target >= root.length) || (self.reverse && target <= 0.0) {
            self.playing = false;
            fired.push(Fired::End);
        }
        fired
    }
}

impl Transport {
    /// Czy transport stoi na zdarzeniu Stop (zegar swobodny tyka).
    pub fn holding(&self) -> bool {
        self.holding
    }
    /// Sekundy od zatrzymania na Stop (0, gdy nie stoimy na Stop).
    pub fn clock(&self) -> f32 {
        self.clock
    }

    /// Ewaluuje drzewo w bieżącym stanie transportu (czas osi + zegar swobodny w stanie Stop).
    pub fn sample(&self, root: &Director, out: &mut Vec<crate::director::Sample>) {
        root.sample_ctx(
            self.t,
            if self.holding { Some(self.clock) } else { None },
            out,
        );
    }

    /// Efekty tekstowe w bieżącym stanie (czas osi; w stanie Stop ślady IN są już poza oknem).
    pub fn sample_texts(
        &self,
        root: &Director,
        units: &std::collections::BTreeMap<String, crate::text::Units>,
        out: &mut Vec<crate::director::UnitSample>,
    ) {
        root.sample_texts(self.t, units, out);
    }
}
