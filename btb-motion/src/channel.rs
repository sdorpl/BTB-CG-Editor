//! Kanał = jedna animowana właściwość z kluczami (Channel + Key Frame w Viz).
use crate::curve::{eval_segment, handle_vec, Ease, Handle};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub t: f32,
    pub v: f32,
    #[serde(default)]
    pub left: Handle,
    #[serde(default)]
    pub right: Handle,
    /// Temporal continuous (AE): prędkość po obu stronach klucza jest taka sama; `Channel::set_ease`
    /// przenosi zmianę prędkości na drugą stronę.
    #[serde(default)]
    pub continuous: bool,
}

impl Key {
    pub fn new(t: f32, v: f32) -> Self {
        Self {
            t,
            v,
            left: Handle::Smooth,
            right: Handle::Smooth,
            continuous: false,
        }
    }
    /// Klucz z Easy Ease po obu stronach.
    pub fn eased(t: f32, v: f32) -> Self {
        Self {
            t,
            v,
            left: Handle::easy(),
            right: Handle::easy(),
            continuous: false,
        }
    }
    pub fn linear(t: f32, v: f32) -> Self {
        Self {
            t,
            v,
            left: Handle::Linear,
            right: Handle::Linear,
            continuous: false,
        }
    }
}

/// Zachowanie poza zakresem kluczy (Swing/Loop In/Out w Viz). `count` = liczba dodatkowych
/// cykli, `None` = nieskończenie wiele.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Extend {
    #[default]
    Constant,
    Loop {
        count: Option<u32>,
    },
    Swing {
        count: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Channel {
    /// Nazwa właściwości, np. "x", "y", "scale", "opacity", "rotation".
    pub prop: String,
    pub keys: Vec<Key>,
    #[serde(default)]
    pub pre: Extend,
    #[serde(default)]
    pub post: Extend,
    /// Bez interpolacji (jak kanały całkowite w Viz): wartość zmienia się dopiero na kluczu.
    #[serde(default)]
    pub stepped: bool,
}

impl Channel {
    pub fn new(prop: &str) -> Self {
        Self {
            prop: prop.into(),
            ..Default::default()
        }
    }

    /// Most do obecnego modelu: tween `from -> to` o czasie `dur` z krzywą CSS cubic-bezier(x1,y1,x2,y2).
    /// Dzięki temu istniejące `MotionSpec { duration, delay, ease }` da się zapisać jako klucze.
    pub fn tween(
        prop: &str,
        from: f32,
        to: f32,
        delay: f32,
        dur: f32,
        bez: (f32, f32, f32, f32),
    ) -> Self {
        let dv = to - from;
        let (x1, y1, x2, y2) = bez;
        let mut a = Key::new(delay, from);
        a.right = Handle::Free {
            dt: x1 * dur,
            dv: y1 * dv,
        };
        let mut b = Key::new(delay + dur, to);
        b.left = Handle::Free {
            dt: -(1.0 - x2) * dur,
            dv: -(1.0 - y2) * dv,
        };
        Self {
            prop: prop.into(),
            keys: vec![a, b],
            ..Default::default()
        }
    }

    pub fn start(&self) -> f32 {
        self.keys.first().map_or(0.0, |k| k.t)
    }
    pub fn end(&self) -> f32 {
        self.keys.last().map_or(0.0, |k| k.t)
    }

    pub fn eval(&self, t: f32) -> Option<f32> {
        let (first, last) = (self.keys.first()?, self.keys.last()?);
        let t = if t < first.t {
            remap(t, first.t, last.t, self.pre, true)
        } else if t > last.t {
            remap(t, first.t, last.t, self.post, false)
        } else {
            t
        };
        if t <= first.t {
            return Some(first.v);
        }
        if t >= last.t {
            return Some(last.v);
        }
        let i = self.keys.partition_point(|k| k.t <= t).saturating_sub(1);
        let (k0, k1) = (&self.keys[i], &self.keys[i + 1]);
        if self.stepped || k0.right == Handle::Hold {
            return Some(k0.v);
        }
        let seg = (k1.t - k0.t, k1.v - k0.v);
        let out0 = handle_vec(k0.right, seg, self.slope(i), true);
        let in1 = handle_vec(k1.left, seg, self.slope(i + 1), false);
        Some(eval_segment((k0.t, k0.v), (k1.t, k1.v), out0, in1, t))
    }

    /// Nachylenie klucza `i` dla uchwytu Smooth: Catmull-Rom; na końcach 0 (łagodny start/koniec).
    fn slope(&self, i: usize) -> f32 {
        if i == 0 || i + 1 >= self.keys.len() {
            return 0.0;
        }
        let (a, b) = (&self.keys[i - 1], &self.keys[i + 1]);
        let dt = b.t - a.t;
        if dt <= 0.0 {
            0.0
        } else {
            (b.v - a.v) / dt
        }
    }
}

/// Mapuje czas spoza zakresu [t0,t1] z powrotem do zakresu, zgodnie z `Extend`.
fn remap(t: f32, t0: f32, t1: f32, ext: Extend, before: bool) -> f32 {
    let span = t1 - t0;
    let edge = if before { t0 } else { t1 };
    if span <= 0.0 {
        return edge;
    }
    // odległość od najbliższej krawędzi zakresu, w cyklach
    let cycles = if before {
        (t0 - t) / span
    } else {
        (t - t1) / span
    };
    match ext {
        Extend::Constant => edge,
        Extend::Loop { count } => {
            if count.is_some_and(|n| cycles >= n as f32) {
                return edge;
            }
            t0 + (t - t0).rem_euclid(span)
        }
        Extend::Swing { count } => {
            // phase 0 = początek zakresu; przed startem lustro względem t0, po końcu zaczyna się od t1
            let mut phase = if before {
                (t0 - t) / span
            } else {
                (t - t0) / span
            };
            let max = count.map(|n| if before { n as f32 } else { 1.0 + n as f32 });
            if let Some(m) = max {
                phase = phase.min(m);
            }
            let mut n = phase.floor();
            let mut frac = phase - n;
            if frac == 0.0 && n > 0.0 {
                n -= 1.0;
                frac = 1.0;
            }
            if (n as i64) % 2 == 0 {
                t0 + frac * span
            } else {
                t1 - frac * span
            }
        }
    }
}

// ── Baking: funkcja czasu -> klucze ─────────────────────────────────────────

/// Krok próbkowania przy wypiekaniu (240 Hz: ponad 4x gęściej niż 60 fps).
const BAKE_DT: f32 = 1.0 / 240.0;

impl Channel {
    /// Wypieka dowolną funkcję `f(t)` na `[t0, t1]` do kluczy. Próbkuje gęsto, rozdziela przebieg
    /// na ciągłe odcinki w miejscach skoków (>15% zakresu między próbkami, zapisywane jako
    /// `Handle::Hold`) i upraszcza każdy odcinek algorytmem Douglasa-Peuckera z błędem
    /// co najwyżej `eps_rel` zakresu wartości (minimum 1e-4 w jednostkach właściwości).
    pub fn bake(prop: &str, t0: f32, t1: f32, eps_rel: f32, f: impl Fn(f32) -> f32) -> Channel {
        let mut ch = Channel::new(prop);
        if t1 <= t0 {
            ch.keys.push(Key::linear(t0, f(t0)));
            return ch;
        }
        let n = (((t1 - t0) / BAKE_DT).ceil() as usize).max(2);
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let t = if i == n {
                    t1
                } else {
                    t0 + (t1 - t0) * i as f32 / n as f32
                };
                (t, f(t))
            })
            .collect();
        let (mn, mx) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let range = mx - mn;
        let eps = (eps_rel * range).max(1e-4);
        let jump = (0.15 * range).max(1e-3);

        // odcinki ciągłe: [start, end] włącznie
        let mut runs = vec![];
        let mut s = 0;
        for i in 0..n {
            if (pts[i + 1].1 - pts[i].1).abs() > jump {
                runs.push((s, i));
                s = i + 1;
            }
        }
        runs.push((s, n));

        for (ri, &(a, b)) in runs.iter().enumerate() {
            for idx in simplify(&pts, a, b, eps) {
                ch.keys.push(Key::linear(pts[idx].0, pts[idx].1));
            }
            if ri + 1 < runs.len() {
                ch.keys.last_mut().unwrap().right = Handle::Hold;
            }
        }
        ch
    }

    /// Kanał stały (jeden klucz), np. punkt odniesienia animacji.
    pub fn constant(prop: &str, v: f32) -> Channel {
        Channel {
            prop: prop.into(),
            keys: vec![Key::linear(0.0, v)],
            ..Default::default()
        }
    }
}

