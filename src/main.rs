//! Tablatures desktop app: window, theme, menu, and file/CLI wiring.
//!
//! The document model and i18n live in the `grat` library crate (see `src/lib.rs`,
//! `src/model.rs`, `src/i18n.rs`); this binary only adds the eframe GUI shell and the
//! `--export` CLI entry point on top of it.

mod canvas;
mod live;

use std::path::PathBuf;

use eframe::egui;
use grat::i18n::{self, t};
use grat::model::{technique_color, Document, Instrument, LoadError, Row, Technique, TuningLabel};

const SHORTCUT_NEW: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
const SHORTCUT_OPEN: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
const SHORTCUT_SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
const SHORTCUT_SAVE_AS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::S,
);
const SHORTCUT_EXPORT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
const SHORTCUT_QUIT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Q);
const SHORTCUT_UNDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
const SHORTCUT_COPY: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C);
const SHORTCUT_CUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::X);
const SHORTCUT_PASTE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::V);
const SHORTCUT_REPEAT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R);
const SHORTCUT_LIVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::L);

/// Every playing technique with its i18n key and the kind it belongs to, for the Help
/// legend and the Edit > note styles submenu. One representative field value per
/// variant, same choices as `tests/visual.rs::technique_bar`. Entries of one kind are
/// consecutive — that is what draws the group headings in the submenu — and a
/// technique's position here is its bit in `EditorState::tech_shown`.
const TECH_LEGEND: &[(Technique, &str, &str)] = &[
    (Technique::Plain, "tech.plain", "techkind.plain"),
    (Technique::HammerOn, "tech.hammer_on", "techkind.legato"),
    (Technique::PullOff, "tech.pull_off", "techkind.legato"),
    (Technique::Slide, "tech.slide", "techkind.legato"),
    (Technique::SlideShift, "tech.slide_shift", "techkind.legato"),
    (
        Technique::SlideIn { from_fret: 3 },
        "tech.slide_in",
        "techkind.legato",
    ),
    (Technique::Grace, "tech.grace", "techkind.legato"),
    (
        Technique::Bend { quarters: 4 },
        "tech.bend",
        "techkind.bend",
    ),
    (
        Technique::BendRelease { quarters: 2 },
        "tech.bend_release",
        "techkind.bend",
    ),
    (
        Technique::PreBend { quarters: 4 },
        "tech.pre_bend",
        "techkind.bend",
    ),
    (Technique::Vibrato, "tech.vibrato", "techkind.ornament"),
    (
        Technique::WideVibrato,
        "tech.wide_vibrato",
        "techkind.ornament",
    ),
    (
        Technique::Trill { to_fret: 9 },
        "tech.trill",
        "techkind.ornament",
    ),
    (Technique::Harmonic, "tech.harmonic", "techkind.harmonic"),
    (
        Technique::PinchHarmonic,
        "tech.pinch_harmonic",
        "techkind.harmonic",
    ),
    (Technique::Tap, "tech.tap", "techkind.attack"),
    (Technique::Slap, "tech.slap", "techkind.attack"),
    (Technique::Pop, "tech.pop", "techkind.attack"),
    (Technique::Dead, "tech.dead", "techkind.muted"),
    (Technique::Ghost, "tech.ghost", "techkind.muted"),
];

/// Which palette technique buttons are switched on, remembered across sessions.
const TECH_SHOWN_STORAGE_KEY: &str = "tech_shown";

/// The licence the About box names, so the line can be clicked through to it.
const LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";

/// GRAT is a recursive acronym that deliberately never settles: the window title
/// shows a different expansion each launch, the About box keeps the canonical one
/// (`[0]`). It all winks at *gratte*, French slang for a guitar. French on purpose,
/// so there is no English column for `i18n/*.json`.
/// ponytail: inline const, not i18n -- untranslatable wordplay; revisit only if a
/// second language ever wants its own set.
const EXPANSIONS: &[&str] = &[
    "GRAT Rythmes, Accords, Tablatures",
    "GRAT Rédige les Accords et Tablatures",
    "GRAT Range Arpèges et Tonalités",
    "GRAT Relie les Accords et Tirés",
    "GRAT Restitue Articulations et Techniques",
    "GRAT Retranscrit Accords et Tempos",
    "GRAT Révèle les Altérations et Transpositions",
    "GRAT Répète les Arpèges à Tempo",
    "GRAT Recompose Accords et Tablatures",
    "GRAT Résout Accordages et Tonalités",
    "GRAT Réaligne Attaques et Tenues",
    "GRAT Reste Agréablement Trivial",
    "GRAT n'a Rien d'Autre qu'une Tablature",
];

/// One expansion, picked by wall-clock nanoseconds -- no rng crate for a cosmetic
/// title-bar pick.
fn random_expansion() -> &'static str {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize);
    EXPANSIONS[n % EXPANSIONS.len()]
}

/// Decode the GRAT logo (a 512-px raster of `logo.svg`, regenerated by
/// `make logo-assets`). One texture serves the window icon, the splash, and the
/// About/Help pages.
fn load_logo_rgba() -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory_with_format(
        include_bytes!("assets/logo.png"),
        image::ImageFormat::Png,
    )
    .expect("bundled logo.png must decode")
    .to_rgba8();
    let (w, h) = (img.width(), img.height());
    (img.into_raw(), w, h)
}

/// `grat --export <in.gtab> <out.pdf>`: load, export, write, no window. Shared
/// entry point for the CLI flag and `make examples`.
fn run_export(input: &str, output: &str) -> Result<(), String> {
    let doc: Document = std::fs::read_to_string(input)
        .ok()
        .and_then(|s| Document::from_json(&s).ok())
        .ok_or_else(|| format!("{}: {input}", t("error.load")))?;
    let bytes = grat::pdf::export(&doc);
    std::fs::write(output, bytes).map_err(|_| format!("{}: {output}", t("error.save")))
}

/// Open a file in the OS's default viewer -- for a PDF, this is how the user reaches
/// the real print dialog, without this app needing its own print path.
fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    // explorer, never `cmd /C start`: cmd reads a `&` in the file name as the
    // start of a second command, and the name defaults to the document's title.
    // Quoted by hand (a Windows path cannot hold `"`) so explorer, which splits
    // its own command line at commas, gets the path whole.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut arg = std::ffi::OsString::from("\"");
        arg.push(path);
        arg.push("\"");
        let _ = std::process::Command::new("explorer").raw_arg(arg).spawn();
    }
}

/// The name the PDF save dialog proposes: the title, less the characters no
/// file system takes (a title like "AC/DC" would otherwise name a folder).
fn pdf_file_name(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            if c.is_control() || r#"\/:*?"<>|"#.contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    // Windows drops trailing dots and spaces, and a leading dot hides the file.
    let stem = stem.trim_matches(|c: char| c == '.' || c.is_whitespace());
    format!("{}.pdf", if stem.is_empty() { "untitled" } else { stem })
}

