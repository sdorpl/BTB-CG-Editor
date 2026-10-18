//! Pakiet fontów (M3.1): manifest `assets/fonts/manifest.json` budowany w BTB-CG (`tools/build-font-pack.js`) — OFL, Latin + Latin-Ext,
//! z metrykami i pokryciem polskich znaków. Edytor ładuje z niego **wszystkie** rodziny do egui (dotąd znał tylko Bahnschrift i
//! Liberation), wybiera najbliższą wagę/kursywę i osadza w stronach wyjściowych dokładnie te kroje, których używa element (D10).
//! Bahnschrift (font lokalny, Microsoft) i Liberation Sans (zamiennik Arial/Helvetica) zostają wkompilowane jak dotąd.
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
struct RawFace {
    weight: serde_json::Value,
    #[serde(default)]
    style: String,
    #[serde(default)]
    ttf: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawMetrics {
    #[serde(rename = "unitsPerEm", default)]
    units_per_em: f32,
    #[serde(default)]
    ascender: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFamily {
    id: String,
    family: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    faces: Vec<RawFace>,
    #[serde(default)]
    metrics: Option<RawMetrics>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAliasFace {
    weight: u16,
    #[serde(default)]
    local: Vec<String>,
}

/// Alias fontu systemowego (D9): nazwa systemowa → `local()` z zapasem z pakietu (`use` = id rodziny zamiennika).
#[derive(Debug, Clone, Deserialize)]
struct RawAlias {
    family: String,
    #[serde(rename = "use")]
    use_id: String,
    #[serde(default)]
    faces: Vec<RawAliasFace>,
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    #[serde(default)]
    families: Vec<RawFamily>,
    #[serde(default)]
    aliases: Vec<RawAlias>,
}

/// Jeden krój (waga + kursywa) z plikami TTF (kolejność: latin, latin-ext, all).
#[derive(Debug, Clone)]
pub struct Face {
    pub weight: u16,
    pub italic: bool,
    /// Nazwa rodziny egui (`FontFamily::Name`) tego kroju.
    pub egui_name: String,
    /// (podzbiór, ścieżka względem katalogu fontów).
    pub files: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Family {
    pub id: String,
    pub name: String,
    /// `pack` (OFL, w repo) albo `local` (Bahnschrift).
    pub source: String,
    pub faces: Vec<Face>,
    /// hhea.ascender / unitsPerEm.
    pub ascent: f32,
    pub aliases: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Registry {
    pub dir: Option<PathBuf>,
    pub families: Vec<Family>,
    by_name: HashMap<String, usize>,
    aliases: Vec<RawAlias>,
}

/// Wynik wyboru kroju dla (rodzina, waga, kursywa).
#[derive(Debug, Clone)]
pub struct Resolved {
    pub family: egui::FontFamily,
    /// hhea.ascender / unitsPerEm.
    pub ascent: f32,
    /// Krój ma prawdziwą grubość (bez nakładania pogrubienia).
    pub real_bold: bool,
    /// Krój ma prawdziwą kursywę (bez sztucznego pochylenia).
    pub real_italic: bool,
    /// Id rodziny z pakietu (`None` = Bahnschrift/Liberation wkompilowane albo rodzina nieznana).
    pub pack_id: Option<String>,
}

const BAHN_ASCENT: f32 = 2036.0 / 2048.0;
const LIBERATION_ASCENT: f32 = 1854.0 / 2048.0;

fn norm(name: &str) -> String {
    name.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '\'' || c == '"').trim().to_ascii_lowercase()
}

fn parse_weight(v: &serde_json::Value) -> u16 {
    match v {
        serde_json::Value::Number(n) => n.as_u64().unwrap_or(400) as u16,
        serde_json::Value::String(s) => s.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(400),
        _ => 400,
    }
}

/// Wagi z CSS (`"700"`, `"bold"`, `"normal"`) → liczba 100–900.
pub fn css_weight(w: &str) -> u16 {
    let w = w.trim();
    if w.eq_ignore_ascii_case("bold") { return 700; }
    if w.eq_ignore_ascii_case("normal") { return 400; }
    w.parse::<u16>().map(|v| v.clamp(100, 900)).unwrap_or(400)
}

fn locate_dir() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = vec![];
    if let Ok(d) = std::env::var("BTB_FONTS_DIR") { cands.push(PathBuf::from(d)); }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("fonts"));                       // Windows: obok pliku .exe
            cands.push(dir.join("assets/fonts"));
            cands.push(dir.join("../Resources/fonts"));          // macOS: .app/Contents/Resources/fonts
            if let Some(stem) = exe.file_stem().and_then(|s| s.to_str()) { cands.push(dir.join(format!("../share/{stem}/fonts"))); }   // deb / AppImage
        }
    }
    cands.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts")));
    cands.into_iter().find(|d| d.join("manifest.json").is_file())
}