/// Douglas-Peucker na odcinku próbek [a,b] (indeksy zachowanych punktów, rosnąco).
fn simplify(pts: &[(f32, f32)], a: usize, b: usize, eps: f32) -> Vec<usize> {
    if a == b {
        return vec![a];
    }
    let mut keep = vec![false; b - a + 1];
    keep[0] = true;
    keep[b - a] = true;
    let mut stack = vec![(a, b)];
    while let Some((lo, hi)) = stack.pop() {
        if hi <= lo + 1 {
            continue;
        }
        let (p, q) = (pts[lo], pts[hi]);
        let (mut worst, mut wi) = (0.0f32, lo);
        for (i, pt) in pts.iter().enumerate().take(hi).skip(lo + 1) {
            let u = (pt.0 - p.0) / (q.0 - p.0);
            let err = (pt.1 - (p.1 + u * (q.1 - p.1))).abs();
            if err > worst {
                worst = err;
                wi = i;
            }
        }
        if worst > eps {
            keep[wi - a] = true;
            stack.push((lo, wi));
            stack.push((wi, hi));
        }
    }
    keep.iter()
        .enumerate()
        .filter(|(_, k)| **k)
        .map(|(i, _)| a + i)
        .collect()
}

// ── Ease (prędkość / wpływ), jak w After Effects ───────────────────────────