/// Write `bytes` to `path` without ever leaving it half-written: into a sibling
/// temporary file, flushed to disk, then renamed over the original, so a crash
/// mid-save leaves the previous version whole instead of a truncated one.
///
/// ponytail: the renamed file takes default permissions and replaces a symlink
/// rather than writing through it; copy the metadata and resolve the link first
/// if anyone saves through one.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn main() -> eframe::Result {
    // CLI mode: `grat --export <in.gtab> <out.pdf>`. Never opens a window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--export") {
        let (input, output) = match (args.get(pos + 1), args.get(pos + 2)) {
            (Some(i), Some(o)) => (i.clone(), o.clone()),
            _ => {
                eprintln!("{}", t("cli.export_usage"));
                std::process::exit(1);
            }
        };
        if let Err(e) = run_export(&input, &output) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let (logo_rgba, logo_w, logo_h) = load_logo_rgba();
    // Today's decoration, if any, is baked into its own copy of the icon pixels --
    // egui only takes an icon once, via `ViewportBuilder::with_icon`, so there is no
    // live-update path to decorate it later. The window/About/Help texture below
    // stays undecorated; the same `Decoration` is instead painted live on top of it
    // (see `splash`/the About window) via `canvas::paint_decoration`.
    let today_decoration = grat::decorations::decoration_for_today();
    let icon_rgba = match &today_decoration {
        Some(deco) => {
            let mut baked = logo_rgba.clone();
            let radius = logo_w.min(logo_h) as f32 * 0.36;
            grat::decorations::bake(
                &mut baked,
                logo_w,
                logo_h,
                logo_w as f32 / 2.0,
                logo_h as f32 / 2.0,
                radius,
                deco,
            );
            baked
        }
        None => logo_rgba.clone(),
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([640.0, 480.0])
            .with_icon(egui::IconData {
                rgba: icon_rgba,
                width: logo_w,
                height: logo_h,
            }),
        ..Default::default()
    };

    eframe::run_native(
        random_expansion(),
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(TablaturesApp::new(
                cc,
                logo_rgba,
                logo_w,
                logo_h,
                today_decoration,
            )))
        }),
    )
}

/// Apply the app's visual identity to both the light and dark style variants, so the
/// look stays consistent whichever one the OS theme selects.
fn style_context(ctx: &egui::Context) {
    // Follow the OS light/dark preference rather than forcing one.
    ctx.set_theme(egui::ThemePreference::System);

    let accent = egui::Color32::from_rgb(0x5E, 0x5C, 0xE6); // indigo
    let radius = egui::CornerRadius::same(8);

    ctx.all_styles_mut(|style| {
        // Generous, breathable spacing rather than the cramped egui default.
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.window_margin = egui::Margin::same(12);
        style.spacing.menu_margin = egui::Margin::same(8);

        // Soft, consistent rounding everywhere.
        style.visuals.window_corner_radius = radius;
        style.visuals.menu_corner_radius = radius;
        style.visuals.widgets.noninteractive.corner_radius = radius;
        style.visuals.widgets.inactive.corner_radius = radius;
        style.visuals.widgets.hovered.corner_radius = radius;
        style.visuals.widgets.active.corner_radius = radius;
        style.visuals.widgets.open.corner_radius = radius;

        // Accent color for selection/links/hovered text.
        style.visuals.selection.bg_fill = accent;
        style.visuals.hyperlink_color = accent;
        style.visuals.widgets.hovered.fg_stroke.color = accent;

        // Borderless, shadow-free panels with discreet separators instead of frames.
        style.visuals.window_stroke = egui::Stroke::NONE;
        style.visuals.window_shadow = egui::Shadow::NONE;
        style.visuals.panel_fill = style.visuals.window_fill;
    });
}

#[derive(Clone, Copy)]
enum PendingAction {
    New,
    Open,
    Quit,
}

struct TablaturesApp {
    doc: Document,
    path: Option<PathBuf>,
    dirty: bool,
    /// Set whenever `doc` changes; cleared once `editor_ui` has repaginated for it.
    layout_dirty: bool,
    /// The last `layout::paginate(&doc)` result, reused across frames while
    /// `layout_dirty` is false instead of recomputing on every single one.
    cached_pages: Vec<grat::layout::Page>,
    pending: Option<PendingAction>,
    show_about: bool,
    show_help: bool,
    /// A PDF was just exported to this path; offer to open it (which is how the
    /// user reaches the OS print dialog).
    pending_open_pdf: Option<PathBuf>,
    /// Last error or informational notice, shown in the status bar.
    status_msg: Option<String>,
    /// A retune waiting on "keep the frets or the pitches?" -- see `request_retune`.
    pending_retune: Option<(Instrument, Vec<u8>)>,
    /// The custom tuning being edited, while its dialog is open.
    tuning_draft: Option<Vec<u8>>,
    editor: canvas::EditorState,
    /// The live player. Holds the window whenever `open`.
    live: live::LiveState,
    /// The GRAT logo, decoded once: window icon, splash, and the About/Help pages.
    logo: egui::TextureHandle,
    /// The splash overlay shows until this instant, then is gone for the session;
    /// `None` once it has been shown or dismissed.
    splash_until: Option<std::time::Instant>,
    /// The reading of GRAT this launch's splash shows -- an independent draw from
    /// the one in the window title.
    splash_reading: &'static str,
    /// Today's calendar decoration, if any -- computed once at startup, shown on the
    /// splash and About windows (and already baked into the dock/taskbar icon).
    today_decoration: Option<grat::decorations::Decoration>,
}

