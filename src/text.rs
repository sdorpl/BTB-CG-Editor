//! Wspólny model tekstu (M3.2): sanityzacja HTML, linie/tekst, RichText v1 ↔ HTML.
//!
//! Port `text-model.js` z BTB-CG (jedna logika w dwóch językach): ten sam tokenizer, te same reguły CSS, ten sam kanoniczny wynik.
//! Zgodność pilnują wspólne wektory `tests/text/model.json` (kopia `test-fixtures/text/model.json` z BTB-CG; test porównuje też z oryginałem,
//! gdy repozytorium BTB-CG jest obok). Bez zależności (brak `regex`): skanery pisane ręcznie; nowe kod bez `unwrap`/`panic`.
//!
//! RichText v1: `{ v: 1, blocks: [ { align?, runs: [ { t, m? } | { br: true } ] } ] }`, `m` = `{ weight, italic, underline, strike, color, bg,
//! family, size (px), tracking (em), transform, squash }`.
#![allow(dead_code)]   // model tekstu czeka na podpięcie w edytorze (M3.4); testy pilnują parytetu z BTB-CG
use serde_json::{json, Map, Value};

pub const VERSION: u32 = 1;
const MAX_HTML: usize = 200_000;
const MAX_DEPTH: usize = 24;

fn tag_map(name: &str) -> Option<&'static str> {
    Some(match name { "strong" | "b" => "strong", "em" | "i" => "em", "u" => "u", "s" | "strike" => "s", "span" => "span", "div" => "div", "p" => "p", "br" => "br", _ => return None })
}
fn drop_with_content(name: &str) -> bool { matches!(name, "script" | "style" | "iframe" | "object" | "embed" | "template" | "noscript" | "head" | "title") }
fn is_block(name: &str) -> bool { matches!(name, "div" | "p") }

// ── pomocnicze (bez regex) ──────────────────────────────────────────────────────────────────────

fn is_num_unsigned(s: &str) -> bool {
    // \d+(\.\d+)?
    let mut it = s.splitn(2, '.');
    let a = it.next().unwrap_or("");
    let b = it.next();
    !a.is_empty() && a.bytes().all(|c| c.is_ascii_digit()) && b.map_or(true, |x| !x.is_empty() && x.bytes().all(|c| c.is_ascii_digit()))
}
fn is_num(s: &str) -> bool { is_num_unsigned(s.strip_prefix('-').unwrap_or(s)) }
fn ends_unit<'a>(v: &'a str, units: &[&str]) -> Option<&'a str> {
    let l = v.to_ascii_lowercase();
    for u in units { if l.ends_with(u) { return Some(&v[..v.len() - u.len()]); } }
    None
}
fn is_len(v: &str) -> bool { ends_unit(v, &["px", "em", "rem", "%", "pt"]).map_or(false, is_num) }

fn is_color(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    if let Some(h) = l.strip_prefix('#') { return (3..=8).contains(&h.len()) && h.bytes().all(|c| c.is_ascii_hexdigit()); }
    if l == "transparent" || l == "currentcolor" { return true; }
    if let Some(inner) = l.strip_prefix("rgba(").or_else(|| l.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        let rgb = parts.len() >= 3 && parts[..3].iter().all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|c| c.is_ascii_digit()));
        let alpha = parts.len() == 3 || (parts.len() == 4 && (matches!(parts[3], "0" | "1") || (parts[3].starts_with("0.") && parts[3].len() > 2 && parts[3][2..].bytes().all(|c| c.is_ascii_digit())) || (parts[3].starts_with('.') && parts[3].len() > 1 && parts[3][1..].bytes().all(|c| c.is_ascii_digit())) || (parts[3].starts_with("1.") && parts[3][2..].bytes().all(|c| c == b'0') && parts[3].len() > 2)));
        return rgb && alpha && (parts.len() == 3 || l.starts_with("rgba(")) && (parts.len() == 4 || l.starts_with("rgb("));
    }
    if let Some(inner) = l.strip_prefix("hsla(").or_else(|| l.strip_prefix("hsl(")).and_then(|r| r.strip_suffix(')')) {
        return !inner.is_empty() && inner.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b',' | b'%') || c.is_ascii_whitespace());
    }
    (3..=20).contains(&l.len()) && l.bytes().all(|c| c.is_ascii_lowercase())
}

