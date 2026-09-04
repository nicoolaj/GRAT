//! Checks on the engraving engine: the decisions that are easy to get wrong and
//! impossible to eyeball once a page is full of notes.

use grat::engrave::{
    bar_duration_secs, bar_ticks, beam_groups, beam_runs, is_complete, metronome_beats,
    natural_bar_width, set_event_dur, set_time_sig, shift_event_dur, split_ticks, system_spacing,
    timeline, BAR_GAP_MM, SLIDE_IN_LEAD_MM,
};
use grat::model::*;

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

fn dotted(base: NoteValue, notes: Vec<Note>) -> Event {
    Event {
        dur: Dur { base, dots: 1 },
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
fn running_eighths_beam_in_half_bar_groups_in_common_time() {
    // 4/4 whose fastest note is a quaver: two groups of four, split at the bar's
    // midpoint -- not four pairs, and never a beam across the centre.
    let doc = doc_with(Bar {
        events: (0..8)
            .map(|i| ev(NoteValue::Eighth, vec![note(i)]))
            .collect(),
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 2, "two half-bar groups: {groups:?}");
    assert_eq!(groups[0].events, vec![0, 1, 2, 3]);
    assert_eq!(groups[1].events, vec![4, 5, 6, 7]);
}

#[test]
fn short_rests_do_not_defeat_half_bar_grouping() {
    // Six quavers then a beat of rest the editor backfilled as two semiquaver
    // rests plus a quaver rest. The fastest *note* is still a quaver, so the six
    // notes stay in two half-bar groups -- the short rests must not count.
    let mut events: Vec<Event> = (0..6)
        .map(|i| ev(NoteValue::Eighth, vec![note(i)]))
        .collect();
    events.push(ev(NoteValue::Sixteenth, vec![]));
    events.push(ev(NoteValue::Sixteenth, vec![]));
    events.push(ev(NoteValue::Eighth, vec![]));
    let doc = doc_with(Bar {
        events,
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 2, "two half-bar groups: {groups:?}");
    assert_eq!(groups[0].events, vec![0, 1, 2, 3]);
    assert_eq!(groups[1].events, vec![4, 5]);
}

#[test]
fn a_note_sustaining_across_a_beat_keeps_the_beam() {
    // `8th. 8th. 8th` filling beats 1-2: the middle dotted quaver (onset 720)
    // sustains across the beat boundary at 960. No note begins on that boundary,
    // so the three stay in one beam -- not [0,1] plus a lone flagged 2.
    // (From bar 0 of "Don't Stop 'Til You Get Enough".)
    let doc = doc_with(Bar {
        events: vec![
            dotted(NoteValue::Eighth, vec![note(4)]),
            dotted(NoteValue::Eighth, vec![note(4)]),
            ev(NoteValue::Eighth, vec![note(4)]),
            ev(NoteValue::Sixteenth, vec![note(2)]),
            ev(NoteValue::Eighth, vec![note(4)]),
            ev(NoteValue::Sixteenth, vec![note(4)]),
            ev(NoteValue::Eighth, vec![]), // rest
            ev(NoteValue::Eighth, vec![]), // rest
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert_eq!(
        groups[0].events,
        vec![0, 1, 2],
        "beats 1-2 beam as one unit"
    );
    assert_eq!(
        groups[1].events,
        vec![3, 4, 5],
        "beat 3's run, started on the beat"
    );
}

#[test]
fn a_sixteenth_pulls_common_time_beaming_back_to_the_beat() {
    // As soon as anything faster than a quaver is in the bar, the beat has to stay
    // visible, so grouping reverts to one beat at a time.
    let doc = doc_with(Bar {
        events: vec![
            ev(NoteValue::Eighth, vec![note(0)]),
            ev(NoteValue::Eighth, vec![note(1)]),
            ev(NoteValue::Eighth, vec![note(2)]),
            ev(NoteValue::Eighth, vec![note(3)]),
            ev(NoteValue::Sixteenth, vec![note(4)]),
            ev(NoteValue::Sixteenth, vec![note(5)]),
            ev(NoteValue::Sixteenth, vec![note(6)]),
            ev(NoteValue::Sixteenth, vec![note(7)]),
            ev(NoteValue::Quarter, vec![note(0)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let groups = beam_groups(&doc, 0);
    assert_eq!(groups.len(), 3, "beat 1, beat 2, beat 3: {groups:?}");
    assert_eq!(groups[0].events, vec![0, 1]);
    assert_eq!(groups[1].events, vec![2, 3]);
    assert_eq!(groups[2].events, vec![4, 5, 6, 7]);
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
fn the_last_event_of_a_bar_still_gets_its_duration_in_paper() {
    // The eye reads duration as horizontal distance. A bar ending on a quarter
    // rest must show a quarter's worth of paper before the barline: leaving the
    // last event out of the width squeezed the final beat of every bar to a
    // fraction of its rightful size.
    let events = vec![
        ev(NoteValue::Quarter, vec![note(5)]),
        ev(NoteValue::Quarter, vec![note(8)]),
        ev(NoteValue::Eighth, vec![note(6)]),
        ev(NoteValue::Eighth, vec![note(5)]),
        ev(NoteValue::Quarter, vec![]), // trailing rest, one whole beat
    ];
    let n = events.len();
    let doc = doc_with(Bar {
        events,
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    for target in [None, Some(200.0)] {
        let sp = system_spacing(&doc, 0..1, target);
        let bar = &sp.bars[0];
        let scale = bar.width / natural_bar_width(&doc.bars[0], 1.0);
        let lead = bar.events[0] - bar.x;
        let trail = bar.x + bar.width - bar.events[n - 1];
        // Air is the same at both ends...
        assert!(
            (lead - BAR_GAP_MM / 2.0 * scale).abs() < 0.01,
            "target {target:?}: lead {lead}"
        );
        // ...but the trailing quarter also owns its own slot on top of that air.
        let want = (BAR_GAP_MM / 2.0
            + grat::engrave::natural_event_width(
                &Dur {
                    base: NoteValue::Quarter,
                    dots: 0,
                },
                1.0,
            ))
            * scale;
        assert!(
            (trail - want).abs() < 0.01,
            "target {target:?}: trail {trail}, want {want}"
        );
    }
}

#[test]
fn a_whole_bar_rest_is_centred_between_its_barlines() {
    // The one symbol notation places at the middle of the bar rather than at the
    // instant it falls on. `for_export` collapses every silent bar to this shape.
    let doc = doc_with(Bar {
        events: vec![ev(NoteValue::Whole, vec![])],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    for target in [None, Some(200.0)] {
        let sp = system_spacing(&doc, 0..1, target);
        let bar = &sp.bars[0];
        let middle = bar.x + bar.width / 2.0;
        assert!(
            (bar.events[0] - middle).abs() < 0.01,
            "target {target:?}: rest at {}, middle {middle}",
            bar.events[0]
        );
    }
}

#[test]
fn an_approach_slide_claims_room_in_front_of_itself() {
    // Its departure digit is drawn left of its own column: without a lead-in it
    // lands on the barline or on the previous note.
    // Both the bar's natural width and the event's x must grow, or the music
    // silently overflows its own bar.
    let plain = Bar {
        events: vec![
            ev(NoteValue::Quarter, vec![note(5)]),
            ev(NoteValue::Quarter, vec![note(7)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    };
    let mut slid = plain.clone();
    slid.events[1].notes[0].tech = Technique::SlideIn { from_fret: 3 };

    assert!(
        (natural_bar_width(&slid, 1.0) - natural_bar_width(&plain, 1.0) - SLIDE_IN_LEAD_MM).abs()
            < 0.001,
        "the bar's natural width must grow by exactly SLIDE_IN_LEAD_MM"
    );

    let plain_sp = system_spacing(&doc_with(plain), 0..1, None);
    let slid_sp = system_spacing(&doc_with(slid), 0..1, None);
    assert!(
        (slid_sp.bars[0].events[1] - plain_sp.bars[0].events[1] - SLIDE_IN_LEAD_MM).abs() < 0.001,
        "the slide-in event's own x must move right by the same lead-in"
    );

    let bar = &slid_sp.bars[0];
    let trail = bar.x + bar.width - bar.events[1];
    let want = BAR_GAP_MM / 2.0
        + grat::engrave::natural_event_width(
            &Dur {
                base: NoteValue::Quarter,
                dots: 0,
            },
            1.0,
        );
    assert!(
        (trail - want).abs() < 0.001,
        "the last quarter's slot plus BAR_GAP_MM / 2 of air, unchanged by the lead-in: got {trail}, want {want}"
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

#[test]
fn timeline_turns_ticks_into_seconds_at_the_document_tempo() {
    let mut doc = doc_with(Bar {
        events: vec![
            ev(NoteValue::Quarter, vec![note(5)]),
            ev(NoteValue::Eighth, vec![]),
            ev(NoteValue::Eighth, vec![note(7)]),
            ev(NoteValue::Half, vec![note(0)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    doc.tempo = 120; // a quarter note lasts half a second

    let cues = timeline(&doc);
    assert_eq!(cues.len(), 4);
    // A rest is a cue like any other: the player counts it.
    assert_eq!((cues[1].bar, cues[1].event), (0, 1));
    for (cue, want) in cues.iter().zip([0.0, 0.5, 0.75, 1.0]) {
        assert!(
            (cue.start - want).abs() < 1e-4,
            "cue {cue:?} should start at {want}"
        );
    }
    // One 4/4 bar at 120 is two seconds, and each cue ends where the next starts.
    assert!((cues[3].end - 2.0).abs() < 1e-4);
    for pair in cues.windows(2) {
        assert!((pair[0].end - pair[1].start).abs() < 1e-6);
    }
}

#[test]
fn timeline_halves_when_the_tempo_doubles() {
    let mut doc = doc_with(Bar {
        events: vec![ev(NoteValue::Whole, vec![note(3)])],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    doc.tempo = 60;
    let slow = timeline(&doc).last().unwrap().end;
    doc.tempo = 120;
    let fast = timeline(&doc).last().unwrap().end;
    assert!(
        (slow - 4.0).abs() < 1e-4,
        "four beats at 60 bpm is four seconds"
    );
    assert!((fast - slow / 2.0).abs() < 1e-4);
}

#[test]
fn metronome_clicks_once_per_beat_and_accents_the_downbeat() {
    // Two 4/4 bars of quarters -- the metronome doesn't care what the events
    // are, only the time signature, so a single whole note per bar still gets
    // four clicks.
    let mut doc = Document::new_empty();
    doc.tempo = 120;
    doc.bars = vec![
        Bar {
            events: vec![ev(NoteValue::Whole, vec![note(0)])],
            time_sig: Some((4, 4)),
            ..Default::default()
        },
        Bar {
            events: vec![ev(NoteValue::Whole, vec![note(0)])],
            ..Default::default()
        },
    ];

    let beats = metronome_beats(&doc);
    assert_eq!(beats.len(), 8, "four quarter-note beats per 4/4 bar");
    for (i, want_time) in [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]
        .into_iter()
        .enumerate()
    {
        assert!(
            (beats[i].time - want_time).abs() < 1e-4,
            "beat {i} ({beats:?}) should land at {want_time}"
        );
    }
    assert_eq!(beats.iter().filter(|b| b.downbeat).count(), 2);
    assert!(beats[0].downbeat && beats[4].downbeat, "{beats:?}");
    assert!(!beats[1].downbeat && !beats[2].downbeat && !beats[3].downbeat);
    assert_eq!(beats[0].bar, 0);
    assert_eq!(beats[4].bar, 1);

    // `index_in_bar` restarts at each bar and `beats_in_bar` is the same four
    // for every beat of a 4/4 bar -- what the live-mode flash dot's colour ramp
    // and printed beat number both read.
    assert_eq!(
        beats.iter().map(|b| b.index_in_bar).collect::<Vec<_>>(),
        [0, 1, 2, 3, 0, 1, 2, 3]
    );
    assert!(beats.iter().all(|b| b.beats_in_bar == 4), "{beats:?}");
}

#[test]
fn metronome_pulses_a_compound_metre_in_dotted_quarters_not_eighths() {
    // 6/8 groups (and so clicks) in two, the same beat unit `beam_groups` beams
    // by -- not six eighth-note clicks.
    let mut doc = Document::new_empty();
    doc.tempo = 120;
    doc.bars = vec![Bar {
        events: vec![ev(NoteValue::Whole, vec![note(0)])], // padded to a bar by refit elsewhere; metronome only reads the metre
        time_sig: Some((6, 8)),
        ..Default::default()
    }];

    let beats = metronome_beats(&doc);
    assert_eq!(beats.len(), 2, "6/8 clicks in two, not six: {beats:?}");
    assert!(beats[0].downbeat && !beats[1].downbeat);
    // A dotted quarter at 120 bpm (quarter = 0.5s) is 0.75s.
    assert!((beats[1].time - 0.75).abs() < 1e-4);
    assert_eq!((beats[0].index_in_bar, beats[1].index_in_bar), (0, 1));
    assert!(beats.iter().all(|b| b.beats_in_bar == 2));
}

#[test]
fn bar_duration_matches_a_bars_worth_of_metronome_beats() {
    assert!((bar_duration_secs((4, 4), 120) - 2.0).abs() < 1e-4);
    assert!((bar_duration_secs((3, 4), 120) - 1.5).abs() < 1e-4);
    // 6/8 at 120 is two dotted-quarter beats, same 0.75s each as above.
    assert!((bar_duration_secs((6, 8), 120) - 1.5).abs() < 1e-4);
}
