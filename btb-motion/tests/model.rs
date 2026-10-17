use btb_motion::*;

fn ease_out() -> (f32, f32, f32, f32) {
    (0.0, 0.0, 0.58, 1.0)
}

/// IN (0..0.5) -> Stop -> OUT (2.0..2.3), jak element belki z presetów IN/OUT.
fn lower_third() -> Director {
    let mut d = Director::new("Default", 2.3);
    d.actors.push(Actor {
        target: "l_bar".into(),
        offset: 0.0,
        active_from: None,
        active_until: None,
        paths: vec![],
        channels: vec![{
            // wejście i wyjście w jednym kanale
            let mut c = Channel::tween("opacity", 0.0, 1.0, 0.0, 0.5, ease_out());
            c.keys.push(Key::linear(2.0, 1.0));
            c.keys.push(Key::linear(2.3, 0.0));
            c
        }],
    });
    d.events.push(Event {
        name: "hold".into(),
        t: 0.5,
        kind: EventKind::Stop,
        dir: Dir::Forward,
    });
    d.events.push(Event {
        name: "pilot1".into(),
        t: 0.25,
        kind: EventKind::Tag,
        dir: Dir::Both,
    });
    d
}

#[test]
fn tween_matches_css_ease_endpoints_and_is_monotonic() {
    let c = Channel::tween("x", 0.0, 100.0, 0.0, 1.0, (0.25, 0.1, 0.25, 1.0));
    assert_eq!(c.eval(-1.0), Some(0.0));
    assert!((c.eval(1.0).unwrap() - 100.0).abs() < 1e-3);
    let mut prev = -1.0;
    for i in 0..=100 {
        let v = c.eval(i as f32 / 100.0).unwrap();
        assert!(v >= prev - 1e-4, "niemonotoniczne w {i}");
        prev = v;
    }
}

#[test]
fn loop_and_swing() {
    let mut c = Channel::new("x");
    c.keys = vec![Key::linear(0.0, 0.0), Key::linear(1.0, 10.0)];
    c.post = Extend::Loop { count: None };
    assert!((c.eval(1.25).unwrap() - 2.5).abs() < 1e-3);
    c.post = Extend::Swing { count: None };
    assert!((c.eval(1.25).unwrap() - 7.5).abs() < 1e-3);
    c.post = Extend::Swing { count: Some(1) }; // jedno dodatkowe wahnięcie, potem trzyma 0
    assert!((c.eval(5.0).unwrap() - 0.0).abs() < 1e-3);
    c.post = Extend::Loop { count: Some(1) };
    assert!((c.eval(5.0).unwrap() - 10.0).abs() < 1e-3);
    c.pre = Extend::Loop { count: None };
    assert!((c.eval(-0.25).unwrap() - 7.5).abs() < 1e-3);
}

#[test]
fn stepped_channel_holds_until_next_key() {
    let mut c = Channel::new("n");
    c.keys = vec![Key::linear(0.0, 1.0), Key::linear(1.0, 2.0)];
    c.stepped = true;
    assert_eq!(c.eval(0.99), Some(1.0));
    assert_eq!(c.eval(1.0), Some(2.0));
}

#[test]
fn transport_stops_at_event_then_continues_to_out() {
    let d = lower_third();
    let mut tr = Transport::default();
    tr.start();
    let mut all = vec![];
    for _ in 0..100 {
        all.extend(tr.advance(&d, 0.1));
    }
    // Tag minął, Stop zatrzymał dokładnie na 0.5, bez End
    assert_eq!(
        all,
        vec![Fired::Tag("pilot1".into()), Fired::Stopped("hold".into())]
    );
    assert!((tr.t - 0.5).abs() < 1e-6 && !tr.playing);
    let mut s = vec![];
    d.sample(tr.t, &mut s);
    assert!((s[0].value - 1.0).abs() < 1e-3);
    // Continue -> OUT do końca
    tr.resume();
    let mut end = vec![];
    for _ in 0..100 {
        end.extend(tr.advance(&d, 0.1));
    }
    assert_eq!(end, vec![Fired::End]);
    let mut s = vec![];
    d.sample(tr.t, &mut s);
    assert!(s[0].value.abs() < 1e-3);
}

#[test]
fn reverse_ignores_forward_only_stop() {
    let d = lower_third();
    let mut tr = Transport::default();
    tr.start_reversed_from(2.3);
    let fired: Vec<_> = (0..30).flat_map(|_| tr.advance(&d, 0.1)).collect();
    assert!(!fired.iter().any(|f| matches!(f, Fired::Stopped(_))));
    assert_eq!(fired.last(), Some(&Fired::End));
}

#[test]
fn pause_resumes_by_itself() {
    let mut d = Director::new("d", 2.0);
    d.events.push(Event {
        name: "p".into(),
        t: 1.0,
        kind: EventKind::Pause { len: 0.5 },
        dir: Dir::Both,
    });
    let mut tr = Transport::default();
    tr.start();
    assert_eq!(tr.advance(&d, 1.2), vec![Fired::Paused("p".into())]);
    assert!(tr.playing && (tr.t - 1.0).abs() < 1e-6);
    tr.advance(&d, 0.3);
    assert!((tr.t - 1.0).abs() < 1e-6); // nadal pauza
    tr.advance(&d, 0.4); // 0.2 dokończa pauzę, 0.2 idzie dalej
    assert!((tr.t - 1.2).abs() < 1e-4);
}

