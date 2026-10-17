use btb_motion::*;

fn path(pts: &[(f32, f32, f32)]) -> PathChannel {
    PathChannel {
        keys: pts.iter().map(|&(t, x, y)| PathKey::new(t, x, y)).collect(),
    }
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn straight_path_moves_linearly_at_constant_speed() {
    let p = path(&[(0.0, 0.0, 0.0), (1.0, 100.0, 0.0)]);
    for k in 0..=10 {
        let t = k as f32 / 10.0;
        let (x, y) = p.eval(t).unwrap();
        assert!(
            near(x, 100.0 * t, 0.05) && near(y, 0.0, 1e-4),
            "t={t}: ({x},{y})"
        );
    }
    assert!(near(p.length(), 100.0, 0.01));
    assert!(near(p.speed(0.5).unwrap(), 100.0, 0.5));
    // poza zakresem kluczy: pozycje skrajne; jeden klucz = stała pozycja
    assert_eq!(p.eval(-1.0), Some((0.0, 0.0)));
    assert_eq!(p.eval(9.0), Some((100.0, 0.0)));
    assert_eq!(path(&[(0.0, 5.0, 6.0)]).eval(3.0), Some((5.0, 6.0)));
    assert_eq!(PathChannel::default().eval(0.0), None);
}

#[test]
fn collinear_auto_keys_stay_on_the_line_with_uniform_speed() {
    let p = path(&[(0.0, 0.0, 0.0), (1.0, 50.0, 25.0), (2.0, 100.0, 50.0)]);
    for k in 0..=20 {
        let t = 2.0 * k as f32 / 20.0;
        let (x, y) = p.eval(t).unwrap();
        assert!(near(y, x / 2.0, 0.02), "poza prostą w t={t}: ({x},{y})");
        assert!(near(x, 50.0 * t, 0.2), "stała prędkość: x={x} t={t}");
    }
}

#[test]
fn curved_path_length_matches_numeric_integration_and_speed_is_constant() {
    let p = path(&[(0.0, 0.0, 0.0), (1.0, 100.0, 100.0), (2.0, 200.0, 0.0)]);
    // długość: całkowanie polilinii z gęstego próbkowania w parametrze czasu przy stałej prędkości
    let mut len = 0.0;
    let mut prev = p.eval(0.0).unwrap();
    for k in 1..=4000 {
        let cur = p.eval(2.0 * k as f32 / 4000.0).unwrap();
        len += ((cur.0 - prev.0).powi(2) + (cur.1 - prev.1).powi(2)).sqrt();
        prev = cur;
    }
    assert!(
        near(p.length(), len, len * 0.002),
        "długość {} vs {}",
        p.length(),
        len
    );
    assert!(p.length() > 223.6, "krzywa jest dłuższa niż cięciwa");
    // stała prędkość po łuku (Linear): wszystkie odczyty bliskie L / T
    let target = p.length() / 2.0;
    for k in 1..20 {
        let sp = p.speed(2.0 * k as f32 / 20.0).unwrap();
        assert!(near(sp, target, target * 0.03), "prędkość {sp} vs {target}");
    }
    // symetria wokół środka
    let (xm, ym) = p.eval(1.0).unwrap();
    assert!(near(xm, 100.0, 0.1) && near(ym, 100.0, 0.1));
}

#[test]
fn manual_tangents_bend_the_path() {
    let mut p = path(&[(0.0, 0.0, 0.0), (1.0, 100.0, 0.0)]);
    p.keys[0].auto = false;
    p.keys[0].tout = [0.0, 60.0];
    p.keys[1].auto = false;
    p.keys[1].tin = [0.0, 60.0];
    let (_, y) = p.eval(0.5).unwrap();
    assert!(y > 20.0, "ścieżka wygięta w górę: y={y}");
    assert!(p.length() > 100.0);
}

#[test]
fn easy_ease_slows_down_at_the_ends_and_hold_jumps() {
    let mut p = path(&[(0.0, 0.0, 0.0), (1.0, 100.0, 0.0)]);
    p.keys[0].right = Handle::easy();
    p.keys[1].left = Handle::easy();
    let (x1, _) = p.eval(0.1).unwrap();
    assert!(x1 < 10.0, "start wolniejszy niż liniowy: {x1}");
    assert!(near(p.eval(0.5).unwrap().0, 50.0, 0.1));
    assert!(p.speed(0.5).unwrap() > p.speed(0.05).unwrap() * 3.0);
    // Hold: stoi w pierwszym kluczu, potem skok
    p.keys[0].right = Handle::Hold;
    assert_eq!(p.eval(0.9).unwrap(), (0.0, 0.0));
    assert_eq!(p.eval(1.0).unwrap(), (100.0, 0.0));
}

#[test]
fn roving_makes_speed_uniform_across_unequal_segments() {
    // środkowy klucz leży 30 px od startu i 270 px od końca; roving ma dać czas proporcjonalny do drogi
    let mut p = path(&[(0.0, 0.0, 0.0), (0.5, 30.0, 0.0), (2.0, 300.0, 0.0)]);
    p.keys[1].roving = true;
    assert!(p.retime_roving());
    assert!(
        near(p.keys[1].t, 0.2, 0.01),
        "t={} (30/300 * 2 s)",
        p.keys[1].t
    );
    let (s1, s2) = (p.speed(0.1).unwrap(), p.speed(1.0).unwrap());
    assert!(near(s1, s2, s1 * 0.02), "prędkości {s1} vs {s2}");
    assert!(!p.retime_roving(), "drugie wywołanie nic nie zmienia");
    // pierwszy i ostatni klucz nigdy nie rovują; za mało kluczy czyści flagi
    p.keys[0].roving = true;
    p.keys[2].roving = true;
    p.retime_roving();
    assert!(!p.keys[0].roving && !p.keys[2].roving);
    let mut two = path(&[(0.0, 0.0, 0.0), (1.0, 5.0, 5.0)]);
    two.keys[1].roving = true;
    assert!(!two.retime_roving() && !two.keys[1].roving);
}

#[test]
fn key_editing_keeps_order_and_replaces_same_time() {
    let mut p = path(&[(0.0, 0.0, 0.0), (2.0, 100.0, 0.0)]);
    // nowy klucz leży na ścieżce
    let i = p.insert_key(1.0, None);
    assert_eq!(i, 1);
    assert!(near(p.keys[1].x, 50.0, 0.1) && near(p.keys[1].y, 0.0, 1e-3));
    // ten sam czas zastępuje pozycję
    p.insert_key(1.00001, Some((40.0, 9.0)));
    assert_eq!(p.keys.len(), 3);
    assert_eq!((p.keys[1].x, p.keys[1].y), (40.0, 9.0));
    // przesuwanie z migawki: ograniczone do t >= 0, kolejność zachowana
    let (m, sel) = p.move_keys(&[1], -5.0);
    assert!(
        near(m.keys[0].t, 0.0, 1e-6) && m.keys.len() == 2 && sel.len() == 1,
        "klucz nadpisał klucz w t=0"
    );
    let (m, sel) = p.move_keys(&[0], 1.5);
    assert_eq!(
        m.keys.iter().map(|k| k.t).collect::<Vec<_>>(),
        vec![1.0, 1.5, 2.0]
    );
    assert_eq!(sel, vec![1]);
    p.remove_keys(&[1, 7]);
    assert_eq!(p.keys.len(), 2);
}

#[test]
fn path_json_roundtrip_and_old_defaults() {
    let mut p = path(&[(0.0, 1.0, 2.0), (1.0, 3.0, 4.0)]);
    p.keys[0].auto = false;
    p.keys[0].tout = [5.0, 6.0];
    p.keys[1].right = Handle::Hold;
    let back: PathChannel = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back.keys[0].tout, [5.0, 6.0]);
    assert!(!back.keys[0].auto && back.keys[1].auto);
    assert_eq!(back.keys[1].right, Handle::Hold);
    let k: PathKey = serde_json::from_str("{\"t\":1.0,\"x\":2.0,\"y\":3.0}").unwrap();
    assert!(k.auto && !k.roving && k.left == Handle::Linear);
}

