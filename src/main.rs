//! Tablatures desktop app: window, theme, menu, and file/CLI wiring.
//!
//! The document model and i18n live in the `tablatures` library crate (see `src/lib.rs`,
//! `src/model.rs`, `src/i18n.rs`); this binary only adds the eframe GUI shell and the
//! `--export` CLI entry point on top of it.

use std::path::PathBuf;

use eframe::egui;
use tablatures::i18n::{self, t};
use tablatures::model::{BlockModel, Document};

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

fn main() -> eframe::Result {
    // CLI mode: `tablatures --export <in.gtab> <out.pdf>`. Never opens a window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--export") {
        match (args.get(pos + 1), args.get(pos + 2)) {
            (Some(_input), Some(_output)) => eprintln!("{}", t("export.not_ready")),
            _ => eprintln!("usage: tablatures --export <in.gtab> <out.pdf>"),
        }
        std::process::exit(1);
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        &t("app.title"),
        native_options,
        Box::new(|cc| Ok(Box::new(TablaturesApp::new(cc)))),
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
    /// Last error or informational notice, shown in the status bar.
    status_msg: Option<String>,
}

impl TablaturesApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            if let Some(lang) = storage.get_string(i18n::LANG_STORAGE_KEY) {
                i18n::set_lang(&lang);
            }
        }
        style_context(&cc.egui_ctx);
        TablaturesApp {
            doc: Document::new_empty(),
            path: None,
            dirty: false,
            pending: None,
            show_about: false,
            status_msg: None,
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
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            Some(doc) => {
                self.doc = doc;
                self.path = Some(path);
                self.dirty = false;
                self.status_msg = None;
            }
            None => self.status_msg = Some(t("error.load")),
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
        let Ok(json) = serde_json::to_string_pretty(&self.doc) else {
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
}

fn model_label(model: BlockModel) -> String {
    match model {
        BlockModel::OneLine => t("model.one_line"),
        BlockModel::TwoLine => t("model.two_line"),
        BlockModel::ThreeLine => t("model.three_line"),
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
            self.status_msg = Some(t("export.not_ready"));
        }
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_QUIT)) {
            self.request_quit(ui.ctx());
        }

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
                        // Wired up but inert until phase 6 lands the PDF backend.
                        self.status_msg = Some(t("export.not_ready"));
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

        // About box: dismissed via click-outside or Esc, like any egui modal.
        if self.show_about {
            let resp = egui::Modal::new(egui::Id::new("about_modal")).show(ui.ctx(), |ui| {
                ui.set_min_width(280.0);
                ui.heading(t("menu.about"));
                ui.label(t("about.body"));
            });
            if resp.should_close() {
                self.show_about = false;
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(t("app.title"));
            ui.add_space(8.0);

            egui::Grid::new("doc_fields")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("field.title"));
                    if ui.text_edit_singleline(&mut self.doc.title).changed() {
                        self.dirty = true;
                    }
                    ui.end_row();

                    ui.label(t("field.author"));
                    if ui.text_edit_singleline(&mut self.doc.author).changed() {
                        self.dirty = true;
                    }
                    ui.end_row();

                    ui.label(t("field.tempo"));
                    if ui
                        .add(egui::DragValue::new(&mut self.doc.tempo).range(1..=400))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.end_row();

                    ui.label(t("field.capo"));
                    if ui
                        .add(egui::DragValue::new(&mut self.doc.capo).range(0..=12))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.end_row();
                });

            ui.add_space(12.0);
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
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(i18n::LANG_STORAGE_KEY, i18n::current_lang());
    }
}
