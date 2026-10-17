use btb_motion::ease::ease;
use btb_motion::*;

fn max_err(c: &Channel, t0: f32, t1: f32, f: impl Fn(f32) -> f32) -> f32 {
    (0..=400)
        .map(|i| t0 + (t1 - t0) * i as f32 / 400.0)
        .map(|t| (c.eval(t).unwrap() - f(t)).abs())
        .fold(0.0, f32::max)
}

#[test]
fn power2_out_is_exactly_two_keys() {
    // 1-(1-t)^3 jest sześcianem w czasie, więc jeden segment Béziera odtwarza go dokładnie
    let f = |t: f32| 100.0 * ease("power2.out", t);
    let c = Channel::fit("x", 0.0, 1.0, 0.0015, f);
    assert_eq!(c.keys.len(), 2, "{:?}", c.keys.len());
    assert!(max_err(&c, 0.0, 1.0, f) < 0.1);
}

#[test]
fn common_eases_fit_with_few_keys_and_small_error() {
    for (name, max_keys) in [
        ("power1.out", 2),
        ("power3.out", 3),
        ("power2.inOut", 3),
        ("back.out(1.7)", 4),
        ("back.out(2)", 4),
        ("sine.inOut", 3),
        ("linear", 2),
    ] {
        let f = |t: f32| 80.0 * ease(name, t) - 10.0;
        let c = Channel::fit("x", 0.0, 0.7, 0.0015, f);
        assert!(c.keys.len() <= max_keys, "{name}: {} kluczy", c.keys.len());
        assert!(
            max_err(&c, 0.0, 0.7, f) < 80.0 * 0.0015 * 2.0 + 0.05,
            "{name}: błąd {}",
            max_err(&c, 0.0, 0.7, f)
        );
    }
}

#[test]
fn oscillating_eases_need_more_keys_but_stay_accurate_and_compact() {
    for (name, max_keys) in [("elastic.out", 11), ("bounce.out", 10)] {
        let f = |t: f32| 100.0 * ease(name, t);
        let c = Channel::fit("x", 0.0, 1.0, 0.0015, f);
        assert!(
            c.keys.len() > 3 && c.keys.len() <= max_keys,
            "{name}: {} kluczy",
            c.keys.len()
        );
        assert!(
            max_err(&c, 0.0, 1.0, f) < 1.2,
            "{name}: błąd {}",
            max_err(&c, 0.0, 1.0, f)
        );
    }
}

#[test]
fn steps_become_hold_keys() {
    let steps = [0.0f32, 1.0, 0.2, 1.0, 0.4, 0.4, 1.0];
    let f = |t: f32| steps[((t * 7.0).floor() as usize).min(6)];
    let c = Channel::fit("opacity", 0.0, 1.0, 0.002, f);
    assert!(c.keys.iter().any(|k| k.right == Handle::Hold));
    let mut bad = 0;
    for i in 0..=500 {
        let t = i as f32 / 500.0;
        let edge = (t * 7.0 - (t * 7.0).round()).abs() / 7.0;
        if edge < 1.0 / 200.0 {
            continue;
        }
        if (c.eval(t).unwrap() - f(t)).abs() > 1e-3 {
            bad += 1;
        }
    }
    assert_eq!(bad, 0);
}

#[test]
fn constant_and_degenerate_inputs() {
    let c = Channel::fit("x", 0.0, 1.0, 0.001, |_| 5.0);
    assert!(c.keys.len() <= 2 && c.eval(0.5) == Some(5.0));
    let c = Channel::fit("x", 2.0, 2.0, 0.001, |_| 7.0);
    assert_eq!(c.keys.len(), 1);
    assert_eq!(c.eval(0.0), Some(7.0));
    // kanał dopasowany jest edytowalny: klucze mają uchwyty Ease albo Linear na skrajach
    let c = Channel::fit("x", 0.0, 1.0, 0.0015, |t| ease("back.out(1.7)", t));
    assert!(c
        .keys
        .iter()
        .all(|k| matches!(k.right, Handle::Ease { .. } | Handle::Linear | Handle::Hold)));
    assert!(c.keys.windows(2).all(|w| w[0].t < w[1].t));
}

#[test]
fn fit_is_much_smaller_than_bake() {
    let f = |t: f32| 1920.0 * (1.0 - ease("back.out(1.2)", t));
    let fit = Channel::fit("x", 0.0, 0.7, 0.0015, f);
    let bake = Channel::bake("x", 0.0, 0.7, 0.0004, f);
    assert!(
        fit.keys.len() * 3 <= bake.keys.len() + 3,
        "fit {} vs bake {}",
        fit.keys.len(),
        bake.keys.len()
    );
}
