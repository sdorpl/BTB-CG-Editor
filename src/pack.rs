//! Paczka elementu `.cgpack` (docs/FORMAT-PACZKI.md): ZIP z `manifest.json`, `element.json` (eksport v3) i `assets/<sha1>.<ext>`.
//!
//! Eksport: każdy osadzony obraz (`data:<mime>;base64,…`) → plik w `assets/`, w elemencie zostaje ścieżka względna.
//! Import: rozpakowanie w pamięci, weryfikacja (ścieżki, limity, SHA-1, typ po zawartości) i osadzenie plików z powrotem jako `data:`,
//! dzięki czemu element po otwarciu jest samowystarczalny. Lustro modułu `pack-io.js` w BTB-CG (te same reguły i limity).
use crate::i18n::trf;
use crate::media;
use crate::model::Element;
use base64::Engine;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

pub const PACK_TYPE: &str = "cg_element_pack";
pub const PACK_VERSION: u64 = 1;
const MAX_FILES: usize = 200;
const MAX_FILE: u64 = 200 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;

fn sha1_hex(b: &[u8]) -> String {
    Sha1::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn ext_of(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/png" => "png", "image/jpeg" => "jpg", "image/gif" => "gif", "image/webp" => "webp", "image/bmp" => "bmp", "image/svg+xml" => "svg",
        _ => return None,
    })
}

/// `assets/<40 hex>.<ext>` → (sha1, ext)
fn parse_asset_path(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("assets/")?;
    let (h, ext) = rest.split_once('.')?;
    (h.len() == 40 && h.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) && (2..=5).contains(&ext.len())
        && ext.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())).then_some((h, ext))
}

struct Asset { bytes: Vec<u8>, mime: &'static str }

/// Zamienia osadzone `data:` w tekście na ścieżki `assets/…`, zbierając pliki.
fn externalize(s: &str, assets: &mut BTreeMap<String, Asset>) -> String {
    if !s.contains(";base64,") { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("data:") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let parsed = tail.find(";base64,").filter(|&p| p <= 64).and_then(|p| {
            let start = p + 8;
            let len = tail[start..].bytes().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=' | b'\n' | b'\r' | b' ')).count();
            let b64: String = tail[start..start + len].chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            let mime = media::sniff(&bytes)?;
            Some((start + len, bytes, mime))
        });
        match parsed {
            Some((consumed, bytes, mime)) => {
                let path = format!("assets/{}.{}", sha1_hex(&bytes), ext_of(mime).unwrap_or("bin"));
                out.push_str(&path);
                assets.entry(path).or_insert(Asset { bytes, mime });
                rest = &tail[consumed..];
            }
            None => { out.push_str("data:"); rest = &tail[5..]; }
        }
    }
    out.push_str(rest);
    out
}

fn walk(v: &mut Value, assets: &mut BTreeMap<String, Asset>) {
    match v {
        Value::String(s) => { let n = externalize(s, assets); if n != *s { *s = n; } }
        Value::Array(a) => a.iter_mut().for_each(|x| walk(x, assets)),
        Value::Object(o) => o.values_mut().for_each(|x| walk(x, assets)),
        _ => {}
    }
}

/// Buduje paczkę z elementu (obrazy `data:` → `assets/`). Zwraca bajty ZIP.
pub fn build_pack(el: &Element) -> Result<Vec<u8>, String> {
    let mut doc = json!({ "_exportVersion": 3, "elements": [el] });
    let mut assets = BTreeMap::new();
    walk(&mut doc, &mut assets);
    let manifest = json!({
        "_type": PACK_TYPE, "_packVersion": PACK_VERSION, "exportVersion": 3,
        "created": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "generator": format!("btb-cgeditor {}", env!("CARGO_PKG_VERSION")),
        "element": { "file": "element.json", "id": el.id, "name": el.name, "type": el.elem_type },
        "assets": assets.iter().map(|(p, a)| json!({ "path": p, "mime": a.mime, "bytes": a.bytes.len(), "sha1": &p[7..47] })).collect::<Vec<_>>(),
    });
    let mut buf = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buf);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, data: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())
    };
    put("manifest.json", serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?.as_bytes())?;
    put("element.json", serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?.as_bytes())?;
    for (p, a) in &assets { put(p, &a.bytes)?; }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

fn read_limited(f: &mut impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    f.take(limit + 1).read_to_end(&mut v).map_err(|e| e.to_string())?;
    if v.len() as u64 > limit { return Err(trf("pack.too_big", &[("mb", &(limit >> 20))])); }
    Ok(v)
}

