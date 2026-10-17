//! Model Ease (prędkość / wpływ) w stylu After Effects. Formuła Béziera sprawdzana niezależną
//! implementacją wzorowaną na crates/keyframe z EffectCraft (MIT OR Apache-2.0).
use btb_motion::*;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

fn two_keys(v0: f32, v1: f32, dur: f32, out: Handle, inn: Handle) -> Channel {
    let mut c = Channel::new("x");
    let mut a = Key::linear(0.0, v0);
    a.right = out;
    let mut b = Key::linear(dur, v1);
    b.left = inn;
    c.keys = vec![a, b];
    c
}

/// Oracle: krzywa w przestrzeni (czas, wartość): P1 = (i0*dur, v0 + s0*i0*dur), P2 = (dur - i1*dur, v1 - s1*i1*dur).
#[allow(clippy::too_many_arguments)]
fn oracle(v0: f32, v1: f32, dur: f32, s0: f32, i0: f32, s1: f32, i1: f32, t: f32) -> f64 {
    let (v0, v1, dur, t) = (v0 as f64, v1 as f64, dur as f64, t as f64);
    let (s0, i0, s1, i1) = (s0 as f64, i0 as f64, s1 as f64, i1 as f64);
    let bez = |a: f64, b: f64, c: f64, d: f64, u: f64| {
        (1.0 - u).powi(3) * a
            + 3.0 * (1.0 - u).powi(2) * u * b
            + 3.0 * (1.0 - u) * u * u * c
            + u.powi(3) * d
    };
    let (x0, x1, x2, x3) = (0.0, i0 * dur, dur - i1 * dur, dur);
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..80 {
        let m = 0.5 * (lo + hi);
        if bez(x0, x1, x2, x3, m) < t {
            lo = m
        } else {
            hi = m
        }
    }
    let u = 0.5 * (lo + hi);
    bez(v0, v0 + s0 * i0 * dur, v1 - s1 * i1 * dur, v1, u)
}

#[test]
fn matches_reference_formula_on_random_segments() {
    let mut r = Lcg(42);
    for _ in 0..300 {
        let (v0, v1) = (r.range(-100.0, 100.0), r.range(-100.0, 100.0));
        let dur = r.range(0.2, 3.0);
        let (s0, s1) = (r.range(-200.0, 200.0), r.range(-200.0, 200.0));
        let (i0, i1) = (r.range(0.01, 1.0), r.range(0.01, 1.0));
        let c = two_keys(
            v0,
            v1,
            dur,
            Handle::Ease {
                speed: s0,
                influence: i0,
            },
            Handle::Ease {
                speed: s1,
                influence: i1,
            },
        );
        for k in 1..10 {
            let t = dur * k as f32 / 10.0;
            let got = c.eval(t).unwrap() as f64;
            // uchwyty przycinane do odcinka (jak w eval_segment), więc oracle liczy na przyciętych wpływach
            let want = oracle(v0, v1, dur, s0, i0.min(1.0), s1, i1.min(1.0), t);
            assert!(
                (got - want).abs() < 0.05 + want.abs() * 1e-4,
                "t={t} got {got} want {want}"
            );
        }
    }
}

#[test]
fn easy_ease_is_symmetric_and_flat_at_the_ends() {
    let c = two_keys(0.0, 100.0, 1.0, Handle::easy(), Handle::easy());
    assert!((c.eval(0.5).unwrap() - 50.0).abs() < 1e-3);
    for k in 1..20 {
        let t = k as f32 / 20.0;
        let sum = c.eval(t).unwrap() + c.eval(1.0 - t).unwrap();
        assert!((sum - 100.0).abs() < 1e-2, "symetria w t={t}");
    }
    // prędkość blisko zera na końcach i największa w środku
    assert!(c.velocity(0.02).unwrap() < 20.0, "{:?}", c.velocity(0.02));
    assert!(c.velocity(0.5).unwrap() > 140.0);
    // monotoniczny
    let mut prev = -1.0;
    for k in 0..=100 {
        let v = c.eval(k as f32 / 100.0).unwrap();
        assert!(v >= prev - 1e-4);
        prev = v;
    }
}

