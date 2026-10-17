//! Pliki graficzne z dysku: wczytanie, rozpoznanie typu po zawartości, osadzenie jako `data:` URI i warstwa obrazu.
//! Osadzony plik zostaje w elemencie (JSON jest samowystarczalny); serwer BTB-CG sam przenosi `data:` do `uploads/` (`media-store.js`).
use crate::i18n::{tr, trf};
use crate::model::{Layer, LayerType, Scene};
use base64::Engine;
use std::path::Path;

/// Twardy limit rozmiaru pliku osadzanego w elemencie.
pub const MAX_BYTES: usize = 12 * 1024 * 1024;
/// Powyżej tego rozmiaru pokazujemy ostrzeżenie (JSON z obrazem rośnie o 1/3 przez base64).
pub const WARN_BYTES: usize = 2 * 1024 * 1024;
/// Rozszerzenia w oknie wyboru pliku.
pub const IMAGE_EXTS: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"];

#[derive(Debug, Clone, PartialEq)]
pub struct MediaAsset {
    /// nazwa pliku bez ścieżki
    pub name: String,
    pub mime: &'static str,
    pub data_uri: String,
    pub w: u32,
    pub h: u32,
    pub bytes: usize,
}

impl MediaAsset {
    pub fn is_large(&self) -> bool { self.bytes > WARN_BYTES }
    /// Nazwa warstwy: nazwa pliku bez rozszerzenia.
    pub fn layer_name(&self) -> String {
        Path::new(&self.name).file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| self.name.clone())
    }
}

/// Typ MIME po początku pliku (rozszerzeniu nie ufamy).
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) { return Some("image/png"); }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) { return Some("image/jpeg"); }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") { return Some("image/gif"); }
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" { return Some("image/webp"); }
    if b.starts_with(b"BM") && b.len() > 14 { return Some("image/bmp"); }
    let head = String::from_utf8_lossy(&b[..b.len().min(1024)]).to_lowercase();
    if head.contains("<svg") { return Some("image/svg+xml"); }
    None
}

/// Wartość atrybutu liczbowego (`width="120px"`) z początku pliku SVG.
fn svg_attr(head: &str, name: &str) -> Option<f32> {
    let i = head.find(&format!("{name}=\""))? + name.len() + 2;
    let v: String = head[i..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    v.parse().ok()
}

fn svg_size(b: &[u8]) -> (u32, u32) {
    let text = String::from_utf8_lossy(&b[..b.len().min(4096)]).to_string();
    let tag = text.find("<svg").map(|i| &text[i..]).unwrap_or("");
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    if let (Some(w), Some(h)) = (svg_attr(tag, "width"), svg_attr(tag, "height")) { return (w.max(1.0) as u32, h.max(1.0) as u32); }
    if let Some(i) = tag.find("viewBox=\"") {
        let nums: Vec<f32> = tag[i + 9..].split('"').next().unwrap_or("").split(|c: char| c == ' ' || c == ',').filter_map(|s| s.parse().ok()).collect();
        if nums.len() == 4 && nums[2] > 0.0 && nums[3] > 0.0 { return (nums[2] as u32, nums[3] as u32); }
    }
    (200, 200)
}

fn dimensions(bytes: &[u8], mime: &str) -> Result<(u32, u32), String> {
    if mime == "image/svg+xml" { return Ok(svg_size(bytes)); }
    let r = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    r.into_dimensions().map_err(|e| e.to_string())
}

/// Buduje zasób z bajtów (testowalne bez dysku).
pub fn asset_from_bytes(name: &str, bytes: &[u8]) -> Result<MediaAsset, String> {
    if bytes.len() > MAX_BYTES {
        return Err(trf("media.too_big", &[("name", &name), ("mb", &(MAX_BYTES / 1024 / 1024))]));
    }
    let mime = sniff(bytes).ok_or_else(|| trf("media.unsupported", &[("name", &name)]))?;
    let (w, h) = dimensions(bytes, mime).map_err(|e| trf("media.read_error", &[("name", &name), ("err", &e)]))?;
    let data_uri = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    Ok(MediaAsset { name: name.to_string(), mime, data_uri, w, h, bytes: bytes.len() })
}

pub fn load_file(path: &Path) -> Result<MediaAsset, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| trf("media.read_error", &[("name", &name), ("err", &e)]))?;
    asset_from_bytes(&name, &bytes)
}

