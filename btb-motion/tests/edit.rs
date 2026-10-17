use btb_motion::*;

fn chan(times: &[(f32, f32)]) -> Channel {
    let mut c = Channel::new("tx");
    c.keys = times.iter().map(|&(t, v)| Key::linear(t, v)).collect();
    c
}
fn times(c: &Channel) -> Vec<f32> {
    c.keys.iter().map(|k| k.t).collect()
}

#[test]
fn insert_keeps_order_and_replaces_same_time() {
    let mut c = chan(&[(0.0, 0.0), (2.0, 20.0)]);
    assert_eq!(c.insert_key(1.0, 5.0), 1);
    assert_eq!(times(&c), vec![0.0, 1.0, 2.0]);
    // ten sam czas: zastępuje wartość i zostawia uchwyty
    c.keys[1].right = Handle::easy();
    assert_eq!(c.insert_key(1.00001, 7.0), 1);
    assert_eq!(c.keys.len(), 3);
    assert_eq!(c.keys[1].v, 7.0);
    assert_eq!(c.keys[1].right, Handle::easy());
    // przed pierwszym i po ostatnim
    assert_eq!(c.insert_key(-1.0, 1.0), 0);
    assert_eq!(c.insert_key(9.0, 2.0), 4);
}

#[test]
fn remove_ignores_missing_indices() {
    let mut c = chan(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]);
    c.remove_keys(&[1, 3, 99]);
    assert_eq!(times(&c), vec![0.0, 2.0]);
    c.remove_keys(&[]);
    assert_eq!(c.keys.len(), 2);
}

#[test]
fn move_keys_shifts_clamps_and_overwrites() {
    let c = chan(&[(0.5, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]);
    // przesunięcie o +1 s kluczy 1 i 2: klucz 2 (t=2 -> 3) nadpisuje niezaznaczony klucz w t=3
    let (m, sel) = c.move_keys(&[1, 2], 1.0);
    assert_eq!(times(&m), vec![0.5, 2.0, 3.0]);
    assert_eq!(m.keys[2].v, 2.0, "przesunięty klucz wygrywa");
    assert_eq!(sel, vec![1, 2]);
    // ograniczenie od dołu: nie wychodzimy poniżej t=0
    let (m, _) = c.move_keys(&[0, 1], -10.0);
    assert_eq!(times(&m), vec![0.0, 0.5, 2.0, 3.0]);
    // przesunięcie przez inny klucz zmienia kolejność, indeksy zaznaczenia nadążają
    let (m, sel) = c.move_keys(&[0], 2.2);
    assert_eq!(times(&m)[..3], [1.0, 2.0, 2.7]);
    assert_eq!(sel, vec![2]);
    assert_eq!(m.keys[2].v, 0.0);
    // oryginał bez zmian (liczone z migawki przy przeciąganiu)
    assert_eq!(times(&c), vec![0.5, 1.0, 2.0, 3.0]);
    // puste zaznaczenie i nieistniejące indeksy
    let (m, sel) = c.move_keys(&[], 5.0);
    assert_eq!(times(&m), times(&c));
    assert!(sel.is_empty());
    let (m, _) = c.move_keys(&[42], 5.0);
    assert_eq!(times(&m), times(&c));
}
