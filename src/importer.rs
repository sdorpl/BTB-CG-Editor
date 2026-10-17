//! Loading elements from files and from a running BTB-CG instance (e.g. http://localhost:3000).
//!
//! The server only serves static files (`express.static`) and `/api/upload` over HTTP; live state
//! travels over Socket.io. The one thing reachable with a plain GET is `db.json` (graphics[] /
//! library[] — the startup pattern; edits made only in SQLite show up after `node export-db.js`).
use crate::i18n::{tr, trf};
use crate::model::Element;
use serde_json::Value;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub struct Loaded {
    pub source: String,
    pub element: Element,
}

pub type FetchResult = Result<Vec<Loaded>, String>;

/// Accepts a v3 export (`elements[]`), a state/db.json (`graphics[]`, `library[]`), an array, or a bare element.
pub fn parse_elements(text: &str) -> Result<Vec<Loaded>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| trf("importer.bad_json", &[("err", &e)]))?;
    let mut raw: Vec<(String, Value)> = vec![];
    let mut push = |src: &str, arr: Option<&Vec<Value>>| if let Some(a) = arr { for x in a { raw.push((src.to_string(), x.clone())); } };
    match &v {
        Value::Array(a) => push("Plik", Some(a)),
        Value::Object(o) => {
            if o.contains_key("elements") || o.contains_key("graphics") || o.contains_key("library") {
                push("Eksport", o.get("elements").and_then(|x| x.as_array()));
                push("Bank grafik", o.get("graphics").and_then(|x| x.as_array()));
                push("Biblioteka", o.get("library").and_then(|x| x.as_array()));
            } else if o.contains_key("id") {
                raw.push(("Plik".into(), v.clone()));
            }
        }
        _ => {}
    }
    if raw.is_empty() {
        return Err(tr("importer.no_elements").into());
    }
    let mut out = vec![];
    let mut errors = vec![];
    for (source, val) in raw {
        match serde_json::from_value::<Element>(val.clone()) {
            Ok(element) => out.push(Loaded { source, element }),
            Err(e) => errors.push(format!("{}: {}", val.get("id").and_then(|x| x.as_str()).unwrap_or("?"), e)),
        }
    }
    if out.is_empty() { return Err(trf("importer.read_errors", &[("err", &errors.join("; "))])); }
    Ok(out)
}

pub fn normalize_base_url(input: &str) -> String {
    let t = input.trim().trim_end_matches('/');
    if t.starts_with("http://") || t.starts_with("https://") { t.to_string() } else { format!("http://{}", t) }
}

pub fn fetch_async(base_url: &str) -> Receiver<FetchResult> {
    let (tx, rx): (Sender<FetchResult>, Receiver<FetchResult>) = std::sync::mpsc::channel();
    let base = normalize_base_url(base_url);
    thread::spawn(move || { let _ = tx.send(fetch_blocking(&base)); });
    rx
}

/// Prefers the live state (`GET /api/state`: bank + library as the operator sees them now);
/// older servers without that endpoint fall back to the startup pattern in `db.json`.
pub fn fetch_blocking(base: &str) -> FetchResult {
    let get = |path: &str| -> Result<String, String> {
        let url = format!("{}{}", base, path);
        ureq::get(&url)
            .timeout(std::time::Duration::from_secs(6))
            .call()
            .map_err(|e| trf("importer.connect_failed", &[("url", &url), ("err", &e)]))?
            .into_string()
            .map_err(|e| trf("importer.read_response", &[("err", &e)]))
    };
    match get("/api/state").and_then(|b| parse_elements(&b)) {
        Ok(list) => Ok(list),
        Err(_) => parse_elements(&get("/db.json")?),
    }
}

/// Where a pushed element lands in BTB-CG.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PushTarget { Bank, Library }

