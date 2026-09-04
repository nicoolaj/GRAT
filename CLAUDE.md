# GRAT — guide for Claude and other agents

GRAT is a desktop editor for six-string guitar tablature: place notes by mouse, save as JSON, print an
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
make app       # dist/GRAT.app (macOS)
make examples  # regenerate exemples/*.pdf
```

Verify with `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.
All four must be clean before a commit.

**End of every iteration: bump the version and tag it.** Raise `version` in `Cargo.toml`
(semver: patch for a fix, minor for a feature, major for a break), commit that bump, then
`git tag vX.Y.Z` on the same commit with the matching number. The tag and `Cargo.toml` must
always agree. No iteration is finished — and no work is handed back — without both.

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
| `engrave.rs` | rhythm only: beat grouping, beams, proportional spacing, justification, and the print normalisation (`for_export`: merge rests, split notes straddling a beat into tied pieces). No pitch, no glyphs |
| `staff.rs` | furniture both rows share: barlines and repeats, rests, arrowheads, waves, text metrics |
| `notation.rs` | the five-line staff: clef, heads, stems, beams, accidentals, ties, ledger lines |
| `tablature.rs` | the tablature row: string lines, fret labels, all 20 technique glyphs, strum row, rhythm stems |
| `layout.rs` | line breaking, block stacking, pagination, headers and footers, hit boxes |
| `canvas.rs` | egui painting of `Prim`, mouse editing, tool palette |
| `live.rs` | the live player: transport, the two scrolling views, the highlighter, the metronome (click + flash + count-in). A module of the binary, like `canvas.rs`, and it paints through `canvas`'s `Prim` painter. The metronome click is macOS/Windows only — see "Verified API facts" |
| `pdf.rs` | `Prim` → printpdf ops |
| `main.rs` | window, theme, menus, dialogs, shortcuts, launch splash, `--export` CLI |

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
   (Lone exception: `EXPANSIONS` in `main.rs`, the recursive-acronym readings of the name — French
   wordplay with no English form to key. Its `ponytail:` comment says so.)
6. **Code and comments in English**; only the interface is translated. The README and this file are
   the exception.
7. **The library stays GUI-free.** `cargo test` and the `--export` CLI must work without opening a
   window, which is why `canvas.rs` is a module of the binary.
8. **The live player's cues, metronome beats, strip and pages all come from one
   `engrave::for_export` of the document.** That call trims the editor's trailing blank bars,
   rewrites its per-beat rests and splits notes at beat boundaries into tied pieces, all of which
   renumbers events — so a cue's `(bar, event)` only addresses the
   right column if the geometry was built from the very same normalised document, and the metronome
   grid only counts the right number of bars if it walks that same document too.
   `LiveState::enter` builds all four together for exactly that reason; don't split it up. A
   count-in's own beat grid is the one exception: built fresh per play, from a throwaway document of
   empty bars, not from `Show` — see `live::start_count_in`.
9. **Fret labels contain `<`, `>`, `(`, `)`** (harmonics `<12>`, ghost notes `(5)`, trills `5(9)`).
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
- `Prim::Curve` maps to a printpdf `Line` of four `LinePoint`s: the start with `bezier: false`,
  both control points with `bezier: true`, the end point with `bezier: false`, `is_closed: false`.
  Verified in printpdf's `serialize.rs` — two consecutive bezier handles followed by an end point
  emit the `c` operator.
- The brand mark is `logo.svg` at the repo root: the name engraved on a one-bar six-string
  tablature. Everything raster is derived from it by `make logo-assets` (macOS-only — it
  rasterises the SVG with QuickLook, `qlmanage -t`, then `sips` + `iconutil`): `src/assets/logo.png`
  (512 px, the app's only bundled image, `include_bytes!`d and decoded with
  `image = { .., features = ["png"] }` → `load_from_memory_with_format(.., ImageFormat::Png)?.to_rgba8()`
  → `egui::ColorImage::from_rgba_unmultiplied`, feeding the viewport icon, the splash, and the
  About/Help pages) and `image.icns` at the repo root (the `.app` icon *and*, via
  `CFBundleTypeIconFile`, the `.gtab` document icon). `make app` copies `image.icns` to
  `Contents/Resources/GRAT.icns` — `CFBundleIconFile` is `GRAT`, so the resource name must stay
  `GRAT.icns`. Edit `logo.svg`, rerun `make logo-assets`, commit both outputs.