fn is_text_deco(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    let mut it = l.split(' ');
    let first = it.next().unwrap_or("");
    matches!(first, "none" | "underline" | "line-through" | "overline") && it.all(|w| matches!(w, "underline" | "line-through" | "overline"))
}

fn is_weight(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    matches!(l.as_str(), "normal" | "bold" | "bolder" | "lighter") || (l.len() == 3 && l.ends_with("00") && l.as_bytes()[0] >= b'1' && l.as_bytes()[0] <= b'9')
}

fn is_scalex(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    let Some(inner) = l.strip_prefix("scalex(").and_then(|r| r.strip_suffix(')')) else { return false };
    let n = inner.trim();
    // \d*\.?\d+
    if n.is_empty() { return false; }
    let mut parts = n.splitn(2, '.');
    let a = parts.next().unwrap_or("");
    match parts.next() {
        Some(b) => a.bytes().all(|c| c.is_ascii_digit()) && !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()),
        None => a.bytes().all(|c| c.is_ascii_digit()),
    }
}

fn is_box_values(v: &str, units: &[&str]) -> bool {
    // (\d+(\.\d+)?(units) ?){1,4}
    let t = v.trim();
    if t.is_empty() { return false; }
    let parts: Vec<&str> = t.split(' ').collect();
    parts.len() <= 4 && parts.iter().all(|p| *p == "0" || ends_unit(p, units).map_or(false, is_num_unsigned))
}

fn valid_value(prop: &str, v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    match prop {
        "color" | "background-color" | "background" => is_color(v),
        "font-family" => { let n = v.chars().count(); (1..=120).contains(&n) && v.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | ',' | '\'' | '"' | '_' | '-')) }
        "font-size" => is_len(v) || matches!(l.as_str(), "xx-small" | "x-small" | "small" | "medium" | "large" | "x-large" | "xx-large" | "xxx-large" | "smaller" | "larger"),
        "font-weight" => is_weight(v),
        "font-style" => matches!(l.as_str(), "normal" | "italic" | "oblique"),
        "text-decoration" | "text-decoration-line" => is_text_deco(v),
        "letter-spacing" => is_len(v) || l == "normal",
        "text-transform" => matches!(l.as_str(), "none" | "uppercase" | "lowercase" | "capitalize"),
        "text-align" => matches!(l.as_str(), "left" | "center" | "right" | "justify"),
        "line-height" => l == "normal" || is_num_unsigned(v) || ends_unit(v, &["px", "em", "%"]).map_or(false, is_num_unsigned),
        "display" => matches!(l.as_str(), "inline" | "inline-block"),
        "transform" => is_scalex(v),
        "transform-origin" => { let p: Vec<&str> = l.split(' ').collect(); matches!(p.len(), 1 | 2) && matches!(p[0], "left" | "center" | "right") && p.get(1).map_or(true, |x| matches!(*x, "center" | "top" | "bottom")) }
        "padding" => is_box_values(v, &["px", "em"]),
        "border-radius" => is_box_values(v, &["px", "em", "%"]),
        _ => false,
    }
}

fn collapse_ws(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }

fn banned(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    if l.contains("expression") || l.contains("javascript:") || l.contains("/*") { return true; }
    if v.contains(['<', '>', '{', '}', '\\']) { return true; }
    // url\s*\(
    let b = l.as_bytes();
    let mut i = 0;
    while let Some(p) = l[i..].find("url") {
        let mut j = i + p + 3;
        while j < b.len() && (b[j] as char).is_ascii_whitespace() { j += 1; }
        if j < b.len() && b[j] == b'(' { return true; }
        i += p + 3;
    }
    false
}

/// Style: "prop: value; …" → tylko dozwolone, kanonicznie (`prop: value; prop: value`).
pub fn clean_style(style: &str) -> String {
    let mut out: Vec<(String, String)> = vec![];
    for decl in style.split(';') {
        let Some(i) = decl.find(':') else { continue };
        if i < 1 { continue; }
        let mut prop = decl[..i].trim().to_ascii_lowercase();
        let mut val = collapse_ws(decl[i + 1..].trim());
        if val.to_ascii_lowercase().ends_with("!important") { val = val[..val.len() - 10].trim().to_string(); }
        if val.is_empty() || banned(&val) || !valid_value(&prop, &val) { continue; }
        if matches!(prop.as_str(), "color" | "background-color" | "background") {
            val = val.to_ascii_lowercase();
            val = val.split(',').map(|p| p.trim()).collect::<Vec<_>>().join(", ");
            val = val.replace("( ", "(").replace(" )", ")");
        }
        if prop == "font-family" { val = val.replace('"', "'"); }
        if prop == "background" { prop = "background-color".into(); }
        out.retain(|(p, _)| *p != prop);
        out.push((prop, val));
    }
    out.iter().map(|(p, v)| format!("{p}: {v}")).collect::<Vec<_>>().join("; ")
}