impl Registry {
    /// Wczytuje manifest; błędy nigdy nie przerywają aplikacji (pakiet jest opcjonalny — zostają Bahnschrift i Liberation).
    pub fn load(dir: Option<PathBuf>) -> Registry {
        let mut reg = Registry { dir: dir.clone(), ..Default::default() };
        let Some(dir) = dir else { return reg };
        let Ok(text) = std::fs::read_to_string(dir.join("manifest.json")) else { return reg };
        let Ok(raw) = serde_json::from_str::<RawManifest>(&text) else { return reg };
        reg.aliases = raw.aliases.into_iter().take(64).collect();
        for f in raw.families.into_iter().take(256) {
            let ascent = f.metrics.as_ref().filter(|m| m.units_per_em > 0.0 && m.ascender > 0.0).map(|m| m.ascender / m.units_per_em).unwrap_or(0.95);
            let faces: Vec<Face> = f.faces.iter().take(64).map(|fc| {
                let weight = parse_weight(&fc.weight);
                let italic = fc.style.eq_ignore_ascii_case("italic");
                let mut files: Vec<(String, String)> = ["latin", "latin-ext", "all"].iter().filter_map(|s| fc.ttf.get(*s).map(|p| (s.to_string(), p.clone()))).collect();
                files.truncate(4);
                Face { weight, italic, egui_name: format!("pack:{}:{}:{}", f.id, weight, if italic { "i" } else { "n" }), files }
            }).collect();
            let idx = reg.families.len();
            reg.by_name.insert(norm(&f.family), idx);
            reg.by_name.insert(f.id.to_ascii_lowercase(), idx);
            for a in &f.aliases { reg.by_name.insert(norm(a), idx); }
            reg.families.push(Family { id: f.id, name: f.family, source: if f.source.is_empty() { "pack".into() } else { f.source }, faces, ascent, aliases: f.aliases });
        }
        reg
    }

    pub fn find(&self, family: &str) -> Option<&Family> { self.by_name.get(&norm(family)).and_then(|i| self.families.get(*i)) }