- The launch splash lives in `TablaturesApp::splash` (`main.rs`): an opaque foreground layer with
  the logo and one random `EXPANSIONS` reading, up for 2.2 s or until the first click/key, then
  gone for the session (`splash_until: Option<Instant>`). Its reading is an independent draw from
  the window title's `random_expansion()` — both re-roll every launch.
- **`muda` (native macOS menu bar) was tried and reverted — do not retry without checking upstream
  first.** muda 0.19.3's custom `NSMenuItem` subclass (`MudaMenuItem`) stores a raw pointer
  (`#[ivars = Cell<*const MenuChild>]`) to its Rust-side data instead of an owned/reference-counted
  handle — muda's own source marks this `// FIXME: Use Rc or something else to access the
  MenuChild.`. Clicking any custom (non-predefined) menu item — File > Open in particular —
  dereferenced that pointer and crashed the process (SIGABRT, uncaught NSException, confirmed via
  `log show` pinpointing the abort to the exact instant AppKit dispatched the click). 0.19.3 is the
  newest release (checked crates.io); no fix exists to upgrade to. `PredefinedMenuItem`-based items
  (Hide, Minimize, Close, Quit, etc.) don't go through that code path and were not observed to
  crash, but the app's own actions (New, Open, Save, Undo, switching the block model...) all need
  custom items, which is most of the menu. Reverted to `egui::Panel::top` + `egui::MenuBar` on every
  platform, including macOS — see `TablaturesApp::egui_menu_bar` in `main.rs`.
- **`rodio 0.22` (the live-mode metronome click) is macOS/Windows only — do not add it back as a
  plain, all-platform dependency.** Its `playback` feature (needed for any actual output; it is not
  in the default feature set either) pulls in `cpal`, whose Linux backend is `alsa-sys`. That crate's
  build script calls `pkg-config` for the *target's* `libasound`, and aborts outright — not a
  degraded fallback, a hard build failure — when none is configured, which is exactly the zig
  cross-linker `make dist-cross` uses for `x86_64-`/`aarch64-unknown-linux-gnu` (confirmed by
  actually running `cargo zigbuild --target x86_64-unknown-linux-gnu` with `rodio`'s `playback`
  feature on: `alsa-sys`'s build script panics with "pkg-config has not been configured to support
  cross-compilation"). macOS (CoreAudio via `coreaudio-rs`) and Windows (WASAPI via the `windows`
  crate) both cross-build clean under zig — confirmed the same way — so only Linux is the problem.
  Cargo.toml scopes the dependency to `[target.'cfg(not(target_os = "linux"))'.dependencies]` and
  `live.rs` carries a `#[cfg(target_os = "linux")]` no-op stub of the same `click::Audio` API instead
  of carrying an ALSA sysroot as a new host-setup step. Revisit only alongside a real Linux ALSA (or
  PipeWire) dev sysroot wired into `check-zigbuild`, not by flipping the feature back on alone.

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

**Change the save format.** `.gtab` is `serde_json` of `Document`, carrying `format_version`
(`model::FORMAT_VERSION`). Adding a field with `#[serde(default)]` needs no bump — old files still
load. Renaming/removing a field, or changing a type, is a breaking change: bump `FORMAT_VERSION`,
and in `Document::from_json` (between the version probe and the final parse) migrate the older JSON
up to today's shape. Every load goes through `from_json`; a file claiming a newer version than this
build is refused (`LoadError::TooNew`), not parsed with fields silently dropped.

The v1 → v2 bump (a type change: `Dur` went from `{base, dots}` to a bare tick count, plus every
field that equals its default stops being written at all) needed no migration code in `from_json`
— a real exception to the paragraph above, not a precedent to follow blindly. It reads both
directions because `Dur`'s hand-written `Deserialize` is a `#[serde(untagged)]` enum trying a tick
count first and the old two-field shape second, and every other field v2 omits is one a v1 file
always wrote explicitly, so plain `#[serde(default)]` fills it in either way. Reach for that trick
again only when the old and new shapes are this cleanly distinguishable on sight (a number vs. an
object); anything murkier belongs in the probe-then-migrate seam instead. `exemples/*.gtab` are
kept as unversioned v1 files on purpose — `make examples` re-reads them on every run, which makes
them a free, permanent v1-compatibility check. Do not "upgrade" them to v2.

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
or Guitar Pro import and export; multi-level undo beyond the snapshot stack. MIDI playback, and
audio playback of the piece's own notes, are still out of scope — the live-mode metronome (`live.rs`)
is a synthesised click marking time, not a synthesiser for what the tablature says to play.
