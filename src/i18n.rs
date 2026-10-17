//! Tłumaczenia interfejsu. Teksty leżą w `assets/lang/<kod>.json` (płaska mapa „klucz → tekst”, `{nazwa}` = argument)
//! i są wkompilowane w binarkę. Brak klucza w bieżącym języku → polski → sam klucz (widać wtedy, czego brakuje).
//!
//! Wybór języka na razie nie ma UI: `set_lang` / `init_from_env` (`BTB_LANG=en`). Domyślnie polski.
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang { Pl, En }

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Pl, Lang::En];

    pub fn code(self) -> &'static str { match self { Lang::Pl => "pl", Lang::En => "en" } }

    /// Przyjmuje „pl”, „en”, „en_US.UTF-8”, „pl-PL” itd. (liczą się pierwsze dwie litery).
    pub fn from_code(s: &str) -> Option<Lang> {
        let s = s.trim().to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| s.starts_with(l.code()))
    }

    fn source(self) -> &'static str {
        match self {
            Lang::Pl => include_str!("../assets/lang/pl.json"),
            Lang::En => include_str!("../assets/lang/en.json"),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);
static TABLES: [OnceLock<HashMap<String, String>>; 2] = [OnceLock::new(), OnceLock::new()];
static MISSING: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);

pub fn lang() -> Lang { if CURRENT.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::Pl } }

pub fn set_lang(l: Lang) { CURRENT.store(l as u8, Ordering::Relaxed); }

/// Ustawia język ze zmiennej `BTB_LANG` (tymczasowo, do czasu wyboru w UI).
pub fn init_from_env() {
    if let Some(l) = std::env::var("BTB_LANG").ok().as_deref().and_then(Lang::from_code) { set_lang(l); }
}

fn table(l: Lang) -> &'static HashMap<String, String> {
    TABLES[l as usize].get_or_init(|| serde_json::from_str(l.source()).unwrap_or_else(|e| panic!("assets/lang/{}.json: {e}", l.code())))
}

/// Tekst w podanym języku, bez zapasowego (do testów i walidacji plików).
pub fn tr_in(l: Lang, key: &str) -> Option<&'static str> { table(l).get(key).map(String::as_str) }

pub fn keys(l: Lang) -> impl Iterator<Item = &'static str> { table(l).keys().map(String::as_str) }

/// Tekst w bieżącym języku; brak klucza → polski → sam klucz.
pub fn tr(key: &str) -> &'static str {
    if let Some(s) = tr_in(lang(), key).or_else(|| tr_in(Lang::Pl, key)) { return s; }
    let mut g = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(HashMap::new).entry(key.to_string()).or_insert_with(|| Box::leak(key.to_string().into_boxed_str()))
}

/// Jak `tr`, z podstawieniem argumentów: `trf("import.found", &[("n", &3)])` zamienia `{n}`.
pub fn trf(key: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut s = tr(key).to_string();
    for (name, v) in args { s = s.replace(&format!("{{{name}}}"), &v.to_string()); }
    s
}