/// Czyta i waliduje paczkę; zwraca tekst `element.json` z plikami osadzonymi jako `data:` (do `importer::parse_elements`).
pub fn read_pack(bytes: &[u8]) -> Result<String, String> {
    let bad = |m: &str| trf("pack.invalid", &[("err", &m)]);
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| bad("to nie jest archiwum ZIP"))?;
    if zip.len() > MAX_FILES { return Err(bad("za dużo plików")); }
    let (mut manifest, mut element): (Option<Value>, Option<String>) = (None, None);
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() { continue; }
        let name = String::from_utf8_lossy(f.name_raw()).into_owned();
        if name.contains("..") || name.starts_with('/') || name.contains('\\') { return Err(bad(&format!("niedozwolona ścieżka: {name}"))); }
        let data = read_limited(&mut f, MAX_FILE)?;
        total += data.len() as u64;
        if total > MAX_TOTAL { return Err(trf("pack.too_big", &[("mb", &(MAX_TOTAL >> 20))])); }
        match name.as_str() {
            "manifest.json" => manifest = Some(serde_json::from_slice(&data).map_err(|_| bad("manifest.json nie jest poprawnym JSON"))?),
            "element.json" => element = Some(String::from_utf8(data).map_err(|_| bad("element.json nie jest UTF-8"))?),
            n if parse_asset_path(n).is_some() => { files.insert(name, data); }
            n => return Err(bad(&format!("nieoczekiwany plik: {n}"))),
        }
    }
    let (manifest, mut text) = match (manifest, element) { (Some(m), Some(e)) => (m, e), _ => return Err(bad("brak manifest.json lub element.json")) };
    if manifest["_type"] != PACK_TYPE { return Err(bad("to nie jest paczka elementu")); }
    if manifest["_packVersion"].as_u64().unwrap_or(u64::MAX) > PACK_VERSION { return Err(trf("pack.newer", &[])); }
    let mut declared = std::collections::BTreeSet::new();
    for a in manifest["assets"].as_array().cloned().unwrap_or_default() {
        let path = a["path"].as_str().unwrap_or("");
        let data = files.get(path).ok_or_else(|| bad(&format!("brakujący plik: {path}")))?;
        let (hash, _ext) = parse_asset_path(path).ok_or_else(|| bad(&format!("niedozwolona ścieżka: {path}")))?;
        if sha1_hex(data) != hash || a["sha1"].as_str() != Some(hash) { return Err(bad(&format!("suma SHA-1 niezgodna: {path}"))); }
        if a["bytes"].as_u64().is_some_and(|n| n != data.len() as u64) { return Err(bad(&format!("rozmiar niezgodny: {path}"))); }
        let mime = media::sniff(data).ok_or_else(|| bad(&format!("niedozwolony typ pliku: {path}")))?;
        if data.len() > media::MAX_BYTES { return Err(trf("media.too_big", &[("name", &path), ("mb", &(media::MAX_BYTES / 1024 / 1024))])); }
        let uri = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(data));
        text = text.replace(path, &uri);
        declared.insert(path.to_string());
    }
    if let Some(extra) = files.keys().find(|k| !declared.contains(*k)) { return Err(bad(&format!("plik spoza manifestu: {extra}"))); }
    Ok(text)
}

/// Liczba plików graficznych w gotowej paczce (do komunikatu w pasku statusu).
pub fn count_assets(bytes: &[u8]) -> usize {
    zip::ZipArchive::new(Cursor::new(bytes)).map(|mut z| (0..z.len()).filter(|&i| z.by_index(i).is_ok_and(|f| f.name_raw().starts_with(b"assets/"))).count()).unwrap_or(0)
}