/// Odbicie uchwytu przy odwróceniu czasu: prędkość zmienia znak, wektor swobodny odbija się w osi czasu.
fn mirror_handle(h: Handle) -> Handle {
    match h {
        Handle::Ease { speed, influence } => Handle::Ease {
            speed: -speed,
            influence,
        },
        Handle::Free { dt, dv } => Handle::Free { dt: -dt, dv },
        other => other,
    }
}

impl Channel {
    /// Ease, jaki strona klucza `i` pokazuje w edytorze krzywych: zapisany `Ease`, prędkość wyliczona
    /// z sąsiadów (`Smooth`/`Auto`) albo odpowiednik prostej (nachylenie odcinka, wpływ 33,3%).
    /// `None` dla Hold i dla strony bez sąsiada.
    pub fn side_ease(&self, i: usize, out: bool) -> Option<Ease> {
        let k = self.keys.get(i)?;
        let j = if out { i + 1 } else { i.checked_sub(1)? };
        let n = self.keys.get(j)?;
        let h = if out { k.right } else { k.left };
        if (out && h == Handle::Hold) || (!out && n.right == Handle::Hold) {
            return None;
        }
        let dur = (k.t - n.t).abs().max(1e-6);
        let dv = if out { n.v - k.v } else { k.v - n.v };
        Some(match h {
            Handle::Linear | Handle::Hold => Ease {
                speed: dv / dur,
                influence: Ease::THIRD,
            },
            Handle::Smooth => Ease {
                speed: self.slope(i),
                influence: Ease::THIRD,
            },
            Handle::Auto => Ease {
                speed: self.slope(i),
                influence: Ease::AUTO_INFLUENCE,
            },
            Handle::Free { dt, dv } => Ease {
                speed: if dt.abs() > 1e-9 { dv / dt } else { 0.0 },
                influence: (dt.abs() / dur).clamp(0.0, 1.0),
            },
            Handle::Ease { speed, influence } => Ease {
                speed,
                influence: influence.clamp(0.0, 1.0),
            },
        })
    }

