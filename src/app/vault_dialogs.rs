//! Changing the master password and choosing a backup version to restore.

use std::sync::mpsc;

use eframe::egui;
use rakshamemo::{
    store::{BackupVersion, NewKey},
    strength::{self, Strength},
};
use zeroize::{Zeroize, Zeroizing};

use super::{Dialog, Notepad, forget_editor_state, style};

pub(super) struct PasswordChange {
    current: Zeroizing<String>,
    new: Zeroizing<String>,
    confirm: Zeroizing<String>,
    /// Delete backups that still open with the old password.
    remove_versions: bool,
    strength: Option<Strength>,
    /// Argon2 runs twice (check the current password, derive the new key),
    /// so it happens on a worker thread, as unlocking does.
    working: Option<mpsc::Receiver<anyhow::Result<NewKey>>>,
    problem: Option<String>,
    /// The first field takes keyboard focus when the dialog opens.
    focused: bool,
}

impl Default for PasswordChange {
    fn default() -> Self {
        Self {
            current: Zeroizing::new(String::new()),
            new: Zeroizing::new(String::new()),
            confirm: Zeroizing::new(String::new()),
            remove_versions: true,
            strength: None,
            working: None,
            problem: None,
            focused: false,
        }
    }
}

impl PasswordChange {
    /// Why the form cannot be submitted yet, if anything.
    fn invalid(&self) -> Option<&'static str> {
        if self.current.is_empty() {
            Some("Enter your current password.")
        } else if self.new.chars().count() < 12 {
            Some("Use at least 12 characters for the new password.")
        } else if *self.new != *self.confirm {
            Some("The new passwords do not match yet.")
        } else if *self.new == *self.current {
            Some("Choose a password different from the current one.")
        } else {
            None
        }
    }
}

fn password_field<'a>(text: &'a mut String, hint: &'a str, id: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .password(true)
        .hint_text(hint)
        .id_salt(("change_password", id))
        .desired_width(f32::INFINITY)
        .min_size(egui::vec2(0.0, 36.0))
        .margin(egui::Margin::symmetric(10, 8))
}

impl Notepad {
    /// Returns whether the dialog should close.
    pub(super) fn change_password_ui(
        &mut self,
        ui: &mut egui::Ui,
        change: &mut PasswordChange,
    ) -> bool {
        if let Some(result) = change
            .working
            .as_ref()
            .and_then(|working| working.try_recv().ok())
        {
            change.working = None;
            match result.and_then(|key| {
                // No autosave can start while this modal is open, but drain
                // one that began just before, so its receipt cannot clash.
                self.complete_pending_save(true);
                self.store.as_mut().unwrap().apply_password_change(key)
            }) {
                Ok(()) => {
                    let mut status = "Master password changed".to_owned();
                    if change.remove_versions {
                        match self.store.as_ref().unwrap().remove_versions() {
                            Ok(count) if count > 0 => {
                                status = format!("{status}; {count} older backups removed");
                            }
                            Ok(_) => {}
                            Err(error) => {
                                self.error = Some(format!(
                                    "Password changed, but older backups could not be removed: {error:#}"
                                ));
                            }
                        }
                    }
                    self.status = status;
                    return true;
                }
                Err(error) => change.problem = Some(format!("{error:#}")),
            }
        }

        ui.heading("Change master password");
        ui.weak("Re-encrypts this vault. Exported copies keep their own password.");
        ui.add_space(4.0);
        let working = change.working.is_some();
        let mut submit = false;
        let mut cancel = false;
        ui.add_enabled_ui(!working, |ui| {
            let current = ui.add(password_field(&mut change.current, "Current password", "current"));
            if !std::mem::replace(&mut change.focused, true) {
                current.request_focus();
            }
            if ui
                .add(password_field(
                    &mut change.new,
                    "New password (at least 12 characters)",
                    "new",
                ))
                .changed()
            {
                change.strength = strength::estimate(&change.new, &[]);
            }
            if let Some(strength) = &change.strength {
                style::strength_meter(ui, strength);
            }
            ui.add(password_field(&mut change.confirm, "Confirm new password", "confirm"));
            ui.checkbox(
                &mut change.remove_versions,
                "Also delete older backups (they open with the current password)",
            )
            .on_hover_text("Recommended if someone else may know the current password. Permanent pre-import backups are kept.");
            ui.add_space(4.0);
            let problem = change.invalid();
            ui.horizontal(|ui| {
                cancel = ui.button("Cancel").clicked();
                submit = style::primary(ui, |ui| {
                    ui.add_enabled(problem.is_none(), egui::Button::new("Change password"))
                })
                .clicked();
            });
            if let Some(problem) = problem
                && !change.new.is_empty()
            {
                ui.weak(problem);
            }
        });
        // Escape cancels, except while the key is being derived.
        if cancel || (!working && ui.input(|input| input.key_pressed(egui::Key::Escape))) {
            return true;
        }
        if submit {
            // Save first, so the re-encrypted vault includes every edit.
            if self.save_now() {
                let job = self
                    .store
                    .as_ref()
                    .unwrap()
                    .prepare_password_change(&change.current, &change.new);
                change.current.zeroize();
                change.new.zeroize();
                change.confirm.zeroize();
                change.strength = None;
                change.problem = None;
                forget_editor_state(ui.ctx());
                let (sender, receiver) = mpsc::channel();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let _ = sender.send(job.run());
                    ctx.request_repaint();
                });
                change.working = Some(receiver);
            } else {
                change.problem =
                    Some("Save your current changes first; see the message above.".into());
            }
        }
        if change.working.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Checking your password and deriving the new key…");
            });
        }
        if let Some(problem) = &change.problem {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        false
    }
}