/// egui nie czyta `data:` URI: dekodujemy raz i rejestrujemy bajty pod kluczem `bytes://<odcisk>.<rozszerzenie>`
/// (rozszerzenie wybiera loader: SVG vs. rastry). Odcisk z długości i próbek treści, żeby nie hashować kilku MB co klatkę.
pub fn register_data_uri(ctx: &egui::Context, uri: &str) -> Option<String> {
    let (head, b64) = uri.strip_prefix("data:")?.split_once(";base64,")?;
    let ext = match head { "image/png" => "png", "image/jpeg" | "image/jpg" => "jpg", "image/gif" => "gif", "image/webp" => "webp", "image/bmp" => "bmp", "image/svg+xml" => "svg", _ => return None };
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b64.len().hash(&mut h);
    let n = b64.len();
    for i in 0..8 { let a = n * i / 8; b64.as_bytes()[a..(a + 256).min(n)].hash(&mut h); }
    b64.as_bytes()[n.saturating_sub(256)..].hash(&mut h);
    let key = format!("bytes://media-{:016x}.{ext}", h.finish());
    let flag = egui::Id::new(&key);
    if !ctx.data(|d| d.get_temp::<bool>(flag)).unwrap_or(false) {
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
        ctx.include_bytes(key.clone(), bytes);
        ctx.data_mut(|d| d.insert_temp(flag, true));
    }
    Some(key)
}

/// Okno wyboru jednego pliku graficznego.
pub fn pick_image() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().add_filter(tr("media.file_filter"), &IMAGE_EXTS).pick_file()
}

/// Okno wyboru wielu plików graficznych.
pub fn pick_images() -> Vec<std::path::PathBuf> {
    rfd::FileDialog::new().add_filter(tr("media.file_filter"), &IMAGE_EXTS).pick_files().unwrap_or_default()
}

/// Ustawia plik jako źródło warstwy obrazu (zachowuje rozmiar i dopasowanie).
pub fn apply_to_layer(l: &mut Layer, a: &MediaAsset) {
    l.src = Some(a.data_uri.clone());
    l.extra.insert("srcName".into(), serde_json::Value::String(a.name.clone()));
}

/// Nowa warstwa obrazu z pliku: rozmiar naturalny wpasowany w 80% ramki (bez powiększania), wyśrodkowana, przesunięta o `offset` px.
pub fn add_image_layer(scene: &mut Scene, a: &MediaAsset, selected: &mut Vec<String>, offset: f32) -> String {
    let mut l = Layer::new(LayerType::Image);
    let (iw, ih) = (a.w.max(1) as f32, a.h.max(1) as f32);
    let s = (scene.frame.w * 0.8 / iw).min(scene.frame.h * 0.8 / ih).min(1.0);
    l.w = (iw * s).round().max(1.0);
    l.h = (ih * s).round().max(1.0);
    l.x = ((scene.frame.w - l.w) / 2.0 + offset).round();
    l.y = ((scene.frame.h - l.h) / 2.0 + offset).round();
    l.name = a.layer_name();
    apply_to_layer(&mut l, a);
    let id = l.id.clone();
    scene.layers.push(l);
    *selected = vec![id.clone()];
    id
}