#[test]
fn polyline_control_points_and_tangent_editing() {
    let mut p = path(&[(0.0, 0.0, 0.0), (1.0, 60.0, 0.0), (2.0, 120.0, 0.0)]);
    let poly = p.polyline(8);
    assert_eq!(
        poly.len(),
        8 * 2 + 1,
        "bez powtórzonego punktu między segmentami"
    );
    assert_eq!(poly[0], [0.0, 0.0]);
    assert!(near(poly[poly.len() - 1][0], 120.0, 1e-3));
    let cp = p.control_points(0).unwrap();
    assert_eq!(cp[0], [0.0, 0.0]);
    assert_eq!(cp[3], [60.0, 0.0]);
    assert!(p.control_points(2).is_none());
    // ręczny tangent wyłącza auto, drugi bez zmian bez mirror; z mirror symetryczny
    let (tin_before, _) = p.tangents(1);
    assert!(p.set_tangent(1, true, 60.0 + 20.0, 15.0, false));
    let k = &p.keys[1];
    assert!(!k.auto && k.tout == [20.0, 15.0] && k.tin == tin_before);
    p.set_tangent(1, false, 60.0 - 10.0, 5.0, true);
    assert_eq!(
        (p.keys[1].tin, p.keys[1].tout),
        ([-10.0, 5.0], [10.0, -5.0])
    );
    assert!(!p.set_tangent(9, true, 0.0, 0.0, false));
}

#[test]
fn distance_handles_roundtrip_to_keys() {
    let mut p = path(&[(0.0, 0.0, 0.0), (1.0, 100.0, 0.0), (2.0, 150.0, 0.0)]);
    let mut d = p.distance_channel();
    assert!(near(d.keys[1].v, 100.0, 0.1) && near(d.keys[2].v, 150.0, 0.5));
    d.set_ease(
        1,
        true,
        Ease {
            speed: 80.0,
            influence: 0.4,
        },
    );
    d.keys[1].continuous = true;
    p.apply_distance_handles(&d);
    assert_eq!(
        p.keys[1].right,
        Handle::Ease {
            speed: 80.0,
            influence: 0.4
        }
    );
    assert!(p.keys[1].continuous);
    // kanał odległości odtworzony z kluczy ma te same uchwyty
    assert_eq!(p.distance_channel().side_ease(1, true).unwrap().speed, 80.0);
}