impl TablaturesApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        logo_rgba: Vec<u8>,
        logo_w: u32,
        logo_h: u32,
        today_decoration: Option<grat::decorations::Decoration>,
    ) -> Self {
        let mut editor = canvas::EditorState::default();
        if let Some(storage) = cc.storage {
            if let Some(lang) = storage.get_string(i18n::LANG_STORAGE_KEY) {
                i18n::set_lang(&lang);
            }
            if let Some(mask) = storage
                .get_string(TECH_SHOWN_STORAGE_KEY)
                .and_then(|s| s.parse().ok())
            {
                editor.tech_shown = mask;
            }
        }
        style_context(&cc.egui_ctx);
        let logo_image = egui::ColorImage::from_rgba_unmultiplied(
            [logo_w as usize, logo_h as usize],
            &logo_rgba,
        );
        let logo = cc
            .egui_ctx
            .load_texture("logo", logo_image, egui::TextureOptions::LINEAR);
        TablaturesApp {
            doc: Document::new_empty(),
            path: None,
            dirty: false,
            layout_dirty: true,
            cached_pages: Vec::new(),
            pending: None,
            show_about: false,
            show_help: false,
            pending_open_pdf: None,
            status_msg: None,
            pending_retune: None,
            tuning_draft: None,
            editor,
            live: live::LiveState::default(),
            logo,
            splash_until: Some(std::time::Instant::now() + std::time::Duration::from_millis(2200)),
            splash_reading: random_expansion(),
            today_decoration,
        }
    }

    /// The launch splash: the logo and one random reading of the name, over an
    /// opaque fill, until `splash_until` or the first click/key. Returns `true`
    /// while it owns the frame, so `ui` paints nothing else.
    fn splash(&mut self, ui: &egui::Ui) -> bool {
        let Some(until) = self.splash_until else {
            return false;
        };
        let dismiss = ui.ctx().input(|i| {
            i.pointer.any_pressed()
                || i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
        });
        if dismiss || std::time::Instant::now() >= until {
            self.splash_until = None;
            return false;
        }

        let screen = ui.ctx().viewport_rect();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("splash"),
        ));
        painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(0x14, 0x13, 0x18));
        let s = (screen.width().min(screen.height()) * 0.42).min(260.0);
        let logo_rect = egui::Rect::from_center_size(
            screen.center() - egui::vec2(0.0, s * 0.16),
            egui::vec2(s, s),
        );
        painter.image(
            self.logo.id(),
            logo_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(screen.center().x, logo_rect.bottom() + 30.0),
            egui::Align2::CENTER_TOP,
            self.splash_reading,
            egui::FontId::proportional(19.0),
            egui::Color32::from_rgb(0xF2, 0xA0, 0x49),
        );
        if let Some(deco) = &self.today_decoration {
            canvas::paint_decoration(&painter, logo_rect.center(), s * 0.55, deco);
            painter.text(
                egui::pos2(screen.center().x, logo_rect.bottom() + 54.0),
                egui::Align2::CENTER_TOP,
                t(deco.caption_key),
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(0xB0, 0xB0, 0xB8),
            );
        }
        ui.ctx().request_repaint();
        true
    }

    fn request_new(&mut self) {
        if self.dirty {
            self.pending = Some(PendingAction::New);
        } else {
            self.do_new();
        }
    }

    fn request_open(&mut self) {
        if self.dirty {
            self.pending = Some(PendingAction::Open);
        } else {
            self.do_open();
        }
    }

    /// Retune straight away when no written note would move or sound different;
    /// otherwise ask first whether to keep the frets or the pitches.
    fn request_retune(&mut self, instrument: Instrument, tuning: Vec<u8>) {
        let has_notes = self
            .doc
            .bars
            .iter()
            .flat_map(|b| &b.events)
            .any(|e| !e.is_rest());
        if has_notes && tuning != self.doc.tuning {
            self.pending_retune = Some((instrument, tuning));
        } else if instrument != self.doc.instrument || tuning != self.doc.tuning {
            self.apply_retune(instrument, tuning, false);
        }
    }

    fn apply_retune(&mut self, instrument: Instrument, tuning: Vec<u8>, keep_pitches: bool) {
        canvas::retune(
            &mut self.editor,
            &mut self.doc,
            instrument,
            tuning,
            keep_pitches,
        );
        self.dirty = true;
        self.layout_dirty = true;
    }

    /// The Edit menu's instrument, string-count, tuning and tuning-display entries.
    fn tuning_menus(&mut self, ui: &mut egui::Ui) {
        let current = self.doc.instrument;
        let strings = self.doc.tuning.len();
        ui.menu_button(t("menu.instrument"), |ui| {
            for inst in Instrument::ALL {
                if ui
                    .selectable_label(inst == current, instrument_label(inst))
                    .clicked()
                {
                    self.request_retune(inst, inst.default_tuning().to_vec());
                    ui.close();
                }
            }
        });
        ui.menu_button(t("menu.strings"), |ui| {
            for n in current.string_counts() {
                if ui.selectable_label(n == strings, n.to_string()).clicked() {
                    if let Some(preset) = current.tunings().find(|p| p.notes.len() == n) {
                        self.request_retune(current, preset.notes.to_vec());
                    }
                    ui.close();
                }
            }
        });
        ui.menu_button(t("menu.tuning"), |ui| {
            for preset in current.tunings().filter(|p| p.notes.len() == strings) {
                let label = format!(
                    "{} — {}",
                    t(preset.key),
                    i18n::tuning_names(preset.notes, false)
                );
                if ui
                    .selectable_label(self.doc.tuning == preset.notes, label)
                    .clicked()
                {
                    self.request_retune(current, preset.notes.to_vec());
                    ui.close();
                }
            }
        });
        if ui.button(t("menu.custom_tuning")).clicked() {
            self.tuning_draft = Some(self.doc.tuning.clone());
            ui.close();
        }
        // Where the page names the open strings. Each choice previews itself with
        // the document's own tuning.
        ui.menu_button(t("menu.tuning_label"), |ui| {
            let names = |letters| i18n::tuning_names(&self.doc.tuning, letters);
            let options = [
                (TuningLabel::Hidden, t("tuning_label.hidden")),
                (
                    TuningLabel::Header { letters: false },
                    format!("{} ({})", t("tuning_label.header"), names(false)),
                ),
                (
                    TuningLabel::Header { letters: true },
                    format!("{} ({})", t("tuning_label.header"), names(true)),
                ),
                (
                    TuningLabel::Strings { letters: true },
                    format!("{} ({})", t("tuning_label.strings"), names(true)),
                ),
                (
                    TuningLabel::Strings { letters: false },
                    format!("{} ({})", t("tuning_label.strings"), names(false)),
                ),
            ];
            for (label, text) in options {
                if ui
                    .selectable_label(self.doc.tuning_label == label, text)
                    .clicked()
                {
                    self.doc.tuning_label = label;
                    self.dirty = true;
                    self.layout_dirty = true;
                    ui.close();
                }
            }
        });
    }

    fn do_new(&mut self) {
        canvas::replace_document(&mut self.editor, &mut self.doc, Document::new_empty());
        self.path = None;
        self.dirty = false;
        self.layout_dirty = true;
        self.status_msg = None;
    }

    fn do_open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("dialog.gtab_filter"), &["gtab"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path)
            .ok()
            .map(|s| Document::from_json(&s))
        {
            Some(Ok(doc)) => {
                canvas::replace_document(&mut self.editor, &mut self.doc, doc);
                self.path = Some(path);
                self.dirty = false;
                self.layout_dirty = true;
                self.status_msg = None;
            }
            Some(Err(LoadError::TooNew(_))) => self.status_msg = Some(t("error.load_too_new")),
            Some(Err(LoadError::Parse)) | None => self.status_msg = Some(t("error.load")),
        }
    }

    /// Mirror the editor's clipboard onto the system clipboard as JSON: egui only
    /// emits `Event::Paste` when that clipboard holds text, and JSON lets another
    /// GRAT window paste it back.
    fn export_clipboard(&self, ctx: &egui::Context) {
        if let Ok(json) = serde_json::to_string(&self.editor.clipboard) {
            ctx.copy_text(json);
        }
    }

    fn do_save(&mut self) {
        match self.path.clone() {
            Some(path) => self.save_to(path),
            None => self.do_save_as(),
        }
    }

    fn do_save_as(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("dialog.gtab_filter"), &["gtab"])
            .set_file_name("untitled.gtab")
            .save_file()
        else {
            return;
        };
        self.save_to(path);
    }

    fn save_to(&mut self, path: PathBuf) {
        let Ok(json) = self.doc.to_json() else {
            self.status_msg = Some(t("error.save"));
            return;
        };
        match write_atomic(&path, json.as_bytes()) {
            Ok(()) => {
                self.path = Some(path);
                self.dirty = false;
                self.status_msg = None;
            }
            Err(_) => self.status_msg = Some(t("error.save")),
        }
    }

    /// Native save dialog defaulting to the document title, then a straight
    /// `pdf::export` write -- no in-app print path; offering to open the result
    /// (wired up by the caller via `pending_open_pdf`) is how the user reaches the
    /// OS print dialog instead.
    fn do_export(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("dialog.pdf_filter"), &["pdf"])
            .set_file_name(pdf_file_name(&self.doc.title))
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, grat::pdf::export(&self.doc)) {
            Ok(()) => {
                self.status_msg = None;
                self.pending_open_pdf = Some(path);
            }
            Err(_) => self.status_msg = Some(t("error.export")),
        }
    }

    /// The menu bar. `egui::Panel::top` draws it inside the window on every
    /// platform -- ponytail: a native `NSMenu` on macOS was tried (via the `muda`
    /// crate) and reverted. Its custom `NSMenuItem` subclass keeps a raw pointer
    /// (`Cell<*const MenuChild>`, muda's own "FIXME: use Rc or something else")
    /// to the Rust side instead of an owned handle, and dereferencing it on click
    /// aborted the process (SIGABRT, uncaught NSException) on this machine's
    /// macOS -- confirmed in muda 0.19.3's vendored source, no newer release
    /// exists. Revisit if muda fixes that FIXME upstream.
    fn egui_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(t("menu.file"), |ui| {
                    let sc_new = ui.ctx().format_shortcut(&SHORTCUT_NEW);
                    let sc_open = ui.ctx().format_shortcut(&SHORTCUT_OPEN);
                    let sc_save = ui.ctx().format_shortcut(&SHORTCUT_SAVE);
                    let sc_save_as = ui.ctx().format_shortcut(&SHORTCUT_SAVE_AS);
                    let sc_export = ui.ctx().format_shortcut(&SHORTCUT_EXPORT);
                    let sc_quit = ui.ctx().format_shortcut(&SHORTCUT_QUIT);

                    if ui
                        .add(egui::Button::new(t("menu.new")).shortcut_text(sc_new))
                        .clicked()
                    {
                        self.request_new();
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new(t("menu.open")).shortcut_text(sc_open))
                        .clicked()
                    {
                        self.request_open();
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new(t("menu.save")).shortcut_text(sc_save))
                        .clicked()
                    {
                        self.do_save();
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new(t("menu.save_as")).shortcut_text(sc_save_as))
                        .clicked()
                    {
                        self.do_save_as();
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new(t("menu.export_pdf")).shortcut_text(sc_export))
                        .clicked()
                    {
                        self.do_export();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(t("menu.about")).clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new(t("menu.quit")).shortcut_text(sc_quit))
                        .clicked()
                    {
                        // `logic` holds the window open while there is unsaved work.
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });

                ui.menu_button(t("menu.edit"), |ui| {
                    let sc_copy = ui.ctx().format_shortcut(&SHORTCUT_COPY);
                    let sc_cut = ui.ctx().format_shortcut(&SHORTCUT_CUT);
                    let sc_paste = ui.ctx().format_shortcut(&SHORTCUT_PASTE);
                    let has_sel = self.editor.selected.is_some();
                    let has_clip = !self.editor.clipboard.is_empty();
                    if ui
                        .add_enabled(
                            has_sel,
                            egui::Button::new(t("menu.copy")).shortcut_text(sc_copy),
                        )
                        .clicked()
                    {
                        if canvas::copy(&mut self.editor, &self.doc) {
                            self.export_clipboard(ui.ctx());
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            has_sel,
                            egui::Button::new(t("menu.cut")).shortcut_text(sc_cut),
                        )
                        .clicked()
                    {
                        if canvas::cut(&mut self.editor, &mut self.doc) {
                            self.export_clipboard(ui.ctx());
                            self.dirty = true;
                            self.layout_dirty = true;
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            has_sel && has_clip,
                            egui::Button::new(t("menu.paste")).shortcut_text(sc_paste),
                        )
                        .clicked()
                    {
                        if canvas::paste(&mut self.editor, &mut self.doc) {
                            self.dirty = true;
                            self.layout_dirty = true;
                        }
                        ui.close();
                    }
                    ui.separator();
                    // Input mode: whether an explicit duration change pushes the
                    // rest of the piece forward or absorbs locally. The status
                    // bar's "INS" light reflects this.
                    ui.menu_button(t("menu.input_mode"), |ui| {
                        if ui
                            .selectable_label(
                                self.editor.shift_following,
                                t("tool.shift_following"),
                            )
                            .clicked()
                        {
                            self.editor.shift_following = true;
                            ui.close();
                        }
                        if ui
                            .selectable_label(
                                !self.editor.shift_following,
                                t("tool.keep_following"),
                            )
                            .clicked()
                        {
                            self.editor.shift_following = false;
                            ui.close();
                        }
                    });
                    // Note styles: which technique buttons the palette carries.
                    // Twenty of them is more than most players ever want, and the
                    // ones left out give the palette its room back.
                    ui.menu_button(t("menu.note_styles"), |ui| {
                        let mut last_kind = "";
                        for (_tech, _key, group) in TECH_LEGEND.iter() {
                            if *group != last_kind {
                                ui.menu_button(t(group), |ui| {
                                    // Emit all techniques in this category.
                                    for (j, (tech2, key2, group2)) in TECH_LEGEND.iter().enumerate()
                                    {
                                        if *group2 != *group {
                                            continue;
                                        }
                                        let bit = 1u32 << j;
                                        let mut on = self.editor.tech_shown & bit != 0;
                                        let label = egui::RichText::new(t(key2))
                                            .color(canvas::rgb(technique_color(tech2)));
                                        if ui.checkbox(&mut on, label).changed() {
                                            self.editor.tech_shown ^= bit;
                                            // Hiding the armed technique would leave a tool
                                            // selected with no button to show for it.
                                            if !on
                                                && std::mem::discriminant(&self.editor.tool_tech)
                                                    == std::mem::discriminant(tech2)
                                            {
                                                self.editor.tool_tech = Technique::Plain;
                                            }
                                        }
                                    }
                                });
                                last_kind = group;
                            }
                        }
                    });
                    ui.separator();
                    self.tuning_menus(ui);
                });

                ui.menu_button(t("menu.bar"), |ui| {
                    if canvas::bar_menu(ui, &mut self.editor, &mut self.doc) {
                        self.dirty = true;
                        self.layout_dirty = true;
                    }
                });

                ui.menu_button(t("menu.tools"), |ui| {
                    ui.menu_button(t("menu.time_sig_all"), |ui| {
                        for sig in canvas::TIME_SIGS {
                            if ui.button(format!("{}/{}", sig.0, sig.1)).clicked() {
                                canvas::set_time_sig_everywhere(
                                    &mut self.editor,
                                    &mut self.doc,
                                    sig,
                                );
                                self.dirty = true;
                                self.layout_dirty = true;
                                ui.close();
                            }
                        }
                    });
                });

                ui.menu_button(t("menu.view"), |ui| {
                    let sc_live = ui.ctx().format_shortcut(&SHORTCUT_LIVE);
                    if ui
                        .add(egui::Button::new(t("menu.live")).shortcut_text(sc_live))
                        .clicked()
                    {
                        self.live.enter(&self.doc);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button(t("menu.language"), |ui| {
                        for lang in i18n::available_langs() {
                            let checked = i18n::current_lang() == lang;
                            if ui.selectable_label(checked, lang.to_uppercase()).clicked() {
                                i18n::set_lang(lang);
                                // The page speaks it too: chord names, the tuning line.
                                self.layout_dirty = true;
                                ui.close();
                            }
                        }
                    });
                });

                // A single action, not a dropdown with one item: opens the Help window.
                if ui.button(t("menu.help")).clicked() {
                    self.show_help = true;
                }
            });
        });
    }
    /// The editor proper: the document toolbar, the tool palette, the status bar
    /// and the page view. Skipped entirely while the live player holds the window.
    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        // Keep a blank line ready under the music: as soon as the last one is
        // written on, the next appears. Not marked dirty -- the appended bars are
        // empty scaffolding, and the invariant re-establishes itself on load.
        grat::layout::ensure_trailing_blank_system(&mut self.doc);

        if self.layout_dirty {
            self.cached_pages = grat::layout::paginate(&self.doc);
            self.layout_dirty = false;
        }
        let pages = &self.cached_pages;

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(t("field.title"));
                if ui
                    .add(egui::TextEdit::singleline(&mut self.doc.title).desired_width(140.0))
                    .changed()
                {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
                ui.label(t("field.author"));
                if ui
                    .add(egui::TextEdit::singleline(&mut self.doc.author).desired_width(120.0))
                    .changed()
                {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
                ui.separator();
                ui.label(t("field.tempo"));
                if ui
                    .add(egui::DragValue::new(&mut self.doc.tempo).range(1..=400))
                    .changed()
                {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
                ui.label(t("field.capo"));
                if ui
                    .add(egui::DragValue::new(&mut self.doc.capo).range(0..=12))
                    .changed()
                {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
                ui.separator();
                ui.label(t("field.tab_scale"));
                if ui
                    .add(
                        // Numbers and technique glyphs only; the string grid is
                        // fixed, so past ~1.05 digits on adjacent strings touch.
                        egui::DragValue::new(&mut self.doc.tab_scale)
                            .range(0.55..=1.05)
                            .speed(0.01)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
                ui.label(t("field.note_spacing"));
                if ui
                    .add(
                        // Fret labels do not shrink with this, so below ~0.75 two
                        // digit numbers on consecutive sixteenths start to touch.
                        egui::DragValue::new(&mut self.doc.note_spacing)
                            .range(0.75..=2.0)
                            .speed(0.01)
                            .max_decimals(2),
                    )
                    .changed()
                {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
                ui.separator();
                egui::containers::menu::MenuButton::new(t("rows.menu"))
                    .config(
                        egui::containers::menu::MenuConfig::new()
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                    )
                    .ui(ui, |ui| {
                        if rows_editor(ui, &mut self.doc.rows) {
                            self.dirty = true;
                            self.layout_dirty = true;
                        }
                    });
            });
            ui.add_space(4.0);
        });

        egui::Panel::left("tool_palette").show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if canvas::palette(ui, &mut self.editor, &mut self.doc).is_some() {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
            });
        });

        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.dirty {
                    ui.weak("●");
                }
                if let Some(path) = &self.path {
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                }
                if let Some(msg) = &self.status_msg {
                    ui.colored_label(egui::Color32::from_rgb(0xFF, 0x3B, 0x30), msg);
                }
                ui.separator();
                canvas::status(ui, &mut self.editor, pages);
            });
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ui, |ui| {
                if canvas::show(ui, &mut self.editor, &mut self.doc, pages).is_some() {
                    self.dirty = true;
                    self.layout_dirty = true;
                }
            });
    }
}