/// Dodaje warstwy z listy plików; błędy zbiera w komunikacie (poprawne pliki i tak trafiają do sceny).
pub fn add_files(scene: &mut Scene, paths: &[std::path::PathBuf], selected: &mut Vec<String>) -> Option<String> {
    let mut errors = vec![];
    let mut ids = vec![];
    let mut warn = false;
    for (i, p) in paths.iter().enumerate() {
        match load_file(p) {
            Ok(a) => { warn |= a.is_large(); ids.push(add_image_layer(scene, &a, &mut vec![], 24.0 * i as f32)); }
            Err(e) => errors.push(e),
        }
    }
    if !ids.is_empty() { *selected = ids; }
    if warn { errors.push(trf("media.big_warning", &[("mb", &(WARN_BYTES / 1024 / 1024))])); }
    if errors.is_empty() { None } else { Some(errors.join(" ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Element;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        let mut out = vec![];
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    fn scene() -> Scene { Element::new_typed("t", "GRAPHIC").scene.unwrap() }

    #[test]
    fn sniff_ignores_extension_and_knows_formats() {
        assert_eq!(sniff(&png(2, 2)), Some("image/png"));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0]), Some("image/jpeg"));
        assert_eq!(sniff(b"GIF89a....."), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"  <?xml?><SVG width=\"1\"/>"), Some("image/svg+xml"));
        assert_eq!(sniff(b"MZ\x90\0 not an image"), None);
    }

    #[test]
    fn asset_has_dimensions_and_roundtrips_through_base64() {
        let bytes = png(40, 20);
        let a = asset_from_bytes("logo.png", &bytes).unwrap();
        assert_eq!((a.w, a.h, a.mime), (40, 20, "image/png"));
        let b64 = a.data_uri.strip_prefix("data:image/png;base64,").unwrap();
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(b64).unwrap(), bytes);
        assert_eq!(a.layer_name(), "logo");
    }

    #[test]
    fn rejects_unknown_type_and_oversized_files() {
        assert!(asset_from_bytes("a.txt", b"hello").unwrap_err().contains("a.txt"));
        assert!(asset_from_bytes("big.png", &vec![0u8; MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn svg_size_from_attributes_and_viewbox() {
        assert_eq!(svg_size(b"<svg width=\"120px\" height=\"60\"></svg>"), (120, 60));
        assert_eq!(svg_size(b"<svg viewBox=\"0 0 300 100\"></svg>"), (300, 100));
        assert_eq!(svg_size(b"<svg></svg>"), (200, 200));
    }

    #[test]
    fn new_layer_fits_80_percent_of_the_frame_without_upscaling() {
        let mut s = scene();
        let (fw, fh) = (s.frame.w, s.frame.h);
        let mut sel = vec![];
        let big = asset_from_bytes("big.png", &png(4000, 2000)).unwrap();
        let id = add_image_layer(&mut s, &big, &mut sel, 0.0);
        let l = s.layers.iter().find(|l| l.id == id).unwrap();
        assert!(l.w <= fw * 0.8 + 1.0 && l.h <= fh * 0.8 + 1.0, "{}x{} in {fw}x{fh}", l.w, l.h);
        assert!((l.w / l.h - 2.0).abs() < 0.02, "proporcje zachowane");
        assert_eq!(sel, vec![id]);
        let small = asset_from_bytes("small.png", &png(30, 10)).unwrap();
        let id = add_image_layer(&mut s, &small, &mut sel, 0.0);
        let l = s.layers.iter().find(|l| l.id == id).unwrap();
        assert_eq!((l.w, l.h), (30.0, 10.0));
        assert_eq!(l.name, "small");
        assert_eq!(l.extra.get("srcName").and_then(|v| v.as_str()), Some("small.png"));
        assert!(l.src.as_deref().unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn add_files_keeps_good_files_and_reports_bad_ones() {
        let dir = std::env::temp_dir().join(format!("btb-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (good, bad) = (dir.join("a.png"), dir.join("b.png"));
        std::fs::write(&good, png(10, 10)).unwrap();
        std::fs::write(&bad, b"not an image").unwrap();
        let mut s = scene();
        let n0 = s.layers.len();
        let mut sel = vec![];
        let msg = add_files(&mut s, &[good, bad], &mut sel).expect("błąd dla złego pliku");
        assert!(msg.contains("b.png"));
        assert_eq!(s.layers.len(), n0 + 1);
        assert_eq!(sel.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
