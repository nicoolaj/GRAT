# GRAT

GRAT is a six-string guitar tablature editor written in Rust (`egui`/`eframe`), with
mouse-driven note entry and print-ready A4 PDF export. Single portable binary (macOS first,
Windows/Linux targeted), light/dark theme following the OS, FR/EN interface auto-detected.

The name is a recursive acronym that never settles: the title bar and splash screen draw,
at every launch, a random reading of "GRAT" (`GRAT Rédige les Accords et Tablatures`, `GRAT
Range Arpèges et Tonalités`, …) — a nod to *gratte* (French slang for guitar). The About
page keeps the canonical reading.

## Building and running

```sh
make run      # cargo run — opens the window
make build    # cargo build --release, copies the binary into dist/
make app      # (macOS) packages dist/GRAT.app
make test     # cargo test
```

See `make help` for the full list of targets.

## The three block models

A document chooses, globally or per setting, between three layouts (`model::BlockModel`):

- **One row** — tablature only.
- **Two rows** — tablature + classical notation staff (treble clef 8va).
- **Three rows** — tablature + strumming (pick direction / tapping) + classical notation staff.

The tablature/staff order is configurable (`StaffOrder::TabFirst` / `NotationFirst`,
tablature on top by default). The three rows of a block share the same horizontal spacing,
computed once, which keeps them vertically aligned by construction.

## Live mode

`View > Live mode` (⌘L) switches the window into playback: the score scrolls on its own at
the document's tempo, and the beat currently being played is highlighted at the centre of
the screen.

- **Scrolling pages** — the score as it prints, continuously recentred on the note to play.
- **Single line** — every bar on one ribbon, sliding under a fixed playhead.

Size (millimetres to pixels) scales well past the editor's own, and speed ranges from ×0.25
to ×2: it's a rehearsal setting, it doesn't touch the tempo stored in the document and isn't
saved. Space starts and pauses, ← / → jump one bar, Escape exits.

A classic metronome marks every beat with a click and, at the same time, a dot that lights
up top-right of the screen with the beat number written inside it — red on the first beat of
the bar, then a gradient from pink to yellow through the bar (the last beat before the next
first beat is always yellow). When playback starts, a number of empty bars (2 by default) can
be counted in before the score actually begins. The metronome can be muted with a checkbox;
it never plays the tablature's own notes, only the beat.

The metronome click is only available on macOS and Windows: on Linux, the dot still lights
up at the right time, but without sound (see the `rodio` dependency's comment in
`Cargo.toml`).

## File format

Documents are saved as readable, diffable JSON, extension `.gtab`
(`serde_json::to_string_pretty`). The format tolerates future evolution: fields missing from
an older file fall back to a sensible default on load (`#[serde(default)]`). Since format
version 2, a field that equals its default is simply not written (a duration is stored as a
tick count rather than `{base, dots}`), which noticeably shrinks file size without changing
the data model; existing v1 files keep loading as-is.

## Makefile targets

| Target | Effect |
|---|---|
| `run` | `cargo run` |
| `build` | `cargo build --release` then copies the binary into `dist/` |
| `app` | packages `dist/GRAT.app` (Info.plist + .icns icon + binary) — macOS |
| `logo-assets` | regenerates `image.icns` and `src/assets/logo.png` from `logo.svg` — macOS |
| `examples` | regenerates `exemples/*.pdf` via `--export` |
| `test` | `cargo test` |
| `fmt` | `cargo fmt` |
| `lint` | `cargo clippy --all-targets -- -D warnings` |
| `clean` | `cargo clean` + empties `dist/` and `build/` |

## Project status

The 6 phases of the development plan are complete: skeleton and theme, data model and JSON
persistence, screen layout and rendering, mouse editing, full engraving (beams, stems, ties,
staff, accidentals), PDF export and polish (Help menu, about, window and bundle icon,
examples). `make run` opens the full application; `make examples` regenerates the PDFs in
the `exemples/` folder, which show the three block models on short but musically plausible
pieces.

## Licence

CC BY-NC-SA 4.0 — see [LICENSE](LICENSE).

This software may be freely reused and modified, under three conditions: credit the author,
no commercial use, and redistribute derivative versions under this same licence.

Author: Nicolas Jalibert <nicoolaj@gmail.com> — <https://github.com/nicoolaj>

## Evolving the software

[CLAUDE.md](CLAUDE.md) is the guide for Claude and other agents: the architectural idea, the
module map, the invariants not to break, already-verified API facts, and recipes for common
additions (a playing technique, a language, a block model).