#[test]
fn sub_director_offset_and_json_roundtrip() {
    let mut child = Director::new("child", 1.0);
    child.offset = 1.0;
    child.actors.push(Actor {
        target: "e".into(),
        offset: 0.0,
        active_from: None,
        active_until: None,
        paths: vec![],
        channels: vec![{
            let mut c = Channel::new("x");
            c.keys = vec![Key::linear(0.0, 0.0), Key::linear(1.0, 10.0)];
            c
        }],
    });
    let mut root = Director::new("root", 2.0);
    root.children.push(child);
    let mut s = vec![];
    root.sample(1.5, &mut s);
    assert!((s[0].value - 5.0).abs() < 1e-3);
    let json = serde_json::to_string(&root).unwrap();
    let back: Director = serde_json::from_str(&json).unwrap();
    let mut s2 = vec![];
    back.sample(1.5, &mut s2);
    assert_eq!(s, s2);
}

// ── Faza 1: Hold, bake, zegar swobodny ──────────────────────────────────────

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }
}

#[test]
fn hold_handle_jumps_at_next_key() {
    let mut c = Channel::new("x");
    c.keys = vec![Key::linear(0.0, 1.0), Key::linear(1.0, 5.0)];
    c.keys[0].right = Handle::Hold;
    assert_eq!(c.eval(0.999), Some(1.0));
    assert_eq!(c.eval(1.0), Some(5.0));
}

#[test]
fn bake_smooth_function_is_close_and_compact() {
    let f = |t: f32| (t * 3.0).sin() * 10.0;
    let c = Channel::bake("x", 0.0, 1.5, 0.002, f);
    assert!(c.keys.len() < 80, "za dużo kluczy: {}", c.keys.len());
    let mut rng = Lcg(7);
    for _ in 0..500 {
        let t = rng.next() * 1.5;
        assert!((c.eval(t).unwrap() - f(t)).abs() < 0.05, "t={t}");
    }
}

#[test]
fn bake_keeps_discontinuities() {
    // schodki jak glitch: 7 slotów na [0,1]
    let steps = [0.0f32, 1.0, 0.2, 1.0, 0.4, 0.4, 1.0];
    let f = |t: f32| steps[((t * 7.0).floor() as usize).min(6)];
    let c = Channel::bake("opacity", 0.0, 1.0, 0.002, f);
    let mut rng = Lcg(11);
    for _ in 0..500 {
        let t = rng.next();
        // pomijamy otoczenie skoku (dokładność położenia skoku to krok próbkowania)
        let edge = (t * 7.0 - (t * 7.0).round()).abs() / 7.0;
        if edge < 1.0 / 200.0 {
            continue;
        }
        assert!((c.eval(t).unwrap() - f(t)).abs() < 1e-3, "t={t}");
    }
}

#[test]
fn free_director_follows_clock_only_while_holding() {
    let mut root = Director::new("root", 1.0);
    root.events.push(Event {
        name: "hold".into(),
        t: 1.0,
        kind: EventKind::Stop,
        dir: Dir::Forward,
    });
    let mut lp = Director::new("loop", 1.0);
    lp.free = true;
    lp.actors.push(Actor {
        target: "e".into(),
        offset: 0.0,
        active_from: None,
        active_until: None,
        paths: vec![],
        channels: vec![{
            let mut c = Channel::new("y");
            c.keys = vec![Key::linear(0.0, 0.0), Key::linear(1.0, 10.0)];
            c.post = Extend::Loop { count: None };
            c
        }],
    });
    root.children.push(lp);

    let mut tr = Transport::default();
    tr.start();
    let mut s = vec![];
    tr.sample(&root, &mut s);
    assert!(s.is_empty(), "pętla nie powinna działać przed Stop");
    tr.advance(&root, 1.5); // dochodzi do Stop na 1.0
    assert!(tr.holding());
    tr.advance(&root, 0.25);
    tr.advance(&root, 1.0); // clock = 1.25 -> pętla: 2.5
    let mut s = vec![];
    tr.sample(&root, &mut s);
    assert!((s[0].value - 2.5).abs() < 1e-3, "{:?}", s);
    tr.resume();
    let mut s = vec![];
    tr.sample(&root, &mut s);
    assert!(s.is_empty(), "po Continue pętla znika");
}

#[test]
fn actor_window_and_sample_free() {
    let mut d = Director::new("d", 2.0);
    let mut c = Channel::new("x");
    c.keys = vec![Key::linear(0.0, 1.0), Key::linear(1.0, 1.0)];
    d.actors.push(Actor {
        target: "a".into(),
        offset: 0.0,
        active_from: Some(0.5),
        active_until: Some(1.5),
        paths: vec![],
        channels: vec![c],
    });
    let at = |t: f32| {
        let mut s = vec![];
        d.sample(t, &mut s);
        s.len()
    };
    assert_eq!((at(0.49), at(0.5), at(1.49), at(1.5)), (0, 1, 1, 0));
    let mut s = vec![];
    d.sample_free(3.0, &mut s);
    assert!(s.is_empty());
}
