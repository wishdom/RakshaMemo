use eframe::egui::{self, RichText};
use rakshamemo::platform;
use zeroize::Zeroizing;

use super::{Dialog, Notepad, ornament, style};
use crate::i18n::{fill, t};
use rakshamemo::settings::Language;

#[cfg(test)]
mod tests;

impl Notepad {
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        style::page(ui);
        let text = t();
        egui::ScrollArea::vertical()
            .id_salt("settings-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(920.0));
                ornament::title(ui, text.settings_title, 24.0);
                ui.weak(text.settings_intro);
                ui.add_space(12.0);

                section(ui, text.appearance, |ui| {
                    let mut language = crate::i18n::language();
                    setting_row(ui, text.language, text.language_description, |ui| {
                        for option in Language::ALL {
                            ui.selectable_value(&mut language, option, option.native_name());
                        }
                    });
                    self.set_language(language);
                    row_separator(ui);
                    let mut dark = self.settings.dark_mode;
                    setting_row(ui, text.theme, text.theme_description, |ui| {
                        ui.selectable_value(&mut dark, false, text.light);
                        ui.selectable_value(&mut dark, true, text.dark);
                    });
                    self.set_dark_mode(ui.ctx(), dark);
                    row_separator(ui);
                    let mut clock_24h = self.settings.clock_24h;
                    setting_row(ui, text.clock_setting, text.clock_description, |ui| {
                        ui.selectable_value(&mut clock_24h, true, text.hours_24);
                        ui.selectable_value(&mut clock_24h, false, text.hours_12);
                    });
                    if clock_24h != self.settings.clock_24h {
                        self.settings.clock_24h = clock_24h;
                        self.persist_settings();
                    }
                });

                section(ui, text.startup_and_security, |ui| {
                    let mut startup = self.startup_enabled;
                    let description = if self.custom_path {
                        text.launch_custom_vault
                    } else {
                        text.launch_description
                    };
                    setting_row(ui, text.launch_at_sign_in, description, |ui| {
                        if ui
                            .add_enabled(
                                !self.custom_path,
                                egui::Checkbox::new(&mut startup, text.enabled),
                            )
                            .changed()
                        {
                            match platform::startup::set_enabled(startup) {
                                Ok(()) => {
                                    self.startup_enabled = startup;
                                    self.status = text.startup_updated.into();
                                }
                                Err(error) => {
                                    self.error = Some(fill(
                                        text.could_not_update_startup,
                                        &[&format!("{error:#}")],
                                    ));
                                }
                            }
                        }
                    });
                    if !self.custom_path {
                        ui.add_space(4.0);
                        ui.weak(text.moved_app_hint);
                    }
                    row_separator(ui);
                    setting_row(
                        ui,
                        text.automatic_lock,
                        text.automatic_lock_description,
                        |ui| {
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.settings.auto_lock_minutes)
                                        .range(0..=1440)
                                        .suffix(text.minutes_suffix)
                                        .speed(1.0),
                                )
                                .on_hover_text(text.drag_hover)
                                .changed()
                            {
                                self.persist_settings();
                            }
                        },
                    );
                    row_separator(ui);
                    let away = if cfg!(target_os = "windows") {
                        text.lock_when_away_windows
                    } else {
                        text.lock_when_away_other
                    };
                    setting_row(ui, text.lock_when_away, away, |ui| {
                        if ui
                            .checkbox(&mut self.settings.lock_when_away, text.lock_automatically)
                            .changed()
                        {
                            self.persist_settings();
                        }
                    });
                    row_separator(ui);
                    setting_row(
                        ui,
                        text.master_password,
                        text.master_password_description,
                        |ui| {
                            if ui.button(text.change_password_ellipsis).clicked() {
                                self.dialog = Some(Dialog::ChangePassword(Box::default()));
                            }
                        },
                    );
                    row_separator(ui);
                    ui.label(RichText::new(text.clipboard_protection).strong());
                    ui.weak(text.clipboard_description);
                });

                section(ui, text.vault_and_backups, |ui| {
                    setting_row(ui, text.export_backup, text.export_description, |ui| {
                        if ui.button(text.export_vault).clicked() {
                            self.export_snapshot();
                        }
                    });
                    row_separator(ui);
                    setting_row(ui, text.replace_vault, text.replace_description, |ui| {
                        if style::danger_button(ui, text.import_vault).clicked()
                            && self.save_now()
                            && let Some(path) = {
                                let path = rfd::FileDialog::new()
                                    .add_filter(text.encrypted_vault_filter, &["json", "bak"])
                                    .pick_file();
                                self.after_blocking_dialog();
                                path
                            }
                        {
                            self.dialog = Some(Dialog::Import {
                                path,
                                password: Zeroizing::new(String::new()),
                            });
                        }
                    });
                    row_separator(ui);
                    setting_row(ui, text.restore_previous, text.restore_description, |ui| {
                        if ui.button(text.choose_version).clicked()
                            && let Some(store) = &self.store
                        {
                            self.dialog = Some(Dialog::Versions {
                                versions: store.versions(),
                                selected: None,
                            });
                        }
                    });
                    row_separator(ui);
                    ui.label(RichText::new(text.vault_location).strong());
                    path_label(ui, &self.vault_path);
                    egui::CollapsingHeader::new(text.backup_details)
                        .id_salt("vault-storage-details")
                        .show(ui, |ui| {
                            if let Some(store) = &self.store {
                                ui.label(RichText::new(text.previous_version).strong());
                                path_label(ui, &store.backup_path());
                                ui.add_space(4.0);
                                ui.label(fill(text.created, &[&store.data.meta.created_at]));
                                ui.label(fill(text.last_saved, &[&store.data.meta.modified_at]));
                            }
                            ui.weak(text.encryption);
                        });
                });

                ui.weak(fill(text.shortcuts_footer, &[&env!("CARGO_PKG_VERSION")]));
                ui.add_space(12.0);
            });
    }
}

fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ornament::card(ui, 18, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new(title)
                .font(ornament::display_font(ui.ctx(), 18.0))
                .color(ui.visuals().hyperlink_color),
        );
        ui.add_space(8.0);
        contents(ui);
    });
    ui.add_space(10.0);
}

/// Keep labels and controls in stable columns on desktop, then stack them when
/// space is limited. Help text wraps within the label column in both layouts.
fn setting_row(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    contents: impl FnOnce(&mut egui::Ui),
) {
    if ui.available_width() >= 580.0 {
        let label_width = ui.available_width() - 216.0;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(label_width);
                    ui.label(RichText::new(label).strong());
                    ui.weak(description);
                },
            );
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(200.0, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                contents,
            );
        });
    } else {
        ui.label(RichText::new(label).strong());
        ui.weak(description);
        ui.horizontal(contents);
    }
}

fn row_separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
}

fn path_label(ui: &mut egui::Ui, path: &std::path::Path) {
    let path = path.display().to_string();
    ui.add(egui::Label::new(&path).truncate())
        .on_hover_text(path);
}
