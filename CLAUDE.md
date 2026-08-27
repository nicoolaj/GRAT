# Strungin — guide for Claude and other agents

Strungin is a desktop editor for six-string guitar tablature: place notes by mouse, save as JSON, print an
A4 PDF. Rust, `eframe`/`egui`, one portable binary (macOS/Windows/Linux, ARM and x86).

Author: Nicolas Jalibert <nicoolaj@gmail.com> — https://github.com/nicoolaj
Licence: CC BY-NC-SA 4.0 (see `LICENSE`). Reuse and modification are welcome; commercial use is
not, and the author must be credited. Keep that notice intact in anything you add.

## Commands

```bash
make run       # cargo run — opens a window; NEVER run this from an agent session, it blocks
make test      # cargo test
make lint      # cargo clippy --all-targets -- -D warnings
make build     # release binary into dist/
make app       # dist/Strungin.app (macOS)
make examples  # regenerate exemples/*.pdf
```

Verify with `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.
All four must be clean before a commit.

## The one architectural idea

**The page is laid out once, in millimetres, and rendered twice.** `layout.rs` turns a `Document`
into `Vec<Page>`, where a page is a list of `Prim` (line, filled polygon, cubic curve, text) in A4
millimetres. `canvas.rs` paints those on screen; `pdf.rs` writes the same list to PDF. WYSIWYG is
therefore structural — there is no second layout engine to keep in sync.

Everything else follows from that. If you are tempted to compute geometry in `canvas.rs` or
`pdf.rs`, you are in the wrong file.

## Module map

| File | Does |
|---|---|
| `model.rs` | `Document` / `Bar` / `Event` / `Note`, note values in ticks, techniques and their colours, serde |
| `i18n.rs` + `i18n/*.json` | locale detection, `t(key)`, note and rest names (they differ FR/US) |
| `engrave.rs` | rhythm only: beat grouping, beams, proportional spacing, justification. No pitch, no glyphs |
| `staff.rs` | furniture both rows share: barlines and repeats, rests, arrowheads, waves, text metrics |
| `notation.rs` | the five-line staff: clef, heads, stems, beams, accidentals, ties, ledger lines |
| `tablature.rs` | the tablature row: string lines, fret labels, all 17 technique glyphs, strum row, rhythm stems |
| `layout.rs` | line breaking, block stacking, pagination, headers and footers, hit boxes |
| `canvas.rs` | egui painting of `Prim`, mouse editing, tool palette |
| `pdf.rs` | `Prim` → printpdf ops |
| `main.rs` | window, theme, menus, dialogs, shortcuts, `--export` CLI |

Dependency direction is strictly downward in that table. `engrave`/`staff`/`notation`/`tablature`
never know about pages, egui or PDF.

## Invariants — breaking these breaks the output

1. **One `Spacing` per system drives every row of a block.** Call `engrave::system_spacing` once and
   pass the same `&Spacing` to `tablature::render`, `tablature::render_strum` and
   `notation::render`. This is what makes a fret number, its strum arrow and its note head land in
   the same vertical column. It is a stated requirement, not an implementation detail.
2. **Millimetres, origin bottom-left, y points up** (PDF convention). egui's y points down, so
   `canvas.rs` flips; nothing else does.
3. **`origin` means the first barline.** For every row renderer, `origin.x` is where the music
   starts and the staff head (the "TAB" letters, the clef, the time signature) is drawn by the
   renderer itself in `[origin.x - HEAD_MM, origin.x]`. The layout reserves that room.
   `origin.y` is the bottom string line / bottom staff line / strum baseline.
4. **`Prim` order is paint order.** The tablature row knocks the string line out behind each fret
   number with a paper-coloured quad, so a row's primitives must be emitted contiguously.
5. **No user-facing English literal outside `src/i18n/*.json`.** Not in `main.rs`, not in
   `canvas.rs`, not in a file-dialog filter. If you need a string, add a key to both language files.
6. **Code and comments in English**; only the interface is translated. The README and this file are
   the exception.
7. **The library stays GUI-free.** `cargo test` and the `--export` CLI must work without opening a
   window, which is why `canvas.rs` is a module of the binary.
8. **Fret labels contain `<`, `>`, `(`, `)`** (harmonics `<12>`, ghost notes `(5)`, trills `5(9)`).
   Any text backend must escape for its own format — this already bit the SVG proof sheet.

## Verified API facts — do not spend a session rediscovering these

- `eframe 0.36.1`: the required `App` method is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut
  eframe::Frame)`, **not** `update(ctx, frame)`. `logic(ctx, frame)` is the optional pre-pass.
- `egui 0.36`: `TopBottomPanel` and `SidePanel` are gone — panels are unified as
  `egui::Panel::top/bottom/left/right(id).show(ui, |ui| ...)`. Menu bar is
  `egui::MenuBar::new().ui(ui, |ui| { ui.menu_button(label, |ui| ...) })`.
- eframe needs `features = ["persistence"]` or `cc.storage` is always `None` and `App::save` never
  runs.
- Reach egui through `eframe::egui`; there is no separate `egui` dependency.
- `printpdf 0.12`: `default-features = false` is deliberate — the default `html` feature drags in
  `azul-layout` and `rust-fontconfig` for nothing. `ops`/`font`/`graphics`/`color`/`serialize` are
  not feature-gated.
- Text in PDF uses `PdfFontHandle::Builtin(BuiltinFont::Helvetica)`; no font file is shipped.
  Helvetica digits advance exactly 0.556 em, which is what `staff::label_width` uses to centre
  fret numbers.
- **printpdf trap**: `Op::SetTextCursor` emits `Td`, which is *relative* to the previous line. Wrap
  every text item in its own `StartTextSection` / `EndTextSection` so `BT` resets the matrix and the
  first `Td` is absolute. Otherwise positions accumulate down the page.
- The cover artwork (`src/assets/cover.jpg`, 1024×1024, progressive JPEG) decodes with
  `image = { version = "0.25", default-features = false, features = ["jpeg"] }` —
  `load_from_memory_with_format(.., ImageFormat::Jpeg)?.to_rgba8()`, tested. Feed that to
  `egui::ColorImage::from_rgba_unmultiplied` for the About and help pages, and to the viewport icon.
  Downscale to ~256 px first with `image::imageops`: the full frame is 4 MB of RGBA for something
  drawn at a fraction of that.
- The macOS icon pipeline is `sips -s format png -z N N src/assets/cover.jpg --out
  icon.iconset/icon_NxN.png` for N in 16/32/128/256/512 plus their `@2x`, then
  `iconutil -c icns icon.iconset`. Tested end to end; both tools ship with macOS, so `make app`
  needs no extra install.

## Recipes

**Add a playing technique.** Add the variant to `model::Technique`, a `COLOR_*` constant and its arm
in `technique_color`, an arm in `tablature::technique` for the glyph (and in `tablature::fret_label`
if it changes the printed number), a `tech.*` key in both language files, and a button in the
`canvas.rs` palette. Then add it to the row in `tests/visual.rs::technique_bar` and look at the
sheet.

**Add a language.** Drop `src/i18n/xx.json` next to the others and add one line to
`i18n::LANGS`. Missing keys fall back to English, so a partial file is safe to ship.

**Add a block model.** Extend `model::BlockModel`, then handle it in `layout.rs` (block height and
row stacking) and in `tablature::shows_rhythm` — a block with no notation staff must carry its own
rhythm stems, or it says which frets to play but never when.

**Change how something is engraved.** Everything rhythmic (what gets beamed, how wide a note is) is
in `engrave.rs` and is unit-tested in `tests/engraving.rs`. Everything visual is in `notation.rs` /
`tablature.rs` and is checked by eye — see below.

## Looking at your changes

```bash
cargo test --test visual   # writes dist/notation-preview.svg
open dist/notation-preview.svg
```

`tests/visual.rs` renders a proof sheet: every technique glyph, a full three-row block with repeat
barlines and palm-mute spans, and a tablature-only block carrying its own rhythm. It also asserts
structure (primitive count, one hit box per string per event), so it fails on a regression even
unattended. Change the engraving, regenerate, and actually look at it — beams, stems and clefs are
not things assertions can judge.

On macOS without an SVG viewer to hand: `qlmanage -t -s 1500 -o /tmp dist/notation-preview.svg`
produces a PNG.

## Conventions

- Deliberate simplifications with a known ceiling carry a `ponytail:` comment naming the ceiling and
  the upgrade path. Grep for them before assuming something was an oversight.
- Comments explain *why*, and are worth writing where a reader would otherwise wonder. Do not
  narrate what the code already says.
- Prefer extending an existing module over adding one. Seven source files is the budget.

## Out of scope for v1 — ask before building

Tuplets (triplets); ties across a barline; da capo / segno and alternate endings (1./2.); MusicXML
or Guitar Pro import and export; audio or MIDI playback; multi-level undo beyond the snapshot stack.
