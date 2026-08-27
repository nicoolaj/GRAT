//! Internationalisation: flat JSON dictionaries, one per language, embedded in the
//! binary. Adding a language is one file in `src/i18n/` plus one entry in [`LANGS`].

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// (language code, raw JSON contents), embedded at compile time.
const LANGS: &[(&str, &str)] = &[
    ("en", include_str!("i18n/en.json")),
    ("fr", include_str!("i18n/fr.json")),
];

const FALLBACK_LANG: &str = "en";

/// eframe `Storage` key under which the manually-selected language is persisted.
pub const LANG_STORAGE_KEY: &str = "lang";

fn dictionaries() -> &'static HashMap<&'static str, HashMap<String, String>> {
    static DICTS: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    DICTS.get_or_init(|| {
        LANGS
            .iter()
            .map(|(lang, json)| {
                let dict: HashMap<String, String> = serde_json::from_str(json)
                    .unwrap_or_else(|e| panic!("invalid i18n JSON for '{lang}': {e}"));
                (*lang, dict)
            })
            .collect()
    })
}

fn current_lang_cell() -> &'static RwLock<String> {
    static CURRENT: OnceLock<RwLock<String>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(detect_locale()))
}

/// Detect the UI language from the OS locale: a locale starting with "fr" selects
/// French, anything else (including a locale we fail to read) falls back to English.
pub fn detect_locale() -> String {
    let locale = sys_locale::get_locale().unwrap_or_default();
    if locale.to_lowercase().starts_with("fr") {
        "fr".to_string()
    } else {
        "en".to_string()
    }
}

/// The current UI language code ("en" or "fr").
pub fn current_lang() -> String {
    current_lang_cell().read().unwrap().clone()
}

/// Every language code available (in [`LANGS`] declaration order), for a language menu.
pub fn available_langs() -> Vec<&'static str> {
    LANGS.iter().map(|(lang, _)| *lang).collect()
}

/// Override the UI language (from the menu). Unknown codes fall back to English.
pub fn set_lang(lang: &str) {
    let lang = if LANGS.iter().any(|(l, _)| *l == lang) {
        lang
    } else {
        FALLBACK_LANG
    };
    *current_lang_cell().write().unwrap() = lang.to_string();
}

/// Resolve `key` in the current language, falling back to English, then to the key
/// itself. Never panics, never returns an empty string for a known key.
pub fn t(key: &str) -> String {
    let dicts = dictionaries();
    let lang = current_lang();
    if let Some(s) = dicts.get(lang.as_str()).and_then(|d| d.get(key)) {
        return s.clone();
    }
    if let Some(s) = dicts.get(FALLBACK_LANG).and_then(|d| d.get(key)) {
        return s.clone();
    }
    key.to_string()
}

// Note/rest/pitch name helpers (`value_name`, `rest_name`, `pitch_name`) land in phase 2
// alongside `model::NoteValue`, which they read.