#[test]
fn linear_equivalent_ease_gives_a_straight_line() {
    // speed = dv/dur, influence = 1/3 odtwarza prostą (jak strona Linear)
    let c = two_keys(
        10.0,
        70.0,
        2.0,
        Handle::Ease {
            speed: 30.0,
            influence: 1.0 / 3.0,
        },
        Handle::Ease {
            speed: 30.0,
            influence: 1.0 / 3.0,
        },
    );
    for k in 0..=20 {
        let t = 2.0 * k as f32 / 20.0;
        assert!((c.eval(t).unwrap() - (10.0 + 30.0 * t)).abs() < 1e-3);
    }
    assert!((c.velocity(1.0).unwrap() - 30.0).abs() < 0.05);
}

#[test]
fn side_ease_reports_effective_values() {
    let mut c = Channel::new("x");
    c.keys = vec![
        Key::linear(0.0, 0.0),
        Key::linear(1.0, 100.0),
        Key::linear(3.0, 100.0),
    ];
    // proste: nachylenie odcinka i wpływ 1/3
    assert_eq!(
        c.side_ease(0, true),
        Some(Ease {
            speed: 100.0,
            influence: Ease::THIRD
        })
    );
    assert_eq!(
        c.side_ease(1, false),
        Some(Ease {
            speed: 100.0,
            influence: Ease::THIRD
        })
    );
    assert_eq!(
        c.side_ease(1, true),
        Some(Ease {
            speed: 0.0,
            influence: Ease::THIRD
        })
    );
    // brak sąsiada
    assert_eq!(c.side_ease(0, false), None);
    assert_eq!(c.side_ease(2, true), None);
    // Hold
    c.keys[1].right = Handle::Hold;
    assert_eq!(c.side_ease(1, true), None);
    assert_eq!(c.side_ease(2, false), None);
    // Auto: nachylenie sąsiadów, wpływ 1/6 (klucz 1 ma poziome sąsiedztwo 0..100 -> 100 po 3 s)
    c.keys[1].right = Handle::Auto;
    c.keys[1].left = Handle::Auto;
    let e = c.side_ease(1, true).unwrap();
    assert!((e.speed - 100.0 / 3.0).abs() < 1e-4 && (e.influence - 1.0 / 6.0).abs() < 1e-6);
    // Free: nachylenie i wpływ z wektora
    c.keys[0].right = Handle::Free { dt: 0.5, dv: 25.0 };
    assert_eq!(
        c.side_ease(0, true),
        Some(Ease {
            speed: 50.0,
            influence: 0.5
        })
    );
}

#[test]
fn temporal_continuous_mirrors_speed_but_keeps_influence() {
    let mut c = Channel::new("x");
    c.keys = vec![
        Key::linear(0.0, 0.0),
        Key::linear(1.0, 50.0),
        Key::linear(2.0, 100.0),
    ];
    c.keys[1].continuous = true;
    c.keys[1].left = Handle::Ease {
        speed: 10.0,
        influence: 0.2,
    };
    assert!(c.set_ease(
        1,
        true,
        Ease {
            speed: 80.0,
            influence: 0.5
        }
    ));
    assert_eq!(
        c.side_ease(1, true),
        Some(Ease {
            speed: 80.0,
            influence: 0.5
        })
    );
    assert_eq!(
        c.side_ease(1, false),
        Some(Ease {
            speed: 80.0,
            influence: 0.2
        })
    );
    // bez continuous druga strona się nie zmienia
    c.keys[1].continuous = false;
    c.set_ease(
        1,
        true,
        Ease {
            speed: 5.0,
            influence: 0.5,
        },
    );
    assert_eq!(c.side_ease(1, false).unwrap().speed, 80.0);
    // wpływ jest przycinany do 0..1, a nieistniejący klucz zwraca false
    c.set_ease(
        1,
        true,
        Ease {
            speed: 1.0,
            influence: 7.0,
        },
    );
    assert_eq!(c.side_ease(1, true).unwrap().influence, 1.0);
    assert!(!c.set_ease(9, true, Ease::EASY));
}