    /// Najbliższy krój: wybrana kursywa (jeśli rodzina ją ma), potem najmniejsza różnica wag, przy remisie cięższy.
    pub fn pick_face<'a>(&self, fam: &'a Family, weight: u16, italic: bool) -> Option<&'a Face> {
        let has_italic = fam.faces.iter().any(|f| f.italic);
        let want_italic = italic && has_italic;
        fam.faces.iter().filter(|f| f.italic == want_italic && !f.files.is_empty())
            .min_by_key(|f| ((f.weight as i32 - weight as i32).abs(), -(f.weight as i32)))
    }

    /// Krój do rysowania w egui. Arial/Helvetica/sans-serif → Liberation Sans (jak `@font-face`/system na stronie), rodziny z pakietu →
    /// ich krój z pakietu, wszystko inne (także Bahnschrift) → Bahnschrift.
    pub fn resolve(&self, family: &str, weight: &str, italic: bool) -> Resolved {
        let w = css_weight(weight);
        let n = norm(family);
        if matches!(n.as_str(), "arial" | "helvetica" | "liberation sans" | "sans-serif" | "arial narrow") {
            let bold = w >= 600;
            return Resolved { family: egui::FontFamily::Name(if bold { "LiberationBold" } else { "LiberationRegular" }.into()), ascent: LIBERATION_ASCENT, real_bold: true, real_italic: false, pack_id: None };
        }
        if let Some(fam) = self.find(family).filter(|f| f.source == "pack") {
            if let Some(face) = self.pick_face(fam, w, italic) {
                return Resolved { family: egui::FontFamily::Name(face.egui_name.clone().into()), ascent: fam.ascent, real_bold: true, real_italic: face.italic, pack_id: Some(fam.id.clone()) };
            }
        }
        Resolved { family: egui::FontFamily::Name("Bahnschrift".into()), ascent: BAHN_ASCENT, real_bold: false, real_italic: false, pack_id: None }
    }

    /// Dodaje wszystkie krój z pakietu do definicji egui: każdy krój = rodzina egui z plikami (latin, latin-ext) i Bahnschriftem jako rezerwą.
    pub fn register(&self, fonts: &mut egui::FontDefinitions) -> usize {
        let Some(dir) = &self.dir else { return 0 };
        let mut n = 0;
        for fam in self.families.iter().filter(|f| f.source == "pack") {
            for face in &fam.faces {
                let mut names = vec![];
                for (sub, rel) in &face.files {
                    let key = format!("{}:{}", face.egui_name, sub);
                    // pliki z podzbioru `all` mogą być duże — czytamy je tak samo; błąd odczytu = pomijamy krój
                    let Ok(bytes) = std::fs::read(dir.join(rel)) else { continue };
                    fonts.font_data.insert(key.clone(), egui::FontData::from_owned(bytes));
                    names.push(key);
                }
                if names.is_empty() { continue; }
                names.push("Bahnschrift".into());
                fonts.families.insert(egui::FontFamily::Name(face.egui_name.clone().into()), names);
                n += 1;
            }
        }
        n
    }

    /// Nazwy rodzin do list wyboru: (nazwa, czy z pakietu). Bahnschrift pierwszy (domyślny krój oprawy), potem pakiet alfabetycznie.
    pub fn family_names(&self) -> Vec<(String, bool)> {
        let mut v: Vec<(String, bool)> = vec![("Bahnschrift".into(), false)];
        let mut pack: Vec<String> = self.families.iter().filter(|f| f.source == "pack" && f.id != "liberation-sans").map(|f| f.name.clone()).collect();
        pack.sort();
        v.extend(pack.into_iter().map(|n| (n, true)));
        v
    }

    fn alias_css(&self, al: &RawAlias, weight: u16) -> String {
        use base64::Engine;
        let (Some(dir), Some(fam)) = (&self.dir, self.families.iter().find(|f| f.id == al.use_id)) else { return String::new() };
        let Some(af) = al.faces.iter().min_by_key(|f| ((f.weight as i32 - weight as i32).abs(), -(f.weight as i32))) else { return String::new() };
        let Some(face) = self.pick_face(fam, af.weight, false) else { return String::new() };
        let locals: Vec<String> = af.local.iter().map(|n| format!("local('{}')", n.replace('\'', ""))).collect();
        let mut css = String::new();
        for (_, rel) in &face.files {
            let Ok(bytes) = std::fs::read(dir.join(rel)) else { continue };
            css += &format!("@font-face {{ font-family: '{}'; font-style: normal; font-weight: {}; font-display: block; src: {}, url(data:font/ttf;base64,{}) format('truetype'); }}\n",
                al.family.replace('\'', ""), af.weight, locals.join(", "), base64::engine::general_purpose::STANDARD.encode(&bytes));
        }
        css
    }

    /// `@font-face` (dane base64) dla krojów wskazanych par (rodzina, waga, kursywa): osadzane w stronach wyjściowych/podglądu (D10 —
    /// font elementu jest częścią jego strony). Rodziny nieznane i systemowe (Arial…) pomijamy; Bahnschrift ma osobne osadzanie w wywołującym.
    pub fn embed_css(&self, used: &[(String, u16, bool)]) -> String {
        use base64::Engine;
        let Some(dir) = &self.dir else { return String::new() };
        let mut done: Vec<String> = vec![];
        let mut css = String::new();
        for (family, weight, italic) in used {
            // alias fontu systemowego (Arial, Segoe UI…): local() + zamiennik z pakietu osadzony w stronie — działa na maszynach bez tego fontu
            if let Some(al) = self.aliases.iter().find(|a| norm(&a.family) == norm(family)) {
                let key = format!("alias:{}:{}", norm(&al.family), weight);
                if !done.contains(&key) {
                    done.push(key);
                    css += &self.alias_css(al, *weight);
                }
                continue;
            }
            let Some(fam) = self.find(family).filter(|f| f.source == "pack") else { continue };
            let Some(face) = self.pick_face(fam, *weight, *italic) else { continue };
            if done.contains(&face.egui_name) { continue; }
            done.push(face.egui_name.clone());
            for (sub, rel) in &face.files {
                let Ok(bytes) = std::fs::read(dir.join(rel)) else { continue };
                let range = match sub.as_str() {
                    "latin" => "unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;",
                    "latin-ext" => "unicode-range: U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF;",
                    _ => "",
                };
                css += &format!("@font-face {{ font-family: '{}'; font-style: {}; font-weight: {}; font-display: block; src: url(data:font/ttf;base64,{}) format('truetype'); {} }}\n",
                    fam.name.replace('\'', ""), if face.italic { "italic" } else { "normal" }, face.weight, base64::engine::general_purpose::STANDARD.encode(&bytes), range);
            }
        }
        css
    }
}

