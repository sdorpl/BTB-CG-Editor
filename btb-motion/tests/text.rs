use btb_motion::text::*;

fn sp(effect: &str) -> TextSpec {
    TextSpec {
        effect: effect.into(),
        duration: 0.5,
        delay: 0.0,
        stagger: 0.0,
        from: "start".into(),
        ease: "linear".into(),
    }
}
fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

#[test]
fn stagger_delays_match_gsap_distribute() {
    // gsap.utils.distribute({each: 0.1, from}) dla n = 5 i 4 (wartości policzone z wzoru z gsap-core.js)
    assert!(close(
        &stagger_delays(5, 0.1, "start"),
        &[0.0, 0.1, 0.2, 0.3, 0.4]
    ));
    assert!(close(
        &stagger_delays(5, 0.1, "end"),
        &[0.4, 0.3, 0.2, 0.1, 0.0]
    ));
    assert!(close(
        &stagger_delays(5, 0.1, "center"),
        &[0.4, 0.2, 0.0, 0.2, 0.4]
    ));
    assert!(close(
        &stagger_delays(5, 0.1, "edges"),
        &[0.0, 0.2, 0.4, 0.2, 0.0]
    ));
    assert!(close(
        &stagger_delays(4, 0.1, "center"),
        &[0.3, 0.0, 0.0, 0.3]
    ));
    assert!(close(&stagger_delays(1, 0.1, "center"), &[0.0]));
}

#[test]
fn split_stagger_is_capped_by_unit_cap() {
    // chars: 0,04 s, ale limit 1,2/cap: przy 60 znakach each = 0,02
    let u = Units {
        n: 60,
        by: "chars".into(),
        lines: 1,
        ..Default::default()
    };
    let t = total(&sp("chars-fade"), &u);
    assert!((t - (0.5 + 0.02 * 59.0)).abs() < 1e-4, "{t}");
    // lines: n = słowa, limit liczy linie
    let u = Units {
        n: 7,
        by: "words".into(),
        lines: 2,
        ..Default::default()
    };
    assert!((total(&sp("lines-up"), &u) - (0.5 + 0.09 * 6.0)).abs() < 1e-4);
}

#[test]
fn in_and_out_endpoints() {
    let u = Units::default();
    // slide-up IN: start y=50, opacity 0; koniec tożsamość
    let a = eval(Mode::In, &sp("slide-up"), &u, 0, 0.0);
    assert!((a.y - 50.0).abs() < 1e-4 && a.opacity == 0.0);
    let b = eval(Mode::In, &sp("slide-up"), &u, 0, 1.0);
    assert_eq!(b, Props::default());
    // OUT startuje od tożsamości, kończy na y=-50
    assert_eq!(
        eval(Mode::Out, &sp("slide-up"), &u, 0, 0.0),
        Props::default()
    );
    let e = eval(Mode::Out, &sp("slide-up"), &u, 0, 1.0);
    assert!((e.y + 50.0).abs() < 1e-4 && e.opacity == 0.0);
}

#[test]
fn clip_blur_and_reveal() {
    let u = Units::default();
    let mid = |m, e: &str| eval(m, &sp(e), &u, 0, 0.25); // linear: połowa
    assert_eq!(mid(Mode::In, "reveal").clip, [0.0, 0.0, 0.5, 1.0]);
    assert_eq!(mid(Mode::In, "clip-up").clip, [0.0, 0.5, 1.0, 1.0]);
    assert_eq!(mid(Mode::Out, "hide").clip, [0.5, 0.0, 1.0, 1.0]);
    assert_eq!(mid(Mode::Out, "clip-down").clip, [0.0, 0.5, 1.0, 1.0]);
    assert_eq!(mid(Mode::Out, "chars-clip").clip, [0.0, 0.0, 1.0, 0.5]);
    assert!((mid(Mode::In, "blur").blur - 10.0).abs() < 1e-4);
    assert!((mid(Mode::In, "chars-blur").blur - 7.0).abs() < 1e-4);
}

#[test]
fn content_effects_fall_back_to_fade() {
    let tree = Units {
        plain: false,
        ..Default::default()
    };
    let p = eval(Mode::In, &sp("scramble"), &tree, 0, 0.25);
    assert!((p.opacity - 0.5).abs() < 1e-4 && p.progress == 1.0);
    let no_num = Units {
        num: false,
        ..Default::default()
    };
    assert!((eval(Mode::In, &sp("counter"), &no_num, 0, 0.25).opacity - 0.5).abs() < 1e-4);
    let ok = eval(Mode::In, &sp("counter"), &Units::default(), 0, 0.25);
    assert!((ok.progress - 0.5).abs() < 1e-4 && ok.opacity == 1.0);
}

#[test]
fn delay_and_unit_start() {
    let mut s = sp("chars-fade");
    s.delay = 0.2;
    s.stagger = 0.1;
    let u = Units {
        n: 3,
        by: "chars".into(),
        lines: 1,
        ..Default::default()
    };
    // jednostka 2 startuje po 0,2 + 0,2
    assert_eq!(eval(Mode::In, &s, &u, 2, 0.39).opacity, 0.0);
    assert!(eval(Mode::In, &s, &u, 2, 0.45).opacity > 0.0);
    assert_eq!(eval(Mode::In, &s, &u, 0, 0.0).opacity, 0.0);
}

#[test]
fn mismatched_split_mode_animates_whole_element() {
    // element podzielony na znaki; efekt words-* nie dzieli ponownie: cały element, bez staggera
    let u = Units {
        n: 20,
        by: "chars".into(),
        lines: 1,
        ..Default::default()
    };
    assert!(!applies(Split::Words, &u));
    assert!(applies(Split::Chars, &u));
    assert!((total(&sp("words-fade"), &u) - 0.5).abs() < 1e-5);
    // niepodzielony element: efekty dzielące też działają na całości
    assert!(!applies(Split::Chars, &Units::default()));
}