#[test]
fn time_reverse_mirrors_values_for_every_handle_kind() {
    let mut r = Lcg(7);
    for _ in 0..100 {
        let n = 3 + (r.next() * 3.0) as usize;
        let mut c = Channel::new("x");
        let mut t = r.range(0.0, 1.0);
        for _ in 0..n {
            t += r.range(0.2, 1.5);
            let mut k = Key::linear(t, r.range(-50.0, 50.0));
            let mk = |r: &mut Lcg| match (r.next() * 5.0) as usize {
                0 => Handle::Linear,
                1 => Handle::Smooth,
                2 => Handle::Auto,
                3 => Handle::Ease {
                    speed: r.range(-80.0, 80.0),
                    influence: r.range(0.05, 1.0),
                },
                _ => Handle::Free {
                    dt: r.range(0.05, 0.4),
                    dv: r.range(-30.0, 30.0),
                },
            };
            k.right = mk(&mut r);
            k.left = match mk(&mut r) {
                Handle::Free { dt, dv } => Handle::Free { dt: -dt, dv },
                h => h,
            };
            c.keys.push(k);
        }
        let (first, last) = (c.keys[0].t, c.keys[n - 1].t);
        let mut rev = c.clone();
        rev.time_reverse();
        assert!(rev.keys.windows(2).all(|w| w[0].t < w[1].t));
        for k in 0..=40 {
            let t = first + (last - first) * k as f32 / 40.0;
            let (a, b) = (c.eval(t).unwrap(), rev.eval(first + last - t).unwrap());
            assert!((a - b).abs() < 0.02 + a.abs() * 1e-4, "t={t} {a} vs {b}");
        }
        // dwukrotne odwrócenie wraca do oryginału
        let mut twice = rev.clone();
        twice.time_reverse();
        for (x, y) in twice.keys.iter().zip(&c.keys) {
            assert!((x.t - y.t).abs() < 1e-4 && (x.v - y.v).abs() < 1e-6);
        }
    }
}

#[test]
fn auto_bezier_passes_straight_through_collinear_keys_and_flattens_peaks() {
    let mut c = Channel::new("x");
    c.keys = vec![
        Key::linear(0.0, 0.0),
        Key::linear(1.0, 50.0),
        Key::linear(2.0, 100.0),
    ];
    c.keys[1].left = Handle::Auto;
    c.keys[1].right = Handle::Auto;
    // współliniowe: ruch w środku jest prawie liniowy
    assert!((c.eval(1.0).unwrap() - 50.0).abs() < 1e-3);
    assert!((c.velocity(1.0).unwrap() - 50.0).abs() < 0.5);
    // szczyt: prędkość w kluczu 0
    c.keys[2].v = 0.0;
    assert!(c.velocity(1.0).unwrap().abs() < 0.5);
    assert!(
        c.eval(1.0).unwrap() <= 50.0 + 1e-3,
        "bez przestrzelenia ponad szczyt"
    );
}

#[test]
fn handle_json_roundtrip_and_old_data_still_parses() {
    let h = Handle::Ease {
        speed: 12.5,
        influence: 0.25,
    };
    let j = serde_json::to_string(&h).unwrap();
    assert_eq!(serde_json::from_str::<Handle>(&j).unwrap(), h);
    assert_eq!(
        serde_json::from_str::<Handle>("{\"kind\":\"auto\"}").unwrap(),
        Handle::Auto
    );
    // stare dane (bez pola continuous i bez nowych uchwytów)
    let k: Key =
        serde_json::from_str("{\"t\":1.0,\"v\":2.0,\"left\":{\"kind\":\"linear\"}}").unwrap();
    assert!(!k.continuous && k.left == Handle::Linear);
}

