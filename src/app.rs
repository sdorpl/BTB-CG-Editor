use crate::{
    canvas::{self, Bg, CanvasState, DrawInputs, SceneParamsBuilder},
    compile,
    importer::{self, Loaded},
    model::{Element, Layer, Scene},
    panels::{
        inspector::{draw_inspector, InspectorTab},
        layers::{draw_layers_panel, duplicate_layer},
        timeline::{self, draw_timeline},
    },
    motion_bridge,
    playback::{self, Phase, Playback},
    render::Values,
};
use crate::i18n::{tr, trf};
use eframe::egui;
use egui::{Color32, RichText};
use std::path::PathBuf;

#[derive(Default, Clone)]
pub struct Cli {
    pub open: Option<PathBuf>,
    pub new_type: Option<String>,
    pub shot: Option<PathBuf>,
    pub phase: Option<(Phase, f32)>,
    pub select: Option<usize>,
    pub tab: Option<String>,
    pub frames: u32,
    pub import_url: Option<String>,
    pub pick: Option<usize>,
    pub fit_frame: bool,
    /// Otwiera edytor krzywych z zaznaczonym pierwszym kluczem (do zrzutów ekranu).
    pub graph: bool,
    /// Po wczytaniu zamienia presety IN/OUT na klucze (do zrzutów ekranu).
    pub convert_keys: bool,
}

struct ImportDialog {
    open: bool,
    url: String,
    status: String,
    results: Vec<Loaded>,
    rx: Option<std::sync::mpsc::Receiver<importer::FetchResult>>,
    /// BTB-CG checkout used for code → scene conversion (scene_convert.js + renderer).
    root: String,
    conv: Option<ConvertJob>,
    conv_log: Vec<String>,
}

struct ConvertJob {
    rx: std::sync::mpsc::Receiver<crate::convert::ConvertResult>,
    /// Ids of the elements being converted (same order as sent); `current` = the open element.
    ids: Vec<String>,
    current: bool,
    load: bool,
}

pub struct CgEditorApp {
    element: Element,
    selected: Vec<String>,
    last_clicked: Option<String>,
    canvas: CanvasState,
    tab: InspectorTab,
    playback: Playback,
    history: Vec<String>,
    hpos: usize,
    last_json: String,
    pending_since: Option<f64>,
    path: Option<PathBuf>,
    dirty: bool,
    status: String,
    base_url: String,
    push_rx: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    import: ImportDialog,
    clipboard: Vec<Layer>,
    timeline: timeline::TimelineUi,
    graph: crate::panels::graph::GraphUi,
    last_time: Option<f64>,
    cli: Cli,
    frame_no: u32,
    shot_requested: bool,
}

fn snapshot(el: &Element) -> String { serde_json::to_string(el).unwrap_or_default() }

impl CgEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: Cli) -> Self {
        setup_fonts(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_gray(26);
        visuals.window_fill = Color32::from_gray(30);
        cc.egui_ctx.set_visuals(visuals);
        Self::build(&cc.egui_ctx, cli)
    }

    pub fn build(_ctx: &egui::Context, cli: Cli) -> Self {
        let element = Element::new_typed(tr("element.new_name"), cli.new_type.as_deref().unwrap_or("LOWER_THIRD"));
        let json = snapshot(&element);
        let mut app = Self {
            element, selected: vec![], last_clicked: None, canvas: CanvasState::default(), tab: InspectorTab::default(),
            playback: Playback::default(), history: vec![json.clone()], hpos: 0, last_json: json, pending_since: None,
            path: None, dirty: false, status: tr("status.ready").into(), base_url: String::new(), push_rx: None,
            import: ImportDialog { open: false, url: "localhost:3000".into(), status: String::new(), results: vec![], rx: None,
                root: crate::convert::find_btb_root().map(|p| p.display().to_string()).unwrap_or_default(), conv: None, conv_log: vec![] },
            clipboard: vec![], timeline: Default::default(), graph: Default::default(), last_time: None, cli: cli.clone(), frame_no: 0, shot_requested: false,
        };
        if let Some(p) = &cli.open { app.open_path(p.clone()); }
        if let Some(u) = &cli.import_url {
            app.import.url = u.clone();
            app.import.open = true;
            app.import.status = tr("import.connecting").into();
            match importer::fetch_blocking(&importer::normalize_base_url(u)) {
                Ok(r) => { app.base_url = importer::normalize_base_url(u); app.import.results = r; app.import.status = trf("import.found", &[("n", &app.import.results.len())]); }
                Err(e) => app.import.status = e,
            }
            if let Some(i) = cli.pick { if i < app.import.results.len() { let l = app.import.results.remove(i); app.load_element(l.element); app.import.open = false; } }
        }
        if let Some(i) = cli.select {
            if let Some(s) = &app.element.scene { if let Some(l) = s.layers.get(i) { app.selected = vec![l.id.clone()]; } }
        }
        if cli.convert_keys { let r = crate::keyconv::convert_presets(&mut app.element, crate::keyconv::Scope::All); app.status = trf("status.presets_to_keys", &[("channels", &r.channels), ("keys", &r.keys)]); }
        if cli.graph {
            app.timeline.graph_open = true;
            if let Some((l, c)) = app.element.scene.as_ref().and_then(|s| s.layers.iter().find_map(|l| l.keys.first().map(|c| (l.id.clone(), c.prop.clone())))) {
                app.timeline.sel = vec![timeline::KeyRef { layer: l, prop: c, idx: 1 }];
            }
        }
        if cli.fit_frame { app.canvas.needs_fit = false; app.canvas.needs_fit_frame = true; }
        if let Some(t) = &cli.tab {
            app.tab = match t.as_str() { "element" => InspectorTab::Element, "anim" => InspectorTab::Anim, "code" => InspectorTab::Code, _ => InspectorTab::Layer };
        }
        if let Some((ph, t)) = cli.phase { app.playback.phase = ph; app.playback.t = t; app.playback.loop_t = t; app.playback.playing = false; }
        app
    }