    /// Ustawia ease jednej strony klucza. Przy `continuous` ta sama prędkość trafia na drugą stronę
    /// (jej wpływ zostaje). Zwraca `false`, gdy klucz nie istnieje.
    pub fn set_ease(&mut self, i: usize, out: bool, e: Ease) -> bool {
        if i >= self.keys.len() {
            return false;
        }
        let e = Ease {
            speed: e.speed,
            influence: e.influence.clamp(0.0, 1.0),
        };
        let other_inf = self
            .side_ease(i, !out)
            .map_or(Ease::EASY.influence, |o| o.influence);
        let k = &mut self.keys[i];
        let (this, other) = if out {
            (&mut k.right, &mut k.left)
        } else {
            (&mut k.left, &mut k.right)
        };
        *this = Handle::from_ease(e);
        if k.continuous {
            *other = Handle::Ease {
                speed: e.speed,
                influence: other_inf,
            };
        }
        true
    }

    /// Easy Ease na wejściu, wyjściu albo obu stronach klucza `i`.
    pub fn easy_ease(&mut self, i: usize, ease_in: bool, ease_out: bool) {
        if let Some(k) = self.keys.get_mut(i) {
            if ease_in {
                k.left = Handle::easy();
            }
            if ease_out {
                k.right = Handle::easy();
            }
        }
    }

    /// Prędkość zmiany wartości (jednostki/s) w czasie `t`, różnicą centralną (wykres prędkości).
    pub fn velocity(&self, t: f32) -> Option<f32> {
        const H: f32 = 1e-3;
        Some((self.eval(t + H)? - self.eval(t - H)?) / (2.0 * H))
    }

    /// Odwraca klucze w czasie w granicach `[pierwszy, ostatni]` (Time-Reverse Keyframes): strony
    /// klucza zamieniają się miejscami, a prędkości zmieniają znak, więc `eval` odwróconego kanału w
    /// czasie `first + last - t` daje to samo co oryginał w `t`. Hold nie ma odbicia (trzymanie działa
    /// tylko w prawo), `stepped` i `Extend` pozostają bez zmian poza zamianą pre/post.
    pub fn time_reverse(&mut self) {
        let (Some(first), Some(last)) = (
            self.keys.first().map(|k| k.t),
            self.keys.last().map(|k| k.t),
        ) else {
            return;
        };
        for k in self.keys.iter_mut() {
            k.t = first + last - k.t;
            std::mem::swap(&mut k.left, &mut k.right);
            k.left = mirror_handle(k.left);
            k.right = mirror_handle(k.right);
        }
        self.keys.reverse();
        std::mem::swap(&mut self.pre, &mut self.post);
    }
}

// ── Edycja kluczy (oś czasu) ───────────────────────────────────────────────

/// Dwa klucze bliżej niż to są tym samym kluczem (wstawienie zastępuje, przesunięcie nadpisuje).
const SAME_TIME: f32 = 1e-4;

impl Channel {
    /// Wstawia klucz w czasie `t` (zastępuje klucz w tym samym czasie, zachowując jego uchwyty).
    /// Zwraca indeks klucza. Nowy klucz ma uchwyty proste, jak w AE.
    pub fn insert_key(&mut self, t: f32, v: f32) -> usize {
        if let Some(i) = self.keys.iter().position(|k| (k.t - t).abs() < SAME_TIME) {
            self.keys[i].v = v;
            return i;
        }
        let i = self.keys.partition_point(|k| k.t < t);
        self.keys.insert(i, Key::linear(t, v));
        i
    }

    /// Usuwa klucze o podanych indeksach (nieistniejące są pomijane).
    pub fn remove_keys(&mut self, idx: &[usize]) {
        let mut i = 0;
        self.keys.retain(|_| {
            let keep = !idx.contains(&i);
            i += 1;
            keep
        });
    }