/// Kroje używane przez scenę elementu: (rodzina, waga, kursywa) widocznych warstw tekstu, zegara i tickera (D10).
pub fn used_by(scene: &crate::model::Scene) -> Vec<(String, u16, bool)> {
    use crate::model::LayerType;
    let mut v: Vec<(String, u16, bool)> = vec![];
    for l in scene.layers.iter().filter(|l| l.visible && matches!(l.layer_type, LayerType::Text | LayerType::Clock | LayerType::Ticker)) {
        let f = l.font_or_default();
        let t = (f.family.clone(), css_weight(&f.weight), f.italic);
        if !v.contains(&t) { v.push(t); }
    }
    v
}

/// CSS `@font-face` krojów z pakietu używanych przez element — osadzany w stronie wyjściowej/podglądu (D10: font jest częścią strony belki).
pub fn page_css(el: &crate::model::Element) -> String {
    match &el.scene { Some(s) => registry().embed_css(&used_by(s)), None => String::new() }
}

// ── Fonty systemowe (D9) ─────────────────────────────────────────────────────────────────────────────────────────────

/// Baza fontów systemu (`fontdb`), ładowana w tle (skanowanie bywa wolne); do czasu załadowania lista jest pusta.
struct SystemDb {
    db: fontdb::Database,
    families: Vec<String>,
}

static SYS: OnceLock<SystemDb> = OnceLock::new();

fn load_system_db() -> SystemDb {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut families: Vec<String> = db.faces().flat_map(|f| f.families.iter().map(|(n, _)| n.clone())).collect();
    families.sort_by_key(|s| s.to_lowercase());
    families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    families.truncate(4000);
    SystemDb { db, families }
}

/// Rozpoczyna skanowanie fontów systemowych w tle (wołane przy starcie aplikacji).
pub fn warm_up_system_fonts() {
    std::thread::spawn(|| { let _ = SYS.get_or_init(load_system_db); });
}

/// Rodziny zainstalowane w systemie (puste, dopóki skan w tle się nie skończy).
pub fn system_families() -> Vec<String> { SYS.get().map(|s| s.families.clone()).unwrap_or_default() }

#[derive(Default)]
struct Extra {
    /// klucz rodziny egui → (bajty pliku, indeks w kolekcji).
    fonts: std::collections::BTreeMap<String, (std::sync::Arc<Vec<u8>>, u32)>,
    /// hhea.ascender / unitsPerEm wczytanych fontów systemowych.
    ascent: std::collections::HashMap<String, f32>,
    /// rodziny, których nie ma w systemie / nie dało się wczytać (nie pytamy co klatkę).
    missing: std::collections::HashSet<String>,
}

fn extra() -> &'static std::sync::Mutex<Extra> {
    static E: OnceLock<std::sync::Mutex<Extra>> = OnceLock::new();
    E.get_or_init(|| std::sync::Mutex::new(Extra::default()))
}

/// Rodziny obsługiwane bez systemu: Bahnschrift, Liberation i ich aliasy (Arial, Helvetica…).
pub fn is_builtin_family(family: &str) -> bool {
    matches!(norm(family).as_str(), "bahnschrift" | "arial" | "helvetica" | "liberation sans" | "sans-serif" | "arial narrow" | "")
}

fn bucket(weight: u16) -> u16 { ((weight as u32 + 50) / 100 * 100).clamp(100, 900) as u16 }

/// Definicje egui: Bahnschrift + Liberation (wkompilowane) + pakiet + dociągnięte fonty systemowe.
pub fn build_definitions() -> egui::FontDefinitions {
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
    registry().register(&mut fonts);
    if let Ok(ex) = extra().lock() {
        for (key, (bytes, index)) in &ex.fonts {
            let mut data = egui::FontData::from_owned(bytes.as_ref().clone());
            data.index = *index;
            fonts.font_data.insert(key.clone(), data);
            fonts.families.insert(egui::FontFamily::Name(key.clone().into()), vec![key.clone(), "Bahnschrift".to_string()]);
        }
    }
    fonts
}

