//! Internationalisation: flat JSON dictionaries, one per language, embedded in the
//! binary. Adding a language is one file in `src/i18n/` plus one entry in [`LANGS`].

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::model::NoteValue;

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

// Keys ordered to match `NoteValue`'s variant order — keep the two in sync.
const VALUE_KEYS: [&str; 6] = [
    "value.whole",
    "value.half",
    "value.quarter",
    "value.eighth",
    "value.sixteenth",
    "value.thirty_second",
];
const REST_KEYS: [&str; 6] = [
    "rest.whole",
    "rest.half",
    "rest.quarter",
    "rest.eighth",
    "rest.sixteenth",
    "rest.thirty_second",
];

/// Localised name of a note value, e.g. "quarter note" / "noire".
pub fn value_name(v: NoteValue) -> String {
    t(VALUE_KEYS[v as usize])
}

/// Localised name of a rest of the given value, e.g. "quarter rest" / "soupir".
pub fn rest_name(v: NoteValue) -> String {
    t(REST_KEYS[v as usize])
}

/// Localised pitch-class name for a MIDI note number, e.g. "C" / "Do" (octave ignored).
pub fn pitch_name(midi: u8) -> String {
    t(&format!("pitch.{}", midi % 12))
}

/// Letter name of a pitch class, whatever the interface language: the "E A D G B E"
/// option exists precisely for a reader who wants letters rather than "Mi La Ré".
/// Music symbols, like the chord suffixes, not English words.
const LETTERS: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A pitch class named in letters (`letters`) or in the interface's own names.
pub fn note_name(midi: u8, letters: bool) -> String {
    if letters {
        LETTERS[(midi % 12) as usize].to_string()
    } else {
        pitch_name(midi)
    }
}

/// A tuning's open strings, lowest string first, the way players say it:
/// "E A D G B E", or "G C E A" for a ukulele.
pub fn tuning_names(tuning: &[u8], letters: bool) -> String {
    tuning
        .iter()
        .rev()
        .map(|&m| note_name(m, letters))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pitch name with its octave, e.g. "E2" / "Mi1": French numbering puts middle C
/// in octave 3, English in octave 4, hence the `pitch.middle_c_octave` key.
pub fn pitch_label(midi: u8) -> String {
    let middle_c: i32 = t("pitch.middle_c_octave").parse().unwrap_or(4);
    format!(
        "{}{}",
        pitch_name(midi),
        i32::from(midi) / 12 - 5 + middle_c
    )
}
