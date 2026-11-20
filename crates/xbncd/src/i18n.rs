//! Translations: what xBNC says over IRC (`*status`, notices, module
//! messages) and the texts of the web panel.
//!
//! Every language is one JSON file (`locales/<code>.json` in the source):
//!
//! ```json
//! { "language": { "code": "pt", "name": "Português", "english_name": "Portuguese",
//!                 "locale": "pt-PT", "translators": ["..."] },
//!   "irc":   { "connected_to": "Ligado a {0} como {1}.", ... },
//!   "panel": { "Joined": "Dentro", ... },
//!   "installer": { "finding_the_program": "A procurar o programa", ... } }
//! ```
//!
//! The "installer" section holds the texts of the installer scripts
//! (`scripts/install.sh` and the scripts it uses), which read it themselves;
//! xBNC only checks it here (`xbncd locales check`).
//!
//! English and Spanish are built in. More languages (or corrections to the
//! built-in ones) are added by dropping files into the locales directory
//! (`locales_dir` in xbnc.toml, `/etc/xbnc/locales` by default, or the
//! `XBNC_LOCALES_DIR` environment variable); they are read at start and on
//! reload. A text missing from a language falls back to English, so an
//! incomplete translation never breaks anything.
//!
//! In IRC texts `{0}`, `{1}`... are replaced by arguments, and `<b>…</b>`
//! and `<u>…</u>` become IRC bold and underline. A translation whose
//! placeholders differ from the English text, or that contains line breaks
//! or control characters, is ignored (and reported by `xbncd locales check`).

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};

const BUILTIN: &[&str] = &[
    include_str!("../../../locales/en.json"),
    include_str!("../../../locales/es.json"),
];

/// Largest locale file accepted.
const MAX_FILE: u64 = 2 * 1024 * 1024;
/// Most languages loaded at once.
const MAX_LANGUAGES: usize = 100;

/// A loaded language. Cheap to copy; compare with [`Lang::EN`] etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lang(u16);

impl Default for Lang {
    fn default() -> Lang {
        Lang::EN
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LangInfo {
    pub code: String,
    /// Name in the language itself ("Español").
    pub name: String,
    pub english_name: String,
    /// BCP-47 tag for dates and numbers ("es-ES").
    pub locale: String,
    pub translators: Vec<String>,
    /// Share of the English texts this language translates (0–100).
    pub complete: u8,
}

struct Catalog {
    info: LangInfo,
    /// IRC texts, with `<b>`/`<u>` already turned into control codes.
    irc: HashMap<String, String>,
    /// Panel texts as written in the file (English text → translation).
    panel: BTreeMap<String, String>,
}

static REGISTRY: LazyLock<RwLock<Vec<Arc<Catalog>>>> = LazyLock::new(|| {
    let mut v = Vec::new();
    for src in BUILTIN {
        let (cat, problems) = parse(src, None).expect("built-in locale is valid");
        debug_assert!(problems.is_empty(), "{problems:?}");
        v.push(Arc::new(cat));
    }
    RwLock::new(v)
});

fn get(lang: Lang) -> Option<Arc<Catalog>> {
    REGISTRY
        .read()
        .expect("locale registry")
        .get(lang.0 as usize)
        .cloned()
}

impl Lang {
    pub const EN: Lang = Lang(0);
    pub const ES: Lang = Lang(1);

    /// A loaded language by code ("es", "pt-BR", "PT_br"); a regional code
    /// falls back to its base language.
    pub fn parse(code: &str) -> Option<Lang> {
        let c = normalize_code(code)?;
        let reg = REGISTRY.read().expect("locale registry");
        let find = |c: &str| reg.iter().position(|l| l.info.code == c);
        find(&c)
            .or_else(|| find(c.split('-').next().unwrap_or("")))
            .map(|i| Lang(i as u16))
    }

    pub fn code(self) -> String {
        get(self)
            .map(|c| c.info.code.clone())
            .unwrap_or_else(|| "en".into())
    }

    /// BCP-47 tag for dates and numbers.
    pub fn locale(self) -> String {
        get(self)
            .map(|c| c.info.locale.clone())
            .unwrap_or_else(|| "en-GB".into())
    }

    /// The user's language, or the server default when unset or unknown.
    pub fn for_user(user_lang: &str, default: &str) -> Lang {
        Lang::parse(user_lang)
            .or_else(|| Lang::parse(default))
            .unwrap_or_default()
    }
}

/// "pt_BR" → "pt-br". None when it is not a plausible language code.
fn normalize_code(code: &str) -> Option<String> {
    let c = code.trim().to_ascii_lowercase().replace('_', "-");
    let mut parts = c.split('-');
    let base = parts.next()?;
    let ok_base = (2..=3).contains(&base.len()) && base.chars().all(|c| c.is_ascii_lowercase());
    let ok_rest =
        parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()));
    (ok_base && ok_rest && c.len() <= 20).then_some(c)
}