/// Rodzina egui dla fontu systemowego, jeśli jest już wczytana. Gdy nie — próbuje ją wczytać z systemu (raz), przebudowuje definicje egui
/// i zwraca `None` (w tej klatce rysujemy zamiennikiem, od następnej krojem systemowym).
pub fn request_system(ctx: &egui::Context, family: &str, weight: u16, italic: bool) -> Option<egui::FontFamily> {
    let name = norm(family);
    if name.is_empty() { return None; }
    let key = format!("system:{}:{}:{}", name, bucket(weight), if italic { "i" } else { "n" });
    {
        let ex = extra().lock().ok()?;
        if ex.fonts.contains_key(&key) { return Some(egui::FontFamily::Name(key.into())); }
        if ex.missing.contains(&key) { return None; }
    }
    request_system_load(ctx, family, weight, italic, key);
    None
}

/// Ascent (hhea.ascender / unitsPerEm) wczytanego fontu systemowego; `None`, gdy font nie jest wczytany.
pub fn system_ascent(family: &str, weight: u16, italic: bool) -> Option<f32> {
    let key = format!("system:{}:{}:{}", norm(family), bucket(weight), if italic { "i" } else { "n" });
    extra().lock().ok()?.ascent.get(&key).copied()
}

fn request_system_load(ctx: &egui::Context, family: &str, weight: u16, italic: bool, key: String) {
    let Some(sys) = SYS.get() else { return };   // skan w tle jeszcze trwa — spróbujemy w następnej klatce
    let q = fontdb::Query {
        families: &[fontdb::Family::Name(family.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '\'' || c == '"'))],
        weight: fontdb::Weight(bucket(weight)),
        stretch: fontdb::Stretch::Normal,
        style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
    };
    let loaded = sys.db.query(&q).and_then(|id| sys.db.with_face_data(id, |data, index| {
        let ascent = ttf_parser::Face::parse(data, index).ok().map(|f| f.ascender() as f32 / f.units_per_em().max(1) as f32).filter(|a| *a > 0.3 && *a < 1.6).unwrap_or(0.95);
        (data.to_vec(), index, ascent)
    }));
    let Ok(mut ex) = extra().lock() else { return };
    match loaded {
        Some((bytes, index, ascent)) if bytes.len() <= 40 * 1024 * 1024 => {
            ex.fonts.insert(key.clone(), (std::sync::Arc::new(bytes), index));
            ex.ascent.insert(key, ascent);
            drop(ex);
            ctx.set_fonts(build_definitions());
            ctx.request_repaint();
        }
        _ => { ex.missing.insert(key); }
    }
}