fn version_label(version: &BackupVersion) -> String {
    let when = version
        .saved_at
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d at %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "Unknown time".into());
    if version.previous_save {
        format!("{when} · state before the latest save")
    } else {
        format!("{when} · saved version")
    }
}

/// Returns whether to close, and the dialog to open next (the import
/// confirmation for the chosen version).
pub(super) fn versions_ui(
    ui: &mut egui::Ui,
    versions: &[BackupVersion],
    selected: &mut Option<usize>,
) -> (bool, Option<Dialog>) {
    ui.heading("Restore a previous version");
    ui.label("Before saving, RakshaMemo keeps an encrypted copy of the previous state, plus one version every 30 minutes (the latest 20).");
    ui.label("Restoring replaces your current notes and passwords once you enter the password that version was saved with. Your current vault is backed up first.");
    ui.add_space(8.0);
    if versions.is_empty() {
        ui.weak("No earlier versions yet. They appear after you make changes.");
    } else {
        egui::ScrollArea::vertical()
            .id_salt("backup_versions")
            .max_height(260.0)
            .show(ui, |ui| {
                for (index, version) in versions.iter().enumerate() {
                    ui.selectable_value(selected, Some(index), version_label(version));
                }
            });
    }
    ui.add_space(8.0);
    let mut close = false;
    let mut next = None;
    ui.horizontal(|ui| {
        close = ui.button("Cancel").clicked();
        let chosen = selected.and_then(|index| versions.get(index));
        if style::primary(ui, |ui| {
            ui.add_enabled(chosen.is_some(), egui::Button::new("Restore…"))
        })
        .clicked()
            && let Some(version) = chosen
        {
            next = Some(Dialog::Import {
                path: version.path.clone(),
                password: Zeroizing::new(String::new()),
            });
        }
    });
    (close, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_change_needs_the_current_and_a_matching_new_password() {
        let mut change = PasswordChange::default();
        assert!(
            change.remove_versions,
            "Old-password backups are removed by default"
        );
        assert!(change.invalid().is_some());
        *change.current = "current password".into();
        *change.new = "too short".into();
        assert!(change.invalid().is_some());
        *change.new = "current password".into();
        *change.confirm = "current password".into();
        assert!(change.invalid().unwrap().contains("different"));
        *change.new = "a brand new passphrase".into();
        assert!(change.invalid().unwrap().contains("match"));
        *change.confirm = "a brand new passphrase".into();
        assert!(change.invalid().is_none());
    }
}