/// Every loaded language, in load order (English first).
pub fn languages() -> Vec<LangInfo> {
    REGISTRY
        .read()
        .expect("locale registry")
        .iter()
        .map(|c| c.info.clone())
        .collect()
}

/// Codes of the loaded languages, for messages.
pub fn codes() -> Vec<String> {
    languages().into_iter().map(|l| l.code).collect()
}

/// The text of `key` in `lang`, else in English.
pub fn lookup(lang: Lang, key: &str) -> Option<String> {
    let own = get(lang).and_then(|c| c.irc.get(key).cloned());
    own.or_else(|| get(Lang::EN).and_then(|c| c.irc.get(key).cloned()))
}

/// The translations of one panel text in every installed language.
pub fn panel_text_everywhere(key: &str) -> Vec<String> {
    REGISTRY
        .read()
        .expect("locale registry")
        .iter()
        .filter_map(|c| c.panel.get(key).cloned())
        .collect()
}

/// Translates `key` with positional arguments; an unknown key is returned
/// as is.
pub fn tr(lang: Lang, key: &str, args: &[&str]) -> String {
    let mut out = lookup(lang, key).unwrap_or_else(|| key.to_string());
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), a);
    }
    out
}

/// The panel texts of a language (English text → translation).
pub fn panel_texts(code: &str) -> Option<BTreeMap<String, String>> {
    let l = Lang::parse(code)?;
    get(l).map(|c| c.panel.clone())
}

/// `<b>`/`</b>` → bold, `<u>`/`</u>` → underline.
fn irc_codes(s: &str) -> String {
    s.replace("<b>", "\x02")
        .replace("</b>", "\x02")
        .replace("<u>", "\x1f")
        .replace("</u>", "\x1f")
}

/// The `{n}` placeholders of a text.
fn placeholders(s: &str) -> Vec<usize> {
    let mut v: Vec<usize> = (0..10)
        .filter(|i| s.contains(&format!("{{{i}}}")))
        .collect();
    v.sort_unstable();
    v
}

fn tag_counts(s: &str) -> [usize; 4] {
    ["<b>", "</b>", "<u>", "</u>"].map(|t| s.matches(t).count())
}

