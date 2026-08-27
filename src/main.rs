//! Tablatures desktop app: window, theme, and menu shell.
//!
//! Phase 1: the window, theme, full menu (with shortcuts shown and active), and i18n are
//! in place, but the file/edit actions are not wired to real behaviour yet — that lands
//! in phase 2 once `model::Document` exists (see `src/model.rs`, coming next).

use eframe::egui;
use tablatures::i18n::{self, t};

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

struct TablaturesApp {
    show_about: bool,
}

impl TablaturesApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        style_context(&cc.egui_ctx);
        TablaturesApp { show_about: false }
    }
}

impl eframe::App for TablaturesApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Quit is the only action already wired: there is no document/dirty state yet
        // to guard it with a confirmation, so it can just close the window.
        if ui.ctx().input_mut(|i| i.consume_shortcut(&SHORTCUT_QUIT)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
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

                    // Not wired up yet: New/Open/Save/Save As land in phase 2 together
                    // with `model::Document`; Export PDF lands in phase 6.
                    ui.add_enabled(
                        false,
                        egui::Button::new(t("menu.new")).shortcut_text(sc_new),
                    );
                    ui.add_enabled(
                        false,
                        egui::Button::new(t("menu.open")).shortcut_text(sc_open),
                    );
                    ui.separator();
                    ui.add_enabled(
                        false,
                        egui::Button::new(t("menu.save")).shortcut_text(sc_save),
                    );
                    ui.add_enabled(
                        false,
                        egui::Button::new(t("menu.save_as")).shortcut_text(sc_save_as),
                    );
                    ui.separator();
                    ui.add_enabled(
                        false,
                        egui::Button::new(t("menu.export_pdf")).shortcut_text(sc_export),
                    );
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
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
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
        });
    }
}
