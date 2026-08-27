//! Checks on the engraving engine: the decisions that are easy to get wrong and
//! impossible to eyeball once a page is full of notes.

use strungin::engrave::{bar_ticks, beam_groups, beam_runs, is_complete, system_spacing};
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