/// Parses a locale file. `english` (the English catalog) is used to check
/// placeholders; problems are returned as messages and the faulty texts
/// left out.
fn parse(src: &str, english: Option<&Catalog>) -> Result<(Catalog, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(src).context("not valid JSON")?;
    let meta = v.get("language").context("missing \"language\" section")?;
    let s = |k: &str| {
        meta.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let code = normalize_code(&s("code"))
        .context("language.code is not a valid language code (\"pt\", \"pt-BR\")")?;
    let name = s("name");
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        bail!("language.name must be the name of the language in itself (up to 40 characters)");
    }
    let english_name = s("english_name");
    let locale = match s("locale") {
        l if l.is_empty() => code.clone(),
        l => l,
    };
    if locale.len() > 20
        || !locale
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        bail!("language.locale must look like \"pt-PT\"");
    }
    let translators: Vec<String> = meta
        .get("translators")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|x| x.chars().filter(|c| !c.is_control()).take(80).collect())
                .take(50)
                .collect()
        })
        .unwrap_or_default();
    let mut problems = Vec::new();
    let mut irc = HashMap::new();
    if let Some(m) = v.get("irc").and_then(|x| x.as_object()) {
        for (k, val) in m {
            let Some(text) = val.as_str() else {
                problems.push(format!("irc.{k}: not a text"));
                continue;
            };
            if text.is_empty() {
                continue;
            }
            if text.chars().any(|c| c.is_control()) || text.len() > 2000 {
                problems.push(format!(
                    "irc.{k}: contains line breaks or control characters"
                ));
                continue;
            }
            if let Some(en) = english {
                match en.irc.get(k) {
                    None => {
                        problems.push(format!("irc.{k}: not an xBNC text (unused)"));
                        continue;
                    }
                    Some(en_text) => {
                        if placeholders(en_text) != placeholders(text) {
                            problems.push(format!(
                                "irc.{k}: the placeholders {{0}}, {{1}}... must be the same as in English"
                            ));
                            continue;
                        }
                    }
                }
            }
            let [bo, bc, uo, uc] = tag_counts(text);
            if bo != bc || uo != uc {
                problems.push(format!(
                    "irc.{k}: every <b> needs its </b> (and <u> its </u>)"
                ));
                continue;
            }
            irc.insert(k.clone(), irc_codes(text));
        }
    }
    let mut panel = BTreeMap::new();
    if let Some(m) = v.get("panel").and_then(|x| x.as_object()) {
        for (k, val) in m {
            let Some(text) = val.as_str() else {
                problems.push(format!("panel.{k:?}: not a text"));
                continue;
            };
            if text.is_empty() {
                continue;
            }
            if text.chars().any(|c| c.is_control() && c != '\n') || text.len() > 4000 {
                problems.push(format!("panel.{k:?}: contains control characters"));
                continue;
            }
            if let Some(en) = english {
                if !en.panel.contains_key(k) {
                    problems.push(format!("panel.{k:?}: not an xBNC text (unused)"));
                    continue;
                }
                let names = |s: &str| {
                    let mut v: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|p| p.split_once('}').map(|x| x.0.to_string()))
                        .filter(|n| {
                            !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_')
                        })
                        .collect();
                    v.sort();
                    v
                };
                if names(k) != names(text) {
                    problems.push(format!(
                        "panel.{k:?}: the placeholders {{name}} must be the same as in English"
                    ));
                    continue;
                }
            }
            panel.insert(k.clone(), text.to_string());
        }
    }
    let complete = match english {
        Some(en) if !en.irc.is_empty() || !en.panel.is_empty() => {
            let total = en.irc.len() + en.panel.len();
            let have = irc.len() + panel.len();
            ((have * 100) / total.max(1)).min(100) as u8
        }
        _ => 100,
    };
    Ok((
        Catalog {
            info: LangInfo {
                code,
                name,
                english_name,
                locale,
                translators,
                complete,
            },
            irc,
            panel,
        },
        problems,
    ))
}

/// Result of [`check`].
pub struct CheckReport {
    pub info: LangInfo,
    /// Texts that will be ignored, and why.
    pub problems: Vec<String>,
    /// Texts of English the file does not have.
    pub missing: Vec<String>,
    /// Texts identical to English (perhaps not translated yet).
    pub same: Vec<String>,
}

/// Checks a locale file against English. Returns the language, the
/// problems (texts that will be ignored), the texts missing and the texts
/// identical to English (perhaps not translated yet). Used by
/// `xbncd locales check`.
pub fn check(src: &str) -> Result<CheckReport> {
    let en = get(Lang::EN).expect("English");
    let (cat, mut problems) = parse(src, Some(&en))?;
    let (installer_problems, installer_missing, installer_same) =
        check_installer(src, &cat.info.code)?;
    problems.extend(installer_problems);
    let mut missing: Vec<String> = en
        .irc
        .keys()
        .filter(|k| !cat.irc.contains_key(*k))
        .map(|k| format!("irc.{k}"))
        .chain(
            en.panel
                .keys()
                .filter(|k| !cat.panel.contains_key(*k))
                .map(|k| format!("panel.{k:?}")),
        )
        .chain(installer_missing)
        .collect();
    missing.sort();
    problems.sort();
    let mut same: Vec<String> = cat
        .irc
        .iter()
        .filter(|_| cat.info.code != "en")
        .filter(|(k, v)| en.irc.get(*k) == Some(*v) && v.chars().any(|c| c.is_alphabetic()))
        .map(|(k, _)| format!("irc.{k}"))
        .chain(installer_same)
        .collect();
    same.sort();
    Ok(CheckReport {
        info: cat.info,
        problems,
        missing,
        same,
    })
}