/// Etykieta wartości z listy opcji (`prefix.wartość`); wartości bez tłumaczenia (np. nazwy easingów) zostają jak są.
pub fn opt(prefix: &str, value: &str) -> String {
    let key = format!("{prefix}.{value}");
    tr_in(lang(), &key).or_else(|| tr_in(Lang::Pl, &key)).map_or_else(|| value.to_string(), str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out); } else if p.extension().is_some_and(|x| x == "rs") { out.push(p); }
        }
    }

    /// Klucze użyte w kodzie jako literały: `tr("…")`, `trf("…"`, `opt("…"` (prefiks) – nie łapie kluczy składanych w locie.
    fn used_literals() -> (BTreeSet<String>, BTreeSet<String>) {
        let mut files = vec![];
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let (mut keys, mut prefixes) = (BTreeSet::new(), BTreeSet::new());
        for f in files.iter().filter(|f| !f.ends_with("i18n.rs")) {
            let src = std::fs::read_to_string(f).unwrap();
            for (name, set) in [("tr", &mut keys), ("trf", &mut keys), ("opt", &mut prefixes)] {
                let pat = format!("{name}(\"");
                for (i, _) in src.match_indices(&pat) {
                    if src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') { continue; }
                    let rest = &src[i + pat.len()..];
                    if let Some(end) = rest.find('"') { set.insert(rest[..end].to_string()); }
                }
            }
        }
        (keys, prefixes)
    }

    #[test]
    fn languages_have_the_same_keys() {
        let pl: BTreeSet<_> = keys(Lang::Pl).collect();
        let en: BTreeSet<_> = keys(Lang::En).collect();
        let only_pl: Vec<_> = pl.difference(&en).collect();
        let only_en: Vec<_> = en.difference(&pl).collect();
        assert!(only_pl.is_empty() && only_en.is_empty(), "tylko pl: {only_pl:?}, tylko en: {only_en:?}");
    }

    #[test]
    fn placeholders_match_between_languages() {
        fn names(s: &str) -> BTreeSet<String> {
            s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|x| x.0.to_string())).collect()
        }
        for k in keys(Lang::Pl) {
            let (a, b) = (tr_in(Lang::Pl, k).unwrap(), tr_in(Lang::En, k).unwrap());
            assert_eq!(names(a), names(b), "argumenty klucza {k}: „{a}” vs „{b}”");
        }
    }

    #[test]
    fn every_key_used_in_code_exists() {
        let (used, prefixes) = used_literals();
        assert!(used.len() > 100, "skaner znalazł za mało kluczy: {}", used.len());
        for l in Lang::ALL {
            let missing: Vec<_> = used.iter().filter(|k| tr_in(l, k).is_none()).collect();
            assert!(missing.is_empty(), "brak kluczy w {}: {missing:?}", l.code());
        }
        for p in prefixes { assert!(keys(Lang::Pl).any(|k| k.starts_with(&format!("{p}."))), "brak opcji z prefiksem {p}"); }
    }

    #[test]
    fn option_lists_are_translated() {
        use crate::anim::*;
        let lists: [(&str, &[&str]); 6] = [
            ("opt.anim", ANIM_TYPES), ("opt.dir", DIRECTIONS), ("opt.loop", LOOP_TYPES),
            ("opt.text_in", TEXT_IN), ("opt.text_out", TEXT_OUT), ("opt.stagger", STAGGER_FROM),
        ];
        for l in Lang::ALL {
            for (prefix, list) in lists {
                let missing: Vec<_> = list.iter().filter(|v| tr_in(l, &format!("{prefix}.{v}")).is_none()).collect();
                assert!(missing.is_empty(), "{} {prefix}: brak {missing:?}", l.code());
            }
            for t in crate::model::Element::ELEM_TYPES { assert!(tr_in(l, &format!("elemtype.{t}")).is_some(), "{} elemtype.{t}", l.code()); }
            for t in crate::model::LayerType::ALL { assert!(tr_in(l, &format!("layertype.{}", t.key())).is_some(), "{} layertype", l.code()); }
            for p in ["tx", "ty", "sx", "sy", "rot", "opacity", "path", "skew", "blur", "ox", "oy", "cl", "ct", "cr", "cb", "fx", "fy", "sh", "dg", "dr", "rgb"] {
                assert!(tr_in(l, &format!("prop.{p}")).is_some(), "{} prop.{p}", l.code());
            }
        }
    }

    #[test]
    fn lookup_falls_back_and_substitutes() {
        assert_eq!(tr_in(Lang::En, "menu.file"), Some("File"));
        assert_eq!(tr("no.such.key"), "no.such.key");
        assert_eq!(Lang::from_code("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_code("de"), None);
        assert_eq!(opt("opt.anim", "nie-ma-takiego"), "nie-ma-takiego");
        let s = trf("status.loaded", &[("name", &"X")]);
        assert!(s.contains('X') && !s.contains("{name}"), "{s}");
    }
}