    /// Wynik przesunięcia zaznaczonych kluczy o `dt` s (bez zmiany `self`, więc można go liczyć z migawki
    /// przy przeciąganiu): czasy ograniczone od dołu do 0, klucze niezaznaczone w tym samym czasie są
    /// nadpisywane. Zwraca nowy kanał i nowe indeksy przesuniętych kluczy.
    pub fn move_keys(&self, sel: &[usize], dt: f32) -> (Channel, Vec<usize>) {
        let min_t = sel
            .iter()
            .filter_map(|&i| self.keys.get(i))
            .map(|k| k.t)
            .fold(f32::MAX, f32::min);
        let dt = if min_t == f32::MAX {
            0.0
        } else {
            dt.max(-min_t)
        };
        let mut keys: Vec<(Key, bool)> = self
            .keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let mut k = k.clone();
                let moved = sel.contains(&i);
                if moved {
                    k.t += dt;
                }
                (k, moved)
            })
            .collect();
        let moved_times: Vec<f32> = keys.iter().filter(|(_, m)| *m).map(|(k, _)| k.t).collect();
        keys.retain(|(k, m)| *m || !moved_times.iter().any(|t| (t - k.t).abs() < SAME_TIME));
        keys.sort_by(|a, b| a.0.t.total_cmp(&b.0.t));
        let new_sel = keys
            .iter()
            .enumerate()
            .filter(|(_, (_, m))| *m)
            .map(|(i, _)| i)
            .collect();
        let mut out = self.clone();
        out.keys = keys.into_iter().map(|(k, _)| k).collect();
        (out, new_sel)
    }
}

// ── Uchwyty w edytorze krzywych ────────────────────────────────────────────

impl Channel {
    /// Punkt uchwytu strony klucza `i` w przestrzeni (czas, wartość): tam rysuje go edytor krzywych.
    /// `None` dla Hold i strony bez sąsiada.
    pub fn handle_point(&self, i: usize, out: bool) -> Option<(f32, f32)> {
        let e = self.side_ease(i, out)?;
        let k = self.keys.get(i)?;
        let n = self.keys.get(if out { i + 1 } else { i.checked_sub(1)? })?;
        let dt = e.influence * (k.t - n.t).abs();
        Some(if out {
            (k.t + dt, k.v + e.speed * dt)
        } else {
            (k.t - dt, k.v - e.speed * dt)
        })
    }

    /// Ustawia ease strony tak, by uchwyt leżał w `(t, v)` (przeciąganie w edytorze krzywych): poziomo
    /// zmienia wpływ, pionowo prędkość. `keep_influence` blokuje oś czasu (Shift w edytorze).
    /// Zwraca `false` dla klucza bez tej strony.
    pub fn set_handle_point(
        &mut self,
        i: usize,
        out: bool,
        t: f32,
        v: f32,
        keep_influence: bool,
    ) -> bool {
        let Some(old) = self.side_ease(i, out) else {
            return false;
        };
        let (k, n) = (&self.keys[i], &self.keys[if out { i + 1 } else { i - 1 }]);
        let dur = (k.t - n.t).abs().max(1e-6);
        let influence = if keep_influence {
            old.influence
        } else {
            (if out { t - k.t } else { k.t - t } / dur).clamp(0.0, 1.0)
        };
        let dt = influence * dur;
        // przy zerowym wpływie nachylenia nie da się wyznaczyć z punktu: zostaje stare
        let speed = if dt > 1e-6 {
            (if out { v - k.v } else { k.v - v }) / dt
        } else {
            old.speed
        };
        self.set_ease(i, out, Ease { speed, influence })
    }
}

// ── Dopasowanie krzywych: funkcja czasu -> kilka kluczy z uchwytami Béziera ───

/// Parametry uchwytów jednego segmentu: [prędkość wyjścia, wpływ wyjścia, prędkość wejścia, wpływ wejścia].
type FitParams = [f32; 4];

fn segment_channel(a: (f32, f32), b: (f32, f32), p: FitParams) -> Channel {
    let mut c = Channel::new("fit");
    let mut k0 = Key::linear(a.0, a.1);
    k0.right = Handle::Ease {
        speed: p[0],
        influence: p[1],
    };
    let mut k1 = Key::linear(b.0, b.1);
    k1.left = Handle::Ease {
        speed: p[2],
        influence: p[3],
    };
    c.keys = vec![k0, k1];
    c
}