/// The texts of the installer scripts: the "installer" section of a file.
fn installer_section(src: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
    let v: serde_json::Value = serde_json::from_str(src).context("not valid JSON")?;
    Ok(v.get("installer")
        .and_then(|x| x.as_object())
        .cloned()
        .unwrap_or_default())
}

/// Checks the "installer" section against English: the same placeholders,
/// no line breaks or tabs (the installer reads one text per line), and only
/// texts the installer uses. Returns the problems, the missing texts and the
/// texts identical to English.
fn check_installer(src: &str, code: &str) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let english = installer_section(BUILTIN[0]).expect("English installer texts");
    let mine = installer_section(src)?;
    let mut problems = Vec::new();
    let mut same = Vec::new();
    for (k, val) in &mine {
        let Some(text) = val.as_str() else {
            problems.push(format!("installer.{k}: not a text"));
            continue;
        };
        let Some(en_text) = english.get(k).and_then(|x| x.as_str()) else {
            problems.push(format!("installer.{k}: not an xBNC text (unused)"));
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if text.chars().any(char::is_control) || text.len() > 1000 {
            problems.push(format!(
                "installer.{k}: contains line breaks, tabs or control characters"
            ));
            continue;
        }
        if placeholders(en_text) != placeholders(text) {
            problems.push(format!(
                "installer.{k}: the placeholders {{0}}, {{1}}... must be the same as in English"
            ));
            continue;
        }
        if code != "en" && text == en_text && text.chars().filter(|c| c.is_alphabetic()).count() > 2
        {
            same.push(format!("installer.{k}"));
        }
    }
    let missing = english
        .iter()
        .filter(|(k, v)| {
            v.as_str().is_some_and(|s| !s.is_empty())
                && mine
                    .get(*k)
                    .and_then(|x| x.as_str())
                    .is_none_or(str::is_empty)
        })
        .map(|(k, _)| format!("installer.{k}"))
        .collect();
    Ok((problems, missing, same))
}

/// Loads every `*.json` of `dir`. A language already loaded is updated (its
/// texts replace the built-in ones where given); a new one is added.
/// Returns one message per file loaded or rejected. A missing directory is
/// not an error.
pub fn load_dir(dir: &Path) -> Vec<String> {
    let mut report = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return report;
    };
    let mut files: Vec<_> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    for p in files {
        let res = (|| -> Result<String> {
            let meta = std::fs::metadata(&p)?;
            if meta.len() > MAX_FILE {
                bail!("larger than 2 MB");
            }
            let src = std::fs::read_to_string(&p)?;
            add(&src)
        })();
        match res {
            Ok(msg) => report.push(format!("{}: {msg}", p.display())),
            Err(e) => report.push(format!("{}: not loaded: {e:#}", p.display())),
        }
    }
    report
}

