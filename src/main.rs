//! Tablatures desktop app: window, theme, menu, and file/CLI wiring.
//!
//! The document model and i18n live in the `strungin` library crate (see `src/lib.rs`,
//! `src/model.rs`, `src/i18n.rs`); this binary only adds the eframe GUI shell and the
//! `--export` CLI entry point on top of it.

mod canvas;

use std::path::PathBuf;

use eframe::egui;
use strungin::i18n::{self, t};
use strungin::model::{technique_color, BlockModel, Document, LoadError, StaffOrder, Technique};

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

/// Every playing technique with its i18n key, for the Help legend. One representative
/// field value per variant, same choices as `tests/visual.rs::technique_bar`.
const TECH_LEGEND: &[(Technique, &str)] = &[
    (Technique::Plain, "tech.plain"),
    (Technique::HammerOn, "tech.hammer_on"),
    (Technique::PullOff, "tech.pull_off"),
    (Technique::Slide, "tech.slide"),
    (Technique::SlideShift, "tech.slide_shift"),
    (Technique::SlideIn { from_fret: 3 }, "tech.slide_in"),
    (Technique::Grace, "tech.grace"),
    (Technique::Bend { quarters: 4 }, "tech.bend"),
    (Technique::BendRelease { quarters: 2 }, "tech.bend_release"),
    (Technique::PreBend { quarters: 4 }, "tech.pre_bend"),
    (Technique::Vibrato, "tech.vibrato"),
    (Technique::WideVibrato, "tech.wide_vibrato"),
    (Technique::Harmonic, "tech.harmonic"),
    (Technique::PinchHarmonic, "tech.pinch_harmonic"),
    (Technique::Tap, "tech.tap"),
    (Technique::Slap, "tech.slap"),
    (Technique::Pop, "tech.pop"),
    (Technique::Dead, "tech.dead"),
    (Technique::Ghost, "tech.ghost"),
    (Technique::Trill { to_fret: 9 }, "tech.trill"),
];

/// Decode and downscale the cover artwork exactly once. Shared by the window icon
/// (built here, before the app exists) and the About/Help texture (built once in
/// `TablaturesApp::new` from the same bytes, and kept for the app's lifetime rather
/// than redecoded per frame): the full frame is 4 MB of RGBA for something drawn at
/// a fraction of that.
fn load_cover_rgba() -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory_with_format(
        include_bytes!("assets/cover.jpg"),
        image::ImageFormat::Jpeg,
    )
    .expect("bundled cover.jpg must decode")
    .thumbnail(256, 256)
    .to_rgba8();
    let (w, h) = (img.width(), img.height());
    (img.into_raw(), w, h)
}

/// `strungin --export <in.gtab> <out.pdf>`: load, export, write, no window. Shared
/// entry point for the CLI flag and `make examples`.
fn run_export(input: &str, output: &str) -> Result<(), String> {
    let doc: Document = std::fs::read_to_string(input)
        .ok()
        .and_then(|s| Document::from_json(&s).ok())
        .ok_or_else(|| format!("{}: {input}", t("error.load")))?;
    let bytes = strungin::pdf::export(&doc);
    std::fs::write(output, bytes).map_err(|_| format!("{}: {output}", t("error.save")))
}

/// Open a file in the OS's default viewer -- for a PDF, this is how the user reaches
/// the real print dialog, without this app needing its own print path.
fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();
}