/// Największy błąd segmentu względem próbek (bez alokacji: bezpośrednio `eval_segment`).
fn segment_error(pts: &[(f32, f32)], p: FitParams) -> f32 {
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let dur = b.0 - a.0;
    let out0 = (
        p[1].clamp(0.0, 1.0) * dur,
        p[0] * p[1].clamp(0.0, 1.0) * dur,
    );
    let in1 = (
        -p[3].clamp(0.0, 1.0) * dur,
        -p[2] * p[3].clamp(0.0, 1.0) * dur,
    );
    pts.iter()
        .map(|&(t, v)| (eval_segment(a, b, out0, in1, t) - v).abs())
        .fold(0.0, f32::max)
}

/// Dopasowuje jeden segment sześcienny do próbek (przeszukiwanie wzorcowe czterech parametrów: prędkości i
/// wpływu po obu stronach, start z pochodnych na końcach). Zwraca parametry i największy błąd.
fn fit_segment(pts: &[(f32, f32)], target: f32) -> (FitParams, f32) {
    let n = pts.len();
    let (a, b) = (pts[0], pts[n - 1]);
    let dur = (b.0 - a.0).max(1e-6);
    let slope = |i: usize, j: usize| (pts[j].1 - pts[i].1) / (pts[j].0 - pts[i].0).max(1e-6);
    let scale = ((b.1 - a.1) / dur)
        .abs()
        .max(slope(0, 1.min(n - 1)).abs())
        .max(slope(n.saturating_sub(2), n - 1).abs())
        .max(1e-3);
    let mut p: FitParams = [
        slope(0, 1.min(n - 1)),
        1.0 / 3.0,
        slope(n.saturating_sub(2), n - 1),
        1.0 / 3.0,
    ];
    // dobry punkt startu: prosta (prędkość odcinka, wpływ 1/3) albo pochodne końców, zależnie od tego, co mniejsze
    let linear: FitParams = [(b.1 - a.1) / dur, 1.0 / 3.0, (b.1 - a.1) / dur, 1.0 / 3.0];
    if segment_error(pts, linear) < segment_error(pts, p) {
        p = linear;
    }
    // optymalizacja na podpróbkowanych punktach (co k-ty + końce); końcowy błąd liczymy na wszystkich
    let work: Vec<(f32, f32)> = if n > 60 {
        let k = n / 40 + 1;
        pts.iter()
            .enumerate()
            .filter(|(i, _)| i % k == 0 || *i == n - 1)
            .map(|(_, p)| *p)
            .collect()
    } else {
        pts.to_vec()
    };
    let goal = target * 0.5;
    let mut best = segment_error(&work, p);
    let mut step: FitParams = [scale * 0.5, 0.15, scale * 0.5, 0.15];
    for _ in 0..80 {
        if best <= goal {
            break;
        }
        let mut improved = false;
        for i in 0..4 {
            for dir in [1.0f32, -1.0] {
                let mut q = p;
                q[i] += dir * step[i];
                if i % 2 == 1 {
                    q[i] = q[i].clamp(0.01, 1.0);
                }
                let e = segment_error(&work, q);
                if e < best - 1e-7 {
                    best = e;
                    p = q;
                    improved = true;
                }
            }
        }
        if !improved {
            for s in &mut step {
                *s *= 0.5;
            }
            if step[1] < 1e-4 {
                break;
            }
        }
    }
    let best = segment_error(pts, p);
    (p, best)
}