/// Adds or updates one language from the text of a locale file.
pub fn add(src: &str) -> Result<String> {
    let en = get(Lang::EN).expect("English");
    let builtin = |code: &str| -> Option<Catalog> {
        BUILTIN
            .iter()
            .filter_map(|s| parse(s, None).ok())
            .map(|(c, _)| c)
            .find(|c| c.info.code == code)
    };
    let (mut cat, problems) = parse(src, Some(&en))?;
    let code = cat.info.code.clone();
    // Over a built-in language, the file only replaces the texts it has.
    if let Some(base) = builtin(&code) {
        let mut irc = base.irc;
        irc.extend(cat.irc);
        cat.irc = irc;
        let mut panel = base.panel;
        panel.extend(cat.panel);
        cat.panel = panel;
        if code == "en" {
            cat.info.complete = 100;
        } else {
            let total = en.irc.len() + en.panel.len();
            cat.info.complete =
                (((cat.irc.len() + cat.panel.len()) * 100) / total.max(1)).min(100) as u8;
        }
    }
    let complete = cat.info.complete;
    let mut reg = REGISTRY.write().expect("locale registry");
    match reg.iter().position(|c| c.info.code == code) {
        // The position stays the same, so languages already in use keep it.
        Some(i) => reg[i] = Arc::new(cat),
        None => {
            if reg.len() >= MAX_LANGUAGES {
                bail!("too many languages");
            }
            reg.push(Arc::new(cat));
        }
    }
    let mut msg = format!("{code} loaded ({complete}% translated)");
    if !problems.is_empty() {
        msg.push_str(&format!(
            "; {} texts ignored (see xbncd locales check)",
            problems.len()
        ));
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_args() {
        assert_eq!(
            tr(Lang::ES, "module_enabled", &["away"]),
            "Módulo away activado."
        );
        assert_eq!(
            tr(Lang::EN, "module_enabled", &["away"]),
            "Module away enabled."
        );
        assert_eq!(tr(Lang::EN, "no.such.key", &[]), "no.such.key");
        assert_eq!(Lang::parse("es-ES"), Some(Lang::ES));
        assert_eq!(Lang::parse("EN_us"), Some(Lang::EN));
        assert_eq!(Lang::parse("xx"), None);
        assert_eq!(Lang::parse("../../etc"), None);
        assert_eq!(Lang::for_user("", "es"), Lang::ES);
        assert_eq!(Lang::for_user("en", "es"), Lang::EN);
        assert_eq!(Lang::for_user("zz", "zz"), Lang::EN);
        // Bold tags become IRC codes.
        assert_eq!(
            tr(Lang::EN, "st.xbnc_status_commands", &[]),
            "\x02xBNC\x02 *status commands"
        );
    }

    #[test]
    fn built_in_languages_are_complete_and_consistent() {
        for src in BUILTIN.iter().skip(1) {
            let r = check(src).unwrap();
            let (info, problems, missing) = (r.info, r.problems, r.missing);
            assert!(problems.is_empty(), "{}: {problems:#?}", info.code);
            assert!(missing.is_empty(), "{}: missing {missing:#?}", info.code);
        }
        let en = get(Lang::EN).unwrap();
        for (k, v) in &en.irc {
            assert!(!v.is_empty(), "{k}");
        }
    }

    #[test]
    fn a_new_language_is_added_and_falls_back_to_english() {
        let src = r#"{"language": {"code": "xq", "name": "Testish"},
            "irc": {"module_enabled": "Moduul {0} aan.",
                    "connected_to": "Verbonden {0}",
                    "shutting_down": "bad\r\nQUIT :x"},
            "panel": {"Joined": "Binnen"}}"#;
        let msg = add(src).unwrap();
        assert!(msg.contains("xq loaded"), "{msg}");
        assert!(msg.contains("2 texts ignored"), "{msg}");
        let l = Lang::parse("xq").unwrap();
        assert_eq!(tr(l, "module_enabled", &["away"]), "Moduul away aan.");
        // Wrong placeholders and line breaks are ignored: English is used.
        assert_eq!(
            tr(l, "connected_to", &["net", "me"]),
            "Connected to net as me."
        );
        assert_eq!(tr(l, "shutting_down", &[]), "xBNC is shutting down.");
        assert_eq!(
            panel_texts("xq").unwrap().get("Joined").map(String::as_str),
            Some("Binnen")
        );
        // Loading it again keeps its position.
        add(src).unwrap();
        assert_eq!(Lang::parse("xq"), Some(l));
        assert!(languages()
            .iter()
            .any(|i| i.code == "xq" && i.complete < 10));
    }

    /// The installer texts of a language file are checked like the others:
    /// same placeholders as English, one line each, only known texts.
    #[test]
    fn installer_texts_are_checked() {
        let src = r#"{"language": {"code": "xi", "name": "Installish"},
            "installer": {"finding_the_program": "Finding it",
                          "installing_the_binary_to": "Installing to {1}",
                          "writing": "Write\nlines {0}",
                          "no_such_text": "x"}}"#;
        let r = check(src).unwrap();
        let p = r.problems.join("\n");
        assert!(
            p.contains("installer.installing_the_binary_to: the placeholders"),
            "{p}"
        );
        assert!(p.contains("installer.writing: contains line breaks"), "{p}");
        assert!(
            p.contains("installer.no_such_text: not an xBNC text"),
            "{p}"
        );
        assert!(!p.contains("finding_the_program"), "{p}");
        assert!(r
            .missing
            .iter()
            .any(|m| m == "installer.creating_directories"));
        assert!(!r
            .missing
            .iter()
            .any(|m| m == "installer.finding_the_program"));
    }

    /// English and Spanish have every installer text, with the right
    /// placeholders: the installer never shows a key instead of a text.
    #[test]
    fn builtin_installer_texts_are_complete() {
        for src in BUILTIN {
            let r = check(src).unwrap();
            let inst: Vec<_> = r
                .missing
                .iter()
                .filter(|m| m.starts_with("installer."))
                .collect();
            assert!(inst.is_empty(), "missing {inst:?}");
            let bad: Vec<_> = r
                .problems
                .iter()
                .filter(|m| m.starts_with("installer."))
                .collect();
            assert!(bad.is_empty(), "problems {bad:?}");
        }
        assert!(installer_section(BUILTIN[0]).unwrap().len() > 100);
    }

    /// Every text key the installer scripts use exists in English, and
    /// every English installer text is used by a script.
    #[test]
    fn installer_keys_match_the_scripts() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/");
        let mut src = String::new();
        for f in [
            "install.sh",
            "prereqs.sh",
            "binary.sh",
            "update.sh",
            "i18n.sh",
        ] {
            src.push_str(&std::fs::read_to_string(format!("{dir}{f}")).unwrap());
            src.push('\n');
        }
        let en = installer_section(BUILTIN[0]).unwrap();
        let word = |s: &str| {
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        };
        let mut used = std::collections::BTreeSet::new();
        for line in src.lines() {
            let line = line.replace("$(", " ");
            let toks: Vec<&str> = line
                .split_whitespace()
                .map(|t| {
                    t.trim_matches(|c| {
                        c == '"' || c == '(' || c == ')' || c == ';' || c == ':' || c == '$'
                    })
                })
                .collect();
            for (i, t) in toks.iter().enumerate() {
                let key = match *t {
                    "say" | "die" | "step" | "box" | "t" | "yes_no" => toks.get(i + 1),
                    "note" | "box_in" => toks.get(i + 2),
                    "ask" | "ask_port" => toks.get(i + 2),
                    _ => None,
                };
                if let Some(k) = key {
                    if word(k) && k.len() > 2 && en.contains_key(*k) {
                        used.insert(k.to_string());
                    } else if word(k) && k.len() > 3 && k.contains('_') {
                        panic!("the scripts use the installer text {k:?}, missing from locales/en.json");
                    }
                }
            }
            // Help options: "--domain|opt_domain"
            for part in line.split('"') {
                if let Some((_, k)) = part.split_once('|') {
                    if k.starts_with("opt_") {
                        used.insert(k.to_string());
                        used.insert(format!("{k}_arg"));
                    }
                }
            }
        }
        let unused: Vec<_> = en.keys().filter(|k| !used.contains(*k)).collect();
        assert!(
            unused.is_empty(),
            "installer texts no script uses: {unused:?}"
        );
    }

    #[test]
    fn bad_files_are_refused() {
        assert!(add("not json").is_err());
        assert!(add(r#"{"irc": {}}"#).is_err());
        assert!(add(r#"{"language": {"code": "x/y", "name": "A"}}"#).is_err());
        assert!(add(r#"{"language": {"code": "xz", "name": ""}}"#).is_err());
    }

    /// Every `self.t("key"…)`, `tr(…, "key"…)` and `tr!(self, "key"…)` in the
    /// source has an English text.
    #[test]
    fn all_used_keys_exist() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files = vec![std::path::PathBuf::from(dir)];
        let mut missing = Vec::new();
        let en = get(Lang::EN).unwrap();
        while let Some(p) = files.pop() {
            if p.is_dir() {
                for e in std::fs::read_dir(&p).unwrap() {
                    files.push(e.unwrap().path());
                }
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            for marker in [
                "self.t(\"",
                "tr(lang, \"",
                "self.tr(\"",
                "tr!(self, \"",
                "i18n::tr(self.lang, \"",
            ] {
                for part in src.split(marker).skip(1) {
                    let key = part.split('"').next().unwrap();
                    // "key" is the placeholder of the doc comments.
                    if key != "key" && !en.irc.contains_key(key) {
                        missing.push(format!("{}: {key}", p.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "keys without text: {missing:#?}");
    }

    /// Every command of `*status` has its help texts.
    #[test]
    fn every_status_command_has_help() {
        let en = get(Lang::EN).unwrap();
        let src = include_str!("network/status_more.rs");
        for part in src.split("cmd!(\"").skip(1) {
            let name = part.split('"').next().unwrap();
            for t in ["desc", "example"] {
                assert!(
                    en.irc.contains_key(&format!("help.{name}.{t}")),
                    "help.{name}.{t}"
                );
            }
        }
    }
}
