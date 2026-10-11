use eframe::egui::{self, RichText};
use rakshamemo::platform;
use zeroize::Zeroizing;

use super::{Dialog, Notepad, style};

#[cfg(test)]
mod tests;

impl Notepad {
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        style::page(ui);
        egui::ScrollArea::vertical()
            .id_salt("settings-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(920.0));
                ui.heading("Settings");
                ui.weak("Changes to your preferences are saved automatically.");
                ui.add_space(12.0);

                section(ui, "Appearance", |ui| {
                    let mut dark = self.settings.dark_mode;
                    setting_row(ui, "Theme", "Choose the appearance of the app.", |ui| {
                        ui.selectable_value(&mut dark, false, "Light");
                        ui.selectable_value(&mut dark, true, "Dark");
                    });
                    self.set_dark_mode(ui.ctx(), dark);
                    row_separator(ui);
                    let mut clock_24h = self.settings.clock_24h;
                    setting_row(ui, "Clock", "How the time appears at the top of the window.", |ui| {
                        ui.selectable_value(&mut clock_24h, true, "24-hour");
                        ui.selectable_value(&mut clock_24h, false, "12-hour");
                    });
                    if clock_24h != self.settings.clock_24h {
                        self.settings.clock_24h = clock_24h;
                        self.persist_settings();
                    }
                });

                section(ui, "Startup and security", |ui| {
                    let mut startup = self.startup_enabled;
                    let description = if self.custom_path {
                        "Available when using the default vault location."
                    } else {
                        "Open RakshaMemo when you sign in. Your vault starts locked."
                    };
                    setting_row(ui, "Launch at sign-in", description, |ui| {
                        if ui
                            .add_enabled(
                                !self.custom_path,
                                egui::Checkbox::new(&mut startup, "Enabled"),
                            )
                            .changed()
                        {
                            match platform::startup::set_enabled(startup) {
                                Ok(()) => {
                                    self.startup_enabled = startup;
                                    self.status = "Startup preference updated".into();
                                }
                                Err(error) => {
                                    self.error = Some(format!(
                                        "Could not update startup preference: {error:#}"
                                    ));
                                }
                            }
                        }
                    });
                    if !self.custom_path {
                        ui.add_space(4.0);
                        ui.weak("If you move the app, turn this option off and on to update its location.");
                    }
                    row_separator(ui);
                    setting_row(
                        ui,
                        "Automatic lock",
                        "Lock after this many minutes without activity. Set to 0 to never lock automatically.",
                        |ui| {
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.settings.auto_lock_minutes)
                                        .range(0..=1440)
                                        .suffix(" min")
                                        .speed(1.0),
                                )
                                .on_hover_text("Drag to adjust, or click the number to type a value.")
                                .changed()
                            {
                                self.persist_settings();
                            }
                        },
                    );
                    row_separator(ui);
                    let away = if cfg!(target_os = "windows") {
                        "Lock when the window is minimized, the computer sleeps, or you lock Windows."
                    } else {
                        "Lock when the window is minimized or the computer sleeps."
                    };
                    setting_row(ui, "Lock when away", away, |ui| {
                        if ui
                            .checkbox(&mut self.settings.lock_when_away, "Lock automatically")
                            .changed()
                        {
                            self.persist_settings();
                        }
                    });
                    row_separator(ui);
                    setting_row(
                        ui,
                        "Master password",
                        "Re-encrypt your vault with a new password. You need the current one.",
                        |ui| {
                            if ui.button("Change password…").clicked() {
                                self.dialog = Some(Dialog::ChangePassword(Box::default()));
                            }
                        },
                    );
                    row_separator(ui);
                    ui.label(RichText::new("Clipboard protection").strong());
                    ui.weak("Copied values are cleared after 30 seconds if the clipboard has not changed.");
                });

                section(ui, "Vault and backups", |ui| {
                    setting_row(
                        ui,
                        "Export a backup",
                        "Save an encrypted copy of your current notes and passwords.",
                        |ui| {
                            if ui.button("Export vault…").clicked() {
                                self.export_snapshot();
                            }
                        },
                    );
                    row_separator(ui);
                    setting_row(
                        ui,
                        "Replace this vault",
                        "Import an encrypted vault after unlocking it with its password. Your current vault is backed up before replacement.",
                        |ui| {
                            if style::danger_button(ui, "Import vault…").clicked()
                                && self.save_now()
                                && let Some(path) = {
                                    let path = rfd::FileDialog::new()
                                        .add_filter("Encrypted vault", &["json", "bak"])
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
                        },
                    );
                    row_separator(ui);
                    setting_row(
                        ui,
                        "Restore a previous version",
                        "Go back to an earlier encrypted state of this vault.",
                        |ui| {
                            if ui.button("Choose a version…").clicked()
                                && let Some(store) = &self.store
                            {
                                self.dialog = Some(Dialog::Versions {
                                    versions: store.versions(),
                                    selected: None,
                                });
                            }
                        },
                    );
                    row_separator(ui);
                    ui.label(RichText::new("Vault location").strong());
                    path_label(ui, &self.vault_path);
                    egui::CollapsingHeader::new("Backup location and details")
                        .id_salt("vault-storage-details")
                        .show(ui, |ui| {
                            if let Some(store) = &self.store {
                                ui.label(RichText::new("Previous encrypted version").strong());
                                path_label(ui, &store.backup_path());
                                ui.add_space(4.0);
                                ui.label(format!("Created: {}", store.data.meta.created_at));
                                ui.label(format!("Last saved: {}", store.data.meta.modified_at));
                            }
                            ui.weak("Encryption: Argon2id + AES-256-GCM");
                        });
                });

                ui.weak(concat!("RakshaMemo ", env!("CARGO_PKG_VERSION"), " · Ctrl/Cmd+N new · Ctrl/Cmd+F search · Ctrl/Cmd+L lock"));
                ui.add_space(12.0);
            });
    }
}

fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(ui.visuals().window_stroke)
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(17.0).strong());
            ui.add_space(8.0);
            contents(ui);
        });
    ui.add_space(8.0);
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