fn main() -> eframe::Result {
    // CLI mode: `strungin --export <in.gtab> <out.pdf>`. Never opens a window.
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

    let (cover_rgba, cover_w, cover_h) = load_cover_rgba();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([640.0, 480.0])
            .with_icon(egui::IconData {
                rgba: cover_rgba.clone(),
                width: cover_w,
                height: cover_h,
            }),
        ..Default::default()
    };

    eframe::run_native(
        &t("app.title"),
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(TablaturesApp::new(
                cc, cover_rgba, cover_w, cover_h,
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
    pending: Option<PendingAction>,
    show_about: bool,
    show_help: bool,
    /// A PDF was just exported to this path; offer to open it (which is how the
    /// user reaches the OS print dialog).
    pending_open_pdf: Option<PathBuf>,
    /// Last error or informational notice, shown in the status bar.
    status_msg: Option<String>,
    editor: canvas::EditorState,
    /// Cover artwork, decoded once and kept as a texture for About/Help.
    cover: egui::TextureHandle,
}

impl TablaturesApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        cover_rgba: Vec<u8>,
        cover_w: u32,
        cover_h: u32,
    ) -> Self {
        if let Some(storage) = cc.storage {
            if let Some(lang) = storage.get_string(i18n::LANG_STORAGE_KEY) {
                i18n::set_lang(&lang);
            }
        }
        style_context(&cc.egui_ctx);
        let cover_image = egui::ColorImage::from_rgba_unmultiplied(
            [cover_w as usize, cover_h as usize],
            &cover_rgba,
        );
        let cover = cc
            .egui_ctx
            .load_texture("cover", cover_image, egui::TextureOptions::default());
        TablaturesApp {
            doc: Document::new_empty(),
            path: None,
            dirty: false,
            pending: None,
            show_about: false,
            show_help: false,
            pending_open_pdf: None,
            status_msg: None,
            editor: canvas::EditorState::default(),
            cover,
        }
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

    fn request_quit(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.pending = Some(PendingAction::Quit);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn do_new(&mut self) {
        self.doc = Document::new_empty();
        self.path = None;
        self.dirty = false;
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
                self.doc = doc;
                self.path = Some(path);
                self.dirty = false;
                self.status_msg = None;
            }
            Some(Err(LoadError::TooNew(_))) => self.status_msg = Some(t("error.load_too_new")),
            Some(Err(LoadError::Parse)) | None => self.status_msg = Some(t("error.load")),
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
        match std::fs::write(&path, json) {
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
        let stem = self.doc.title.trim();
        let default_name = format!("{}.pdf", if stem.is_empty() { "untitled" } else { stem });
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("dialog.pdf_filter"), &["pdf"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, strungin::pdf::export(&self.doc)) {
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
                        self.request_quit(ui.ctx());
                        ui.close();
                    }
                });

                ui.menu_button(t("menu.edit"), |ui| {
                    // Input mode: whether an explicit duration change pushes the
                    // rest of the piece forward or absorbs locally. The status
                    // bar's "INS" light reflects this.
                    ui.label(t("menu.input_mode"));
                    if ui
                        .selectable_label(self.editor.shift_following, t("tool.shift_following"))
                        .clicked()
                    {
                        self.editor.shift_following = true;
                        ui.close();
                    }
                    if ui
                        .selectable_label(!self.editor.shift_following, t("tool.keep_following"))
                        .clicked()
                    {
                        self.editor.shift_following = false;
                        ui.close();
                    }
                });

                ui.menu_button(t("menu.view"), |ui| {
                    ui.menu_button(t("menu.language"), |ui| {
                        for lang in i18n::available_langs() {
                            let checked = i18n::current_lang() == lang;
                            if ui.selectable_label(checked, lang.to_uppercase()).clicked() {
                                i18n::set_lang(lang);
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
}

fn model_label(model: BlockModel) -> String {
    match model {
        BlockModel::OneLine => t("model.one_line"),
        BlockModel::TwoLine => t("model.two_line"),
        BlockModel::ThreeLine => t("model.three_line"),
    }
}

fn staff_order_label(order: StaffOrder) -> String {
    match order {
        StaffOrder::TabFirst => t("staff_order.tab_first"),
        StaffOrder::NotationFirst => t("staff_order.notation_first"),
    }
}

impl eframe::App for TablaturesApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            self.request_quit(ui.ctx());
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_UNDO))
            && canvas::undo(&mut self.editor, &mut self.doc)
        {
            self.dirty = true;
        }

        self.egui_menu_bar(ui);

        // Keep a blank line ready under the music: as soon as the last one is
        // written on, the next appears. Not marked dirty -- the appended bars are
        // empty scaffolding, and the invariant re-establishes itself on load.
        strungin::layout::ensure_trailing_blank_system(&mut self.doc);

        // ponytail: re-paginated every frame rather than cached and invalidated on
        // edit -- simplest correct thing for a desktop editor's document sizes;
        // revisit with a dirty-flag cache if a very large score ever feels laggy.
        let pages = strungin::layout::paginate(&self.doc);

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(t("field.title"));
                if ui
                    .add(egui::TextEdit::singleline(&mut self.doc.title).desired_width(140.0))
                    .changed()
                {
                    self.dirty = true;
                }
                ui.label(t("field.author"));
                if ui
                    .add(egui::TextEdit::singleline(&mut self.doc.author).desired_width(120.0))
                    .changed()
                {
                    self.dirty = true;
                }
                ui.separator();
                ui.label(t("field.tempo"));
                if ui
                    .add(egui::DragValue::new(&mut self.doc.tempo).range(1..=400))
                    .changed()
                {
                    self.dirty = true;
                }
                ui.label(t("field.capo"));
                if ui
                    .add(egui::DragValue::new(&mut self.doc.capo).range(0..=12))
                    .changed()
                {
                    self.dirty = true;
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
                }
                ui.separator();
                egui::ComboBox::from_id_salt("block_model")
                    .selected_text(model_label(self.doc.model))
                    .show_ui(ui, |ui| {
                        for m in [
                            BlockModel::OneLine,
                            BlockModel::TwoLine,
                            BlockModel::ThreeLine,
                        ] {
                            if ui
                                .selectable_value(&mut self.doc.model, m, model_label(m))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("staff_order")
                    .selected_text(staff_order_label(self.doc.staff_order))
                    .show_ui(ui, |ui| {
                        for o in [StaffOrder::TabFirst, StaffOrder::NotationFirst] {
                            if ui
                                .selectable_value(
                                    &mut self.doc.staff_order,
                                    o,
                                    staff_order_label(o),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                    });
            });
            ui.add_space(4.0);
        });

        egui::Panel::left("tool_palette").show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if canvas::palette(ui, &mut self.editor, &mut self.doc).is_some() {
                    self.dirty = true;
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
                canvas::status(ui, &mut self.editor, &pages);
            });
        });

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

        // About: the cover artwork, name, version, and the authorship/licence line
        // the licence itself requires to stay attached to the software. `Window`
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
                    ui.image((self.cover.id(), egui::vec2(120.0, 120.0)));
                    ui.heading(t("app.title"));
                    ui.label(format!(
                        "{} {}",
                        t("about.version"),
                        env!("CARGO_PKG_VERSION")
                    ));
                });
                ui.add_space(8.0);
                ui.label(t("about.tagline"));
                ui.separator();
                ui.label(t("about.author_line"));
                ui.label(t("about.license"));
            });

        // Help: the legend of technique colours -- the single most useful thing on
        // this page, since the colours ARE the notation and nothing else explains
        // them -- plus the shortcuts, the block models, and the per-event/per-bar
        // spans (palm mute, let ring, repeats).
        egui::Window::new(t("menu.help"))
            .id(egui::Id::new("help_window"))
            .open(&mut self.show_help)
            .collapsible(false)
            .default_size(egui::vec2(420.0, 520.0))
            .scroll(true)
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.image((self.cover.id(), egui::vec2(96.0, 96.0)));
                });
                ui.add_space(8.0);

                ui.heading(t("help.legend_title"));
                for &(tech, key) in TECH_LEGEND {
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
                    "help.key_zoom",
                ] {
                    ui.label(t(key));
                }

                ui.add_space(8.0);
                ui.separator();
                ui.heading(t("help.models_title"));
                ui.label(format!(
                    "{} — {}",
                    t("model.one_line"),
                    t("help.model_one_desc")
                ));
                ui.label(format!(
                    "{} — {}",
                    t("model.two_line"),
                    t("help.model_two_desc")
                ));
                ui.label(format!(
                    "{} — {}",
                    t("model.three_line"),
                    t("help.model_three_desc")
                ));

                ui.add_space(8.0);
                ui.separator();
                ui.heading(t("help.spans_title"));
                ui.label(t("help.spans_body"));
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ui, |ui| {
                if canvas::show(ui, &mut self.editor, &mut self.doc, &pages).is_some() {
                    self.dirty = true;
                }
            });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(i18n::LANG_STORAGE_KEY, i18n::current_lang());
    }
}