// ── tokenizer ────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Tok {
    Text(String),
    Open { name: String, attrs: Vec<(String, String)>, self_close: bool },
    Close(String),
}

fn attr_of<'a>(attrs: &'a [(String, String)], k: &str) -> &'a str { attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or("") }

/// Dopasowanie `<(/?)(name)(attrs)>` od pozycji `lt` (ten sam zakres 4000 znaków co w JS). Zwraca (zamykający?, nazwa, surowe atrybuty, długość).
fn match_tag(chars: &[char], lt: usize) -> Option<(bool, String, String, usize)> {
    let end = chars.len().min(lt + 4000);
    let mut i = lt + 1;
    let closing = chars.get(i) == Some(&'/');
    if closing { i += 1; }
    let name_start = i;
    if !chars.get(i).map_or(false, |c| c.is_ascii_alphabetic()) { return None; }
    while i < end && chars[i].is_ascii_alphanumeric() { i += 1; }
    let name: String = chars[name_start..i].iter().collect::<String>().to_ascii_lowercase();
    let attr_start = i;
    while i < end {
        match chars[i] {
            '>' => { let raw: String = chars[attr_start..i].iter().collect(); return Some((closing, name, raw, i + 1 - lt)); }
            q @ ('"' | '\'') => { let mut j = i + 1; while j < end && chars[j] != q { j += 1; } if j >= end { return None; } i = j + 1; }
            _ => i += 1,
        }
    }
    None
}

fn parse_attrs(raw: &str) -> Vec<(String, String)> {
    let c: Vec<char> = raw.trim_end().trim_end_matches('/').chars().collect();   // `\/\s*$` usunięte
    let mut out: Vec<(String, String)> = vec![];
    let mut i = 0;
    let n = c.len();
    while i < n {
        if !(c[i].is_ascii_alphabetic() || c[i] == '_' || c[i] == ':') { i += 1; continue; }
        let s = i;
        while i < n && (c[i].is_ascii_alphanumeric() || matches!(c[i], '-' | '_' | ':' | '.')) { i += 1; }
        let name: String = c[s..i].iter().collect::<String>().to_ascii_lowercase();
        // opcjonalnie: \s*=\s*(wartość)
        let mut j = i;
        while j < n && c[j].is_whitespace() { j += 1; }
        let mut val = String::new();
        if j < n && c[j] == '=' {
            j += 1;
            while j < n && c[j].is_whitespace() { j += 1; }
            if j < n && (c[j] == '"' || c[j] == '\'') {
                let q = c[j];
                let mut k = j + 1;
                while k < n && c[k] != q { k += 1; }
                if k < n { val = c[j + 1..k].iter().collect(); i = k + 1; }
            } else {
                let mut k = j;
                while k < n && !c[k].is_whitespace() && !matches!(c[k], '"' | '\'' | '>') { k += 1; }
                if k > j { val = c[j..k].iter().collect(); i = k; }
            }
        }
        out.push((name, val));
    }
    // jak `Object` w JS: późniejszy atrybut o tej samej nazwie nadpisuje wcześniejszy
    let mut dedup: Vec<(String, String)> = vec![];
    for (k, v) in out { if let Some(e) = dedup.iter_mut().find(|(n, _)| *n == k) { e.1 = v; } else { dedup.push((k, v)); } }
    dedup
}

fn tokenize(html: &str) -> Vec<Tok> {
    let chars: Vec<char> = html.chars().collect();
    let mut toks = vec![];
    let n = chars.len();
    let mut i = 0;
    let mut text = String::new();
    let flush = |text: &mut String, toks: &mut Vec<Tok>| { if !text.is_empty() { toks.push(Tok::Text(std::mem::take(text))); } };
    while i < n {
        if chars[i] != '<' { text.push(chars[i]); i += 1; continue; }
        if chars[i..].starts_with(&['<', '!', '-', '-']) {
            flush(&mut text, &mut toks);
            let mut j = i + 4;
            let mut found = None;
            while j + 2 < n { if chars[j] == '-' && chars[j + 1] == '-' && chars[j + 2] == '>' { found = Some(j); break; } j += 1; }
            i = found.map_or(n, |f| f + 3);
            continue;
        }
        match match_tag(&chars, i) {
            None => { text.push('<'); i += 1; }
            Some((closing, name, raw, len)) => {
                flush(&mut text, &mut toks);
                if closing { toks.push(Tok::Close(name)); }
                else {
                    let attrs = parse_attrs(&raw);
                    let self_close = raw.trim_end().ends_with('/') || name == "br";
                    toks.push(Tok::Open { name, attrs, self_close });
                }
                i += len;
            }
        }
    }
    flush(&mut text, &mut toks);
    toks
}