/// `POST /api/elements` — the server upserts by `id` (operator state of an on-air bank element is kept)
/// and answers with `{ added[], updated[], warnings[] }`; returns a one-line summary for the status bar.
pub fn push_blocking(base: &str, target: PushTarget, element: &Element) -> Result<String, String> {
    let url = format!("{}/api/elements", base);
    let body = serde_json::json!({
        "target": if target == PushTarget::Bank { "bank" } else { "library" },
        "mode": "update",
        "_exportVersion": 3,
        "elements": [element],
    });
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(15))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| match e {
            ureq::Error::Status(code, r) => trf("importer.rejected", &[("code", &code), ("err", &r.into_string().unwrap_or_default())]),
            e => trf("importer.connect_failed", &[("url", &url), ("err", &e)]),
        })?;
    let v: Value = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?).map_err(|e| trf("importer.bad_response", &[("err", &e)]))?;
    let n = |k: &str| v.get(k).and_then(|x| x.as_array()).map_or(0, |a| a.len());
    let mut msg = trf("importer.pushed", &[("target", &tr(if target == PushTarget::Bank { "importer.target_bank" } else { "importer.target_library" })), ("added", &n("added")), ("updated", &n("updated"))]);
    // element na wizji nie jest podmieniany w trakcie emisji — jego nowa wersja czeka w kolejce serwera
    if n("deferred") > 0 { msg.push_str(&format!(" {}", trf("importer.deferred", &[("count", &n("deferred"))]))); }
    if let Some(w) = v.get("warnings").and_then(|x| x.as_array()) {
        for item in w {
            for t in item.get("warnings").and_then(|x| x.as_array()).into_iter().flatten().filter_map(|x| x.as_str()) { msg.push_str(&format!(" ⚠ {t}")); }
        }
    }
    Ok(msg)
}

pub fn push_async(base_url: &str, target: PushTarget, element: Element) -> Receiver<Result<String, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let base = normalize_base_url(base_url);
    thread::spawn(move || { let _ = tx.send(push_blocking(&base, target, &element)); });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn num_eq(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-4,
            (Value::Object(x), Value::Object(y)) => x.iter().all(|(k, v)| y.get(k).map_or(false, |w| num_eq(v, w))),
            (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| num_eq(p, q)),
            _ => a == b,
        }
    }

    /// Every key present in the source JSON must survive load → save unchanged (extra keys are allowed).
    fn missing(orig: &Value, out: &Value, path: &str, acc: &mut Vec<String>) {
        match (orig, out) {
            (Value::Object(o), Value::Object(r)) => for (k, v) in o {
                if v.is_null() { continue; }
                match r.get(k) { Some(w) => missing(v, w, &format!("{path}.{k}"), acc), None => acc.push(format!("{path}.{k} missing")) }
            },
            (Value::Array(o), Value::Array(r)) if o.len() == r.len() => for (i, (a, b)) in o.iter().zip(r).enumerate() { missing(a, b, &format!("{path}[{i}]"), acc); },
            _ => if !num_eq(orig, out) { acc.push(format!("{path}: {orig} != {out}")); },
        }
    }

    #[test]
    fn real_btb_cg_templates_roundtrip_losslessly() {
        let dir = std::path::Path::new("/home/maciej/Nextcloud/GitHub/BTB-CG-System-alpha/templates");
        if !dir.exists() { return; }
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().extension().map_or(false, |x| x == "json")) {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let orig: Value = serde_json::from_str(&text).unwrap();
            let list = parse_elements(&text).unwrap_or_else(|e| panic!("{:?}: {e}", entry.file_name()));
            let origs: Vec<Value> = match orig.get("elements") { Some(Value::Array(a)) => a.clone(), _ => vec![orig.clone()] };
            for (l, o) in list.iter().zip(origs.iter()) {
                let out = serde_json::to_value(&l.element).unwrap();
                let mut acc = vec![];
                missing(o, &out, "", &mut acc);
                assert!(acc.is_empty(), "{:?} lost data: {:?}", entry.file_name(), &acc[..acc.len().min(8)]);
                checked += 1;
            }
        }
        assert!(checked >= 6, "checked {checked} templates");
    }

    #[test]
    fn real_db_json_elements_all_parse() {
        let p = std::path::Path::new("/home/maciej/Nextcloud/GitHub/BTB-CG-System-alpha/db.json");
        if !p.exists() { return; }
        let list = parse_elements(&std::fs::read_to_string(p).unwrap()).unwrap();
        assert!(list.len() >= 30, "parsed {} elements", list.len());
    }
}