pub fn is_pack_path(p: &std::path::Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cgpack"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer;

    fn png() -> Vec<u8> {
        // poprawny 1×1 PNG (sniff + image::ImageReader)
        let mut v = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])).write_to(&mut Cursor::new(&mut v), image::ImageFormat::Png).unwrap();
        v
    }
    fn element_with(png: &[u8]) -> Element {
        let uri = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png));
        let v = json!({ "id": "pk1", "name": "Paczka", "type": "IMAGE", "html_template": format!("<div style=\"background:url({uri})\"></div>"),
            "scene": { "version": 1, "detached": false, "frame": { "w": 100.0, "h": 50.0 }, "layers": [
                { "id": "i1", "type": "image", "x": 0, "y": 0, "w": 100, "h": 50, "src": uri } ] } });
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn roundtrip_keeps_element_and_dedups_assets() {
        let p = png();
        let el = element_with(&p);
        let zip = build_pack(&el).unwrap();
        // ta sama grafika użyta dwa razy = jeden plik w paczce
        let mut z = zip::ZipArchive::new(Cursor::new(&zip)).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| String::from_utf8_lossy(z.by_index(i).unwrap().name_raw()).into_owned()).collect();
        assert_eq!(names.iter().filter(|n| n.starts_with("assets/")).count(), 1, "{names:?}");
        let text = read_pack(&zip).unwrap();
        let back = importer::parse_elements(&text).unwrap().remove(0).element;
        assert_eq!(back.id, "pk1");
        let src = back.scene.as_ref().unwrap().layers[0].src.clone().unwrap();
        assert!(src.starts_with("data:image/png;base64,"));
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(src.split(',').nth(1).unwrap()).unwrap(), p);
    }

    fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut buf);
        for (n, d) in files { z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap(); z.write_all(d).unwrap(); }
        z.finish().unwrap();
        buf.into_inner()
    }
    fn man(assets: Value) -> Vec<u8> { json!({ "_type": PACK_TYPE, "_packVersion": 1, "assets": assets }).to_string().into_bytes() }

    #[test]
    fn rejects_malicious_and_broken_packs() {
        let doc = br#"{"_exportVersion":3,"elements":[]}"#.to_vec();
        let p = png();
        let path = format!("assets/{}.png", sha1_hex(&p));
        let good = |extra: &[(&str, Vec<u8>)], m: Vec<u8>| { let mut f = vec![("manifest.json", m), ("element.json", doc.clone())]; f.extend(extra.iter().cloned()); zip_of(&f) };
        assert!(read_pack(&good(&[(path.as_str(), p.clone())], man(json!([{ "path": path, "bytes": p.len(), "sha1": sha1_hex(&p) }])))).is_ok());
        let err = |b: Vec<u8>| read_pack(&b).unwrap_err();
        assert!(err(good(&[("../evil.txt", vec![1])], man(json!([])))).contains("niedozwolona"));
        assert!(err(good(&[("run.sh", vec![1])], man(json!([])))).contains("nieoczekiwany"));
        assert!(err(good(&[(path.as_str(), p.clone())], man(json!([{ "path": path, "sha1": "f".repeat(40) }])))).contains("SHA-1"));
        assert!(err(good(&[(path.as_str(), p.clone())], man(json!([])))).contains("spoza manifestu"));
        let junk = b"MZ-not-an-image".to_vec(); let jp = format!("assets/{}.png", sha1_hex(&junk));
        assert!(err(good(&[(jp.as_str(), junk.clone())], man(json!([{ "path": jp, "sha1": sha1_hex(&junk) }])))).contains("typ pliku"));
        assert!(err(zip_of(&[("manifest.json", br#"{"_type":"x"}"#.to_vec()), ("element.json", doc.clone())])).contains("to nie jest paczka"));
        assert!(err(b"to nie zip".to_vec()).contains("ZIP"));
    }

    /// Obieg z BTB-CG: paczka z Rust czytana przez pack-io.js i paczka z pack-io.js czytana w Rust.
    #[test]
    fn interop_with_btb_cg_pack_io() {
        let root = std::path::Path::new("/home/maciej/Nextcloud/GitHub/BTB-CG-System-alpha");
        if !root.join("pack-io.js").exists() || !root.join("node_modules/fflate").exists() { return; }
        let dir = std::env::temp_dir().join(format!("cgpack-interop-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("uploads")).unwrap();
        let p = png();
        let el = element_with(&p);
        // Rust → JS
        let rust_pack = dir.join("rust.cgpack");
        std::fs::write(&rust_pack, build_pack(&el).unwrap()).unwrap();
        let script = format!(
            "const P=require('{r}/pack-io.js');const fs=require('fs');const pk=P.readPack(fs.readFileSync(process.argv[1]));\
             const d=P.installPack(pk,process.argv[2]);console.log(JSON.stringify({{id:d.elements[0].id,src:d.elements[0].scene.layers[0].src,n:pk.assets.size}}));",
            r = root.display());
        let out = std::process::Command::new("node").arg("-e").arg(&script).arg(&rust_pack).arg(dir.join("uploads")).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let j: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!((j["id"].as_str(), j["n"].as_u64()), (Some("pk1"), Some(1)));
        assert!(j["src"].as_str().unwrap().starts_with("/uploads/") && j["src"].as_str().unwrap().ends_with(".png"));
        // JS → Rust (element z /uploads → paczka z assets/ → osadzone data:)
        let sha = sha1_hex(&p);
        std::fs::write(dir.join("uploads").join(format!("{sha}.png")), &p).unwrap();
        let js_pack = dir.join("js.cgpack");
        let script2 = format!(
            "const P=require('{r}/pack-io.js');const fs=require('fs');const el={{id:'js1',name:'Z JS',type:'IMAGE',html_template:'',css_template:'',js_template:'',ocgInputs:[],\
             scene:{{version:1,detached:false,frame:{{w:100,h:50}},layers:[{{id:'i1',type:'image',x:0,y:0,w:100,h:50,src:'/uploads/{sha}.png'}}]}}}};\
             fs.writeFileSync(process.argv[1],Buffer.from(P.buildPack(el,process.argv[2]).data));", r = root.display());
        let o2 = std::process::Command::new("node").arg("-e").arg(&script2).arg(&js_pack).arg(dir.join("uploads")).output().unwrap();
        assert!(o2.status.success(), "{}", String::from_utf8_lossy(&o2.stderr));
        let back = importer::parse_elements(&read_pack(&std::fs::read(&js_pack).unwrap()).unwrap()).unwrap().remove(0).element;
        assert_eq!(back.id, "js1");
        assert!(back.scene.unwrap().layers[0].src.clone().unwrap().starts_with("data:image/png;base64,"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