fn esc_text(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, ch) in c.iter().enumerate() {
        match ch {
            '&' => { if entity_len(&c, i).is_some() { out.push('&'); } else { out.push_str("&amp;"); } }
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(*ch),
        }
    }
    out
}

/// Długość poprawnej encji `&…;` zaczynającej się w `c[i]` (`&#\d{1,7};`, `&#x…{1,6};`, `&nazwa;` 2–11 znaków).
fn entity_len(c: &[char], i: usize) -> Option<usize> {
    if c.get(i) != Some(&'&') { return None; }
    let mut j = i + 1;
    let start = j;
    if c.get(j) == Some(&'#') {
        j += 1;
        let hex = matches!(c.get(j), Some('x') | Some('X'));
        if hex { j += 1; }
        let ds = j;
        while j < c.len() && (if hex { c[j].is_ascii_hexdigit() } else { c[j].is_ascii_digit() }) { j += 1; }
        let nd = j - ds;
        if nd == 0 || nd > if hex { 6 } else { 7 } || c.get(j) != Some(&';') { return None; }
        return Some(j + 1 - i);
    }
    if !c.get(j).map_or(false, |x| x.is_ascii_alphabetic()) { return None; }
    j += 1;
    let mut k = 0;
    while j < c.len() && c[j].is_ascii_alphanumeric() { j += 1; k += 1; }
    if (1..=10).contains(&k) && c.get(j) == Some(&';') && j > start { Some(j + 1 - i) } else { None }
}

/// Sanityzuje HTML (kanonicznie). `keep_p`: zostaw `<p>` (domyślnie `p → div`).
pub fn sanitize(html: &str, keep_p: bool) -> String {
    let src: String = html.chars().take(MAX_HTML).collect();
    let mut out = String::new();
    let mut stack: Vec<&'static str> = vec![];
    let mut skip = 0usize;
    let mut skip_name = String::new();
    for t in tokenize(&src) {
        if skip > 0 {
            match &t {
                Tok::Open { name, self_close, .. } if *name == skip_name && !self_close => skip += 1,
                Tok::Close(name) if *name == skip_name => skip -= 1,
                _ => {}
            }
            continue;
        }
        match t {
            Tok::Text(v) => out.push_str(&esc_text(&v)),
            Tok::Open { name, attrs, self_close } => {
                if drop_with_content(&name) { if !self_close { skip = 1; skip_name = name; } continue; }
                let Some(mut tag) = tag_map(&name) else { continue };
                if tag == "p" && !keep_p { tag = "div"; }
                if tag == "br" { out.push_str("<br>"); continue; }
                if stack.len() >= MAX_DEPTH { continue; }
                let mut attr = String::new();
                let st = clean_style(&decode(attr_of(&attrs, "style")));   // atrybut może mieć encje (&quot;)
                if !st.is_empty() { attr += &format!(" style=\"{st}\""); }
                if tag == "span" && has_word(attr_of(&attrs, "class"), "cg-text-squash") { attr = format!(" class=\"cg-text-squash\"{attr}"); }
                out.push_str(&format!("<{tag}{attr}>"));
                stack.push(tag);
            }
            Tok::Close(name) => {
                let Some(mut tag) = tag_map(&name) else { continue };
                if tag == "br" { continue; }
                if tag == "p" && !keep_p { tag = "div"; }
                let Some(k) = stack.iter().rposition(|s| *s == tag) else { continue };
                while stack.len() > k { if let Some(s) = stack.pop() { out.push_str(&format!("</{s}>")); } }
            }
        }
    }
    while let Some(s) = stack.pop() { out.push_str(&format!("</{s}>")); }
    out
}

fn has_word(s: &str, w: &str) -> bool {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).any(|x| x == w) || s.contains(w) && s.split_whitespace().any(|x| x == w)
}

