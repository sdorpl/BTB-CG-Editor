mod anim;
mod app;
mod canvas;
mod compile;
mod convert;
mod i18n;
mod importer;
mod keyconv;
mod links;
mod model;
mod motion_bridge;
mod motion_page;
mod panels;
mod playback;
mod preview;
mod presets;
mod render;

use app::{Cli, CgEditorApp};
use playback::Phase;

fn parse_cli() -> Cli {
    let mut cli = Cli { frames: 12, ..Default::default() };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--open" => cli.open = it.next().map(Into::into),
            "--new" => cli.new_type = it.next(),
            "--shot" => cli.shot = it.next().map(Into::into),
            "--frames" => cli.frames = it.next().and_then(|v| v.parse().ok()).unwrap_or(12),
            "--select" => cli.select = it.next().and_then(|v| v.parse().ok()),
            "--tab" => cli.tab = it.next(),
            "--import" => cli.import_url = it.next(),
            "--fit-frame" => cli.fit_frame = true,
            "--graph" => cli.graph = true,
            "--convert-keys" => cli.convert_keys = true,
            "--pick" => cli.pick = it.next().and_then(|v| v.parse().ok()),
            "--phase" => {
                // e.g. in:0.3 | hold:0.5 | out:0.2
                if let Some(v) = it.next() {
                    let (p, t) = v.split_once(':').unwrap_or((&v, "0"));
                    let ph = match p { "in" => Phase::In, "hold" => Phase::Hold, "out" => Phase::Out, _ => Phase::Idle };
                    cli.phase = Some((ph, t.parse().unwrap_or(0.0)));
                }
            }
            _ => {}
        }
    }
    cli
}

fn dump_compile(args: &[String]) {
    // --dump-compile in.json out.json : compile first scene element, write {html,css,js,ocg} (used to verify vs scene-model.js)
    let text = std::fs::read_to_string(&args[0]).expect("read");
    let list = importer::parse_elements(&text).expect("parse");
    let mut outs = vec![];
    for l in list {
        if let Some(scene) = l.element.scene {
            let c = compile::compile(&scene);
            outs.push(serde_json::json!({ "id": l.element.id, "html": c.html, "css": c.css, "js": c.js, "ocg": c.ocg_inputs, "title": c.title, "fields": c.fields }));
        }
    }
    std::fs::write(&args[1], serde_json::to_string(&outs).unwrap()).expect("write");
}

fn main() -> eframe::Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(|s| s.as_str()) == Some("--write-preview") {
        // --write-preview in.json out.html [index]
        let list = importer::parse_elements(&std::fs::read_to_string(&raw[1]).expect("read")).expect("parse");
        let i: usize = raw.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut el = list.into_iter().nth(i).expect("element").element;
        if el.scene.is_some() { compile::recompile(&mut el); }
        std::fs::write(&raw[2], preview::build_html(&el, "").expect("preview")).expect("write");
        return Ok(());
    }
    if raw.first().map(|s| s.as_str()) == Some("--write-motion-html") {
        // --write-motion-html in.json out.html [index] [manual] : samodzielna strona dla OBS (silnik btb-motion/WASM)
        let list = importer::parse_elements(&std::fs::read_to_string(&raw[1]).expect("read")).expect("parse");
        let i: usize = raw.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
        let manual = raw.get(4).is_some_and(|v| v == "manual");
        let mut el = list.into_iter().nth(i).expect("element").element;
        if el.scene.is_some() { compile::recompile(&mut el); }
        std::fs::write(&raw[2], motion_page::build_html(&el, manual).expect("motion html")).expect("write");
        return Ok(());
    }
    if raw.first().map(|s| s.as_str()) == Some("--dump-compile") { dump_compile(&raw[1..]); return Ok(()); }
    i18n::init_from_env();
    let cli = parse_cli();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BTB CGEditor")
            .with_inner_size([1600.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("BTB CGEditor", options, Box::new(move |cc| Ok(Box::new(CgEditorApp::new(cc, cli.clone())))))
}