    // ── element lifecycle ──

    fn reset_history(&mut self) {
        let j = snapshot(&self.element);
        self.history = vec![j.clone()];
        self.hpos = 0;
        self.last_json = j;
        self.pending_since = None;
        self.dirty = false;
        self.selected.clear();
        self.playback.stop();
        self.canvas.needs_fit = true;
    }

    fn load_element(&mut self, el: Element) {
        self.element = el;
        self.reset_history();
        self.status = trf("status.loaded", &[("name", &self.element.name)]);
    }

    fn new_element(&mut self, t: &str) {
        self.element = Element::new_typed(Element::elem_type_label(t), t);
        self.path = None;
        self.reset_history();
    }

    fn open_path(&mut self, p: PathBuf) {
        // .cgpack: ZIP z elementem i plikami graficznymi (docs/FORMAT-PACZKI.md); obrazy wracają osadzone jako data:
        let text = if crate::pack::is_pack_path(&p) { std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| crate::pack::read_pack(&b)) } else { std::fs::read_to_string(&p).map_err(|e| e.to_string()) };
        match text.and_then(|t| importer::parse_elements(&t)) {
            Ok(mut list) => {
                if list.len() == 1 {
                    let l = list.remove(0);
                    self.load_element(l.element);
                    self.path = if crate::pack::is_pack_path(&p) { None } else { Some(p) };   // paczka to wymiana, nie plik roboczy: zapis pyta o nazwę
                } else {
                    self.import.results = list;
                    self.import.status = trf("import.file_many", &[("n", &self.import.results.len())]);
                    self.import.open = true;
                }
            }
            Err(e) => self.status = trf("status.open_failed", &[("err", &e)]),
        }
    }

    fn save_to(&mut self, p: PathBuf) {
        self.flush_compile();
        let doc = serde_json::json!({ "_exportVersion": 3, "elements": [&self.element] });
        match serde_json::to_string_pretty(&doc).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&p, s).map_err(|e| e.to_string())) {
            Ok(_) => { self.status = trf("status.saved", &[("path", &p.display())]); self.path = Some(p); self.dirty = false; }
            Err(e) => self.status = trf("status.save_failed", &[("err", &e)]),
        }
    }

    fn save(&mut self) {
        match self.path.clone() { Some(p) => self.save_to(p), None => self.save_as() }
    }

    fn save_as(&mut self) {
        let name = format!("{}.json", self.element.name.replace(' ', "_"));
        if let Some(p) = rfd::FileDialog::new().set_file_name(&name).add_filter("BTB-CG element", &["json"]).save_file() { self.save_to(p); }
    }

    fn export_pack(&mut self) {
        self.flush_compile();
        let name = format!("{}.cgpack", self.element.name.replace(' ', "_"));
        let Some(p) = rfd::FileDialog::new().set_file_name(&name).add_filter("BTB-CG pack", &["cgpack"]).save_file() else { return };
        match crate::pack::build_pack(&self.element).and_then(|b| { let n = b.len(); std::fs::write(&p, b).map(|_| n).map_err(|e| e.to_string()) }) {
            Ok(_) => {
                let files = crate::pack::count_assets(&std::fs::read(&p).unwrap_or_default());
                self.status = trf("status.pack_saved", &[("path", &p.display()), ("files", &files)]);
            }
            Err(e) => self.status = trf("status.pack_failed", &[("err", &e)]),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("BTB-CG element / db.json / .cgpack", &["json", "cgpack"]).pick_file() { self.open_path(p); }
    }

    fn open_preview(&mut self) {
        self.flush_compile();
        self.status = match crate::preview::open(&self.element, &self.base_url) {
            Ok(p) => trf("status.preview_opened", &[("path", &p.display())]),
            Err(e) => trf("status.preview_unavailable", &[("err", &e)]),
        };
    }

    fn flush_compile(&mut self) {
        if let Some(s) = &self.element.scene { if !s.detached { compile::recompile(&mut self.element); } }
    }

    // ── history ──

    fn track_changes(&mut self, ctx: &egui::Context, now: f64) {
        let cur = snapshot(&self.element);
        if cur != self.last_json {
            self.last_json = cur;
            self.pending_since = Some(now);
            self.dirty = true;
        }
        if let Some(t0) = self.pending_since {
            let down = ctx.input(|i| i.pointer.any_down());
            if !down && now - t0 > 0.35 {
                self.flush_compile();
                let j = snapshot(&self.element);
                self.last_json = j.clone();
                self.pending_since = None;
                if self.history.get(self.hpos) != Some(&j) {
                    self.history.truncate(self.hpos + 1);
                    self.history.push(j);
                    self.hpos = self.history.len() - 1;
                    if self.history.len() > 200 { self.history.remove(0); self.hpos -= 1; }
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
        }
    }

    fn restore(&mut self, idx: usize) {
        if let Ok(el) = serde_json::from_str::<Element>(&self.history[idx]) {
            self.element = el;
            self.hpos = idx;
            self.last_json = self.history[idx].clone();
            self.pending_since = None;
            if let Some(s) = &self.element.scene { let ids: Vec<String> = s.layers.iter().map(|l| l.id.clone()).collect(); self.selected.retain(|x| ids.contains(x)); }
        }
    }

    fn undo(&mut self) {
        if self.pending_since.is_some() { self.flush_compile(); let j = snapshot(&self.element); self.history.truncate(self.hpos + 1); if self.history[self.hpos] != j { self.history.push(j); self.hpos += 1; } self.pending_since = None; }
        if self.hpos > 0 { self.restore(self.hpos - 1); }
    }
    fn redo(&mut self) { if self.hpos + 1 < self.history.len() { self.restore(self.hpos + 1); } }

    // ── editing helpers ──

    fn scene_mut(&mut self) -> Option<&mut Scene> { self.element.scene.as_mut() }

    fn delete_selected(&mut self) {
        let sel = self.selected.clone();
        if let Some(s) = self.scene_mut() { s.layers.retain(|l| !sel.contains(&l.id) || l.locked); }
        self.selected.clear();
    }

    fn duplicate_selected(&mut self) {
        let sel = self.selected.clone();
        let mut new_sel = vec![];
        if let Some(s) = self.element.scene.as_mut() {
            for id in sel { let mut tmp = vec![]; duplicate_layer(s, &id, &mut tmp); new_sel.extend(tmp); }
        }
        if !new_sel.is_empty() { self.selected = new_sel; }
    }

    fn copy_selected(&mut self) {
        if let Some(s) = &self.element.scene { self.clipboard = s.layers.iter().filter(|l| self.selected.contains(&l.id)).cloned().collect(); }
    }

    fn paste(&mut self) {
        let clip = self.clipboard.clone();
        let mut ids = vec![];
        if let Some(s) = self.element.scene.as_mut() {
            for mut l in clip {
                l.id = format!("l_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
                l.x += 16.0; l.y += 16.0;
                ids.push(l.id.clone());
                s.layers.push(l);
            }
        }
        if !ids.is_empty() { self.selected = ids; }
    }

    fn nudge(&mut self, dx: f32, dy: f32) {
        let sel = self.selected.clone();
        if let Some(s) = self.scene_mut() { for l in s.layers.iter_mut().filter(|l| sel.contains(&l.id) && !l.locked) { l.x += dx; l.y += dy; } }
    }

    fn apply_text_edit(&mut self, layer_id: &str, text: String) {
        let field = self.element.scene.as_ref().and_then(|s| s.layers.iter().find(|l| l.id == layer_id)).and_then(|l| l.field.clone());
        if let Some(s) = self.element.scene.as_mut() { if let Some(l) = s.layers.iter_mut().find(|l| l.id == layer_id) { if l.field.as_deref().is_some_and(|f| !f.is_empty()) { l.default_text = Some(text.clone()); } else { l.text = Some(text.clone()); } } }
        if let Some(f) = field.filter(|f| !f.is_empty()) {
            let id = compile::field_id(&f);
            if id == "TITLE" { self.element.title = text; } else { self.element.fields.insert(id, serde_json::Value::String(text)); }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() { return; }
        let (ctrl, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let key = |k: egui::Key| ctx.input(|i| i.key_pressed(k));
        if ctrl && key(egui::Key::Z) { if shift { self.redo() } else { self.undo() } }
        if ctrl && key(egui::Key::Y) { self.redo(); }
        if ctrl && key(egui::Key::S) { if shift { self.save_as() } else { self.save() } }
        if ctrl && key(egui::Key::O) { self.open_dialog(); }
        if ctrl && key(egui::Key::P) { self.open_preview(); }
        if ctrl && key(egui::Key::D) { self.duplicate_selected(); }
        if ctrl && key(egui::Key::C) { self.copy_selected(); }
        if ctrl && key(egui::Key::V) { self.paste(); }
        if ctrl && key(egui::Key::A) { if let Some(s) = &self.element.scene { self.selected = s.layers.iter().map(|l| l.id.clone()).collect(); } }
        if (key(egui::Key::Delete) || key(egui::Key::Backspace)) && !self.timeline.has_key_selection() { self.delete_selected(); }
        if key(egui::Key::Escape) { if self.playback.active() { self.playback.stop(); } else { self.selected.clear(); } }
        let step = if shift { 10.0 } else { 1.0 };
        if key(egui::Key::ArrowLeft) { self.nudge(-step, 0.0); }
        if key(egui::Key::ArrowRight) { self.nudge(step, 0.0); }
        if key(egui::Key::ArrowUp) { self.nudge(0.0, -step); }
        if key(egui::Key::ArrowDown) { self.nudge(0.0, step); }
        if key(egui::Key::F) { self.canvas.needs_fit = true; }
    }

    fn start_convert(&mut self, els: Vec<Element>, current: bool, load: bool) {
        if els.is_empty() || self.import.conv.is_some() { return; }
        self.import.conv_log.clear();
        self.status = trf("convert.running", &[("n", &els.len())]);
        self.import.status = self.status.clone();
        let ids = els.iter().map(|e| e.id.clone()).collect();
        self.import.conv = Some(ConvertJob { rx: crate::convert::convert_async(&self.import.root, els), ids, current, load });
    }

    fn convert_current(&mut self) {
        if !crate::convert::needs_conversion(&self.element) { self.status = tr("convert.already_scene").into(); return; }
        self.flush_compile();
        self.start_convert(vec![self.element.clone()], true, true);
    }

    fn poll_convert(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.import.conv else { return };
        let res = match job.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => { ctx.request_repaint_after(std::time::Duration::from_millis(150)); return; }
            Err(_) => Err(tr("convert.aborted").into()),
        };
        let Some(job) = self.import.conv.take() else { return };
        match res {
            Err(e) => { self.import.status = e.clone(); self.status = e; }
            Ok(list) => {
                let (mut ok, mut failed) = (0, 0);
                let mut to_load: Option<Element> = None;
                for (id, c) in job.ids.iter().zip(list) {
                    for w in &c.warnings { self.import.conv_log.push(format!("{id}: {w}")); }
                    for n in &c.notes { self.import.conv_log.push(trf("convert.log_note", &[("id", &id), ("note", &n)])); }
                    match c.element {
                        Some(el) => {
                            ok += 1;
                            if let Some(r) = self.import.results.iter_mut().find(|r| &r.element.id == id) { r.element = el.clone(); }
                            if job.load && to_load.is_none() { to_load = Some(el); }
                        }
                        None => { failed += 1; self.import.conv_log.push(trf("convert.log_error", &[("id", &id), ("err", &c.error.unwrap_or_default())])); }
                    }
                }
                let msg = if failed > 0 { trf("convert.done_failed", &[("ok", &ok), ("failed", &failed)]) } else { trf("convert.done", &[("ok", &ok)]) };
                if let Some(el) = to_load { self.load_element(el); self.import.open = false; if job.current { self.dirty = true; } }
                self.import.status = msg.clone();
                self.status = msg;
            }
        }
    }

    /// Sends the open element to the running BTB-CG (server address from the import dialog).
    fn push_to_btb(&mut self, target: importer::PushTarget) {
        self.flush_compile();
        self.status = tr("push.sending").into();
        self.push_rx = Some(importer::push_async(&self.import.url, target, self.element.clone()));
    }

    fn poll_push(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.push_rx else { return };
        match rx.try_recv() {
            Ok(Ok(msg)) => { self.status = msg; self.push_rx = None; }
            Ok(Err(e)) => { self.status = e; self.push_rx = None; }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Err(_) => self.push_rx = None,
        }
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        self.poll_push(ctx);
        self.poll_convert(ctx);
        if let Some(rx) = &self.import.rx {
            match rx.try_recv() {
                Ok(Ok(r)) => { self.import.status = trf("import.found", &[("n", &r.len())]); self.import.results = r; self.import.rx = None; self.base_url = importer::normalize_base_url(&self.import.url); }
                Ok(Err(e)) => { self.import.status = e; self.import.rx = None; }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(_) => self.import.rx = None,
            }
        }
        if !self.import.open { return; }
        let mut open = true;
        let mut pick: Option<usize> = None;
        let mut convert_one: Option<usize> = None;
        egui::Window::new(tr("import.title")).open(&mut open).default_width(520.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("BTB-CG:");
                ui.add(egui::TextEdit::singleline(&mut self.import.url).desired_width(220.0));
                if ui.add_enabled(self.import.rx.is_none(), egui::Button::new(tr("import.fetch"))).clicked() {
                    self.import.status = tr("import.connecting").into();
                    self.import.results.clear();
                    self.import.rx = Some(importer::fetch_async(&self.import.url));
                }
                if ui.button(tr("import.from_file")).clicked() { self.open_dialog(); }
            });
            ui.label(RichText::new(tr("import.hint")).small().color(Color32::from_gray(140)));
            ui.horizontal(|ui| {
                ui.label(tr("import.convert_root"));
                ui.add(egui::TextEdit::singleline(&mut self.import.root).desired_width(260.0));
                let code: Vec<Element> = self.import.results.iter().map(|r| r.element.clone()).filter(crate::convert::needs_conversion).collect();
                if ui.add_enabled(self.import.conv.is_none() && !code.is_empty(), egui::Button::new(trf("import.convert_all", &[("n", &code.len())]))).clicked() {
                    self.start_convert(code, false, false);
                }
            });
            if !self.import.status.is_empty() { ui.colored_label(Color32::from_rgb(251, 191, 36), &self.import.status); }
            if !self.import.conv_log.is_empty() {
                egui::CollapsingHeader::new(trf("import.notes", &[("n", &self.import.conv_log.len())])).show(ui, |ui| {
                    for l in &self.import.conv_log { ui.label(RichText::new(l).small().color(Color32::from_gray(170))); }
                });
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                for (i, r) in self.import.results.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("[{}]", r.source)).small().color(Color32::from_gray(130)));
                        let name = if r.element.name.is_empty() { &r.element.id } else { &r.element.name };
                        ui.label(RichText::new(name).strong());
                        ui.label(RichText::new(Element::elem_type_label(&r.element.elem_type)).small().color(Color32::from_gray(160)));
                        let code_only = crate::convert::needs_conversion(&r.element);
                        if code_only { ui.label(RichText::new(tr("import.manual_code")).small().color(Color32::from_rgb(239, 68, 68))); }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(tr("import.load")).clicked() { pick = Some(i); }
                            if code_only && ui.add_enabled(self.import.conv.is_none(), egui::Button::new(tr("import.convert_load")).small()).clicked() { convert_one = Some(i); }
                        });
                    });
                }
            });
        });
        self.import.open = open;
        if let Some(i) = convert_one { let el = self.import.results[i].element.clone(); self.start_convert(vec![el], false, true); }
        if let Some(i) = pick { let l = self.import.results.remove(i); self.load_element(l.element); self.import.open = false; }
    }

    fn screenshot_flow(&mut self, ctx: &egui::Context) {
        let Some(path) = self.cli.shot.clone() else { return };
        self.frame_no += 1;
        if self.frame_no == self.cli.frames.max(8) && !self.shot_requested {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let shot = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut buf = image::RgbaImage::new(w as u32, h as u32);
            for (i, px) in img.pixels.iter().enumerate() { buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba(px.to_array())); }
            let _ = buf.save(&path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

pub(crate) fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("Bahnschrift".into(), egui::FontData::from_static(include_bytes!("../assets/BAHNSCHRIFT.TTF")));
    for (name, data) in [("LiberationRegular", &include_bytes!("../assets/LiberationSans-Regular.ttf")[..]), ("LiberationBold", &include_bytes!("../assets/LiberationSans-Bold.ttf")[..])] {
        fonts.font_data.insert(name.into(), egui::FontData::from_static(data));
        fonts.families.insert(egui::FontFamily::Name(name.into()), vec![name.to_string(), "Bahnschrift".to_string()]);
    }
    let mut fam = vec!["Bahnschrift".to_string()];
    fam.extend(fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(egui::FontFamily::Name("Bahnschrift".into()), fam);
    // Bahnschrift as a last-resort fallback for UI symbols (arrows, geometric shapes) missing in egui's defaults.
    if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) { f.push("Bahnschrift".into()); }
    ctx.set_fonts(fonts);
}

impl eframe::App for CgEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) { self.ui_frame(ctx); }
}

