//! Kanał pozycji ze ścieżką ruchu (motion path) w stylu After Effects.
//!
//! Przestrzennie ścieżka składa się z segmentów Béziera między kluczami (tangenty ręczne albo auto-Bézier).
//! Czasowo ruch odbywa się po długości łuku: wewnętrznie budujemy zwykły `Channel` odległości (wartość
//! klucza = długość ścieżki od startu), więc cały model uchwytów czasowych (Linear, Ease, Hold) działa
//! bez zmian: Linear daje stałą prędkość wzdłuż ścieżki, Easy Ease zwalnia przy kluczu.
//! Pomysł i semantyka (tangenty auto = 1/6 odległości sąsiadów, roving) wg crates/keyframe z EffectCraft
//! (MIT OR Apache-2.0); implementacja własna, 2D w `f32`.
use crate::channel::{Channel, Key};
use crate::curve::Handle;
use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathKey {
    pub t: f32,
    pub x: f32,
    pub y: f32,
    /// Tangenty przestrzenne względem klucza (używane, gdy `auto` = false).
    #[serde(default)]
    pub tin: [f32; 2],
    #[serde(default)]
    pub tout: [f32; 2],
    /// Auto-Bézier: tangenty z sąsiadów, bez wpływu czasu kluczy.
    #[serde(default = "yes")]
    pub auto: bool,
    /// Roving: czas klucza wyliczany tak, by prędkość po ścieżce była stała (nigdy pierwszy ani ostatni).
    #[serde(default)]
    pub roving: bool,
    /// Uchwyty czasowe (prędkość wzdłuż ścieżki, jednostki/s), jak w `Key`.
    #[serde(default = "lin")]
    pub left: Handle,
    #[serde(default = "lin")]
    pub right: Handle,
    #[serde(default)]
    pub continuous: bool,
}

fn lin() -> Handle {
    Handle::Linear
}