fn named_entity(e: &str) -> Option<&'static str> {
    Some(match e {
        "nbsp" => "\u{a0}", "lt" => "<", "gt" => ">", "quot" => "\"", "apos" => "'", "amp" => "&", "copy" => "©", "reg" => "®", "deg" => "°", "euro" => "€",
        "ndash" => "–", "mdash" => "—", "hellip" => "…", "laquo" => "«", "raquo" => "»", "bdquo" => "„", "ldquo" => "“", "rdquo" => "”", "lsquo" => "‘", "rsquo" => "’",
        "middot" => "·", "bull" => "•", _ => return None,
    })
}

fn decode(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] == '&' {
            if let Some(len) = entity_len(&c, i) {
                let e: String = c[i + 1..i + len - 1].iter().collect();
                let rep = if let Some(num) = e.strip_prefix('#') {
                    let code = if let Some(h) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) { u32::from_str_radix(h, 16).ok() } else { num.parse::<u32>().ok() };
                    Some(code.filter(|c| *c > 0 && *c < 0x110000 && !(0xd800..0xe000).contains(c)).and_then(char::from_u32).map(|c| c.to_string()).unwrap_or_default())
                } else { named_entity(&e).map(|x| x.to_string()) };
                if let Some(r) = rep { out.push_str(&r); i += len; continue; }
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

// ── RichText v1 ──────────────────────────────────────────────────────────────────────────────────

fn px(v: &str) -> Option<f64> { let n = ends_unit(v, &["px"])?; if is_num(n) { n.parse::<f64>().ok().map(|x| (x * 100.0).round() / 100.0) } else { None } }
fn emv(v: &str) -> Option<f64> { let n = ends_unit(v, &["em"])?; if is_num(n) { n.parse::<f64>().ok().map(|x| (x * 1000.0).round() / 1000.0) } else { None } }
fn wnum(v: &str) -> Option<u32> {
    let l = v.to_ascii_lowercase();
    if l == "bold" { Some(700) } else if l == "normal" { Some(400) } else if is_weight(&l) && l.len() == 3 { l.parse().ok() } else { None }
}

fn marks_from_tag(tag: &str, style: &str, class: &str) -> Map<String, Value> {
    let mut m = Map::new();
    match tag { "strong" => { m.insert("weight".into(), json!(700)); } "em" => { m.insert("italic".into(), json!(true)); } "u" => { m.insert("underline".into(), json!(true)); } "s" => { m.insert("strike".into(), json!(true)); } _ => {} }
    let mut squash_class = class.contains("cg-text-squash");
    for d in style.split(';') {
        let Some(i) = d.find(':') else { continue };
        if i < 1 { continue; }
        let p = d[..i].trim();
        let v = d[i + 1..].trim();
        match p {
            "color" => { m.insert("color".into(), json!(v)); }
            "background-color" => { m.insert("bg".into(), json!(v)); }
            "font-family" => { m.insert("family".into(), json!(v.split(',').next().unwrap_or("").replace(['\'', '"'], "").trim())); }
            "font-size" => { if let Some(n) = px(v) { m.insert("size".into(), json!(n)); } }
            "font-weight" => { if let Some(n) = wnum(v) { m.insert("weight".into(), json!(n)); } }
            "font-style" => { let l = v.to_ascii_lowercase(); if l.contains("italic") || l.contains("oblique") { m.insert("italic".into(), json!(true)); } else if l.contains("normal") { m.insert("italic".into(), json!(false)); } }
            "text-decoration" | "text-decoration-line" => { if v.contains("underline") { m.insert("underline".into(), json!(true)); } if v.contains("line-through") { m.insert("strike".into(), json!(true)); } }
            "letter-spacing" => { if let Some(n) = emv(v) { m.insert("tracking".into(), json!(n)); } }
            "text-transform" => { m.insert("transform".into(), json!(v.to_ascii_lowercase())); }
            "transform" => {
                let l = v.to_ascii_lowercase();
                if let Some(inner) = l.strip_prefix("scalex(").and_then(|r| r.strip_suffix(')')) {
                    if let Ok(n) = inner.trim().parse::<f64>() { m.insert("squash".into(), json!((n * 1000.0).round() / 1000.0)); squash_class = false; }
                }
            }
            _ => {}
        }
    }
    let _ = squash_class;   // sama klasa bez scaleX nie ustawia znacznika (JS: squash=null → usuwane)
    m
}

fn marks_equal(a: &Map<String, Value>, b: &Map<String, Value>) -> bool { a == b }

/// HTML (dowolny) → RichText v1.
pub fn to_rich(html: &str) -> Value {
    struct Frame { tag: String, marks: Map<String, Value>, block: bool }
    let clean = sanitize(html, false);
    let mut blocks: Vec<Value> = vec![];
    let mut cur: Option<usize> = None;
    let mut stack: Vec<Frame> = vec![];
    let mut block_align: Option<String> = None;

    fn ensure(blocks: &mut Vec<Value>, cur: &mut Option<usize>, align: &Option<String>) -> usize {
        if let Some(i) = *cur { return i; }
        let mut b = Map::new();
        b.insert("runs".into(), json!([]));
        if let Some(a) = align { b.insert("align".into(), json!(a)); }
        blocks.push(Value::Object(b));
        *cur = Some(blocks.len() - 1);
        blocks.len() - 1
    }

    for t in tokenize(&clean) {
        match t {
            Tok::Text(v) => {
                let d = decode(&v);
                if d.is_empty() { continue; }
                let ws = d.chars().all(char::is_whitespace);
                let start_of_line = match cur {
                    None => true,
                    Some(i) => blocks[i]["runs"].as_array().map_or(true, |r| r.last().map_or(true, |x| x.get("br").is_some())),
                };
                if ws && start_of_line { continue; }
                let i = ensure(&mut blocks, &mut cur, &block_align);
                let mut m = Map::new();
                for f in &stack { for (k, v) in &f.marks { m.insert(k.clone(), v.clone()); } }
                let runs = blocks[i]["runs"].as_array_mut();
                if let Some(runs) = runs {
                    if let Some(last) = runs.last_mut() {
                        let same = last.get("t").is_some() && marks_equal(last.get("m").and_then(|x| x.as_object()).unwrap_or(&Map::new()), &m);
                        if same { if let Some(Value::String(s)) = last.get_mut("t") { s.push_str(&d); } continue; }
                    }
                    let mut r = Map::new();
                    r.insert("t".into(), json!(d));
                    if !m.is_empty() { r.insert("m".into(), Value::Object(m)); }
                    runs.push(Value::Object(r));
                }
            }
            Tok::Open { name, attrs, .. } => {
                if name == "br" {
                    let i = ensure(&mut blocks, &mut cur, &block_align);
                    if let Some(runs) = blocks[i]["runs"].as_array_mut() { runs.push(json!({ "br": true })); }
                    continue;
                }
                if is_block(&name) {
                    cur = None;
                    let st = attr_of(&attrs, "style");
                    block_align = st.find("text-align:").map(|p| st[p + 11..].trim_start().chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>()).filter(|a| matches!(a.as_str(), "left" | "center" | "right"));
                    let rest: String = st.split(';').filter(|d| !d.trim_start().starts_with("text-align:")).collect::<Vec<_>>().join(";");
                    stack.push(Frame { tag: name.clone(), marks: marks_from_tag(&name, &rest, ""), block: true });
                    continue;
                }
                let tag = tag_map(&name).unwrap_or("span");
                stack.push(Frame { tag: tag.to_string(), marks: marks_from_tag(tag, attr_of(&attrs, "style"), attr_of(&attrs, "class")), block: false });
            }
            Tok::Close(name) => {
                let want = if name == "b" { "strong" } else { name.as_str() };
                let Some(k) = stack.iter().rposition(|f| f.tag == want) else { continue };
                let closed: Vec<Frame> = stack.drain(k..).collect();
                if closed.iter().any(|c| c.block) { cur = None; block_align = None; }
            }
        }
    }
    while blocks.last().map_or(false, |b| b["runs"].as_array().map_or(true, |r| r.is_empty())) { blocks.pop(); }
    json!({ "v": VERSION, "blocks": blocks })
}

fn marks_to_style(m: &Map<String, Value>) -> String {
    let mut st: Vec<String> = vec![];
    let s = |k: &str| m.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty());
    let n = |k: &str| m.get(k).and_then(|v| v.as_f64());
    if let Some(c) = s("color") { st.push(format!("color: {c}")); }
    if let Some(c) = s("bg") { st.push(format!("background-color: {c}")); }
    if let Some(c) = s("family") { st.push(format!("font-family: '{c}'")); }
    if let Some(v) = n("size") { st.push(format!("font-size: {}px", fmt(v))); }
    if let Some(v) = n("weight") { if v != 700.0 { st.push(format!("font-weight: {}", fmt(v))); } }
    if let Some(v) = n("tracking") { st.push(format!("letter-spacing: {}em", fmt(v))); }
    if let Some(v) = s("transform") { if v != "none" { st.push(format!("text-transform: {v}")); } }
    if let Some(v) = n("squash") { st.push(format!("transform: scaleX({})", fmt(v))); st.push("display: inline-block".into()); st.push("transform-origin: left center".into()); }
    st.join("; ")
}