/// Rekurencyjnie dzieli odcinek próbek tak, by każdy segment mieścił się w `eps`; dopisuje klucze (bez
/// pierwszego, który dodaje wywołujący) z uchwytami Ease.
fn fit_run(pts: &[(f32, f32)], eps: f32, depth: u32, keys: &mut Vec<Key>) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let (p, err) = fit_segment(pts, eps);
    if err <= eps || n <= 3 || depth >= 8 {
        if let Some(last) = keys.last_mut() {
            last.right = Handle::Ease {
                speed: p[0],
                influence: p[1],
            };
        }
        let mut k = Key::linear(pts[n - 1].0, pts[n - 1].1);
        k.left = Handle::Ease {
            speed: p[2],
            influence: p[3],
        };
        keys.push(k);
        return;
    }
    // kandydaci na punkt podziału: największy błąd, największa zmiana krzywizny, środek i ćwiartki;
    // wybieramy ten, po którym obie połowy mają najmniejszy największy błąd
    let seg = segment_channel(pts[0], pts[n - 1], p);
    let argmax_err = (1..n - 1)
        .max_by(|&i, &j| {
            let ei = (seg.eval(pts[i].0).unwrap_or(0.0) - pts[i].1).abs();
            let ej = (seg.eval(pts[j].0).unwrap_or(0.0) - pts[j].1).abs();
            ei.total_cmp(&ej)
        })
        .unwrap_or(n / 2);
    let argmax_curv = (1..n - 1)
        .max_by(|&i, &j| {
            let c = |k: usize| (pts[k + 1].1 - 2.0 * pts[k].1 + pts[k - 1].1).abs();
            c(i).total_cmp(&c(j))
        })
        .unwrap_or(n / 2);
    let mut cands = vec![argmax_err, argmax_curv, n / 2, n / 4, 3 * n / 4];
    cands.iter_mut().for_each(|c| *c = (*c).clamp(1, n - 2));
    cands.sort_unstable();
    cands.dedup();
    let split = cands
        .into_iter()
        .map(|c| {
            (
                c,
                fit_segment(&pts[..=c], eps)
                    .1
                    .max(fit_segment(&pts[c..], eps).1),
            )
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(n / 2, |(c, _)| c);
    fit_run(&pts[..=split], eps, depth + 1, keys);
    fit_run(&pts[split..], eps, depth + 1, keys);
}

impl Channel {
    /// Dopasowuje funkcję `f(t)` na `[t0, t1]` do kilku kluczy z uchwytami Béziera (`Handle::Ease`), tak by
    /// błąd nie przekraczał `eps_rel` zakresu wartości (minimum 1e-4). Skoki (np. glitch) zapisuje jako
    /// `Handle::Hold`. W odróżnieniu od `bake` daje klucze, które da się sensownie edytować w edytorze krzywych:
    /// `power2.out` to dwa klucze, a elastic/bounce kilka.
    pub fn fit(prop: &str, t0: f32, t1: f32, eps_rel: f32, f: impl Fn(f32) -> f32) -> Channel {
        let mut ch = Channel::new(prop);
        if t1 <= t0 {
            ch.keys.push(Key::linear(t0, f(t0)));
            return ch;
        }
        let n = (((t1 - t0) / BAKE_DT).ceil() as usize).max(2);
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let t = if i == n {
                    t1
                } else {
                    t0 + (t1 - t0) * i as f32 / n as f32
                };
                (t, f(t))
            })
            .collect();
        let (mn, mx) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let range = mx - mn;
        let eps = (eps_rel * range).max(1e-4);
        let jump = (0.15 * range).max(1e-3);
        let mut runs = vec![];
        let mut s = 0;
        for i in 0..n {
            if (pts[i + 1].1 - pts[i].1).abs() > jump {
                runs.push((s, i));
                s = i + 1;
            }
        }
        runs.push((s, n));
        for (ri, &(a, b)) in runs.iter().enumerate() {
            let run = &pts[a..=b];
            ch.keys.push(Key::linear(run[0].0, run[0].1));
            fit_run(run, eps, 0, &mut ch.keys);
            if ri + 1 < runs.len() {
                ch.keys.last_mut().unwrap().right = Handle::Hold;
            }
        }
        // skrajne uchwyty nie mają odcinka po zewnętrznej stronie
        if let Some(k) = ch.keys.first_mut() {
            k.left = Handle::Linear;
        }
        if let Some(k) = ch.keys.last_mut() {
            k.right = Handle::Linear;
        }
        ch
    }
}
