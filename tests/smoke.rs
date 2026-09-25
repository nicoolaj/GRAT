//! Integration tests against the public `tablatures` crate API.

use grat::model::{
    Bar, Document, Dur, Event, Instrument, LoadError, Note, NoteValue, Row, Strum, Technique,
    TuningLabel, FORMAT_VERSION, MAX_DOTS,
};
use grat::{engrave, i18n, layout, pdf};

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
            Note {
                string: 2,
                fret: 9,
                tech: Technique::Bend { quarters: 2 },
                tie_next: false,
            },
            Note {
                string: 3,
                fret: 5,
                tech: Technique::SlideIn { from_fret: 2 },
                tie_next: false,
            },
        ],
        strum: Some(Strum::Down),
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

    let json = doc.to_json().expect("serialize");
    let round_tripped = Document::from_json(&json).expect("deserialize");

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
    assert_eq!((doc.tab_scale, doc.note_spacing), (1.0, 1.0));
}

#[test]
fn from_json_versions_the_format() {
    // A file with no version key predates versioning: load it as version 1.
    let unversioned = Document::from_json(r#"{"title": "Old Doc"}"#).expect("unversioned loads");
    assert_eq!(unversioned.format_version, 1);

    // A current save round-trips and carries the current version.
    let json = serde_json::to_string_pretty(&Document::new_empty()).unwrap();
    assert_eq!(
        Document::from_json(&json).unwrap().format_version,
        FORMAT_VERSION
    );

    // A file from a newer build is refused, not silently parsed with fields dropped.
    let future = format!(
        r#"{{"format_version": {}, "title": "x"}}"#,
        FORMAT_VERSION + 1
    );
    assert_eq!(
        Document::from_json(&future),
        Err(LoadError::TooNew(FORMAT_VERSION + 1))
    );

    // Garbage is a parse error.
    assert_eq!(Document::from_json("not json"), Err(LoadError::Parse));
}

#[test]
fn v2_block_model_migrates_to_rows() {
    let rows = |json: &str| Document::from_json(json).unwrap().rows;
    assert_eq!(rows(r#"{"format_version": 2}"#), [Row::Tab, Row::Rhythm]);
    assert_eq!(
        rows(r#"{"format_version": 2, "model": "TwoLine", "staff_order": "NotationFirst"}"#),
        [Row::Notation, Row::Tab]
    );
    assert_eq!(
        rows(r#"{"model": "ThreeLine"}"#),
        [Row::Tab, Row::Strum, Row::Notation]
    );
    // A v3 file keeps its own order, and one missing the tab gets it back.
    assert_eq!(
        rows(r#"{"format_version": 3, "rows": ["Chords", "Notation"]}"#),
        [Row::Tab, Row::Chords, Row::Notation]
    );
    // The default rows are not written; any other choice round-trips.
    let mut doc = Document::new_empty();
    assert!(!doc.to_json().unwrap().contains("\"rows\""));
    doc.rows = vec![Row::Notation, Row::Chords, Row::Tab];
    assert_eq!(rows(&doc.to_json().unwrap()), doc.rows);
}

/// A document with one event holding `notes`, each `(string, fret)`.
fn one_event(notes: &[(u8, u8)]) -> Document {
    let mut doc = Document::new_empty();
    doc.bars[0].events[0].notes = notes
        .iter()
        .map(|&(string, fret)| Note {
            string,
            fret,
            tech: Technique::Plain,
            tie_next: false,
        })
        .collect();
    doc
}

fn frets(doc: &Document) -> Vec<(u8, u8)> {
    doc.bars[0].events[0]
        .notes
        .iter()
        .map(|n| (n.string, n.fret))
        .collect()
}

#[test]
fn a_bass_document_round_trips_and_bad_tunings_are_refused() {
    let mut doc = one_event(&[(3, 5)]);
    doc.retune(
        Instrument::Bass,
        Instrument::Bass.default_tuning().to_vec(),
        false,
    );
    doc.tuning_label = TuningLabel::Strings { letters: true };
    let json = doc.to_json().unwrap();
    assert_eq!(Document::from_json(&json).unwrap(), doc);
    assert_eq!(doc.pitch(&doc.bars[0].events[0].notes[0]), 28 + 5);

    // A tuning with no strings, or a note on a string the tuning lacks, would
    // index out of bounds: refused like any other unreadable file.
    assert_eq!(
        Document::from_json(r#"{"tuning": []}"#),
        Err(LoadError::Parse)
    );
    let orphan = r#"{"tuning": [43, 38, 33, 28],
        "bars": [{"events": [{"dur": 960, "notes": [{"string": 5, "fret": 0}]}]}]}"#;
    assert_eq!(Document::from_json(orphan), Err(LoadError::Parse));
}

#[test]
fn from_json_refuses_or_tames_what_the_editor_never_writes() {
    // A v1 duration's dots feed a shift in `Dur::ticks`: 200 of them panicked.
    let dotted = |dots: u8| {
        format!(r#"{{"bars": [{{"events": [{{"dur": {{"base": "Quarter", "dots": {dots}}}}}]}}]}}"#)
    };
    assert!(Document::from_json(&dotted(MAX_DOTS)).is_ok());
    assert_eq!(Document::from_json(&dotted(200)), Err(LoadError::Parse));

    for sig in ["[0, 4]", "[4, 0]", "[4, 3]"] {
        let json = format!(r#"{{"bars": [{{"time_sig": {sig}, "events": [{{"dur": 960}}]}}]}}"#);
        assert_eq!(Document::from_json(&json), Err(LoadError::Parse), "{sig}");
    }

    // 1e39 overflows an f32 into infinity; both knobs come back on the page.
    let doc = Document::from_json(r#"{"note_spacing": 1e39, "tab_scale": -1}"#).unwrap();
    assert_eq!((doc.note_spacing, doc.tab_scale), (4.0, 0.25));
    let old = Document::from_json(r#"{"note_spacing": 0.7, "tab_scale": 1.3}"#).unwrap();
    assert_eq!(
        (old.note_spacing, old.tab_scale),
        (0.7, 1.3),
        "a pre-0.3.0 file is left alone"
    );
}

#[test]
fn pitch_caps_at_the_top_of_the_midi_range() {
    // 64 + 255 + 255 wrapped a u8 (and panicked in a debug build).
    let doc = Document::from_json(
        r#"{"capo": 255, "bars": [{"events": [{"dur": 960, "notes": [{"string": 0, "fret": 255}]}]}]}"#,
    )
    .unwrap();
    assert_eq!(doc.pitch(&doc.bars[0].events[0].notes[0]), 127);
    assert!(!layout::paginate(&doc).is_empty());
    assert!(!pdf::export(&doc).is_empty());
}

#[test]
fn retune_keeps_the_frets_or_the_pitches() {
    // Keeping the frets: the tab is untouched, notes on a vanished string go.
    let mut doc = one_event(&[(0, 3), (5, 2)]);
    doc.retune(
        Instrument::Ukulele,
        Instrument::Ukulele.default_tuning().to_vec(),
        false,
    );
    assert_eq!(frets(&doc), [(0, 3)]);

    // Keeping the pitches: the low E open, dropped to D, is now fret 2.
    let mut doc = one_event(&[(5, 0), (0, 0)]);
    doc.retune(Instrument::Guitar, vec![64, 59, 55, 50, 45, 38], true);
    assert_eq!(frets(&doc), [(0, 0), (5, 2)]);

    // A pitch its own string can no longer reach moves to the nearest free one;
    // one no string reaches is dropped.
    let mut doc = one_event(&[(5, 0), (4, 0), (0, 24)]);
    doc.retune(Instrument::Bass, vec![43, 38, 33, 28], true);
    // Guitar low E (40) = bass E string fret 12 (index 3); guitar A (45) = bass A
    // string fret 12 (index 2); the guitar's 88 is past the bass's 24th fret.
    assert_eq!(frets(&doc), [(2, 12), (3, 12)]);
}

const V1_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.gtab");

#[test]
fn v1_fixture_loads_as_the_expected_document() {
    let json = std::fs::read_to_string(V1_FIXTURE).expect("v1.gtab fixture must exist");
    let doc = Document::from_json(&json).expect("v1 fixture loads");

    assert_eq!(doc.format_version, 1, "no format_version key means v1");

    let expected = Document {
        format_version: 1,
        title: "V1 Fixture".to_string(),
        author: String::new(),
        instrument: Instrument::Guitar,
        tuning: vec![64, 59, 55, 50, 45, 40],
        tuning_label: TuningLabel::Hidden,
        capo: 0,
        tempo: 120,
        tab_scale: 1.0,
        note_spacing: 1.0,
        rows: vec![Row::Tab, Row::Rhythm],
        bars: vec![Bar {
            events: vec![
                Event {
                    dur: Dur {
                        base: NoteValue::Quarter,
                        dots: 1,
                    },
                    notes: vec![Note {
                        string: 0,
                        fret: 3,
                        tech: Technique::Plain,
                        tie_next: false,
                    }],
                    strum: Some(Strum::Down),
                    palm_mute: false,
                    let_ring: false,
                },
                Event {
                    dur: Dur {
                        base: NoteValue::Eighth,
                        dots: 0,
                    },
                    notes: vec![Note {
                        string: 1,
                        fret: 7,
                        tech: Technique::SlideIn { from_fret: 7 },
                        tie_next: false,
                    }],
                    strum: None,
                    palm_mute: false,
                    let_ring: false,
                },
                Event {
                    dur: Dur {
                        base: NoteValue::Quarter,
                        dots: 0,
                    },
                    notes: Vec::new(), // "notes": [] rest event
                    strum: None,
                    palm_mute: false,
                    let_ring: false,
                },
            ],
            time_sig: Some((4, 4)),
            repeat_start: false,
            repeat_end: Some(2),
        }],
    };

    assert_eq!(doc, expected);
}

#[test]
fn resaving_a_v1_file_declares_the_current_version() {
    let json = std::fs::read_to_string(V1_FIXTURE).expect("v1.gtab fixture must exist");
    let doc = Document::from_json(&json).expect("v1 fixture loads");
    let resaved = doc.to_json().expect("serialize");
    assert!(
        resaved.contains(&format!("\"format_version\": {FORMAT_VERSION}")),
        "re-saving a v1 document must declare the current version, got:\n{resaved}"
    );
}

#[test]
fn new_empty_document_omits_every_default_field() {
    let json = Document::new_empty().to_json().expect("serialize");
    for absent in [
        "\"tie_next\"",
        "\"palm_mute\"",
        "\"strum\"",
        "\"Quarter\"",
        ": null",
        ": false",
    ] {
        assert!(
            !json.contains(absent),
            "did not expect `{absent}` in an all-default document:\n{json}"
        );
    }
    assert!(
        json.contains("\"dur\": 960"),
        "a duration should be a bare tick count:\n{json}"
    );
}

#[test]
fn dur_from_ticks_inverts_ticks_for_every_combination() {
    let values = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ];
    for base in values {
        for dots in 0..=MAX_DOTS {
            let d = Dur { base, dots };
            assert_eq!(
                Dur::from_ticks(d.ticks()),
                Some(d),
                "from_ticks(ticks({base:?} + {dots} dots)) should invert"
            );
        }
    }
    assert_eq!(
        Dur::from_ticks(7),
        None,
        "7 ticks matches none of the 24 valid combinations"
    );
}

#[test]
fn v2_output_is_less_than_half_the_size_of_v1() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/exemples/legato-study.gtab");
    let original = std::fs::read_to_string(path).expect("legato-study.gtab must exist");
    let doc = Document::from_json(&original).expect("example must load");
    let resaved = doc.to_json().expect("serialize");
    assert!(
        resaved.len() < original.len() / 2,
        "expected v2 output under half of {} bytes, got {} bytes",
        original.len(),
        resaved.len()
    );
    // Shrinking the file is only worth anything if it still says the same thing: a real
    // v1 document (techniques, ties, dotted values and all) must survive the trip out
    // and back unchanged. `format_version` is the one field allowed to move, 1 -> 2.
    let reloaded = Document::from_json(&resaved).expect("v2 output must load back");
    assert_eq!(
        Document {
            format_version: doc.format_version,
            ..reloaded
        },
        doc
    );
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

const ONE: &[Row] = &[Row::Tab, Row::Rhythm];
const THREE: &[Row] = &[Row::Tab, Row::Strum, Row::Notation];

/// `n` identical bars of four quarter notes each, with the given block rows.
fn doc_of_bars(n: usize, rows: &[Row]) -> Document {
    let mut doc = Document::new_empty();
    doc.rows = rows.to_vec();
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
    let small = layout::paginate(&doc_of_bars(6, THREE));
    assert_eq!(small.len(), 1, "a handful of bars fits on one page");

    // ...while a large enough count is guaranteed to spill onto more than one,
    // regardless of the exact margin/title-block constants layout.rs tunes.
    let large = layout::paginate(&doc_of_bars(200, THREE));
    assert!(
        large.len() > 1,
        "200 bars must not fit on a single A4 page, got {} page(s)",
        large.len()
    );
}

#[test]
fn editor_keeps_a_blank_line_below_the_music() {
    let mut doc = Document::new_empty(); // 8 empty bars
    let start = doc.bars.len();
    layout::ensure_trailing_blank_system(&mut doc);
    assert_eq!(
        doc.bars.len(),
        start,
        "an all-empty document already has a blank line"
    );

    // Put a note in every bar, so the music now reaches the end of the last line.
    for b in &mut doc.bars {
        b.events[0].notes.push(Note {
            string: 0,
            fret: 3,
            tech: Technique::Plain,
            tie_next: false,
        });
    }
    layout::ensure_trailing_blank_system(&mut doc);
    assert!(
        doc.bars.len() > start,
        "a blank line is appended once the music fills the width"
    );
    assert!(
        doc.bars[start..]
            .iter()
            .all(|b| b.events.iter().all(|e| e.is_rest())),
        "the appended bars are empty"
    );

    let grown = doc.bars.len();
    layout::ensure_trailing_blank_system(&mut doc);
    assert_eq!(
        doc.bars.len(),
        grown,
        "idempotent: a blank line already present is not doubled"
    );
}

#[test]
fn page_count_grows_from_one_line_to_three_line() {
    // Enough bars that ThreeLine's taller block (tab + strum + notation) needs
    // strictly more pages than OneLine's shorter one (tab alone), for the exact
    // same music — the whole reason a block's height is computed per model.
    let bars = 120;
    let one = layout::paginate(&doc_of_bars(bars, ONE));
    let three = layout::paginate(&doc_of_bars(bars, THREE));
    assert!(
        three.len() > one.len(),
        "ThreeLine ({} pages) should need more pages than OneLine ({} pages)",
        three.len(),
        one.len()
    );
}

#[test]
fn denser_note_spacing_shrinks_the_system_and_fits_more_pages() {
    let mut doc = doc_of_bars(60, ONE);

    // The natural (unjustified) system width scales linearly with note_spacing.
    doc.note_spacing = 1.0;
    let wide = engrave::system_spacing(&doc, 0..doc.bars.len(), None).width;
    doc.note_spacing = 0.7;
    let dense = engrave::system_spacing(&doc, 0..doc.bars.len(), None).width;
    assert!(
        (dense / wide - 0.7).abs() < 0.01,
        "0.7x note spacing should give ~0.7x width, got {:.3}x",
        dense / wide
    );

    // ...and that feeds line breaking, so the whole piece needs no more pages
    // dense than wide (usually fewer).
    doc.note_spacing = 1.0;
    let wide_pages = layout::paginate(&doc).len();
    doc.note_spacing = 0.7;
    let dense_pages = layout::paginate(&doc).len();
    assert!(
        dense_pages <= wide_pages,
        "denser spacing packs more bars per line: {dense_pages} vs {wide_pages} pages"
    );
}

#[test]
fn identical_bars_get_identical_widths_across_systems() {
    // Bars of four quarters throughout, except one whole-note bar dropped in to
    // make the systems break unevenly -- that is exactly the situation where
    // per-system justification used to stretch one line harder than the next and
    // pull the barlines of identical music out of column.
    let mut doc = doc_of_bars(24, ONE);
    doc.bars[9].events = vec![Event {
        dur: Dur {
            base: NoteValue::Whole,
            dots: 0,
        },
        notes: vec![Note {
            string: 0,
            fret: 2,
            tech: Technique::Plain,
            tie_next: false,
        }],
        ..Default::default()
    }];

    // Span of a bar, read off the clickable cells: first event to last event on
    // one string, keyed by bar so the same music can be compared line to line.
    let mut span: std::collections::BTreeMap<usize, (f32, f32)> = Default::default();
    for page in layout::paginate(&doc) {
        for hit in page.hits.iter().filter(|h| h.string == 0) {
            let x = (hit.min.x + hit.max.x) * 0.5;
            let e = span.entry(hit.bar).or_insert((x, x));
            *e = (e.0.min(x), e.1.max(x));
        }
    }

    let widths: Vec<f32> = span
        .iter()
        .filter(|(bar, _)| doc.bars[**bar].events.len() == 4)
        .map(|(_, (lo, hi))| hi - lo)
        .collect();
    let lo = widths.iter().copied().fold(f32::MAX, f32::min);
    let hi = widths.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        widths.len() > 12,
        "expected many four-quarter bars to compare, got {}",
        widths.len()
    );
    assert!(hi > 0.0, "bars should have a non-zero span");
    assert!(
        hi - lo < 0.01,
        "four-quarter bars must be the same width on every system: {lo} .. {hi}"
    );
}

#[test]
fn tab_scale_grows_the_fret_numbers_but_not_the_grid() {
    let mut doc = doc_of_bars(6, ONE);

    // doc_of_bars frets everything at 2, so a "2" text prim is a fret number.
    let fret_pt = |d: &Document| -> f32 {
        layout::paginate(d)[0]
            .prims
            .iter()
            .find_map(|p| match p {
                grat::Prim::Text { s, pt, .. } if s == "2" => Some(*pt),
                _ => None,
            })
            .expect("a fret-number prim")
    };
    let cell_h = |d: &Document| -> f32 {
        let h = &layout::paginate(d)[0].hits[0];
        h.max.y - h.min.y
    };

    // The standard size: a fret number is 2.6 mm cap-high and a quarter note is
    // allotted 8.4 mm at scale 1.0. Both are absolute millimetres on paper, and
    // both are the values that used to need tab_scale 1.3 / note_spacing 0.7 --
    // pinned here so a later tidy-up of engrave's table or tablature's SCALE_BASE
    // cannot quietly move what "1.0" means.
    doc.tab_scale = 1.0;
    let (pt1, grid1) = (fret_pt(&doc), cell_h(&doc));
    assert!(
        (pt1 - grat::staff::pt_for_cap(2.6)).abs() < 0.01,
        "default fret numbers should be 2.6 mm cap-high, got {pt1} pt"
    );
    assert!(
        (grat::engrave::natural_event_width(
            &Dur {
                base: NoteValue::Quarter,
                dots: 0
            },
            1.0
        ) - 8.4)
            .abs()
            < 0.01,
        "a quarter note is allotted 8.4 mm at the standard density"
    );

    doc.tab_scale = 1.3;
    let (pt2, grid2) = (fret_pt(&doc), cell_h(&doc));

    assert!(
        (pt2 / pt1 - 1.3).abs() < 0.01,
        "1.3x tab_scale should give ~1.3x fret-number size, got {:.3}x",
        pt2 / pt1
    );
    assert!(
        (grid2 - grid1).abs() < 1e-4,
        "the clickable grid must not move with tab_scale: {grid1} vs {grid2}"
    );
}

#[test]
fn every_hit_is_non_empty_and_stays_on_the_page() {
    let pages = layout::paginate(&doc_of_bars(40, THREE));
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
                    && hit.max.x <= grat::PAGE_W_MM
                    && hit.min.y >= 0.0
                    && hit.max.y <= grat::PAGE_H_MM,
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

#[test]
fn pdf_export_starts_with_the_pdf_header() {
    let bytes = pdf::export(&doc_of_bars(4, ONE));
    assert!(
        bytes.starts_with(b"%PDF-"),
        "exported bytes must start with the %PDF- header"
    );
}

#[test]
fn pdf_export_page_count_matches_a_two_page_layout() {
    // Search for a bar count that lands on exactly two pages rather than
    // hardcoding one -- keeps the test from going stale if layout.rs's
    // pagination constants ever move.
    let bars = (1..=300)
        .find(|&n| layout::paginate(&doc_of_bars(n, ONE)).len() == 2)
        .expect("some bar count must yield exactly two pages");
    let doc = doc_of_bars(bars, ONE);
    assert_eq!(layout::paginate(&doc).len(), 2, "fixture must be two pages");

    let bytes = pdf::export(&doc);
    let mut warnings = Vec::new();
    let parsed =
        printpdf::PdfDocument::parse(&bytes, &printpdf::PdfParseOptions::default(), &mut warnings)
            .expect("exported bytes must parse back as a PDF");
    assert_eq!(
        parsed.pages.len(),
        2,
        "PDF page count must match the 2-page layout"
    );
}

#[test]
fn pdf_export_handles_harmonic_and_ghost_labels_without_panicking() {
    // `<12>` (harmonic) and `(5)` (ghost) are exactly the fret-label characters
    // that broke the SVG proof sheet (see tests/visual.rs); prove pdf.rs's text
    // path, which goes through printpdf's own text encoding rather than XML
    // escaping, handles them too.
    let mut doc = Document::new_empty();
    doc.bars[0].events[0].notes.push(Note {
        string: 0,
        fret: 12,
        tech: Technique::Harmonic,
        tie_next: false,
    });
    doc.bars[0].events[1].notes.push(Note {
        string: 1,
        fret: 5,
        tech: Technique::Ghost,
        tie_next: false,
    });

    let bytes = pdf::export(&doc);
    assert!(bytes.starts_with(b"%PDF-"));
}

// --- engrave::for_export: rest consolidation and trailing-bar trim -----------

fn q_note(string: u8) -> Event {
    Event {
        dur: Dur {
            base: NoteValue::Quarter,
            dots: 0,
        },
        notes: vec![Note {
            string,
            fret: 2,
            tech: Technique::Plain,
            tie_next: false,
        }],
        ..Default::default()
    }
}

fn beat_rest(base: NoteValue) -> Event {
    Event {
        dur: Dur { base, dots: 0 },
        ..Default::default()
    }
}

#[test]
fn for_export_collapses_a_blank_bar_to_one_whole_rest_and_keeps_only_one() {
    // A fresh document is eight bars of four quarter rests each. Export keeps one
    // bar (the trim floor) and writes its silence as a single whole rest.
    let out = engrave::for_export(&Document::new_empty());
    assert_eq!(out.bars.len(), 1, "trailing blank bars drop, one is kept");
    assert_eq!(out.bars[0].events.len(), 1, "four beat rests merge to one");
    assert_eq!(out.bars[0].events[0].dur.base, NoteValue::Whole);
    assert!(out.bars[0].events[0].is_rest());
}

#[test]
fn for_export_drops_blank_bars_after_the_last_sounded_note() {
    let mut doc = Document::new_empty();
    doc.bars[0].events[0] = q_note(0);
    let out = engrave::for_export(&doc);
    assert_eq!(out.bars.len(), 1, "only bar 0 sounds a note");
    assert!(!out.bars[0].events[0].is_rest());
}

#[test]
fn for_export_keeps_a_blank_bar_that_sits_inside_the_music() {
    let mut doc = Document::new_empty();
    doc.bars.truncate(3);
    doc.bars[0].events[0] = q_note(0);
    doc.bars[2].events[0] = q_note(0); // bar 1 stays blank, between two sounded bars
    let out = engrave::for_export(&doc);
    assert_eq!(
        out.bars.len(),
        3,
        "an inner blank bar is not a trailing one"
    );
    assert_eq!(
        out.bars[1].events.len(),
        1,
        "it is still merged to a whole rest"
    );
    assert_eq!(out.bars[1].events[0].dur.base, NoteValue::Whole);
}

#[test]
fn for_export_merges_sub_beat_rests_without_crossing_a_beat() {
    let mut doc = Document::new_empty();
    doc.bars.truncate(1);
    doc.bars[0].events = vec![
        q_note(0),
        beat_rest(NoteValue::Eighth),
        beat_rest(NoteValue::Eighth), // both halves of beat 2
        q_note(1),
        q_note(2),
    ];
    let out = engrave::for_export(&doc);
    let shape: Vec<(NoteValue, bool)> = out.bars[0]
        .events
        .iter()
        .map(|e| (e.dur.base, e.is_rest()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (NoteValue::Quarter, false),
            (NoteValue::Quarter, true), // the two eighth rests, now one quarter rest
            (NoteValue::Quarter, false),
            (NoteValue::Quarter, false),
        ]
    );
}

#[test]
fn for_export_merges_a_beats_3_4_rest_run_into_a_half_rest() {
    // Beats 3-4 don't cross the bar's midpoint, so they merge into one half rest.
    let mut doc = Document::new_empty();
    doc.bars.truncate(1);
    doc.bars[0].events = vec![
        Event {
            dur: Dur {
                base: NoteValue::Half,
                dots: 0,
            },
            notes: vec![Note {
                string: 0,
                fret: 2,
                tech: Technique::Plain,
                tie_next: false,
            }],
            ..Default::default()
        },
        beat_rest(NoteValue::Quarter),
        beat_rest(NoteValue::Quarter),
    ];
    let out = engrave::for_export(&doc);
    let rests: Vec<NoteValue> = out.bars[0]
        .events
        .iter()
        .filter(|e| e.is_rest())
        .map(|e| e.dur.base)
        .collect();
    assert_eq!(rests, vec![NoteValue::Half]);
}

#[test]
fn for_export_merges_a_beats_1_2_rest_run_into_a_half_rest() {
    let mut doc = Document::new_empty();
    doc.bars.truncate(1);
    doc.bars[0].events = vec![
        beat_rest(NoteValue::Quarter),
        beat_rest(NoteValue::Quarter),
        q_note(0),
        q_note(1),
    ];
    let out = engrave::for_export(&doc);
    let rests: Vec<NoteValue> = out.bars[0]
        .events
        .iter()
        .filter(|e| e.is_rest())
        .map(|e| e.dur.base)
        .collect();
    assert_eq!(rests, vec![NoteValue::Half]);
}

#[test]
fn for_export_keeps_a_midpoint_crossing_rest_run_as_two_quarter_rests() {
    // The user's own example: a silence over beats 2-3 of 4/4 must never print
    // as a half rest, or beat 3's onset -- the bar's secondary strong beat --
    // disappears.
    let mut doc = Document::new_empty();
    doc.bars.truncate(1);
    doc.bars[0].events = vec![
        q_note(0),
        beat_rest(NoteValue::Quarter),
        beat_rest(NoteValue::Quarter),
        q_note(1),
    ];
    let out = engrave::for_export(&doc);
    let rests: Vec<NoteValue> = out.bars[0]
        .events
        .iter()
        .filter(|e| e.is_rest())
        .map(|e| e.dur.base)
        .collect();
    assert_eq!(rests, vec![NoteValue::Quarter, NoteValue::Quarter]);
}

#[test]
fn for_export_leaves_a_three_four_rest_run_as_two_quarter_rests() {
    // 3/4 has no secondary strong beat to protect, but it still isn't widened --
    // has_wide_midpoint keys on `num >= 4`, not merely on "two beats long".
    let mut doc = Document::new_empty();
    doc.bars.truncate(1);
    doc.bars[0].time_sig = Some((3, 4));
    doc.bars[0].events = vec![
        q_note(0),
        beat_rest(NoteValue::Quarter),
        beat_rest(NoteValue::Quarter),
    ];
    let out = engrave::for_export(&doc);
    let rests: Vec<NoteValue> = out.bars[0]
        .events
        .iter()
        .filter(|e| e.is_rest())
        .map(|e| e.dur.base)
        .collect();
    assert_eq!(rests, vec![NoteValue::Quarter, NoteValue::Quarter]);
}

#[test]
fn for_export_is_idempotent() {
    let mut doc = doc_of_bars(6, ONE);
    doc.bars[2].events = vec![
        q_note(0),
        beat_rest(NoteValue::Eighth),
        beat_rest(NoteValue::Eighth),
        q_note(1),
        q_note(2),
    ];
    doc.bars.push(Bar::new_empty(None)); // a trailing blank to trim on the first pass
    let once = engrave::for_export(&doc);
    let twice = engrave::for_export(&once);
    assert_eq!(once, twice, "a second pass must change nothing");
}

#[test]
fn pdf_export_does_not_print_trailing_blank_bars() {
    // One page of music, then enough blank bars to fill more pages on their own.
    // The blank tail must never reach the paper.
    let mut doc = doc_of_bars(6, ONE);
    assert_eq!(
        layout::paginate(&doc).len(),
        1,
        "fixture must be one page of music"
    );
    for _ in 0..400 {
        doc.bars.push(Bar::new_empty(None));
    }
    assert!(
        layout::paginate(&doc).len() >= 2,
        "the blank tail alone spills past page one"
    );

    let bytes = pdf::export(&doc);
    assert!(bytes.starts_with(b"%PDF-"));
    let mut warnings = Vec::new();
    let parsed =
        printpdf::PdfDocument::parse(&bytes, &printpdf::PdfParseOptions::default(), &mut warnings)
            .expect("exported bytes must parse back as a PDF");
    assert_eq!(
        parsed.pages.len(),
        1,
        "trailing blank bars must not add printed pages"
    );
}

#[test]
fn the_live_strip_puts_every_bar_on_one_line() {
    let mut doc = Document::new_empty();
    for bar in doc.bars.iter_mut() {
        bar.events[0].notes.push(Note {
            string: 2,
            fret: 5,
            tech: Technique::Plain,
            tie_next: false,
        });
    }

    let strip = layout::strip(&doc);

    // One line: as wide as every bar's natural width put together, plus the room
    // the staff head is drawn in. Nothing is justified, nothing wraps.
    let natural: f32 = doc
        .bars
        .iter()
        .enumerate()
        .map(|(i, b)| engrave::natural_bar_width(b, doc.time_sig_at(i), doc.note_spacing))
        .sum();
    assert!((strip.end_x() - (strip.x + natural)).abs() < 0.01);
    assert!(strip.height > 0.0);

    // Six clickable cells per event, exactly as on a page -- the live player
    // highlights a column by taking the union of an event's six.
    let events: usize = doc.bars.iter().map(|b| b.events.len()).sum();
    assert_eq!(strip.hits.len(), events * 6);

    // A bass has four lines: four cells per event, and a staff two string gaps
    // shorter.
    let mut bass = doc.clone();
    bass.retune(
        Instrument::Bass,
        Instrument::Bass.default_tuning().to_vec(),
        false,
    );
    let bass_strip = layout::strip(&bass);
    assert_eq!(bass_strip.hits.len(), events * 4);
    let shorter = strip.height - bass_strip.height;
    assert!(
        (shorter - 2.0 * grat::tablature::STRING_MM).abs() < 0.01,
        "{shorter}"
    );

    // Every event has a place on the line, and they run left to right.
    let xs: Vec<f32> = doc
        .bars
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| (0..b.events.len()).map(move |ei| (bi, ei)))
        .map(|(bi, ei)| {
            strip
                .event_x(bi, ei)
                .expect("every event sits on the strip")
        })
        .collect();
    assert!(xs.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn a_prims_x_span_covers_every_point_it_draws_through() {
    use grat::{Align, Prim, Rgb, P};
    let ink = Rgb(0, 0, 0);
    let line = Prim::Line {
        a: P::new(5.0, 0.0),
        b: P::new(1.0, 9.0),
        w: 0.2,
        color: ink,
    };
    assert_eq!(line.x_span(), (1.0, 5.0));
    // The control points count: a slur bulges past its two ends.
    let curve = Prim::Curve {
        a: P::new(0.0, 0.0),
        c1: P::new(-2.0, 1.0),
        c2: P::new(7.0, 1.0),
        b: P::new(4.0, 0.0),
        w: 0.2,
        color: ink,
    };
    assert_eq!(curve.x_span(), (-2.0, 7.0));
    let head = Prim::Poly {
        pts: grat::staff::ellipse(P::new(10.0, 0.0), 1.0, 0.5, 0.0),
        color: ink,
    };
    let (lo, hi) = head.x_span();
    assert!(
        (lo - 9.0).abs() < 1e-4 && (hi - 11.0).abs() < 1e-4,
        "{lo} {hi}"
    );
    let text = Prim::Text {
        pos: P::new(3.0, 0.0),
        s: "Am7".into(),
        pt: 10.0,
        color: ink,
        align: Align::Center,
    };
    assert_eq!(text.x_span(), (3.0, 3.0));
}

#[test]
fn labels_are_measured_in_helvetica() {
    let em = |text: &str| grat::staff::label_width(text, 1.0) / grat::staff::MM_PER_PT;
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    // Digits keep their 0.556 em: fret numbers centre exactly as they did.
    assert!(close(em("12"), 1.112));
    // Counted as sixteen digits this was 8.896 em, and the PDF title sat 8.6 mm
    // left of centre. Adobe's AFM sums it to 6.835.
    assert!(
        close(em("Hotel California"), 6.835),
        "{}",
        em("Hotel California")
    );
    assert!(close(em("Été"), 0.667 + 0.278 + 0.556));
    // Past WinAnsi the PDF prints "?", 0.556 wide.
    assert!(close(em("♯"), 0.556));
}

#[test]
fn a_slur_stops_just_short_of_its_partners_label() {
    use grat::{staff::label_width, Prim};
    for (from, to) in [(5, 12), (12, 5)] {
        let mut doc = Document::new_empty();
        let events = &mut doc.bars[0].events;
        events[0].notes.push(Note {
            string: 0,
            fret: from,
            tech: Technique::HammerOn,
            tie_next: false,
        });
        events[1].notes.push(Note {
            string: 0,
            fret: to,
            tech: Technique::Plain,
            tie_next: false,
        });
        let prims = &layout::paginate(&doc)[0].prims;
        let slur_end = prims
            .iter()
            .find_map(|p| match p {
                Prim::Curve { b, color, .. } if *color == grat::model::COLOR_HAMMER_ON => Some(b.x),
                _ => None,
            })
            .expect("a hammer-on slur");
        let label_left = prims
            .iter()
            .find_map(|p| match p {
                Prim::Text { pos, s, pt, .. } if *s == to.to_string() => {
                    Some(pos.x - label_width(s, *pt) / 2.0)
                }
                _ => None,
            })
            .expect("the partner's fret label");
        // Before: "5h12" ended the slur 0.4 mm inside the "12", and "12h5" left
        // a gap half a digit wider than the one after the "12".
        assert!(
            (label_left - 1.0..=label_left).contains(&slur_end),
            "{from}h{to}: slur ends at {slur_end}, label starts at {label_left}"
        );
    }
}

#[test]
fn bar_numbers_open_each_system_and_mark_every_bar_of_the_strip() {
    use grat::Prim;
    let mut doc = Document::new_empty();
    doc.bars = (0..40).map(|_| Bar::new_empty(None)).collect();
    let pt = grat::staff::pt_for_cap(layout::BAR_NUMBER_CAP_MM);
    let numbers = |prims: &[Prim]| -> Vec<(usize, f32, f32)> {
        prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { s, pt: q, pos, .. } if (q - pt).abs() < 1e-4 => {
                    Some((s.parse().ok()?, pos.x, pos.y))
                }
                _ => None,
            })
            .collect()
    };

    // A page: one number per system but the first, each at the head of its line.
    let pages = layout::paginate(&doc);
    let on_pages: Vec<_> = pages.iter().flat_map(|p| numbers(&p.prims)).collect();
    assert!(on_pages.len() >= 5, "{on_pages:?}");
    assert!(on_pages[0].0 > 1);
    assert!(on_pages.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(on_pages.iter().all(|n| (n.1 - on_pages[0].1).abs() < 1e-3));

    // The strip: every bar but the first, on the strip's own paper.
    let strip = layout::strip(&doc);
    let on_strip = numbers(&strip.prims);
    let bars: Vec<usize> = on_strip.iter().map(|n| n.0).collect();
    assert_eq!(bars, (2..=40).collect::<Vec<_>>());
    assert!(on_strip
        .iter()
        .all(|n| n.2 + layout::BAR_NUMBER_CAP_MM <= strip.height));
}

#[test]
fn a_repeat_count_loads_within_what_the_menus_offer() {
    let doc = Document::from_json(
        r#"{"bars": [{"repeat_end": 200, "events": [{"dur": 3840}]},
                     {"repeat_end": 0, "events": [{"dur": 3840}]}]}"#,
    )
    .unwrap();
    assert_eq!(doc.bars[0].repeat_end, Some(grat::model::MAX_REPEAT_PLAYS));
    assert_eq!(doc.bars[1].repeat_end, Some(2));
}

#[test]
fn inserting_a_bar_takes_the_metre_in_force_there() {
    let mut doc = Document::new_empty();
    engrave::set_time_sig(&mut doc, 2, Some((3, 4)));
    doc.insert_bar(3); // inside the 3/4 section
    assert_eq!(doc.time_sig_at(3), (3, 4));
    assert!(engrave::is_complete(&doc.bars[3], (3, 4)));
    assert_eq!(
        doc.bars[3].time_sig, None,
        "it inherits, like its neighbours"
    );
    doc.insert_bar(0);
    assert_eq!(
        doc.bars[0].time_sig,
        Some((4, 4)),
        "the top bar names the metre"
    );
    let mut empty = Document::new_empty();
    empty.bars.clear();
    empty.insert_bar(0);
    assert_eq!(empty.bars.len(), 1);
}

#[test]
fn deleting_bars_hands_on_their_metre_and_repeats() {
    // 4/4 | 4/4 | 3/4 | (3/4) | (3/4) ...
    let mut doc = Document::new_empty();
    engrave::set_time_sig(&mut doc, 2, Some((3, 4)));
    let len = doc.bars.len();
    doc.delete_bars(2, 2); // the bar that changed the metre
    assert_eq!(doc.bars.len(), len - 1);
    assert_eq!(
        doc.time_sig_at(2),
        (3, 4),
        "the change moves to the next bar"
    );

    // |: 1 2 3 :|x3 -- cut bars 0-1: the opening repeat moves to bar 2's heir.
    let mut doc = Document::new_empty();
    doc.bars[0].repeat_start = true;
    doc.bars[3].repeat_end = Some(3);
    doc.delete_bars(0, 1);
    assert!(doc.bars[0].repeat_start && doc.bars[1].repeat_end == Some(3));

    // The same passage, cut at its end: the closing repeat moves back a bar.
    let mut doc = Document::new_empty();
    doc.bars[0].repeat_start = true;
    doc.bars[3].repeat_end = Some(3);
    doc.delete_bars(3, 4);
    assert!(doc.bars[0].repeat_start && doc.bars[2].repeat_end == Some(3));

    // A passage cut out whole takes its marks with it.
    let mut doc = Document::new_empty();
    doc.bars[1].repeat_start = true;
    doc.bars[2].repeat_end = Some(2);
    doc.delete_bars(1, 2);
    assert!(doc
        .bars
        .iter()
        .all(|b| !b.repeat_start && b.repeat_end.is_none()));

    // Everything goes: one bar stays.
    let mut doc = Document::new_empty();
    let n = doc.bars.len();
    doc.delete_bars(0, n - 1);
    assert_eq!(doc.bars.len(), 1);
}

#[test]
fn duplicated_bars_keep_their_metre_but_not_their_repeats() {
    let mut doc = Document::new_empty();
    doc.bars[1].events[0].notes.push(Note {
        string: 0,
        fret: 7,
        tech: Technique::Plain,
        tie_next: false,
    });
    doc.bars[1].repeat_start = true;
    engrave::set_time_sig(&mut doc, 2, Some((3, 4)));
    doc.bars[2].repeat_end = Some(2);
    let len = doc.bars.len();
    doc.duplicate_bars(1, 2); // a 4/4 bar then a 3/4 one
    assert_eq!(doc.bars.len(), len + 2);
    assert_eq!(doc.bars[3].events[0].notes[0].fret, 7);
    assert_eq!(doc.time_sig_at(3), (4, 4), "the copy goes back to 4/4");
    assert_eq!(doc.time_sig_at(4), (3, 4));
    assert!(!doc.bars[3].repeat_start && doc.bars[4].repeat_end.is_none());
    assert!(doc.bars[1].repeat_start && doc.bars[2].repeat_end == Some(2));
}