/// Rejestr procesu (wczytywany leniwie z `BTB_FONTS_DIR`, `<exe>/fonts`, `<exe>/assets/fonts` albo `assets/fonts` z drzewa źródeł).
pub fn registry() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| Registry::load(locate_dir()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Registry { Registry::load(Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts")))) }

    #[test]
    fn manifest_loads_pack_families() {
        let r = reg();
        let names: Vec<String> = r.families.iter().map(|f| f.name.clone()).collect();
        for n in ["Inter", "Roboto", "Roboto Condensed", "Barlow Condensed", "Montserrat", "Oswald", "Source Sans 3", "Encode Sans Condensed"] {
            assert!(names.iter().any(|x| x == n), "brak rodziny {n} w {names:?}");
        }
        assert!(r.find("inter").is_some() && r.find("'Roboto Condensed', sans-serif").is_some(), "wyszukiwanie po nazwie bez wielkości liter i z listą CSS");
        // każdy krój z pakietu ma pliki TTF obu podzbiorów
        for f in r.families.iter().filter(|f| f.source == "pack" && f.id != "liberation-sans") {
            for face in &f.faces { assert!(face.files.iter().any(|(s, _)| s == "latin") && face.files.iter().any(|(s, _)| s == "latin-ext"), "{} {}", f.id, face.weight); }
        }
    }

    #[test]
    fn picks_nearest_weight_and_italic() {
        let r = reg();
        let inter = r.find("Inter").unwrap();
        assert_eq!(r.pick_face(inter, 700, false).unwrap().weight, 700);
        assert_eq!(r.pick_face(inter, 650, false).unwrap().weight, 700, "remis (600/700) → cięższy");
        assert_eq!(r.pick_face(inter, 300, false).unwrap().weight, 400, "najbliższa dostępna");
        let it = r.pick_face(inter, 700, true).unwrap();
        assert!(it.italic && it.weight == 700);
        let oswald = r.find("Oswald").unwrap();
        assert!(!r.pick_face(oswald, 400, true).unwrap().italic, "brak kursywy w rodzinie → krój prosty (pochylenie sztuczne)");
    }

    #[test]
    fn resolve_routes_families() {
        let r = reg();
        assert!(r.resolve("Arial", "400", false).pack_id.is_none() && r.resolve("Helvetica", "700", false).real_bold);
        assert!(r.resolve("Bahnschrift", "700", false).pack_id.is_none() && !r.resolve("Bahnschrift", "700", false).real_bold);
        assert!(r.resolve("Nieznany Krój", "400", false).pack_id.is_none(), "nieznana rodzina → Bahnschrift jak dotąd");
        let c = r.resolve("Roboto Condensed", "bold", true);
        assert_eq!(c.pack_id.as_deref(), Some("roboto-condensed"));
        assert!(c.real_bold && c.real_italic && c.ascent > 0.8 && c.ascent < 1.2, "{:?}", c.ascent);
    }

    #[test]
    fn embed_css_contains_only_used_faces() {
        let r = reg();
        let css = r.embed_css(&[("Oswald".into(), 600, false), ("oswald".into(), 600, false), ("Arial".into(), 400, false)]);
        assert_eq!(css.matches("font-family: 'Oswald'").count(), 2, "latin + latin-ext jednego kroju, bez duplikatu: {}", css.len());
        assert!(css.contains("font-weight: 600") && css.contains("unicode-range") && !css.contains("Montserrat"));
        assert!(r.embed_css(&[("Nieznany".into(), 400, false)]).is_empty());
    }

    #[test]
    fn alias_embeds_local_with_pack_fallback() {
        let r = reg();
        let css = r.embed_css(&[("Arial".into(), 700, false), ("arial".into(), 700, false)]);
        assert!(css.contains("font-family: 'Arial'") && css.contains("local('Arial Bold')") && css.contains("font-weight: 700") && css.contains("base64,"), "{}", &css[..css.len().min(160)]);
        assert_eq!(css.matches("@font-face").count(), 1, "jeden krój (Liberation ma plik `all`), bez duplikatu");
        let seg = r.embed_css(&[("Segoe UI".into(), 400, false)]);
        assert!(seg.contains("local('Segoe UI')") && seg.matches("@font-face").count() == 2, "Source Sans 3: latin + latin-ext");
    }

    #[test]
    fn system_font_is_loaded_on_demand() {
        let sys = SYS.get_or_init(load_system_db);
        let Some(name) = ["DejaVu Sans", "Liberation Serif", "Noto Sans", "Arial", "Segoe UI", "Helvetica"].iter().find(|n| sys.families.iter().any(|f| f.eq_ignore_ascii_case(n))) else { return };
        let ctx = egui::Context::default();
        ctx.set_fonts(build_definitions());
        let _ = ctx.run(Default::default(), |_| {});
        assert!(!system_families().is_empty() && system_families().iter().any(|f| f.eq_ignore_ascii_case(name)));
        assert!(!is_builtin_family(name) || *name == "Arial" || *name == "Helvetica");
        if is_builtin_family(name) { return; }
        assert!(request_system(&ctx, name, 400, false).is_none(), "pierwsze wywołanie ładuje font i zwraca None (zamiennik w tej klatce)");
        let _ = ctx.run(Default::default(), |_| {});
        let fam = request_system(&ctx, name, 400, false).expect("po załadowaniu rodzina egui");
        assert!(matches!(fam, egui::FontFamily::Name(ref n) if n.starts_with("system:")));
        assert!(system_ascent(name, 400, false).is_some_and(|a| a > 0.5 && a < 1.5));
        assert!(request_system(&ctx, "Zupelnie Nieistniejacy Kroj", 400, false).is_none());
    }

    #[test]
    fn missing_dir_is_harmless() {
        let r = Registry::load(Some(PathBuf::from("/nie/ma/takiego/katalogu")));
        assert!(r.families.is_empty() && r.resolve("Inter", "400", false).pack_id.is_none());
        assert_eq!(r.family_names()[0].0, "Bahnschrift");
        let mut fd = egui::FontDefinitions::default();
        assert_eq!(r.register(&mut fd), 0);
    }
}
