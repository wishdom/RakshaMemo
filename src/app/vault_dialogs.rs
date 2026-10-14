//! Changing the master password and choosing a backup version to restore.

use std::sync::mpsc;

use eframe::egui;
use rakshamemo::{
    store::{BackupVersion, NewKey},
    strength::{self, Strength},
};
use zeroize::{Zeroize, Zeroizing};

use super::{Dialog, Notepad, forget_editor_state, style};
use crate::i18n::{self, fill, t};

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
            Some(t().enter_current_password)
        } else if self.new.chars().count() < 12 {
            Some(t().new_password_too_short)
        } else if *self.new != *self.confirm {
            Some(t().new_passwords_do_not_match)
        } else if *self.new == *self.current {
            Some(t().choose_different_password)
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
                    let mut status = t().password_changed.to_owned();
                    if change.remove_versions {
                        match self.store.as_ref().unwrap().remove_versions() {
                            Ok(count) if count > 0 => {
                                status = i18n::plural(count, t().password_changed_backups_removed);
                            }
                            Ok(_) => {}
                            Err(error) => {
                                self.error =
                                    Some(fill(t().backups_not_removed, &[&format!("{error:#}")]));
                            }
                        }
                    }
                    self.status = status;
                    return true;
                }
                Err(error) => change.problem = Some(i18n::describe(&error)),
            }
        }

        let text = t();
        ui.heading(text.change_password_title);
        ui.weak(text.change_password_intro);
        ui.add_space(4.0);
        let working = change.working.is_some();
        let mut submit = false;
        let mut cancel = false;
        ui.add_enabled_ui(!working, |ui| {
            let current = ui.add(password_field(
                &mut change.current,
                text.current_password_hint,
                "current",
            ));
            if !std::mem::replace(&mut change.focused, true) {
                current.request_focus();
            }
            if ui
                .add(password_field(
                    &mut change.new,
                    text.new_password_hint,
                    "new",
                ))
                .changed()
            {
                change.strength = strength::estimate(&change.new, &[]);
            }
            if let Some(strength) = &change.strength {
                style::strength_meter(ui, strength);
            }
            ui.add(password_field(
                &mut change.confirm,
                text.confirm_new_password_hint,
                "confirm",
            ));
            ui.checkbox(&mut change.remove_versions, text.remove_old_backups)
                .on_hover_text(text.remove_old_backups_hover);
            ui.add_space(4.0);
            let problem = change.invalid();
            ui.horizontal(|ui| {
                cancel = ui.button(text.cancel).clicked();
                submit = style::primary(ui, |ui| {
                    ui.add_enabled(problem.is_none(), egui::Button::new(text.change_password))
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
                change.problem = Some(text.save_changes_first.into());
            }
        }
        if change.working.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(text.deriving_new_key);
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
        .map(|date| i18n::date_time(&date.with_timezone(&chrono::Local)))
        .unwrap_or_else(|| t().unknown_time.into());
    if version.previous_save {
        fill(t().version_before_save, &[&when])
    } else {
        fill(t().version_saved, &[&when])
    }
}

/// Returns whether to close, and the dialog to open next (the import
/// confirmation for the chosen version).
pub(super) fn versions_ui(
    ui: &mut egui::Ui,
    versions: &[BackupVersion],
    selected: &mut Option<usize>,
) -> (bool, Option<Dialog>) {
    let text = t();
    ui.heading(text.restore_title);
    ui.label(text.restore_explained);
    ui.label(text.restore_how);
    ui.add_space(8.0);
    if versions.is_empty() {
        ui.weak(text.no_versions);
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
        close = ui.button(text.cancel).clicked();
        let chosen = selected.and_then(|index| versions.get(index));
        if style::primary(ui, |ui| {
            ui.add_enabled(chosen.is_some(), egui::Button::new(text.restore_ellipsis))
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
