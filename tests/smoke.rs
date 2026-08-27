//! Integration tests against the public `tablatures` crate API.

use strungin::i18n;
use strungin::layout;
use strungin::model::{Bar, BlockModel, Document, Dur, Event, Note, NoteValue, Technique};

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
                tech: Technique::Slide,
                tie_next: true,
            },
        ],
        strum: None,
        ..Default::default()
    });
    doc.bars[0].events.push(Event {
        dur: Dur {
            base: NoteValue::Eighth,
            dots: 1,
        },
        notes: Vec::new(), // rest
        strum: None,
        ..Default::default()
    });
    doc.bars.push(Bar {
        events: Vec::new(),
        time_sig: Some((3, 4)),
        ..Default::default()
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

/// `n` identical bars of four quarter notes each, in the given block model.
fn doc_of_bars(n: usize, model: BlockModel) -> Document {
    let mut doc = Document::new_empty();
    doc.model = model;
    doc.bars = (0..n)
        .map(|i| Bar {
            events: (0..4)
                .map(|s| Event {
                    dur: Dur {
                        base: NoteValue::Quarter,
                        dots: 0,
                    },
                    notes: vec![Note {
                        string: (s % 6) as u8,
                        fret: 2,
                        tech: Technique::Plain,
                        tie_next: false,
                    }],
                    ..Default::default()
                })
                .collect(),
            time_sig: if i == 0 { Some((4, 4)) } else { None },
            repeat_start: false,
            repeat_end: None,
        })
        .collect();
    doc
}

#[test]
fn paginate_fits_expected_number_of_pages_for_n_bars() {
    // A handful of bars is well within one page...
    let small = layout::paginate(&doc_of_bars(6, BlockModel::ThreeLine));
    assert_eq!(small.len(), 1, "a handful of bars fits on one page");

    // ...while a large enough count is guaranteed to spill onto more than one,
    // regardless of the exact margin/title-block constants layout.rs tunes.
    let large = layout::paginate(&doc_of_bars(200, BlockModel::ThreeLine));
    assert!(
        large.len() > 1,
        "200 bars must not fit on a single A4 page, got {} page(s)",
        large.len()
    );
}

#[test]
fn page_count_grows_from_one_line_to_three_line() {
    // Enough bars that ThreeLine's taller block (tab + strum + notation) needs
    // strictly more pages than OneLine's shorter one (tab alone), for the exact
    // same music — the whole reason a block's height is computed per model.
    let bars = 120;
    let one = layout::paginate(&doc_of_bars(bars, BlockModel::OneLine));
    let three = layout::paginate(&doc_of_bars(bars, BlockModel::ThreeLine));
    assert!(
        three.len() > one.len(),
        "ThreeLine ({} pages) should need more pages than OneLine ({} pages)",
        three.len(),
        one.len()
    );
}

#[test]
fn every_hit_is_non_empty_and_stays_on_the_page() {
    let pages = layout::paginate(&doc_of_bars(40, BlockModel::ThreeLine));
    assert!(!pages.is_empty());
    let mut total_hits = 0;
    for page in &pages {
        for hit in &page.hits {
            total_hits += 1;
            assert!(
                hit.max.x > hit.min.x && hit.max.y > hit.min.y,
                "hit box must be non-empty: {hit:?}"
            );
            assert!(
                hit.min.x >= 0.0
                    && hit.max.x <= strungin::PAGE_W_MM
                    && hit.min.y >= 0.0
                    && hit.max.y <= strungin::PAGE_H_MM,
                "hit box must stay inside the A4 page: {hit:?}"
            );
        }
    }
    assert!(
        total_hits > 0,
        "40 bars of notes must produce clickable cells"
    );
}

#[test]
fn an_empty_document_still_produces_one_furnished_page() {
    let mut doc = Document::new_empty();
    doc.bars = Vec::new();
    let pages = layout::paginate(&doc);
    assert_eq!(
        pages.len(),
        1,
        "no bars at all still yields exactly one page"
    );
    assert!(
        !pages[0].prims.is_empty(),
        "the empty page still carries title/footer furniture"
    );
    assert!(pages[0].hits.is_empty(), "no bars means no clickable cells");
}
