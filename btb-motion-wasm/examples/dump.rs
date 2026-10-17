//! Generuje dane referencyjne dla testu zgodności WASM <-> natywnie.
//! `cargo run -p btb-motion-wasm --example dump -- <katalog> [fixture.json ...]`
//! Zapisuje `<nazwa>.fixture.json` i `<nazwa>.expected.json` (próbki + zdarzenia skryptowanego przebiegu).
use btb_motion::*;
use serde_json::{json, Value};

fn builtin() -> Director {
    let mut d = Director::new("Default", 2.3);
    let mut c = Channel::tween("opacity", 0.0, 1.0, 0.0, 0.5, (0.0, 0.0, 0.58, 1.0));
    c.keys.push(Key::linear(2.0, 1.0));
    c.keys.push(Key::linear(2.3, 0.0));
    d.actors.push(Actor {
        target: "bar".into(),
        offset: 0.0,
        active_from: None,
        active_until: None,
        paths: vec![],
        channels: vec![c],
    });
    d.events.push(Event {
        name: "pilot1".into(),
        t: 0.25,
        kind: EventKind::Tag,
        dir: Dir::Both,
    });
    d.events.push(Event {
        name: "hold".into(),
        t: 0.5,
        kind: EventKind::Stop,
        dir: Dir::Forward,
    });
    let mut lp = Director::new("loop", 1.0);
    lp.free = true;
    let mut y = Channel::new("y");
    y.keys = vec![Key::new(0.0, 0.0), Key::new(0.5, -6.0), Key::new(1.0, 0.0)];
    y.post = Extend::Loop { count: None };
    lp.actors.push(Actor {
        target: "bar".into(),
        offset: 0.0,
        active_from: None,
        active_until: None,
        paths: vec![],
        channels: vec![y],
    });
    d.children.push(lp);
    d
}

/// Skrypt: start, 40 kroków po 1/60 s; po Stop dodatkowe 30 kroków (zegar swobodny), resume, 150 kroków.
fn run(d: &Director) -> Value {
    let mut tr = Transport::default();
    let mut log = vec![];
    let snap = |tr: &Transport, fired: &[Fired], log: &mut Vec<Value>| {
        let mut s = vec![];
        tr.sample(d, &mut s);
        log.push(json!({
            "t": tr.t, "playing": tr.playing,
            "fired": serde_json::from_str::<Value>(&btb_motion_wasm::fired_json(fired)).unwrap(),
            "samples": s.iter().map(|x| json!({"target": x.target, "prop": x.prop, "value": x.value})).collect::<Vec<_>>(),
        }));
    };
    tr.start();
    for _ in 0..40 {
        let f = tr.advance(d, 1.0 / 60.0);
        snap(&tr, &f, &mut log);
    }
    for _ in 0..30 {
        let f = tr.advance(d, 1.0 / 60.0);
        snap(&tr, &f, &mut log);
    }
    tr.resume();
    for _ in 0..150 {
        let f = tr.advance(d, 1.0 / 60.0);
        snap(&tr, &f, &mut log);
    }
    Value::Array(log)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::path::Path::new(&args[0]);
    std::fs::create_dir_all(dir).unwrap();
    let mut inputs: Vec<(String, Director)> = vec![("builtin".into(), builtin())];
    for p in &args[1..] {
        let name = std::path::Path::new(p)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        inputs.push((
            name,
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap(),
        ));
    }
    for (name, d) in inputs {
        std::fs::write(
            dir.join(format!("{name}.fixture.json")),
            serde_json::to_string(&d).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("{name}.expected.json")),
            run(&d).to_string(),
        )
        .unwrap();
    }
}