#[test]
fn handle_point_and_set_handle_point_are_inverse() {
    let mut r = Lcg(99);
    for _ in 0..200 {
        let (v0, v1) = (r.range(-100.0, 100.0), r.range(-100.0, 100.0));
        let dur = r.range(0.3, 3.0);
        let e0 = Ease {
            speed: r.range(-150.0, 150.0),
            influence: r.range(0.05, 1.0),
        };
        let e1 = Ease {
            speed: r.range(-150.0, 150.0),
            influence: r.range(0.05, 1.0),
        };
        let mut c = two_keys(v0, v1, dur, Handle::from_ease(e0), Handle::from_ease(e1));
        for (i, out, e) in [(0, true, e0), (1, false, e1)] {
            let (t, v) = c.handle_point(i, out).unwrap();
            let mut copy = c.clone();
            assert!(copy.set_handle_point(i, out, t, v, false));
            let got = copy.side_ease(i, out).unwrap();
            assert!(
                (got.speed - e.speed).abs() < 0.05 && (got.influence - e.influence).abs() < 1e-4,
                "{got:?} vs {e:?}"
            );
        }
        // przesunięcie punktu uchwytu zmienia wpływ i prędkość zgodnie z geometrią
        let (t0, v0h) = c.handle_point(0, true).unwrap();
        let (tn, vn) = (t0 + 0.1 * dur, v0h + 3.0);
        c.set_handle_point(0, true, tn, vn, false);
        let (t2, v2) = c.handle_point(0, true).unwrap();
        if tn <= dur {
            assert!((t2 - tn).abs() < 1e-3 && (v2 - vn).abs() < 0.05);
        }
    }
}

#[test]
fn handle_drag_clamps_blocks_axis_and_respects_hold() {
    let mut c = two_keys(0.0, 100.0, 2.0, Handle::easy(), Handle::easy());
    // za daleko w prawo: wpływ przycięty do 100% odcinka
    c.set_handle_point(0, true, 50.0, 10.0, false);
    assert_eq!(c.side_ease(0, true).unwrap().influence, 1.0);
    // Shift (keep_influence): zmienia tylko prędkość
    let before = c.side_ease(0, true).unwrap().influence;
    c.set_handle_point(0, true, 0.3, 40.0, true);
    let e = c.side_ease(0, true).unwrap();
    assert_eq!(e.influence, before);
    assert!((e.speed - 40.0 / (before * 2.0)).abs() < 1e-3);
    // uchwyt wejścia Easy Ease leży poziomo od klucza (prędkość 0); z dodatnią prędkością schodzi poniżej wartości
    let (t, v) = c.handle_point(1, false).unwrap();
    assert!(t < 2.0 && (v - 100.0).abs() < 1e-4);
    c.set_ease(
        1,
        false,
        Ease {
            speed: 30.0,
            influence: 0.5,
        },
    );
    let (t, v) = c.handle_point(1, false).unwrap();
    assert!((t - 1.0).abs() < 1e-4 && (v - 70.0).abs() < 1e-3);
    // hold i strona bez sąsiada nie mają uchwytu
    c.keys[0].right = Handle::Hold;
    assert!(c.handle_point(0, true).is_none() && !c.set_handle_point(0, true, 1.0, 1.0, false));
    assert!(c.handle_point(0, false).is_none());
}

#[test]
fn handle_drag_on_continuous_key_mirrors_speed() {
    let mut c = Channel::new("x");
    c.keys = vec![
        Key::linear(0.0, 0.0),
        Key::linear(1.0, 50.0),
        Key::linear(2.0, 100.0),
    ];
    c.keys[1].continuous = true;
    c.set_handle_point(1, true, 1.3, 50.0 + 60.0 * 0.3, false);
    assert!((c.side_ease(1, true).unwrap().speed - 60.0).abs() < 0.05);
    assert!(
        (c.side_ease(1, false).unwrap().speed - 60.0).abs() < 0.05,
        "ciągły: ta sama prędkość z drugiej strony"
    );
}
