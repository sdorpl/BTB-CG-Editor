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

#[derive(Debug, Deserialize)]
struct RawManifest {
    #[serde(default)]
    families: Vec<RawFamily>,
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

    /// `@font-face` (dane base64) dla krojów wskazanych par (rodzina, waga, kursywa): osadzane w stronach wyjściowych/podglądu (D10 —
    /// font elementu jest częścią jego strony). Rodziny nieznane i systemowe (Arial…) pomijamy; Bahnschrift ma osobne osadzanie w wywołującym.
    pub fn embed_css(&self, used: &[(String, u16, bool)]) -> String {
        use base64::Engine;
        let Some(dir) = &self.dir else { return String::new() };
        let mut done: Vec<String> = vec![];
        let mut css = String::new();
        for (family, weight, italic) in used {
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
    fn missing_dir_is_harmless() {
        let r = Registry::load(Some(PathBuf::from("/nie/ma/takiego/katalogu")));
        assert!(r.families.is_empty() && r.resolve("Inter", "400", false).pack_id.is_none());
        assert_eq!(r.family_names()[0].0, "Bahnschrift");
        let mut fd = egui::FontDefinitions::default();
        assert_eq!(r.register(&mut fd), 0);
    }
}