/// Liczba jak w JS (`String(number)`): całkowite bez „.0”.
fn fmt(v: f64) -> String { if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") } }

/// RichText v1 → kanoniczny HTML.
pub fn to_html(rich: &Value) -> String {
    let Some(blocks) = rich.get("blocks").and_then(|b| b.as_array()) else { return String::new() };
    blocks.iter().take(500).map(|b| {
        let inner: String = b.get("runs").and_then(|r| r.as_array()).map(|runs| runs.iter().take(2000).map(|r| {
            if r.get("br").is_some() { return "<br>".to_string(); }
            let mut s = esc_text(r.get("t").and_then(|t| t.as_str()).unwrap_or(""));
            let empty = Map::new();
            let m = r.get("m").and_then(|m| m.as_object()).unwrap_or(&empty);
            if m.get("weight").and_then(|v| v.as_f64()) == Some(700.0) { s = format!("<strong>{s}</strong>"); }
            if m.get("italic").and_then(|v| v.as_bool()) == Some(true) { s = format!("<em>{s}</em>"); }
            if m.get("underline").and_then(|v| v.as_bool()) == Some(true) { s = format!("<u>{s}</u>"); }
            if m.get("strike").and_then(|v| v.as_bool()) == Some(true) { s = format!("<s>{s}</s>"); }
            let st = marks_to_style(m);
            if !st.is_empty() { s = format!("<span{} style=\"{st}\">{s}</span>", if m.get("squash").is_some() { " class=\"cg-text-squash\"" } else { "" }); }
            s
        }).collect()).unwrap_or_default();
        let al = b.get("align").and_then(|a| a.as_str()).filter(|a| *a != "left" && !a.is_empty()).map(|a| format!(" style=\"text-align: {a}\"")).unwrap_or_default();
        format!("<div{al}>{inner}</div>")
    }).collect()
}

/// Linie tekstu RichText: blok = jedna lub więcej linii (podział na `br`); końcowy `br` nie daje pustej linii, blok z samym `br` to jedna pusta linia.
pub fn rich_lines(rich: &Value) -> Vec<String> {
    let mut out = vec![];
    for b in rich.get("blocks").and_then(|b| b.as_array()).map(|v| v.as_slice()).unwrap_or(&[]) {
        let mut segs = vec![String::new()];
        let mut any = false;
        for r in b.get("runs").and_then(|r| r.as_array()).map(|v| v.as_slice()).unwrap_or(&[]) {
            if r.get("br").is_some() { segs.push(String::new()); any = true; }
            else if let Some(t) = r.get("t").and_then(|t| t.as_str()).filter(|t| !t.is_empty()) { if let Some(l) = segs.last_mut() { l.push_str(t); } any = true; }
        }
        if !any { continue; }
        if segs.len() > 1 && segs.last().map_or(false, |l| l.is_empty()) { segs.pop(); }
        out.extend(segs);
    }
    out
}

/// Tekst bez znaczników (linie rozdzielone `\n`, bez pustych linii na końcu).
pub fn plain(html: &str) -> String { rich_lines(&to_rich(html)).join("\n").trim_end_matches('\n').to_string() }

/// Linie tekstu (jak `plain`, jako wektor).
pub fn lines(html: &str) -> Vec<String> { let p = plain(html); if p.is_empty() { vec![] } else { p.split('\n').map(|s| s.to_string()).collect() } }

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap_or(0.0) - y.as_f64().unwrap_or(1e9)).abs() < 1e-9,
            (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q)),
            (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).map_or(false, |w| close(v, w))),
            _ => a == b,
        }
    }

    fn vectors() -> Option<Value> {
        let mine = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/text/model.json")).ok()?;
        let v: Value = serde_json::from_str(&mine).ok()?;
        // kopia w edytorze musi być identyczna z oryginałem w BTB-CG (gdy repozytorium leży obok)
        let orig = std::path::Path::new("/home/maciej/Nextcloud/GitHub/BTB-CG-System-alpha/test-fixtures/text/model.json");
        if let Ok(o) = std::fs::read_to_string(orig) { assert_eq!(serde_json::from_str::<Value>(&o).ok(), Some(v.clone()), "tests/text/model.json różni się od BTB-CG (skopiuj ponownie)"); }
        Some(v)
    }

    #[test]
    fn matches_btb_cg_vectors() {
        let Some(v) = vectors() else { return };
        let arr = |k: &str| v[k].as_array().cloned().unwrap_or_default();
        let mut n = 0;
        for c in arr("sanitize") { assert_eq!(sanitize(c["in"].as_str().unwrap_or(""), false), c["out"].as_str().unwrap_or(""), "sanitize {:?}", c["in"]); n += 1; }
        for c in arr("sanitizeKeepP") { assert_eq!(sanitize(c["in"].as_str().unwrap_or(""), true), c["out"].as_str().unwrap_or(""), "sanitize(keep p) {:?}", c["in"]); n += 1; }
        for c in arr("plain") { assert_eq!(plain(c["in"].as_str().unwrap_or("")), c["out"].as_str().unwrap_or(""), "plain {:?}", c["in"]); n += 1; }
        for c in arr("lines") {
            let want: Vec<String> = c["out"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            assert_eq!(lines(c["in"].as_str().unwrap_or("")), want, "lines {:?}", c["in"]); n += 1;
        }
        for c in arr("rich") { let got = to_rich(c["in"].as_str().unwrap_or("")); assert!(close(&got, &c["out"]), "rich {:?}\n  got  {}\n  want {}", c["in"], got, c["out"]); n += 1; }
        for c in arr("html") { assert_eq!(to_html(&c["rich"]), c["out"].as_str().unwrap_or(""), "html {:?}", c["rich"]); n += 1; }
        assert!(n >= 100, "za mało wektorów ({n})");
    }

    #[test]
    fn properties_on_random_inputs() {
        const PARTS: [&str; 32] = ["<div>", "</div>", "<p>", "</p>", "<span style=\"color:red\">", "</span>", "<b>", "</b>", "<i>", "<u>", "<br>", "<script>", "</script>", "<img src=x onerror=1>", "tekst", " ", "&amp;", "&lt;", "<", ">", "ą", "\"", "'", "<a href=\"x\">", "</a>", "<style>", "</style>", "style=\"x\"", "<!--", "-->", "<span style=\"font-size:20px;background:url(x)\">", "\n"];
        let mut seed: u32 = 12345;
        let mut rnd = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); seed as f64 / 4294967296.0 };
        for _ in 0..3000 {
            let mut s = String::new();
            let len = 1 + (rnd() * 14.0) as usize;
            for _ in 0..len { s.push_str(PARTS[(rnd() * PARTS.len() as f64) as usize % PARTS.len()]); }
            let c = sanitize(&s, false);
            assert_eq!(sanitize(&c, false), c, "idempotencja: {s:?}");
            let low = c.replace("&lt;", "").to_ascii_lowercase();
            assert!(!low.contains("<script") && !low.contains("<style") && !low.contains("<iframe") && !low.contains("<img") && !low.contains("<a ") && !low.contains("javascript:") && !low.contains("url("), "bezpieczny wynik: {s:?} → {c:?}");
            assert_eq!(plain(&c), plain(&s), "plain(sanitize(x)) = plain(x): {s:?}");
            let h = to_html(&to_rich(&s));
            assert_eq!(sanitize(&h, false), h, "toHtml kanoniczny: {s:?} → {h:?}");
            assert_eq!(plain(&h), plain(&s), "tekst zachowany: {s:?} → {h:?}");
        }
        assert!(sanitize(&"<div>".repeat(100), false).matches("<div>").count() <= 24);
        assert!(sanitize(&"x".repeat(300_000), false).chars().count() <= MAX_HTML);
    }

    #[test]
    fn hostile_input_never_panics() {
        for s in ["<", "<<<<", "<a", "<a b='c", "<a b=\"c", "&", "&#", "&#x;", "&#99999999;", "<!--", "<!-- -->", "\u{0}\u{1}", "<\u{301}>", &"<".repeat(5000), &"&".repeat(5000), "<div style=\"color:rgb(\">", "<span style=\";;;:::\">"] {
            let _ = sanitize(s, false); let _ = to_rich(s); let _ = plain(s); let _ = lines(s);
        }
    }
}