/// The block's rows: the shown ones in order, each with a checkbox and up/down
/// arrows, then the hidden ones, which a tick appends at the bottom. The tab
/// cannot be unticked — its cells are what the editor clicks. True on any change.
fn rows_editor(ui: &mut egui::Ui, rows: &mut Vec<Row>) -> bool {
    let mut changed = false;
    let hidden: Vec<Row> = Row::ALL.into_iter().filter(|r| !rows.contains(r)).collect();
    let shown = rows.len();
    for (i, row) in rows.clone().into_iter().chain(hidden).enumerate() {
        ui.horizontal(|ui| {
            let mut on = i < shown;
            let tick = ui.add_enabled(
                row != Row::Tab,
                egui::Checkbox::new(&mut on, row_label(row)),
            );
            if tick.changed() {
                if on {
                    rows.push(row);
                } else {
                    rows.retain(|&r| r != row);
                }
                changed = true;
            }
            if i < shown {
                if ui.add_enabled(i > 0, egui::Button::new("⬆")).clicked() {
                    rows.swap(i, i - 1);
                    changed = true;
                }
                if ui
                    .add_enabled(i + 1 < shown, egui::Button::new("⬇"))
                    .clicked()
                {
                    rows.swap(i, i + 1);
                    changed = true;
                }
            }
        });
    }
    changed
}