impl CgEditorApp {
    pub fn ui_frame(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let dt = (now - self.last_time.unwrap_or(now)) as f32;
        self.last_time = Some(now);

        self.handle_keys(ctx);

        // playback
        let values_title = self.element.title.clone();
        let (plan, _) = {
            let empty = serde_json::Map::new();
            let extra = crate::model::Extra::new();
            let values = Values { title: &values_title, fields: &self.element.fields, extra: &self.element.extra, base_url: &self.base_url };
            let _ = (&empty, &extra);
            match &self.element.scene {
                Some(s) => (playback::plan(&self.element, &crate::links::resolve(s, &values), &values), ()),
                None => (playback::Plan { in_total: 0.5, out_total: 0.5, base_end: 0.0, pre_dur: 0.0, base_out_start: 0.0 }, ()),
            }
        };
        self.playback.advance(dt, &plan);
        if self.playback.playing { ctx.request_repaint(); }

        // ── menu ──
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label(RichText::new("BTB CGEditor").strong().color(Color32::from_rgb(59, 130, 246)));
                ui.menu_button(tr("menu.file"), |ui| {
                    ui.menu_button(tr("menu.file.new"), |ui| {
                        for t in Element::ELEM_TYPES { if ui.button(Element::elem_type_label(t)).clicked() { self.new_element(t); ui.close_menu(); } }
                    });
                    if ui.button(format!("{}   Ctrl+O", tr("menu.file.open"))).clicked() { self.open_dialog(); ui.close_menu(); }
                    if ui.button(tr("menu.file.import")).clicked() { self.import.open = true; ui.close_menu(); }
                    if ui.add_enabled(self.element.scene.is_some(), egui::Button::new(tr("menu.file.add_image"))).clicked() {
                        let paths = crate::media::pick_images();
                        if let Some(scene) = self.element.scene.as_mut().filter(|_| !paths.is_empty()) {
                            self.status = crate::media::add_files(scene, &paths, &mut self.selected).unwrap_or_else(|| tr("status.ready").into());
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(crate::convert::needs_conversion(&self.element) && self.import.conv.is_none(), egui::Button::new(tr("menu.file.convert"))).clicked() { self.convert_current(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+P", tr("menu.file.html_preview"))).clicked() { self.open_preview(); ui.close_menu(); }
                    ui.separator();
                    if ui.add_enabled(self.push_rx.is_none(), egui::Button::new(tr("menu.file.push_bank"))).on_hover_text(tr("menu.file.push_bank.hover")).clicked() { self.push_to_btb(importer::PushTarget::Bank); ui.close_menu(); }
                    if ui.add_enabled(self.push_rx.is_none(), egui::Button::new(tr("menu.file.push_library"))).on_hover_text(tr("menu.file.push_library.hover")).clicked() { self.push_to_btb(importer::PushTarget::Library); ui.close_menu(); }
                    ui.separator();
                    if ui.button(tr("menu.file.export_pack")).on_hover_text(tr("menu.file.export_pack.hover")).clicked() { self.export_pack(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+S", tr("menu.file.save"))).clicked() { self.save(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+Shift+S", tr("menu.file.save_as"))).clicked() { self.save_as(); ui.close_menu(); }
                    if ui.button(tr("menu.file.copy_json")).clicked() {
                        self.flush_compile();
                        let doc = serde_json::json!({ "_exportVersion": 3, "elements": [&self.element] });
                        ctx.copy_text(serde_json::to_string_pretty(&doc).unwrap_or_default());
                        self.status = tr("status.json_copied").into();
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr("menu.edit"), |ui| {
                    if ui.button(format!("{}   Ctrl+Z", tr("menu.edit.undo"))).clicked() { self.undo(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+Y", tr("menu.edit.redo"))).clicked() { self.redo(); ui.close_menu(); }
                    ui.separator();
                    if ui.button(format!("{}   Ctrl+C", tr("menu.edit.copy"))).clicked() { self.copy_selected(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+V", tr("menu.edit.paste"))).clicked() { self.paste(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+D", tr("menu.edit.duplicate"))).clicked() { self.duplicate_selected(); ui.close_menu(); }
                    if ui.button(format!("{}   Del", tr("menu.edit.delete"))).clicked() { self.delete_selected(); ui.close_menu(); }
                    if ui.button(format!("{}   Ctrl+A", tr("menu.edit.select_all"))).clicked() { if let Some(s) = &self.element.scene { self.selected = s.layers.iter().map(|l| l.id.clone()).collect(); } ui.close_menu(); }
                });
                ui.menu_button(tr("menu.view"), |ui| {
                    if ui.button(format!("{}   F", tr("menu.view.fit"))).clicked() { self.canvas.needs_fit = true; ui.close_menu(); }
                    ui.checkbox(&mut self.canvas.safe_areas, tr("menu.view.safe_areas"));
                    ui.checkbox(&mut self.canvas.snap, tr("menu.view.snap"));
                    ui.menu_button(tr("menu.view.bg"), |ui| { for b in Bg::ALL { ui.selectable_value(&mut self.canvas.bg, b, b.label()); } });
                });
                ui.separator();
                let title = if self.element.name.is_empty() { self.element.id.clone() } else { self.element.name.clone() };
                ui.label(RichText::new(format!("{}{}", title, if self.dirty { " •" } else { "" })).color(Color32::from_gray(200)));
                ui.label(RichText::new(format!("[{}]", Element::elem_type_label(&self.element.elem_type))).color(Color32::from_rgb(147, 197, 253)).size(12.0));
            });
        });

        // ── status bar ──
        egui::TopBottomPanel::bottom("status").exact_height(22.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).small().color(Color32::from_gray(170)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(trf("status.zoom", &[("pct", &format!("{:.0}", self.canvas.zoom * 100.0))])).small().monospace());
                    if let Some(s) = &self.element.scene { ui.label(RichText::new(trf("status.layers", &[("layers", &s.layers.len()), ("selected", &self.selected.len())])).small().monospace()); }
                    if let Some(p) = &self.path { ui.label(RichText::new(p.display().to_string()).small().color(Color32::from_gray(120))); }
                });
            });
        });

        // ── timeline ──
        egui::TopBottomPanel::bottom("timeline").resizable(true).default_height(250.0).min_height(110.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("timeline.title")).size(10.0).strong().color(Color32::from_gray(150)));
                if ui.button("▶ IN").clicked() { self.playback.auto_cycle = false; self.playback.play_in(); }
                if ui.button("▶ OUT").clicked() { self.playback.auto_cycle = false; self.playback.play_out(); }
                if ui.button(tr("timeline.cycle")).on_hover_text(tr("timeline.cycle.hover")).clicked() { self.playback.cycle(); }
                if ui.button("■ Stop").clicked() { self.playback.stop(); }
                ui.add(egui::DragValue::new(&mut self.playback.hold_secs).speed(0.05).range(0.2..=20.0).prefix(tr("timeline.hold_prefix")).suffix(" s"));
                let (txt, col) = match self.playback.phase {
                    Phase::Idle => (tr("phase.idle"), Color32::from_gray(140)),
                    Phase::In => (tr("phase.in"), Color32::from_rgb(34, 197, 94)),
                    Phase::Hold => (tr("phase.hold"), Color32::from_rgb(251, 191, 36)),
                    Phase::Out => (tr("phase.out"), Color32::from_rgb(239, 68, 68)),
                };
                ui.label(RichText::new(txt).color(col));
            });
            ui.separator();
            let mut sel = self.selected.clone();
            draw_timeline(ui, &mut self.element, &plan, &mut self.playback, &mut sel, &mut self.timeline);
            self.selected = sel;
        });

        // ── layers ──
        egui::SidePanel::left("layers").resizable(true).default_width(230.0).min_width(170.0).show(ctx, |ui| {
            if let Some(scene) = self.element.scene.as_mut() {
                draw_layers_panel(ui, scene, &mut self.selected, &mut self.last_clicked);
            } else {
                ui.add_space(8.0);
                ui.label(tr("layers.manual_code"));
            }
        });

        // ── inspector ──
        egui::SidePanel::right("inspector").resizable(true).default_width(300.0).min_width(240.0).show(ctx, |ui| {
            draw_inspector(ui, &mut self.element, &mut self.selected, &mut self.tab);
        });

        // ── canvas ──
        let mut edited: Option<(String, String)> = None;
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::from_gray(18))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(6.0);
                if ui.small_button(tr("canvas.fit_field")).on_hover_text(tr("canvas.fit_field.hover")).clicked() { self.canvas.needs_fit = true; }
                if ui.small_button(tr("canvas.fit_frame")).on_hover_text(tr("canvas.fit_frame.hover")).clicked() {
                    let rect = ui.available_rect_before_wrap();
                    if let Some(s) = &self.element.scene { let fo = self.element.layout.frame_origin(s.frame.w, s.frame.h); self.canvas.fit_frame(rect, fo, (s.frame.w, s.frame.h)); }
                }
                if ui.small_button("1:1").clicked() { self.canvas.zoom = 1.0; }
                if ui.small_button(tr("canvas.html_preview")).on_hover_text(tr("canvas.html_preview.hover")).clicked() { self.open_preview(); }
                ui.separator();
                ui.checkbox(&mut self.canvas.safe_areas, tr("canvas.safe_areas"));
                ui.checkbox(&mut self.canvas.snap, tr("canvas.snap"));
                ui.separator();
                for b in Bg::ALL { ui.selectable_value(&mut self.canvas.bg, b, b.label()); }
            });
            if self.element.scene.is_none() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading(tr("canvas.manual.heading"));
                    ui.label(tr("canvas.manual.line1"));
                    ui.label(tr("canvas.manual.line2"));
                    if ui.button(tr("canvas.manual.preview")).clicked() { self.open_preview(); }
                    if ui.button(tr("canvas.manual.create_scene")).clicked() {
                        let backup = serde_json::json!({ "html_template": self.element.html_template, "css_template": self.element.css_template, "js_template": self.element.js_template });
                        self.element.extra.insert("codeBackup".into(), backup);
                        let scene = crate::presets::scene_for(&self.element.elem_type);
                        compile::apply_to_element(&mut self.element, scene);
                    }
                });
                return;
            }
            let title = self.element.title.clone();
            let fields = self.element.fields.clone();
            let extra = self.element.extra.clone();
            let base_url = self.base_url.clone();
            let values = Values { title: &title, fields: &fields, extra: &extra, base_url: &base_url };
            let Some(scene_ref) = self.element.scene.as_ref() else { return };
            let scene_snapshot = crate::links::resolve(scene_ref, &values);
            let plan = playback::plan(&self.element, &scene_snapshot, &values);
            let (elem_tr, fx) = if motion_bridge::enabled() || motion_bridge::uses_keys(&self.element) {
                motion_bridge::evaluate(&self.playback, &self.element, &scene_snapshot, &values, &plan)
            } else {
                playback::evaluate(&self.playback, &self.element, &scene_snapshot, &values, &plan)
            };
            // egui nie rysuje masek ani filtrów: efekty "dokładne" zamieniamy na przybliżenia
            let elem_tr = elem_tr.approx();
            let fx: std::collections::HashMap<_, _> = fx.into_iter().map(|(k, mut v)| { v.tr = v.tr.map(|t| t.approx()); (k, v) }).collect();
            // zaznaczony klucz ścieżki: oś czasu jest źródłem prawdy, kanwa może go zmienić kliknięciem
            self.canvas.path_sel = self.timeline.sel.iter().find(|k| k.prop == timeline::PATH && self.selected.len() == 1 && self.selected[0] == k.layer).map(|k| k.idx);
            let Some(scene) = self.element.scene.as_mut() else { return };
            let out = canvas::draw_canvas(ui, &mut self.canvas, scene, &mut self.element.layout, &mut self.selected, DrawInputs {
                sp: SceneParamsBuilder { elem_tr, fx: &fx, values, time: ui.input(|i| i.time), playing: self.playback.active(), badge_squash: self.element.elem_type == "BADGE" },
            });
            edited = out.edited_text;
            if let Some((layer, idx)) = self.canvas.path_picked.take() {
                self.timeline.sel = vec![timeline::KeyRef { layer, prop: timeline::PATH.to_string(), idx }];
            }
        });
        if let Some((id, text)) = edited { self.apply_text_edit(&id, text); }

        crate::panels::graph::draw_graph(ctx, &mut self.element, &mut self.timeline, &mut self.graph);
        self.import_window(ctx);
        self.track_changes(ctx, now);
        self.screenshot_flow(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, RawInput, Rect, Vec2};

    struct H { ctx: egui::Context, app: CgEditorApp, t: f64 }

    impl H {
        fn new() -> Self {
            let ctx = egui::Context::default();
            setup_fonts(&ctx);
            let mut app = CgEditorApp::build(&ctx, Cli::default());
            app.canvas.needs_fit = false;
            app.canvas.needs_fit_frame = true;
            let mut h = H { ctx, app, t: 0.0 };
            for _ in 0..4 { h.run(vec![]); }
            h
        }
        fn run(&mut self, events: Vec<Event>) {
            let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0))), time: Some(self.t), events, ..Default::default() };
            self.t += 0.05;
            let _ = self.ctx.run(raw, |ctx| self.app.ui_frame(ctx));
        }
        fn mv(&mut self, p: Pos2) { self.run(vec![Event::PointerMoved(p)]); }
        fn press(&mut self, p: Pos2) { self.run(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Default::default() }]); }
        fn release(&mut self, p: Pos2) { self.run(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Default::default() }]); }
        fn drag(&mut self, from: Pos2, to: Pos2) {
            self.mv(from);
            self.press(from);
            for i in 1..=8 { let k = i as f32 / 8.0; self.mv(from + (to - from) * k); }
            self.release(to);
            for _ in 0..12 { self.run(vec![]); }
        }
        fn to_screen(&self, layer_idx: usize, fx: f32, fy: f32) -> Pos2 {
            let s = self.app.element.scene.as_ref().unwrap();
            let l = &s.layers[layer_idx];
            let fo = self.app.element.layout.frame_origin(s.frame.w, s.frame.h);
            let z = self.app.canvas.zoom;
            let origin = self.app.canvas.last_rect.min + self.app.canvas.pan;
            origin + Vec2::new((fo.0 + l.x + l.w * fx) * z, (fo.1 + l.y + l.h * fy) * z)
        }
        fn layer(&self, i: usize) -> crate::model::Layer { self.app.element.scene.as_ref().unwrap().layers[i].clone() }
    }

    #[test]
    fn click_selects_and_drag_moves_layer() {
        let mut h = H::new();
        // layers[0] = accent (left edge), [1] = background, [2] = title. Click the title area.
        let p = h.to_screen(2, 0.5, 0.5);
        h.mv(p); h.press(p); h.release(p);
        for _ in 0..3 { h.run(vec![]); }
        let sel = h.app.selected.clone();
        assert_eq!(sel.len(), 1, "one layer selected");
        let idx = h.app.element.scene.as_ref().unwrap().layers.iter().position(|l| l.id == sel[0]).unwrap();
        let before = h.layer(idx);
        let from = h.to_screen(idx, 0.5, 0.5);
        h.drag(from, from + Vec2::new(60.0, 30.0));
        let after = h.layer(idx);
        let z = h.app.canvas.zoom;
        assert!((after.x - before.x - 60.0 / z).abs() < 2.5, "x moved by ~60/zoom: {} -> {} (zoom {})", before.x, after.x, z);
        assert!((after.y - before.y - 30.0 / z).abs() < 2.5, "y moved: {} -> {}", before.y, after.y);
    }

    #[test]
    fn undo_restores_position_after_drag() {
        let mut h = H::new();
        let p = h.to_screen(1, 0.5, 0.5);
        h.mv(p); h.press(p); h.release(p);
        for _ in 0..3 { h.run(vec![]); }
        let idx = h.app.element.scene.as_ref().unwrap().layers.iter().position(|l| l.id == h.app.selected[0]).unwrap();
        let x0 = h.layer(idx).x;
        let from = h.to_screen(idx, 0.5, 0.5);
        h.drag(from, from + Vec2::new(80.0, 0.0));
        for _ in 0..20 { h.run(vec![]); }
        assert!(h.layer(idx).x != x0, "moved");
        assert!(h.app.history.len() >= 2, "history committed after drag: {}", h.app.history.len());
        h.app.undo();
        assert_eq!(h.layer(idx).x, x0, "undo restores x");
        h.app.redo();
        assert!(h.layer(idx).x != x0, "redo re-applies");
    }

    #[test]
    fn resize_right_handle_changes_width() {
        let mut h = H::new();
        let p = h.to_screen(1, 0.5, 0.5);
        h.mv(p); h.press(p); h.release(p);
        for _ in 0..3 { h.run(vec![]); }
        let idx = h.app.element.scene.as_ref().unwrap().layers.iter().position(|l| l.id == h.app.selected[0]).unwrap();
        let w0 = h.layer(idx).w;
        let handle = h.to_screen(idx, 1.0, 0.5);
        h.drag(handle, handle + Vec2::new(-40.0, 0.0));
        let z = h.app.canvas.zoom;
        let w1 = h.layer(idx).w;
        assert!((w0 - w1 - 40.0 / z).abs() < 3.0, "width shrank by ~40/zoom: {} -> {}", w0, w1);
    }

    #[test]
    fn added_text_layer_is_static_and_edit_changes_only_its_text() {
        let mut h = H::new();
        let mut sel = vec![];
        let id = crate::panels::layers::add_layer(h.app.element.scene.as_mut().unwrap(), crate::model::LayerType::Text, &mut sel);
        h.app.apply_text_edit(&id, "Nowy".into());
        let l = h.app.element.scene.as_ref().unwrap().layers.iter().find(|l| l.id == id).unwrap().clone();
        assert!(l.field.is_none());
        assert_eq!(l.text.as_deref(), Some("Nowy"));
    }

    #[test]
    fn compile_runs_on_commit_and_updates_code() {
        let mut h = H::new();
        let html_before = h.app.element.html_template.clone();
        h.app.element.scene.as_mut().unwrap().layers[2].default_text = Some("ZMIENIONY".into());
        for _ in 0..30 { h.run(vec![]); }
        assert!(h.app.element.ocg_inputs.iter().any(|i| i["default"] == "ZMIENIONY"), "ocgInputs default updated by recompile");
        assert_eq!(html_before, h.app.element.html_template, "html structure unchanged");
    }

    #[test]
    fn playback_cycle_goes_through_phases() {
        let mut h = H::new();
        h.app.playback.cycle();
        let mut seen = vec![];
        for _ in 0..200 { h.run(vec![]); let ph = h.app.playback.phase; if seen.last() != Some(&ph) { seen.push(ph); } }
        assert_eq!(seen, vec![Phase::In, Phase::Hold, Phase::Out, Phase::Idle]);
    }
}
