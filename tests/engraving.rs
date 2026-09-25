//! Checks on the engraving engine: the decisions that are easy to get wrong and
//! impossible to eyeball once a page is full of notes.

use grat::engrave::{
    bar_duration_secs, bar_ticks, beam_groups, beam_run_span, beam_runs, is_complete,
    metronome_beats, natural_bar_width, play_order, set_event_dur, set_time_sig,
    set_time_sig_everywhere, shift_event_dur, split_ticks, system_spacing, time_sig_marks,
    timeline, BeamGroup, BAR_LEAD_MM, GROUP_GAP, SLIDE_IN_LEAD_MM,
};
use grat::model::*;
use grat::notation;

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

/// Durations and tie flags of a bar, the shape a printed page actually shows.
fn printed(doc: &Document, bar: usize) -> Vec<(u32, bool)> {
    grat::engrave::for_export(doc).bars[bar]
        .events
        .iter()
        .map(|e| (e.dur.ticks(), e.notes.iter().any(|n| n.tie_next)))
        .collect()
}

#[test]
fn a_note_straddling_a_beat_is_split_and_tied_for_print() {
    // `8th. 8th. 8th` over beats 1-2: the middle quaver starts off the beat and
    // runs past it, hiding where beat 2 falls. Print cuts it at the boundary and
    // sews it back with a tie -- `8th. | 16th ~ 8th | 8th` -- so the beams show
    // both beats. (Bar 0 of "Don't Stop 'Til You Get Enough".)
    let doc = doc_with(Bar {
        events: vec![
            dotted(NoteValue::Eighth, vec![note(4)]),
            dotted(NoteValue::Eighth, vec![note(4)]),
            ev(NoteValue::Eighth, vec![note(4)]),
            ev(NoteValue::Quarter, vec![note(2)]),
            ev(NoteValue::Quarter, vec![]), // rest, one beat
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    assert_eq!(
        printed(&doc, 0),
        vec![
            (720, false), // 8th.  on the beat, left whole
            (240, true),  // 16th ~ the cut, tied...
            (480, false), // 8th   ...across beat 2
            (480, false), // 8th
            (960, false), // quarter
            (960, false), // quarter rest
        ]
    );

    // Two beamed pairs, one per beat, which is the whole point of the split.
    let groups = beam_groups(&grat::engrave::for_export(&doc), 0);
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert_eq!(groups[0].events, vec![0, 1], "8th. + 16th, beat 1");
    assert_eq!(groups[1].events, vec![2, 3], "8th + 8th, beat 2");
}

#[test]
fn a_note_starting_on_the_beat_is_never_split() {
    // A half note on beat 1 spans the beat boundary at 960 and the bar's midpoint,
    // and is still a half note -- not two or four tied quarters.
    let doc = doc_with(Bar {
        events: vec![
            ev(NoteValue::Half, vec![note(4)]),
            ev(NoteValue::Half, vec![note(4)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    assert_eq!(printed(&doc, 0), vec![(1920, false), (1920, false)]);
}

#[test]
fn an_off_grid_bar_is_not_shattered_into_tied_thirty_seconds() {
    // Bar 1 of the same song: a dotted sixteenth where a quaver belongs leaves the
    // bar 120 ticks short and every later onset off the grid. Splitting there
    // would need thirty-second fragments; better to print the bar as written and
    // let the incomplete-bar warning carry the news.
    let doc = doc_with(Bar {
        events: vec![
            dotted(NoteValue::Eighth, vec![note(4)]),
            dotted(NoteValue::Eighth, vec![note(4)]),
            dotted(NoteValue::Sixteenth, vec![note(2)]), // 360 -- the typo
            ev(NoteValue::Eighth, vec![note(4)]),        // onset 1800, crosses 1920
            ev(NoteValue::Sixteenth, vec![note(4)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let printed = printed(&doc, 0);
    assert!(
        printed.iter().all(|&(ticks, _)| ticks >= 240),
        "nothing faster than the bar's own sixteenths: {printed:?}"
    );
    assert!(
        !is_complete(&doc.bars[0], (4, 4)),
        "and the bar is still reported incomplete"
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
fn a_lone_beamlet_hooks_toward_its_neighbour() {
    // The "croche pointée - double" cell, both orderings: a dotted eighth (1
    // beam) next to a sixteenth (2 beams). At level 2 the sixteenth has no
    // partner, so it prints as a beamlet -- a short stub, not a full beam --
    // pointing back at the note it shares its rhythmic unit with.
    let xs = [0.0, 10.0];

    // Dotted eighth, then sixteenth: the stub (at column 1) points backward,
    // into the group, toward the dotted eighth.
    let descending = BeamGroup {
        events: vec![0, 1],
        beams: vec![1, 2],
    };
    let run = beam_runs(&descending, 2).into_iter().next().unwrap();
    assert_eq!(beam_run_span(&run, &xs, 1.5), (10.0, 8.5));

    // Sixteenth, then dotted eighth: the stub (at column 0) points forward.
    let ascending = BeamGroup {
        events: vec![0, 1],
        beams: vec![2, 1],
    };
    let run = beam_runs(&ascending, 2).into_iter().next().unwrap();
    assert_eq!(beam_run_span(&run, &xs, 1.5), (0.0, 1.5));

    // A run with a partner spans column to column -- no stub involved.
    let run = beam_runs(&descending, 1).into_iter().next().unwrap();
    assert_eq!(beam_run_span(&run, &xs, 1.5), (0.0, 10.0));
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
        let scale = bar.width / natural_bar_width(&doc.bars[0], (4, 4), 1.0);
        let lead = bar.events[0] - bar.x;
        let trail = bar.x + bar.width - bar.events[n - 1];
        // Barline air in front...
        assert!(
            (lead - BAR_LEAD_MM * scale).abs() < 0.01,
            "target {target:?}: lead {lead}"
        );
        // ...and behind the trailing quarter, its own slot is the air: nothing on top.
        let want = grat::engrave::natural_event_width(
            &Dur {
                base: NoteValue::Quarter,
                dots: 0,
            },
            1.0,
        ) * scale;
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
        (natural_bar_width(&slid, (4, 4), 1.0)
            - natural_bar_width(&plain, (4, 4), 1.0)
            - SLIDE_IN_LEAD_MM)
            .abs()
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
    let want = grat::engrave::natural_event_width(
        &Dur {
            base: NoteValue::Quarter,
            dots: 0,
        },
        1.0,
    );
    assert!(
        (trail - want).abs() < 0.001,
        "the last quarter's slot, unchanged by the lead-in: got {trail}, want {want}"
    );
}

/// Column-to-column distances of bar 0, laid out at its natural width.
fn steps(doc: &Document) -> Vec<f32> {
    let sp = system_spacing(doc, 0..1, None);
    sp.bars[0].events.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn groups_breathe_apart_by_the_beat_or_the_half_bar() {
    let close = |a: f32, b: f32| (a - b).abs() < 0.001;
    // Quavers: one unit inside a beat, 1.3 across each beat.
    let eighths = doc_with(Bar {
        events: (0..8)
            .map(|_| ev(NoteValue::Eighth, vec![note(5)]))
            .collect(),
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    let s = steps(&eighths);
    for (i, &step) in s.iter().enumerate() {
        let want = if i % 2 == 1 {
            s[0] * (1.0 + GROUP_GAP)
        } else {
            s[0]
        };
        assert!(close(step, want), "eighths step {i}: {step}, want {want}");
    }
    // Crotchets at most: the gap falls every two beats, at mid-bar only.
    let quarters = doc_with(four_quarters());
    let s = steps(&quarters);
    assert!(close(s[0], s[2]), "{s:?}");
    assert!(close(s[1], s[0] * (1.0 + GROUP_GAP)), "{s:?}");
}

#[test]
fn every_beat_is_the_same_distance_from_the_next() {
    // `8th. 8th. q 8th 8th 8th` in 4/4 prints as
    // `8th. 16th~ | 8th 8th~ | 8th 8th | 8th 8th`. Beat 1 closes on a sixteenth,
    // beats 2 and 3 on quavers: the space across each beat must be the same, or
    // the tie across beat 2 comes out longer than the one across beat 1.
    let doc = grat::engrave::for_export(&doc_with(Bar {
        events: vec![
            dotted(NoteValue::Eighth, vec![note(5)]),
            dotted(NoteValue::Eighth, vec![note(3)]),
            ev(NoteValue::Quarter, vec![note(5)]),
            ev(NoteValue::Eighth, vec![note(3)]),
            ev(NoteValue::Eighth, vec![note(6)]),
            ev(NoteValue::Eighth, vec![note(5)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    }));
    let tied: Vec<usize> = doc.bars[0]
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.notes.iter().any(|n| n.tie_next))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(tied, vec![1, 3], "the sixteenth and the quaver are tied on");
    let s = steps(&doc);
    // Steps 1, 3 and 5 cross beats 2, 3 and 4.
    assert!(
        (s[1] - s[3]).abs() < 0.001 && (s[3] - s[5]).abs() < 0.001,
        "{s:?}"
    );
    // ...and inside a beat, the quavers keep one unit to 1.3.
    assert!((s[3] - s[2] * (1.0 + GROUP_GAP)).abs() < 0.001, "{s:?}");
}

#[test]
fn the_barline_gets_the_last_slot_and_no_more() {
    let trail = |last: NoteValue| {
        let mut bar = four_quarters();
        bar.events.pop();
        bar.events.push(ev(last, vec![note(5)]));
        let sp = system_spacing(&doc_with(bar), 0..1, None);
        let b = &sp.bars[0];
        b.x + b.width - b.events.last().unwrap()
    };
    let slot = |base| grat::engrave::natural_event_width(&Dur { base, dots: 0 }, 1.0);
    assert!((trail(NoteValue::Eighth) - slot(NoteValue::Eighth)).abs() < 0.001);
    // A sixteenth's slot is narrower than the barline air in front of the bar:
    // the back never gets less than the front.
    assert!((trail(NoteValue::Sixteenth) - BAR_LEAD_MM).abs() < 0.001);
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
fn set_time_sig_reflows_notes_instead_of_dropping_them() {
    // 4/4 -> 2/4: two bars become four, every note kept, in order.
    let mut doc = two_bars_of_distinct_quarters();
    doc.bars[1].repeat_end = Some(2);
    set_time_sig(&mut doc, 0, Some((2, 4)));
    assert_eq!(doc.bars.len(), 4);
    let frets: Vec<u8> = doc
        .bars
        .iter()
        .flat_map(|b| &b.events)
        .flat_map(|e| &e.notes)
        .map(|n| n.fret)
        .collect();
    assert_eq!(frets, (0..8).collect::<Vec<_>>());
    assert!(doc.bars.iter().all(|b| is_complete(b, (2, 4))));
    assert_eq!(
        doc.bars[3].repeat_end,
        Some(2),
        "repeat sign follows the music"
    );

    // And back: 2/4 -> 4/4 merges them again.
    set_time_sig(&mut doc, 0, Some((4, 4)));
    assert_eq!(doc.bars.len(), 2);
    assert_eq!(total_notes(&doc), 8);
    assert_eq!(doc, two_bars_of_distinct_quarters_with_repeat());
}

#[test]
fn set_time_sig_everywhere_drops_inner_changes_and_keeps_notes() {
    let mut doc = two_bars_of_distinct_quarters();
    doc.bars[1].time_sig = Some((3, 4));
    doc.bars[1].events.pop(); // a full 3/4 bar: frets 4, 5, 6
    set_time_sig_everywhere(&mut doc, (2, 4));
    // 4/4 + 3/4 = 7 quarters -> four 2/4 bars, the last padded.
    assert_eq!(doc.bars.len(), 4);
    assert_eq!(total_notes(&doc), 7);
    assert_eq!(doc.bars[0].time_sig, Some((2, 4)));
    assert!(doc.bars[1..].iter().all(|b| b.time_sig.is_none()));
    assert!(doc.bars.iter().all(|b| is_complete(b, (2, 4))));
}

fn two_bars_of_distinct_quarters_with_repeat() -> Document {
    let mut doc = two_bars_of_distinct_quarters();
    doc.bars[1].repeat_end = Some(2);
    doc
}

#[test]
fn set_time_sig_stops_at_the_next_signature_and_keeps_onsets() {
    // 4/4: half(0) quarter(1) quarter(2) | 3/4 bar that must not move.
    let mut doc = doc_with(Bar {
        events: vec![
            ev(NoteValue::Quarter, vec![note(0)]),
            ev(NoteValue::Half, vec![note(1)]),
            ev(NoteValue::Quarter, vec![note(2)]),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    });
    doc.bars.push(Bar {
        events: vec![ev(NoteValue::Quarter, vec![note(9)]); 3],
        time_sig: Some((3, 4)),
        ..Default::default()
    });
    set_time_sig(&mut doc, 0, Some((2, 4)));

    // The half note straddles the new barline: cut to what fits, and the
    // quarter after it still starts on beat 4.
    assert_eq!(doc.bars.len(), 3);
    assert_eq!(doc.bars[0].events.len(), 2);
    assert_eq!(
        doc.bars[0].events[1].dur.ticks(),
        NoteValue::Quarter.ticks()
    );
    assert!(doc.bars[1].events[0].is_rest());
    assert_eq!(doc.bars[1].events[1].notes[0].fret, 2);
    assert_eq!(doc.bars[2].time_sig, Some((3, 4)));
    assert_eq!(doc.bars[2].events.len(), 3);
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

    let cues = timeline(&doc, &play_order(&doc));
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
    let slow = timeline(&doc, &play_order(&doc)).last().unwrap().end;
    doc.tempo = 120;
    let fast = timeline(&doc, &play_order(&doc)).last().unwrap().end;
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

    let beats = metronome_beats(&doc, &play_order(&doc));
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

    let beats = metronome_beats(&doc, &play_order(&doc));
    assert_eq!(beats.len(), 2, "6/8 clicks in two, not six: {beats:?}");
    assert!(beats[0].downbeat && !beats[1].downbeat);
    // Tempo counts the dotted-quarter beat, not a plain quarter: at 120 bpm a
    // dotted quarter is 0.5s.
    assert!((beats[1].time - 0.5).abs() < 1e-4);
    assert_eq!((beats[0].index_in_bar, beats[1].index_in_bar), (0, 1));
    assert!(beats.iter().all(|b| b.beats_in_bar == 2));
}

#[test]
fn timeline_counts_a_compound_metre_in_dotted_quarters() {
    let mut doc = doc_with(Bar {
        events: vec![ev(NoteValue::Eighth, vec![note(0)]); 6],
        time_sig: Some((6, 8)),
        ..Default::default()
    });
    doc.tempo = 120; // dotted quarter = 120 bpm, not a plain quarter

    let cues = timeline(&doc, &play_order(&doc));
    assert_eq!(cues.len(), 6);
    // Two dotted-quarter beats of 0.5s each, three eighths per beat.
    let wants = [
        0.0,
        1.0 / 6.0,
        2.0 / 6.0,
        0.5,
        0.5 + 1.0 / 6.0,
        0.5 + 2.0 / 6.0,
    ];
    for (cue, want) in cues.iter().zip(wants) {
        assert!(
            (cue.start - want).abs() < 1e-4,
            "cue {cue:?} should start at {want}"
        );
    }
    assert!(
        (cues.last().unwrap().end - 1.0).abs() < 1e-4,
        "one 6/8 bar at dotted-quarter=120 is one second"
    );
}

#[test]
fn bar_duration_matches_a_bars_worth_of_metronome_beats() {
    assert!((bar_duration_secs((4, 4), 120) - 2.0).abs() < 1e-4);
    assert!((bar_duration_secs((3, 4), 120) - 1.5).abs() < 1e-4);
    // 6/8 at 120 is two dotted-quarter beats, same 0.5s each as above.
    assert!((bar_duration_secs((6, 8), 120) - 1.0).abs() < 1e-4);
}

#[test]
fn row_extent_matches_todays_constants_for_a_bar_that_stays_on_the_staff() {
    let doc = doc_with(Bar {
        events: vec![ev(NoteValue::Quarter, vec![note(5)])],
        ..Default::default()
    });
    let half_range = notation::half_range(&doc, 0..1);
    assert_eq!(
        half_range,
        (0, 8),
        "a plain bar never leaves the staff floor"
    );

    let (above, below) = notation::row_extent(half_range);
    assert_eq!(above, notation::ROW_MM - notation::BASELINE_OFFSET_MM);
    assert_eq!(below, notation::BASELINE_OFFSET_MM);
}

#[test]
fn row_extent_grows_above_the_staff_for_a_high_run_but_not_below() {
    let high = Note {
        string: 0,
        fret: 20,
        tech: Technique::Plain,
        tie_next: false,
    };
    let doc = doc_with(Bar {
        events: vec![ev(NoteValue::Quarter, vec![high])],
        ..Default::default()
    });
    let half_range = notation::half_range(&doc, 0..1);
    let (above, below) = notation::row_extent(half_range);

    assert!(
        above > notation::ROW_MM - notation::BASELINE_OFFSET_MM,
        "a note several ledger lines up must grow the gap above the staff, got {above}"
    );
    assert_eq!(
        below,
        notation::BASELINE_OFFSET_MM,
        "a note only above the staff must not grow the room below it"
    );
}

#[test]
fn the_clef_follows_the_instrument() {
    let at = |instrument: Instrument, tuning: Vec<u8>, n: Note| {
        let mut doc = doc_with(Bar {
            events: vec![ev(NoteValue::Quarter, vec![n])],
            ..Default::default()
        });
        doc.retune(instrument, tuning, false);
        notation::half_range(&doc, 0..1).0
    };
    let open = |string| Note {
        string,
        fret: 0,
        tech: Technique::Plain,
        tie_next: false,
    };
    // A bass reads the bass clef an octave up: its open E sits on the first
    // ledger line below the staff, not seven below a treble one.
    assert_eq!(at(Instrument::Bass, vec![43, 38, 33, 28], open(3)), -2);
    // A ukulele reads treble at pitch: its C string is middle C, one ledger
    // below. The guitar's own middle C (G string, 5th fret) is written an octave
    // up, inside the staff.
    assert_eq!(at(Instrument::Ukulele, vec![69, 64, 60, 67], open(2)), -2);
    assert_eq!(
        notation::half_range(
            &doc_with(Bar {
                events: vec![ev(NoteValue::Quarter, vec![note(5)])],
                ..Default::default()
            }),
            0..1
        ),
        (0, 8)
    );
}

#[test]
fn time_sig_marks_follow_engraving_rules() {
    // 4/4 4/4 | 3/4 3/4, broken after bar 1 and after bar 2.
    let mut doc = two_bars_of_distinct_quarters();
    doc.bars.push(Bar {
        events: vec![ev(NoteValue::Quarter, vec![note(1)]); 3],
        time_sig: Some((3, 4)),
        ..Default::default()
    });
    doc.bars.push(Bar {
        events: vec![ev(NoteValue::Quarter, vec![note(1)]); 3],
        ..Default::default()
    });
    let sigs = |r: std::ops::Range<usize>| -> Vec<(bool, (u8, u8))> {
        let sp = system_spacing(&doc, r, None);
        time_sig_marks(&doc, &sp)
            .into_iter()
            .map(|(x, sig)| (x > sp.width - 0.01, sig))
            .collect()
    };
    // Start of the piece, plus a courtesy 3/4 after the closing barline.
    assert_eq!(sigs(0..2), vec![(false, (4, 4)), (true, (3, 4))]);
    // The change itself, opening the next system.
    assert_eq!(sigs(2..3), vec![(false, (3, 4))]);
    // Not restated just because a new system starts.
    assert_eq!(sigs(3..4), vec![]);
    // A change inside a system prints at its barline.
    let sp = system_spacing(&doc, 1..3, None);
    let marks = time_sig_marks(&doc, &sp);
    assert_eq!(marks.len(), 1);
    assert!(marks[0].0 > sp.bars[1].x && marks[0].0 < sp.bars[1].events[0]);
}

#[test]
fn play_order_unrolls_every_kind_of_repeat() {
    let piece = |marks: &[(bool, Option<u8>)]| {
        let mut doc = Document::new_empty();
        doc.bars = marks
            .iter()
            .map(|&(repeat_start, repeat_end)| Bar {
                repeat_start,
                repeat_end,
                ..Bar::new_empty(None)
            })
            .collect();
        play_order(&doc)
    };
    let plain = (false, None);
    assert_eq!(piece(&[plain; 3]), [0, 1, 2]);
    // |: 1 2 :| then on.
    assert_eq!(
        piece(&[plain, (true, None), (false, Some(2)), plain]),
        [0, 1, 2, 1, 2, 3]
    );
    assert_eq!(
        piece(&[plain, (true, None), (false, Some(3)), plain]),
        [0, 1, 2, 1, 2, 1, 2, 3]
    );
    // A closing repeat with no opening one goes back to the top...
    assert_eq!(piece(&[plain, (false, Some(2)), plain]), [0, 1, 0, 1, 2]);
    // ...or to just after the previous closing repeat.
    assert_eq!(
        piece(&[(false, Some(2)), plain, (false, Some(2))]),
        [0, 0, 1, 2, 1, 2]
    );
    // One bar repeated on itself.
    assert_eq!(piece(&[(true, Some(3))]), [0, 0, 0]);
}

#[test]
fn a_repeat_plays_its_bars_again_one_step_further_on() {
    let mut doc = Document::new_empty(); // bars of four quarter rests
    doc.bars.truncate(2);
    doc.bars[0].repeat_start = true;
    doc.bars[1].repeat_end = Some(2);
    let order = play_order(&doc);
    let cues = timeline(&doc, &order);
    assert_eq!(cues.len(), 16);
    assert_eq!((cues[8].bar, cues[8].step, cues[8].event), (0, 2, 0));
    assert!(cues
        .windows(2)
        .all(|w| (w[1].start - w[0].end).abs() < 1e-5));
    assert_eq!(metronome_beats(&doc, &order).len(), 16);
}
