//! Integration tests against the public `tablatures` crate API.

use tablatures::i18n;
use tablatures::model::{Bar, Document, Dur, Event, Note, NoteValue, Strum, Technique};

#[test]
fn pitch_resolves_tuning_fret_and_capo() {
    let doc = Document::new_empty();

    // String index 5 (low E, string 6), open (fret 0): MIDI 40.
    let low_e_open = Note {
        string: 5,
        fret: 0,
        tech: Technique::Plain,
        tie_next: false,
    };
    assert_eq!(doc.pitch(&low_e_open), 40);

    // String index 0 (high E, string 1), fret 3: MIDI 67 (G4).
    let high_e_3rd_fret = Note {
        string: 0,
        fret: 3,
        tech: Technique::Plain,
        tie_next: false,
    };
    assert_eq!(doc.pitch(&high_e_3rd_fret), 67);
}

#[test]
fn dur_ticks_matches_expected_values() {
    let dotted_quarter = Dur {
        base: NoteValue::Quarter,
        dots: 1,
    };
    assert_eq!(dotted_quarter.ticks(), 1440);

    let sixteenth = Dur {
        base: NoteValue::Sixteenth,
        dots: 0,
    };
    assert_eq!(sixteenth.ticks(), 240);

    let double_dotted_half = Dur {
        base: NoteValue::Half,
        dots: 2,
    };
    assert_eq!(double_dotted_half.ticks(), 3360);
}

#[test]
fn document_round_trips_through_json() {
    let mut doc = Document::new_empty();
    doc.title = "Test Song".to_string();
    doc.author = "Test Author".to_string();
    doc.bars[0].events.push(Event {
        dur: Dur {
            base: NoteValue::Quarter,
            dots: 0,
        },
        notes: vec![
            Note {
                string: 0,
                fret: 0,
                tech: Technique::Plain,
                tie_next: false,
            },
            Note {
                string: 1,
                fret: 1,
                tech: Technique::Slide { to_fret: 3 },
                tie_next: true,
            },
        ],
        strum: Some(Strum::Down),
    });
    doc.bars[0].events.push(Event {
        dur: Dur {
            base: NoteValue::Eighth,
            dots: 1,
        },
        notes: Vec::new(), // rest
        strum: None,
    });
    doc.bars.push(Bar {
        events: Vec::new(),
        time_sig: Some((3, 4)),
    });

    let json = serde_json::to_string_pretty(&doc).expect("serialize");
    let round_tripped: Document = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(doc, round_tripped);
}

#[test]
fn old_json_without_new_fields_still_loads() {
    // Simulates a document saved by an earlier format version, missing later fields.
    let minimal = r#"{"title": "Old Doc"}"#;
    let doc: Document = serde_json::from_str(minimal).expect("deserialize minimal document");
    assert_eq!(doc.title, "Old Doc");
    assert_eq!(doc.tuning, [64, 59, 55, 50, 45, 40]);
    assert_eq!(doc.tempo, 120);
}

#[test]
// ponytail: i18n's current language is a process-global; safe here because no other
// test in this binary reads/sets it. Revisit (per-call lang param, or --test-threads=1)
// if a future test starts depending on the language too.
fn t_resolves_known_keys_per_language_and_falls_back_for_unknown_keys() {
    i18n::set_lang("en");
    let en_new = i18n::t("menu.new");

    i18n::set_lang("fr");
    let fr_new = i18n::t("menu.new");

    assert_ne!(
        en_new, fr_new,
        "menu.new should differ between English and French"
    );

    let unknown = i18n::t("this.key.does.not.exist");
    assert_eq!(unknown, "this.key.does.not.exist");
}