fn instrument_label(instrument: Instrument) -> String {
    t(match instrument {
        Instrument::Guitar => "instrument.guitar",
        Instrument::Bass => "instrument.bass",
        Instrument::Ukulele => "instrument.ukulele",
        Instrument::BaritoneGuitar => "instrument.baritone_guitar",
        Instrument::BaritoneUkulele => "instrument.baritone_ukulele",
        Instrument::Banjo => "instrument.banjo",
        Instrument::Mandolin => "instrument.mandolin",
    })
}

fn row_label(row: Row) -> String {
    t(match row {
        Row::Tab => "row.tab",
        Row::Rhythm => "row.rhythm",
        Row::Notation => "row.notation",
        Row::Strum => "row.strum",
        Row::Chords => "row.chords",
    })
}

impl eframe::App for TablaturesApp {
    /// Every close request -- the window's own close button, Alt+F4, Quit from
    /// the menu or its shortcut -- is held while there is unsaved work, and the
    /// unsaved-changes question asked instead. Here rather than in `ui`: eframe
    /// runs only `logic` for a minimised window, so a quit from the dock or the
    /// taskbar would otherwise slip through. The window is brought back so the
    /// question can be seen.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.dirty && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.pending = Some(PendingAction::Quit);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.splash(ui) {
            return;
        }

        // Global keyboard shortcuts: active regardless of which menu (if any) is open.
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_NEW)) {
            self.request_new();
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_OPEN)) {
            self.request_open();
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_SAVE)) {
            self.do_save();
        }
        if ui
            .ctx()
            .input_mut(|i| i.consume_shortcut(&SHORTCUT_SAVE_AS))
        {
            self.do_save_as();
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_EXPORT)) {
            self.do_export();
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_QUIT)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_UNDO))
            && canvas::undo(&mut self.editor, &mut self.doc)
        {
            self.dirty = true;
            self.layout_dirty = true;
        }
        // egui-winit turns Cmd/Ctrl+C, X, V into `Event::Copy`/`Cut`/`Paste` and sends
        // no `Key` event for them, so `consume_shortcut` would never see these three.
        // `Paste` only arrives when the system clipboard holds text, hence
        // `export_clipboard` on every copy/cut. Left alone while a text field (title,
        // author) has focus, so plain text copy/paste keeps working there.
        let (mut copy, mut cut, mut paste) = (false, false, None);
        if !ui.ctx().text_edit_focused() {
            ui.ctx().input_mut(|i| {
                i.events.retain(|e| {
                    match e {
                        egui::Event::Copy => copy = true,
                        egui::Event::Cut => cut = true,
                        egui::Event::Paste(text) => paste = Some(text.clone()),
                        _ => return true,
                    }
                    false
                })
            });
        }
        if copy && canvas::copy(&mut self.editor, &self.doc) {
            self.export_clipboard(ui.ctx());
        }
        if cut && canvas::cut(&mut self.editor, &mut self.doc) {
            self.export_clipboard(ui.ctx());
            self.dirty = true;
            self.layout_dirty = true;
        }
        if let Some(text) = paste {
            // A copy from another GRAT window lands here as JSON; anything else on the
            // system clipboard just triggers a paste of our own clipboard.
            if let Ok(events) = serde_json::from_str::<Vec<grat::model::Event>>(&text) {
                self.editor.clipboard = events;
            }
            if canvas::paste(&mut self.editor, &mut self.doc) {
                self.dirty = true;
                self.layout_dirty = true;
            }
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_REPEAT))
            && canvas::repeat_selection(&mut self.editor, &mut self.doc)
        {
            self.dirty = true;
            self.layout_dirty = true;
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_LIVE)) {
            if self.live.open {
                self.live.exit();
            } else {
                self.live.enter(&self.doc);
            }
        }

        self.egui_menu_bar(ui);

        // Unsaved-changes confirmation, shown for New/Open/Quit while `dirty`.
        if let Some(pending) = self.pending {
            let resp = egui::Modal::new(egui::Id::new("unsaved_modal")).show(ui.ctx(), |ui| {
                ui.set_min_width(320.0);
                ui.heading(t("dialog.unsaved_title"));
                ui.label(t("dialog.unsaved_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("dialog.discard")).clicked() {
                        match pending {
                            PendingAction::New => self.do_new(),
                            PendingAction::Open => self.do_open(),
                            PendingAction::Quit => {
                                // The close request `logic` held back comes round
                                // again; with nothing left unsaved it goes through.
                                self.dirty = false;
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        self.pending = None;
                    }
                    if ui.button(t("dialog.cancel")).clicked() {
                        self.pending = None;
                    }
                });
            });
            if resp.should_close() {
                self.pending = None;
            }
        }

        // A retune over written notes: the tab can stay as written (and sound
        // different) or follow the sound (and be re-fretted).
        if let Some((instrument, tuning)) = self.pending_retune.clone() {
            let resp = egui::Modal::new(egui::Id::new("retune_modal")).show(ui.ctx(), |ui| {
                ui.set_min_width(340.0);
                ui.heading(t("dialog.retune_title"));
                ui.label(t("dialog.retune_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for (key, keep_pitches) in
                        [("dialog.keep_frets", false), ("dialog.keep_pitches", true)]
                    {
                        if ui.button(t(key)).clicked() {
                            self.apply_retune(instrument, tuning.clone(), keep_pitches);
                            self.pending_retune = None;
                        }
                    }
                    if ui.button(t("dialog.cancel")).clicked() {
                        self.pending_retune = None;
                    }
                });
            });
            if resp.should_close() {
                self.pending_retune = None;
            }
        }

        // Custom tuning: each string stepped a semitone at a time, applied on OK
        // through the same keep-frets-or-pitches question as a preset.
        if let Some(draft) = &mut self.tuning_draft {
            let (mut apply, mut close) = (None, false);
            let resp = egui::Modal::new(egui::Id::new("tuning_modal")).show(ui.ctx(), |ui| {
                ui.set_min_width(240.0);
                ui.heading(t("dialog.custom_tuning_title"));
                ui.add_space(4.0);
                egui::Grid::new("tuning_grid").show(ui, |ui| {
                    for (i, midi) in draft.iter_mut().enumerate() {
                        ui.label(format!("{} {}", t("tuning.string"), i + 1));
                        if ui.small_button("−").clicked() {
                            *midi = midi.saturating_sub(1).max(12);
                        }
                        ui.label(i18n::pitch_label(*midi));
                        if ui.small_button("+").clicked() {
                            *midi = (*midi + 1).min(96);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("dialog.ok")).clicked() {
                        apply = Some(draft.clone());
                    }
                    if ui.button(t("dialog.cancel")).clicked() {
                        close = true;
                    }
                });
            });
            if let Some(tuning) = apply {
                self.tuning_draft = None;
                self.request_retune(self.doc.instrument, tuning);
            } else if close || resp.should_close() {
                self.tuning_draft = None;
            }
        }

        // A PDF was just exported: offer to open it, which is how the user reaches
        // the OS print dialog -- there is no in-app print path.
        if let Some(path) = self.pending_open_pdf.clone() {
            let resp = egui::Modal::new(egui::Id::new("open_pdf_modal")).show(ui.ctx(), |ui| {
                ui.set_min_width(300.0);
                ui.heading(t("dialog.open_pdf_title"));
                ui.label(t("dialog.open_pdf_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("dialog.open_pdf")).clicked() {
                        open_file(&path);
                        self.pending_open_pdf = None;
                    }
                    if ui.button(t("dialog.cancel")).clicked() {
                        self.pending_open_pdf = None;
                    }
                });
            });
            if resp.should_close() {
                self.pending_open_pdf = None;
            }
        }

        // About: the logo, name, version, and the authorship/licence line the
        // licence itself requires to stay attached to the software. `Window`
        // (rather than `Modal`) self-guards on `open`, so this can run every frame.
        egui::Window::new(t("menu.about"))
            .id(egui::Id::new("about_window"))
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .scroll(true)
            .show(ui.ctx(), |ui| {
                ui.set_min_width(300.0);
                ui.vertical_centered(|ui| {
                    let logo_resp = ui.image((self.logo.id(), egui::vec2(120.0, 120.0)));
                    if let Some(deco) = &self.today_decoration {
                        canvas::paint_decoration(ui.painter(), logo_resp.rect.center(), 60.0, deco);
                    }
                    ui.heading(t("app.title"));
                    ui.label(EXPANSIONS[0]);
                    ui.label(format!(
                        "{} {}",
                        t("about.version"),
                        env!("CARGO_PKG_VERSION")
                    ));
                    if let Some(deco) = &self.today_decoration {
                        ui.label(t(deco.caption_key));
                    }
                });
                ui.add_space(8.0);
                ui.label(t("about.tagline"));
                ui.separator();
                ui.label(t("about.author_line"));
                ui.hyperlink_to(t("about.license"), LICENSE_URL);
            });

        // Help: the legend of technique colours -- the single most useful thing on
        // this page, since the colours ARE the notation and nothing else explains
        // them -- plus the shortcuts, the block rows, and the per-event/per-bar
        // spans (palm mute, let ring, repeats).
        egui::Window::new(t("menu.help"))
            .id(egui::Id::new("help_window"))
            .open(&mut self.show_help)
            .collapsible(false)
            .default_size(egui::vec2(420.0, 520.0))
            .scroll(true)
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.image((self.logo.id(), egui::vec2(96.0, 96.0)));
                });
                ui.add_space(8.0);

                ui.heading(t("help.legend_title"));
                for &(tech, key, _) in TECH_LEGEND {
                    ui.horizontal(|ui| {
                        let (swatch, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(swatch, 3.0, canvas::rgb(technique_color(&tech)));
                        ui.label(t(key));
                    });
                }

                ui.add_space(8.0);
                ui.separator();
                ui.heading(t("help.shortcuts_title"));
                for (label_key, sc) in [
                    ("menu.new", &SHORTCUT_NEW),
                    ("menu.open", &SHORTCUT_OPEN),
                    ("menu.save", &SHORTCUT_SAVE),
                    ("menu.save_as", &SHORTCUT_SAVE_AS),
                    ("menu.export_pdf", &SHORTCUT_EXPORT),
                    ("tool.undo", &SHORTCUT_UNDO),
                    ("menu.copy", &SHORTCUT_COPY),
                    ("menu.cut", &SHORTCUT_CUT),
                    ("menu.paste", &SHORTCUT_PASTE),
                    ("menu.repeat_selection", &SHORTCUT_REPEAT),
                    ("menu.live", &SHORTCUT_LIVE),
                    ("menu.quit", &SHORTCUT_QUIT),
                ] {
                    ui.label(format!(
                        "{} — {}",
                        ui.ctx().format_shortcut(sc),
                        t(label_key)
                    ));
                }
                for key in [
                    "help.key_digits",
                    "help.key_backspace",
                    "help.key_space",
                    "help.key_arrows",
                    "help.key_range_select",
                    "help.key_duration",
                    "help.key_zoom",
                    "help.key_live",
                ] {
                    ui.label(t(key));
                }

                ui.add_space(8.0);
                ui.separator();
                ui.heading(t("help.rows_title"));
                ui.label(t("help.rows_body"));
                for (row, key) in [
                    (Row::Tab, "help.row_tab_desc"),
                    (Row::Rhythm, "help.row_rhythm_desc"),
                    (Row::Notation, "help.row_notation_desc"),
                    (Row::Strum, "help.row_strum_desc"),
                    (Row::Chords, "help.row_chords_desc"),
                ] {
                    ui.label(format!("{} — {}", row_label(row), t(key)));
                }

                ui.add_space(8.0);
                ui.separator();
                ui.heading(t("help.spans_title"));
                ui.label(t("help.spans_body"));
            });

        // The live player takes the window over: the editor's toolbar, palette,
        // status bar and page view all stand down while it holds it.
        if self.live.open {
            live::show(ui, &mut self.live, &self.doc);
        } else {
            self.editor_ui(ui);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(i18n::LANG_STORAGE_KEY, i18n::current_lang());
        storage.set_string(TECH_SHOWN_STORAGE_KEY, self.editor.tech_shown.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App as _;

    /// The app run headless, one frame at a time, with each frame painted into
    /// an image: egui's own meshes and font atlas, rasterised here. It is how the
    /// UI gets looked at without a window (`make run` blocks an agent session):
    /// `cargo test --bin grat ui_screenshots -- --ignored` writes `dist/ui-*.png`.
    struct Shooter {
        ctx: egui::Context,
        app: TablaturesApp,
        size: egui::Vec2,
        time: f64,
        textures: std::collections::HashMap<egui::TextureId, egui::ColorImage>,
        last: Option<egui::FullOutput>,
    }

    impl Shooter {
        fn new(doc: Document) -> Shooter {
            let ctx = egui::Context::default();
            let (rgba, w, h) = load_logo_rgba();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = TablaturesApp::new(&cc, rgba, w, h, None);
            app.splash_until = None;
            app.doc = doc;
            let mut shooter = Shooter {
                ctx,
                app,
                size: egui::vec2(1100.0, 760.0),
                time: 0.0,
                textures: Default::default(),
                last: None,
            };
            shooter.frame(Vec::new());
            shooter.frame(Vec::new());
            shooter
        }

        /// Run one frame with `events` as its input, keeping the font atlas up to date.
        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let mut out = self.ctx.run_ui(input, |ui| {
                let mut frame = eframe::Frame::_new_kittest();
                self.app.logic(ui.ctx(), &mut frame);
                self.app.ui(ui, &mut frame);
            });
            for (id, deltas) in out.textures_delta.set.drain() {
                for delta in deltas {
                    let egui::ImageData::Color(patch) = delta.image;
                    let Some([x0, y0]) = delta.pos else {
                        self.textures.insert(id, (*patch).clone());
                        continue;
                    };
                    let tex = self
                        .textures
                        .get_mut(&id)
                        .expect("patch of a known texture");
                    for y in 0..patch.size[1] {
                        for x in 0..patch.size[0] {
                            tex.pixels[(y0 + y) * tex.size[0] + x0 + x] =
                                patch.pixels[y * patch.size[0] + x];
                        }
                    }
                }
            }
            out.textures_delta.clear();
            self.last = Some(out);
        }

        /// Where the last frame painted exactly `needle`: its centre.
        fn text(&self, needle: &str) -> egui::Pos2 {
            fn find(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::Shape::Text(t) if t.galley.text() == needle => {
                        Some(t.pos + t.galley.rect.center().to_vec2())
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, needle)),
                    _ => None,
                }
            }
            let out = self.last.as_ref().expect("a frame ran");
            out.shapes
                .iter()
                .find_map(|c| find(&c.shape, needle))
                .unwrap_or_else(|| panic!("no text {needle:?} on screen"))
        }

        /// Click (`button`) at `pos`: move there, press and release, a frame each.
        fn click(
            &mut self,
            pos: egui::Pos2,
            button: egui::PointerButton,
            modifiers: egui::Modifiers,
        ) {
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                self.frame(vec![egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                }]);
            }
            self.frame(Vec::new());
        }

        /// Paint the last frame and write it to `dist/ui-<name>.png`.
        fn shoot(&mut self, name: &str) {
            let out = self.last.take().expect("a frame ran");
            let (w, h) = (self.size.x as usize, self.size.y as usize);
            let mut fb = vec![[0u8, 0, 0, 255]; w * h];
            for prim in self.ctx.tessellate(out.shapes.clone(), 1.0) {
                let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else {
                    continue;
                };
                let tex = &self.textures[&mesh.texture_id];
                for tri in mesh.indices.chunks(3) {
                    let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[tri[k] as usize]);
                    raster(&mut fb, w, h, prim.clip_rect, tex, [a, b, c]);
                }
            }
            self.last = Some(out);
            let bytes: Vec<u8> = fb.into_iter().flatten().collect();
            std::fs::create_dir_all("dist").unwrap();
            image::RgbaImage::from_raw(w as u32, h as u32, bytes)
                .unwrap()
                .save(format!("dist/ui-{name}.png"))
                .unwrap();
        }
    }

    /// One textured, coloured triangle, blended over `fb` the way egui blends:
    /// premultiplied alpha, in gamma space.
    fn raster(
        fb: &mut [[u8; 4]],
        w: usize,
        h: usize,
        clip: egui::Rect,
        tex: &egui::ColorImage,
        v: [egui::epaint::Vertex; 3],
    ) {
        let edge = |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| {
            (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
        };
        let area = edge(v[0].pos, v[1].pos, v[2].pos);
        if area.abs() < 1e-9 {
            return;
        }
        let lo = v.iter().fold(clip.max, |m, x| m.min(x.pos)).max(clip.min);
        let hi = v.iter().fold(clip.min, |m, x| m.max(x.pos)).min(clip.max);
        let (x0, y0) = (
            lo.x.floor().max(0.0) as usize,
            lo.y.floor().max(0.0) as usize,
        );
        let (x1, y1) = ((hi.x.ceil() as usize).min(w), (hi.y.ceil() as usize).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                let wt = [
                    edge(v[1].pos, v[2].pos, p) / area,
                    edge(v[2].pos, v[0].pos, p) / area,
                    edge(v[0].pos, v[1].pos, p) / area,
                ];
                if wt.iter().any(|&k| k < 0.0) || !clip.contains(p) {
                    continue;
                }
                let uv = v[0].uv.to_vec2() * wt[0]
                    + v[1].uv.to_vec2() * wt[1]
                    + v[2].uv.to_vec2() * wt[2];
                let tx = ((uv.x * tex.size[0] as f32) as usize).min(tex.size[0] - 1);
                let ty = ((uv.y * tex.size[1] as f32) as usize).min(tex.size[1] - 1);
                let texel = tex.pixels[ty * tex.size[0] + tx].to_array();
                let src: [f32; 4] = std::array::from_fn(|c| {
                    let colour: f32 = (0..3)
                        .map(|k| v[k].color.to_array()[c] as f32 * wt[k])
                        .sum();
                    colour * texel[c] as f32 / 255.0
                });
                let dst = &mut fb[y * w + x];
                for c in 0..4 {
                    dst[c] = (src[c] + dst[c] as f32 * (1.0 - src[3] / 255.0))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    #[test]
    fn a_right_click_inside_the_selection_keeps_it() {
        let mut doc = Document::new_empty();
        for (bar, fret) in [(0, 5), (3, 7)] {
            doc.bars[bar].events[0].notes.push(grat::model::Note {
                string: 1,
                fret,
                tech: Technique::Plain,
                tie_next: false,
            });
        }
        let mut shot = Shooter::new(doc);
        let range = (
            Some(canvas::Sel {
                bar: 1,
                event: 3,
                string: 0,
            }),
            Some(canvas::Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
        );
        (shot.app.editor.selected, shot.app.editor.range_anchor) = range;
        let inside = shot.text("5");
        shot.click(
            inside,
            egui::PointerButton::Secondary,
            egui::Modifiers::NONE,
        );
        assert_eq!(
            (shot.app.editor.selected, shot.app.editor.range_anchor),
            range
        );

        let outside = shot.text("7");
        shot.click(
            outside,
            egui::PointerButton::Secondary,
            egui::Modifiers::NONE,
        );
        assert_eq!(shot.app.editor.range_anchor, None);
        assert_eq!(
            shot.app.editor.selected.map(|s| (s.bar, s.event)),
            Some((3, 0))
        );
    }

    #[test]
    #[ignore]
    fn ui_screenshots() {
        let mut doc = Document::new_empty();
        doc.title = "Screenshot".into();
        doc.bars[0].events[0].notes.push(grat::model::Note {
            string: 1,
            fret: 5,
            tech: Technique::Plain,
            tie_next: false,
        });
        let mut shot = Shooter::new(doc);
        // Bars 2 to 3 selected, repeated three times, then the Bar menu opened.
        shot.app.editor.selected = Some(canvas::Sel {
            bar: 2,
            event: 3,
            string: 0,
        });
        shot.app.editor.range_anchor = Some(canvas::Sel {
            bar: 1,
            event: 0,
            string: 0,
        });
        assert!(canvas::repeat_selection(
            &mut shot.app.editor,
            &mut shot.app.doc
        ));
        shot.app.doc.bars[2].repeat_end = Some(3);
        shot.app.layout_dirty = true;
        shot.frame(Vec::new());
        shot.shoot("editor");
        let menu = shot.text(&t("menu.bar"));
        shot.click(menu, egui::PointerButton::Primary, egui::Modifiers::NONE);
        shot.shoot("bar-menu");
        shot.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        shot.app.editor.selected = Some(canvas::Sel {
            bar: 0,
            event: 3,
            string: 0,
        });
        shot.app.editor.range_anchor = Some(canvas::Sel {
            bar: 0,
            event: 0,
            string: 0,
        });
        let cell = shot.text("5");
        shot.click(cell, egui::PointerButton::Secondary, egui::Modifiers::NONE);
        shot.shoot("right-click");
    }

    /// One close request, as the window's close button sends it, run through
    /// `logic` the way eframe does for a minimised window. True if it was held.
    fn close_is_held(ctx: &egui::Context, app: &mut TablaturesApp) -> bool {
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let out = ctx.run_logic(&input, |ctx| {
            app.logic(ctx, &mut eframe::Frame::_new_kittest())
        });
        out.viewport_commands
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|c| c.contains(&egui::ViewportCommand::CancelClose))
    }

    #[test]
    fn closing_the_window_over_unsaved_work_asks_first() {
        let ctx = egui::Context::default();
        let (rgba, w, h) = load_logo_rgba();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = TablaturesApp::new(&cc, rgba, w, h, None);

        assert!(!close_is_held(&ctx, &mut app), "nothing to lose: it closes");
        assert!(app.pending.is_none());

        app.dirty = true;
        assert!(close_is_held(&ctx, &mut app));
        assert!(matches!(app.pending, Some(PendingAction::Quit)));
    }

    #[test]
    fn every_technique_has_exactly_one_legend_entry() {
        // No wildcard arm: a new `Technique` stops this compiling until it gets a
        // number here, and then the count below asks for its `TECH_LEGEND` row --
        // which is its palette button, its Help line and its note-styles entry.
        fn number(t: Technique) -> usize {
            match t {
                Technique::Plain => 0,
                Technique::HammerOn => 1,
                Technique::PullOff => 2,
                Technique::Slide => 3,
                Technique::SlideShift => 4,
                Technique::SlideIn { .. } => 5,
                Technique::Grace => 6,
                Technique::Bend { .. } => 7,
                Technique::BendRelease { .. } => 8,
                Technique::PreBend { .. } => 9,
                Technique::Vibrato => 10,
                Technique::WideVibrato => 11,
                Technique::Harmonic => 12,
                Technique::PinchHarmonic => 13,
                Technique::Tap => 14,
                Technique::Slap => 15,
                Technique::Pop => 16,
                Technique::Dead => 17,
                Technique::Ghost => 18,
                Technique::Trill { .. } => 19,
            }
        }
        let mut seen: Vec<usize> = TECH_LEGEND.iter().map(|&(t, _, _)| number(t)).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn the_pdf_name_is_the_title_as_a_file_system_takes_it() {
        assert_eq!(pdf_file_name(""), "untitled.pdf");
        assert_eq!(pdf_file_name("  ...  "), "untitled.pdf");
        assert_eq!(pdf_file_name("AC/DC: Live?"), "AC-DC- Live-.pdf");
        assert_eq!(pdf_file_name(" .Été. "), "Été.pdf");
    }

    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("grat-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("song.gtab");
        let tmp = dir.join("song.gtab.tmp");

        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!tmp.exists());

        let missing = dir.join("no-such-folder").join("song.gtab");
        assert!(write_atomic(&missing, b"x").is_err());
        assert!(!dir.join("no-such-folder").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