impl PathKey {
    pub fn new(t: f32, x: f32, y: f32) -> Self {
        Self {
            t,
            x,
            y,
            tin: [0.0; 2],
            tout: [0.0; 2],
            auto: true,
            roving: false,
            left: Handle::Linear,
            right: Handle::Linear,
            continuous: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathChannel {
    pub keys: Vec<PathKey>,
}

const ARC_N: usize = 64;
/// Węzły i wagi kwadratury Gaussa-Legendre'a (5 punktów) na przedziale -1..1.
const GAUSS: [(f32, f32); 5] = [
    (0.0, 0.568_888_9),
    (-0.538_469_3, 0.478_628_67),
    (0.538_469_3, 0.478_628_67),
    (-0.906_179_85, 0.236_926_89),
    (0.906_179_85, 0.236_926_89),
];

fn bez(p: [f32; 4], u: f32) -> f32 {
    let v = 1.0 - u;
    v * v * v * p[0] + 3.0 * v * v * u * p[1] + 3.0 * v * u * u * p[2] + u * u * u * p[3]
}
fn bez_d(p: [f32; 4], u: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * (p[1] - p[0]) + 6.0 * v * u * (p[2] - p[1]) + 3.0 * u * u * (p[3] - p[2])
}

/// Segment sześcienny Béziera z tablicą długości łuku.
struct Seg {
    x: [f32; 4],
    y: [f32; 4],
    table: Vec<f32>,
}

impl Seg {
    fn new(p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], p3: [f32; 2]) -> Seg {
        let (x, y) = ([p0[0], c1[0], c2[0], p3[0]], [p0[1], c1[1], c2[1], p3[1]]);
        let mut s = Seg {
            x,
            y,
            table: Vec::with_capacity(ARC_N + 1),
        };
        s.table.push(0.0);
        let mut acc = 0.0;
        for i in 1..=ARC_N {
            acc += s.arc((i - 1) as f32 / ARC_N as f32, i as f32 / ARC_N as f32);
            s.table.push(acc);
        }
        s
    }
    fn speed_u(&self, u: f32) -> f32 {
        (bez_d(self.x, u).powi(2) + bez_d(self.y, u).powi(2)).sqrt()
    }
    fn arc(&self, a: f32, b: f32) -> f32 {
        let (m, h) = ((a + b) * 0.5, (b - a) * 0.5);
        GAUSS
            .iter()
            .map(|(n, w)| w * self.speed_u(m + h * n))
            .sum::<f32>()
            * h
    }
    fn length(&self) -> f32 {
        self.table.last().copied().unwrap_or(0.0)
    }
    fn point(&self, u: f32) -> [f32; 2] {
        [bez(self.x, u), bez(self.y, u)]
    }
    /// Punkt na długości łuku `s`: zgrubne `u` z tablicy, dopracowane krokami Newtona na prawdziwej długości.
    fn at_length(&self, s: f32) -> [f32; 2] {
        let len = self.length();
        if !(len > 0.0 && len.is_finite()) {
            return self.point(0.0);
        }
        let s = s.clamp(0.0, len);
        let i = self.table.partition_point(|&v| v < s).clamp(1, ARC_N);
        let (a, b) = (self.table[i - 1], self.table[i]);
        let (u0, u1) = ((i - 1) as f32 / ARC_N as f32, i as f32 / ARC_N as f32);
        let mut u = u0 + if b > a { (s - a) / (b - a) } else { 0.0 } * (u1 - u0);
        for _ in 0..4 {
            let d = self.speed_u(u);
            if d <= 1e-9 {
                break;
            }
            let e = a + self.arc(u0, u) - s;
            u = (u - e / d).clamp(u0, u1);
            if e.abs() <= 1e-6 * len.max(1.0) {
                break;
            }
        }
        self.point(u)
    }
}

impl PathChannel {
    /// Efektywne tangenty przestrzenne klucza `i`: auto-Bézier (jedna szósta drogi od poprzedniego do
    /// następnego klucza; klucz skrajny używa jedynego sąsiada, długość ograniczona jak niżej) albo zapisane.
    pub fn tangents(&self, i: usize) -> ([f32; 2], [f32; 2]) {
        let Some(k) = self.keys.get(i) else {
            return ([0.0; 2], [0.0; 2]);
        };
        if !k.auto {
            return (k.tin, k.tout);
        }
        if self.keys.len() < 2 {
            return ([0.0; 2], [0.0; 2]);
        }
        let a = &self.keys[i.saturating_sub(1)];
        let c = &self.keys[(i + 1).min(self.keys.len() - 1)];
        let base = [(c.x - a.x) / 6.0, (c.y - a.y) / 6.0];
        let base_len = (base[0] * base[0] + base[1] * base[1]).sqrt();
        // Adaptacja względem AE: tangent nie dłuższy niż połowa odległości do sąsiada po swojej stronie.
        // Dla równych odstępów nic się nie zmienia, a przy nierównych (np. klucze współliniowe 30 i 270 px)
        // ścieżka nie cofa się i nie robi pętli.
        let dist =
            |j: usize| ((self.keys[j].x - k.x).powi(2) + (self.keys[j].y - k.y).powi(2)).sqrt();
        let cap = |side: Option<usize>| side.map_or(f32::MAX, |j| 0.5 * dist(j));
        let scale = |cap: f32| {
            if base_len > cap && base_len > 1e-9 {
                cap / base_len
            } else {
                1.0
            }
        };
        let (prev, next) = (i.checked_sub(1), (i + 1 < self.keys.len()).then_some(i + 1));
        let (sout, sin) = (scale(cap(next)), scale(cap(prev)));
        (
            [-base[0] * sin, -base[1] * sin],
            [base[0] * sout, base[1] * sout],
        )
    }

    fn seg(&self, i: usize) -> Option<Seg> {
        let (a, b) = (self.keys.get(i)?, self.keys.get(i + 1)?);
        let (_, out_t) = self.tangents(i);
        let (in_t, _) = self.tangents(i + 1);
        Some(Seg::new(
            [a.x, a.y],
            [a.x + out_t[0], a.y + out_t[1]],
            [b.x + in_t[0], b.y + in_t[1]],
            [b.x, b.y],
        ))
    }

    /// Długość łuku segmentu `i -> i+1`.
    pub fn segment_length(&self, i: usize) -> f32 {
        self.seg(i).map_or(0.0, |s| s.length())
    }

    /// Całkowita długość ścieżki.
    pub fn length(&self) -> f32 {
        (0..self.keys.len().saturating_sub(1))
            .map(|i| self.segment_length(i))
            .sum()
    }

    /// Segmenty i kanał odległości (wartość klucza = długość ścieżki od startu, uchwyty czasowe z kluczy).
    fn build(&self) -> (Vec<Seg>, Channel) {
        let segs: Vec<Seg> = (0..self.keys.len().saturating_sub(1))
            .filter_map(|i| self.seg(i))
            .collect();
        let mut ch = Channel::new("path");
        let mut acc = 0.0;
        for (i, k) in self.keys.iter().enumerate() {
            if i > 0 {
                acc += segs.get(i - 1).map_or(0.0, |s| s.length());
            }
            ch.keys.push(Key {
                t: k.t,
                v: acc,
                left: k.left,
                right: k.right,
                continuous: k.continuous,
            });
        }
        (segs, ch)
    }

    /// Kanał odległości (wartość klucza = długość ścieżki od startu).
    pub fn distance_channel(&self) -> Channel {
        self.build().1
    }

    /// Pozycja w czasie `t`.
    pub fn eval(&self, t: f32) -> Option<(f32, f32)> {
        let first = self.keys.first()?;
        if self.keys.len() == 1 {
            return Some((first.x, first.y));
        }
        let (segs, dist) = self.build();
        let s = dist.eval(t)?;
        // segment zawierający odległość s (klucze po prawej mają większą skumulowaną długość)
        let i = dist
            .keys
            .partition_point(|k| k.v <= s)
            .saturating_sub(1)
            .min(segs.len() - 1);
        let p = segs[i].at_length(s - dist.keys[i].v);
        Some((p[0], p[1]))
    }

    /// Prędkość ruchu po ścieżce (jednostki/s) w czasie `t`.
    pub fn speed(&self, t: f32) -> Option<f32> {
        self.distance_channel().velocity(t).map(f32::abs)
    }

    /// Wstawia klucz w czasie `t` (zastępuje klucz w tym samym czasie); nowy klucz leży na ścieżce,
    /// jeśli podano `None`. Zwraca indeks.
    pub fn insert_key(&mut self, t: f32, pos: Option<(f32, f32)>) -> usize {
        let (x, y) = pos.or_else(|| self.eval(t)).unwrap_or((0.0, 0.0));
        if let Some(i) = self.keys.iter().position(|k| (k.t - t).abs() < 1e-4) {
            self.keys[i].x = x;
            self.keys[i].y = y;
            return i;
        }
        let i = self.keys.partition_point(|k| k.t < t);
        self.keys.insert(i, PathKey::new(t, x, y));
        i
    }

    /// Usuwa klucze o podanych indeksach.
    pub fn remove_keys(&mut self, idx: &[usize]) {
        let mut i = 0;
        self.keys.retain(|_| {
            let keep = !idx.contains(&i);
            i += 1;
            keep
        });
    }

    /// Przesuwa zaznaczone klucze w czasie (jak `Channel::move_keys`, z migawki); zwraca nowy kanał i indeksy.
    pub fn move_keys(&self, sel: &[usize], dt: f32) -> (PathChannel, Vec<usize>) {
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
        let mut keys: Vec<(PathKey, bool)> = self
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
        let moved_t: Vec<f32> = keys.iter().filter(|(_, m)| *m).map(|(k, _)| k.t).collect();
        keys.retain(|(k, m)| *m || !moved_t.iter().any(|t| (t - k.t).abs() < 1e-4));
        keys.sort_by(|a, b| a.0.t.total_cmp(&b.0.t));
        let new_sel = keys
            .iter()
            .enumerate()
            .filter(|(_, (_, m))| *m)
            .map(|(i, _)| i)
            .collect();
        (
            PathChannel {
                keys: keys.into_iter().map(|(k, _)| k).collect(),
            },
            new_sel,
        )
    }

    /// Roving ("Rove Across Time"): klucze z `roving` między dwoma stałymi dostają czasy proporcjonalne do
    /// długości ścieżki, więc ruch przez nie ma stałą prędkość. Pierwszy i ostatni klucz nigdy nie rovują.
    /// Zwraca `true`, gdy jakiś czas się zmienił.
    pub fn retime_roving(&mut self) -> bool {
        let n = self.keys.len();
        if n < 3 {
            for k in &mut self.keys {
                k.roving = false;
            }
            return false;
        }
        self.keys[0].roving = false;
        self.keys[n - 1].roving = false;
        if !self.keys.iter().any(|k| k.roving) {
            return false;
        }
        let mut cum = vec![0.0f32];
        for i in 0..n - 1 {
            cum.push(cum[i] + self.segment_length(i));
        }
        let mut changed = false;
        let mut a = 0;
        while a < n - 1 {
            let Some(b) = (a + 1..n).find(|&j| !self.keys[j].roving) else {
                break;
            };
            if b > a + 1 {
                let total = cum[b] - cum[a];
                let (t0, t1) = (self.keys[a].t, self.keys[b].t);
                for i in a + 1..b {
                    let f = if total > 1e-6 {
                        (cum[i] - cum[a]) / total
                    } else {
                        (i - a) as f32 / (b - a) as f32
                    };
                    // klucz zostaje ściśle między sąsiadami
                    let nt = (t0 + (t1 - t0) * f)
                        .clamp(self.keys[i - 1].t + 1e-3, t1 - (b - i) as f32 * 1e-3);
                    if (nt - self.keys[i].t).abs() > 1e-6 {
                        self.keys[i].t = nt;
                        changed = true;
                    }
                    // rovujące klucze przepuszczają prędkość
                    self.keys[i].left = Handle::Linear;
                    self.keys[i].right = Handle::Linear;
                }
            }
            a = b;
        }
        changed
    }

    /// Punkty kontrolne Béziera segmentu `i -> i+1` (P0, C1, C2, P3), do rysowania ścieżki.
    pub fn control_points(&self, i: usize) -> Option<[[f32; 2]; 4]> {
        let (a, b) = (self.keys.get(i)?, self.keys.get(i + 1)?);
        let (_, out_t) = self.tangents(i);
        let (in_t, _) = self.tangents(i + 1);
        Some([
            [a.x, a.y],
            [a.x + out_t[0], a.y + out_t[1]],
            [b.x + in_t[0], b.y + in_t[1]],
            [b.x, b.y],
        ])
    }

    /// Punkty krzywej (w parametrze Béziera, `per_seg` próbek na segment) do narysowania ścieżki.
    pub fn polyline(&self, per_seg: usize) -> Vec<[f32; 2]> {
        let n = per_seg.max(2);
        let mut out = vec![];
        for i in 0..self.keys.len().saturating_sub(1) {
            let Some(p) = self.control_points(i) else {
                continue;
            };
            for k in 0..=n {
                if k == 0 && i > 0 {
                    continue;
                }
                let u = k as f32 / n as f32;
                out.push([
                    bez([p[0][0], p[1][0], p[2][0], p[3][0]], u),
                    bez([p[0][1], p[1][1], p[2][1], p[3][1]], u),
                ]);
            }
        }
        if out.is_empty() {
            out.extend(self.keys.first().map(|k| [k.x, k.y]));
        }
        out
    }

    /// Przepisuje uchwyty czasowe (prędkość / wpływ) z kanału odległości z powrotem do kluczy ścieżki
    /// (edytor krzywych pracuje na kanale odległości).
    pub fn apply_distance_handles(&mut self, dist: &Channel) {
        for (k, d) in self.keys.iter_mut().zip(&dist.keys) {
            k.left = d.left;
            k.right = d.right;
            k.continuous = d.continuous;
        }
    }

    /// Ustawia tangent przestrzenny klucza `i` tak, by uchwyt leżał w `(x, y)`. Wyłącza auto-Béziera
    /// (zachowując drugi tangent bez zmian); `mirror` ustawia drugi uchwyt symetrycznie (gładki węzeł).
    pub fn set_tangent(&mut self, i: usize, out: bool, x: f32, y: f32, mirror: bool) -> bool {
        let (tin, tout) = self.tangents(i);
        let Some(k) = self.keys.get_mut(i) else {
            return false;
        };
        let v = [x - k.x, y - k.y];
        k.auto = false;
        k.tin = tin;
        k.tout = tout;
        if out {
            k.tout = v;
            if mirror {
                k.tin = [-v[0], -v[1]];
            }
        } else {
            k.tin = v;
            if mirror {
                k.tout = [-v[0], -v[1]];
            }
        }
        true
    }
}
