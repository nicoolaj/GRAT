//! Checks on the engraving engine: the decisions that are easy to get wrong and
//! impossible to eyeball once a page is full of notes.

use strungin::engrave::{
    bar_ticks, beam_groups, beam_runs, is_complete, set_event_dur, set_time_sig, shift_event_dur,
    split_ticks, system_spacing,
};
use strungin::model::*;

fn note(fret: u8) -> Note {
    Note {
        string: 2,
        fret,
        tech: Technique::Plain,
        tie_next: false,
    }
}

fn ev(base: NoteValue, notes: Vec<Note>) -> Event {
    Event {
        dur: Dur { base, dots: 0 },
        notes,
        ..Default::default()
    }
}

fn doc_with(bar: Bar) -> Document {
    let mut doc = Document::new_empty();
    doc.bars = vec![bar];
    doc
}

#[test]
fn eighths_beam_by_beat_not_across_the_bar() {
    let doc = doc_with(Bar {
        events: (0..8)
            .map(|i| ev(NoteValue::Eighth, vec![note(i)]))
            .collect(),
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 4, "four beats of 4/4 give four beamed pairs");
    assert!(groups.iter().all(|g| g.events.len() == 2));
}

#[test]
fn compound_metre_beams_in_threes() {
    let doc = doc_with(Bar {
        events: (0..6)
            .map(|i| ev(NoteValue::Eighth, vec![note(i)]))
            .collect(),
        time_sig: Some((6, 8)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 2, "6/8 reads as two dotted-quarter beats");
    assert!(groups.iter().all(|g| g.events.len() == 3));
}

#[test]
fn a_rest_breaks_a_beam_and_a_lone_eighth_gets_no_group() {
    let doc = doc_with(Bar {
        events: vec![
            ev(NoteValue::Eighth, vec![note(5)]),
            ev(NoteValue::Eighth, vec![]), // rest
            ev(NoteValue::Eighth, vec![note(7)]),
            ev(NoteValue::Eighth, vec![note(9)]),
            ev(NoteValue::Half, vec![note(5)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(
        groups.len(),
        1,
        "only the pair that is neither split nor alone"
    );
    assert_eq!(groups[0].events, vec![2, 3]);
}

#[test]
fn secondary_beams_cover_only_the_shorter_values() {
    let doc = doc_with(Bar {
        events: vec![
            ev(NoteValue::Sixteenth, vec![note(5)]),
            ev(NoteValue::Sixteenth, vec![note(7)]),
            ev(NoteValue::Eighth, vec![note(9)]),
            ev(NoteValue::Quarter, vec![note(5)]),
            ev(NoteValue::Quarter, vec![note(5)]),
            ev(NoteValue::Quarter, vec![note(5)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].beams, vec![2, 2, 1]);
    // The primary beam spans all three, the secondary only the two sixteenths.
    assert_eq!(beam_runs(&groups[0], 1), vec![0..=2]);
    assert_eq!(beam_runs(&groups[0], 2), vec![0..=1]);
    assert!(beam_runs(&groups[0], 3).is_empty());
}

#[test]
fn justification_fills_the_line_without_reordering_events() {
    let doc = doc_with(Bar {
        events: (0..4)
            .map(|i| ev(NoteValue::Quarter, vec![note(i)]))
            .collect(),
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let natural = system_spacing(&doc, 0..1, None);
    let justified = system_spacing(&doc, 0..1, Some(natural.width * 2.0));

    assert!((justified.width - natural.width * 2.0).abs() < 0.01);
    let xs = &justified.bars[0].events;
    assert!(
        xs.windows(2).all(|w| w[1] > w[0]),
        "events stay in order: {xs:?}"
    );
    assert!(
        xs.last().unwrap() < &justified.width,
        "the last event stays inside the system"
    );
}

#[test]
fn an_incomplete_bar_is_reported_but_still_laid_out() {
    let bar = Bar {
        events: vec![ev(NoteValue::Quarter, vec![note(5)])],
        time_sig: Some((4, 4)),
        ..Default::default()
    };
    let doc = doc_with(bar.clone());
    assert_eq!(bar_ticks((4, 4)), TICKS_WHOLE);
    assert!(!is_complete(&bar, (4, 4)));
    assert_eq!(system_spacing(&doc, 0..1, None).bars[0].events.len(), 1);
}

fn four_quarters() -> Bar {
    Bar {
        events: (0..4)
            .map(|i| ev(NoteValue::Quarter, vec![note(i)]))
            .collect(),
        time_sig: Some((4, 4)),
        ..Default::default()
    }
}

#[test]
fn shortening_an_event_backfills_the_freed_time_with_a_rest() {
    let mut bar = four_quarters();
    set_event_dur(
        &mut bar,
        (4, 4),
        0,
        Dur {
            base: NoteValue::Eighth,
            dots: 0,
        },
    );
    assert_eq!(bar.events.len(), 5, "the freed eighth becomes its own rest");
    assert!(bar.events[1].is_rest());
    assert!(is_complete(&bar, (4, 4)));
}

#[test]
fn lengthening_an_event_swallows_the_next_one_first() {
    let mut bar = four_quarters();
    set_event_dur(
        &mut bar,
        (4, 4),
        0,
        Dur {
            base: NoteValue::Half,
            dots: 0,
        },
    );
    assert_eq!(bar.events.len(), 3, "one whole quarter note was consumed");
    assert!(is_complete(&bar, (4, 4)));
    assert_eq!(
        bar.events[1].notes[0].fret, 2,
        "the 2nd quarter is swallowed, not the last"
    );
}

#[test]
fn set_event_dur_is_bounded_by_the_bar_line() {
    let mut bar = four_quarters();
    set_event_dur(
        &mut bar,
        (4, 4),
        3,
        Dur {
            base: NoteValue::Whole,
            dots: 0,
        },
    );
    assert_eq!(
        bar.events.len(),
        4,
        "a whole note can't fit in the last quarter's remaining room"
    );
    assert!(is_complete(&bar, (4, 4)));
}

#[test]
fn set_time_sig_refits_every_bar_it_governs() {
    let mut doc = doc_with(four_quarters());
    doc.bars.push(Bar {
        time_sig: None, // inherits from bar 0
        ..four_quarters()
    });

    set_time_sig(&mut doc, 0, Some((3, 4)));

    for bar in &doc.bars {
        assert!(is_complete(bar, (3, 4)));
    }
}

#[test]
fn split_ticks_terminates_on_an_unrepresentable_remainder() {
    // 225 ticks has no exact undotted decomposition; the greedy loop must stop
    // rather than spin, and must never hand back more than it was given.
    let durs = split_ticks(225);
    let total: u32 = durs.iter().map(|d| d.ticks()).sum();
    assert!(total <= 225);
}

/// Two 4/4 bars of four quarters each, with eight distinct frets (0..8) so a
/// note can be identified by fret alone after it moves.
fn two_bars_of_distinct_quarters() -> Document {
    let mut doc = doc_with(Bar {
        events: (0..4)
            .map(|i| ev(NoteValue::Quarter, vec![note(i)]))
            .collect(),
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    doc.bars.push(Bar {
        events: (4..8)
            .map(|i| ev(NoteValue::Quarter, vec![note(i)]))
            .collect(),
        time_sig: None, // inherits 4/4 from bar 0
        ..Default::default()
    });
    doc
}

/// Total number of notes across every event of the document, rests excluded --
/// the invariant `shift_event_dur` must preserve no matter how it reflows bars.
fn total_notes(doc: &Document) -> usize {
    doc.bars
        .iter()
        .flat_map(|b| &b.events)
        .map(|e| e.notes.len())
        .sum()
}

#[test]
fn shift_event_dur_lengthening_pushes_forward_without_swallowing() {
    let mut doc = two_bars_of_distinct_quarters();
    shift_event_dur(
        &mut doc,
        0,
        0,
        Dur {
            base: NoteValue::Half,
            dots: 0,
        },
    );
    assert_eq!(
        doc.bars[0].events.len(),
        3,
        "half note + two quarters fill bar 0"
    );
    assert_eq!(doc.bars[0].events[0].dur.base, NoteValue::Half);
    assert_eq!(
        doc.bars[1].events[0].notes[0].fret, 3,
        "the quarter evicted from bar 0 opens bar 1"
    );
    assert_eq!(total_notes(&doc), 8, "no note is lost, only reflowed");
    for bar in &doc.bars {
        assert!(is_complete(bar, (4, 4)));
    }
}

#[test]
fn shift_event_dur_shortening_pulls_forward_and_pads_the_end() {
    let mut doc = two_bars_of_distinct_quarters();
    shift_event_dur(
        &mut doc,
        0,
        0,
        Dur {
            base: NoteValue::Eighth,
            dots: 0,
        },
    );
    assert_eq!(doc.bars.len(), 2, "shortening never needs a new bar");
    assert!(
        doc.bars[0].events.last().unwrap().is_rest(),
        "bar 0 ends on a rest"
    );
    assert_eq!(total_notes(&doc), 8, "no note is lost, only reflowed");
    for bar in &doc.bars {
        assert!(is_complete(bar, (4, 4)));
    }
}

#[test]
fn shift_event_dur_overflow_grows_the_document() {
    let mut doc = doc_with(four_quarters());
    shift_event_dur(
        &mut doc,
        0,
        0,
        Dur {
            base: NoteValue::Whole,
            dots: 0,
        },
    );
    assert_eq!(
        doc.bars.len(),
        2,
        "the overflow grows the document by a bar"
    );
    assert_eq!(total_notes(&doc), 4, "no note is lost");
    for bar in &doc.bars {
        assert!(is_complete(bar, (4, 4)));
    }
}
