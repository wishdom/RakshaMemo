use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use eframe::egui::{self, RichText};
use secure_notepad::{
    generator,
    model::{self, Note, PasswordItem},
    platform,
    settings::Settings,
    store::{SaveReceipt, Store},
};
use zeroize::{Zeroize, Zeroizing};

mod password_editor;
mod password_table;
mod settings_page;
mod style;

struct PendingSave {
    result: mpsc::Receiver<anyhow::Result<SaveReceipt>>,
    revision: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Notes,
    Passwords,
    Settings,
}

struct EntryEditor {
    item: PasswordItem,
    show_password: bool,
    generator: generator::Options,
}

impl Drop for EntryEditor {
    fn drop(&mut self) {
        self.item.password.zeroize();
        self.item.username.zeroize();
        self.item.title.zeroize();
        self.item.extra.zeroize();
        self.item.kind.zeroize();
    }
}

enum Dialog {
    DiscardEdits,
    DeleteNote(String),
    DeletePassword(String),
    Types {
        selected: String,
        name: String,
    },
    Import {
        path: PathBuf,
        password: Zeroizing<String>,
    },
}

struct Clipboard {
    device: Option<arboard::Clipboard>,
    copied: Option<(Zeroizing<String>, Instant)>,
}

impl Clipboard {
    fn new() -> Self {
        Self {
            device: None,
            copied: None,
        }
    }
    fn copy(&mut self, text: String) -> anyhow::Result<()> {
        let text = Zeroizing::new(text);
        if self.device.is_none() {
            self.device = Some(arboard::Clipboard::new()?);
        }
        self.device.as_mut().unwrap().set_text(text.to_string())?;
        self.copied = Some((text, Instant::now()));
        Ok(())
    }
    fn clear(&mut self, force: bool) -> anyhow::Result<()> {
        if let Some((text, time)) = &self.copied {
            if !force && time.elapsed() < Duration::from_secs(30) {
                return Ok(());
            }
            if let Some(device) = &mut self.device {
                match device.get_text() {
                    Ok(current) => {
                        let current = Zeroizing::new(current);
                        if *current == **text {
                            device.clear()?;
                        }
                    }
                    Err(arboard::Error::ContentNotAvailable) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            self.copied = None;
        }
        Ok(())
    }
}

pub struct Notepad {
    vault_path: PathBuf,
    settings_path: PathBuf,
    settings: Settings,
    _instance: platform::SingleInstance,
    custom_path: bool,
    store: Option<Store>,
    auth: Zeroizing<String>,
    confirm: Zeroizing<String>,
    unlocking: Option<mpsc::Receiver<anyhow::Result<Store>>>,
    tab: Tab,
    query: String,
    category: String,
    dialog: Option<Dialog>,
    password_editor: Option<Box<EntryEditor>>,
    error: Option<String>,
    status: String,
    dirty_since: Option<Instant>,
    last_edit: Instant,
    last_activity: Instant,
    last_save_attempt: Instant,
    startup_enabled: bool,
    clipboard: Clipboard,
    auth_focus: bool,
    password_table: password_table::TableState,
    pending_save: Option<PendingSave>,
    revision: u64,
    save_failed: bool,
}

impl Notepad {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        vault_path: PathBuf,
        settings_path: PathBuf,
        settings: Settings,
        mut warning: Option<String>,
        instance: platform::SingleInstance,
        custom_path: bool,
    ) -> Self {
        style::apply(&cc.egui_ctx, settings.dark_mode);
        let startup_enabled = match platform::startup::is_enabled() {
            Ok(value) => value,
            Err(error) => {
                warning = Some(format!("Could not read startup setting: {error:#}"));
                false
            }
        };
        Self {
            vault_path,
            settings_path,
            settings,
            _instance: instance,
            custom_path,
            store: None,
            auth: Zeroizing::new(String::new()),
            confirm: Zeroizing::new(String::new()),
            unlocking: None,
            tab: Tab::Notes,
            query: String::new(),
            category: String::new(),
            dialog: None,
            password_editor: None,
            error: warning,
            status: "Locked".into(),
            dirty_since: None,
            last_edit: Instant::now(),
            last_activity: Instant::now(),
            last_save_attempt: Instant::now(),
            startup_enabled,
            clipboard: Clipboard::new(),
            auth_focus: true,
            password_table: password_table::TableState::default(),
            pending_save: None,
            revision: 0,
            save_failed: false,
        }
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.dirty_since.get_or_insert_with(Instant::now);
        self.last_edit = Instant::now();
        self.status = "Saving changes…".into();
    }

    fn apply_save_result(&mut self, revision: u64, result: anyhow::Result<SaveReceipt>) -> bool {
        let result = result.and_then(|receipt| {
            self.store
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Vault was locked before the save completed"))?
                .complete_save(receipt)
        });
        match result {
            Ok(()) => {
                if revision == self.revision {
                    self.dirty_since = None;
                    self.status = "All changes saved".into();
                }
                if self.save_failed {
                    self.error = None;
                }
                self.save_failed = false;
                true
            }
            Err(error) => {
                self.save_failed = true;
                self.last_save_attempt = Instant::now();
                self.error = Some(format!(
                    "Could not save. Your edits are still in this window. {error:#}"
                ));
                false
            }
        }
    }

    fn complete_pending_save(&mut self, wait: bool) -> bool {
        let Some(pending) = self.pending_save.take() else {
            return true;
        };
        let result = if wait {
            pending
                .result
                .recv()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Save worker stopped unexpectedly")))
        } else {
            match pending.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    self.pending_save = Some(pending);
                    return false;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(anyhow::anyhow!("Save worker stopped unexpectedly"))
                }
            }
        };
        self.apply_save_result(pending.revision, result)
    }

    fn queue_save(&mut self, ctx: &egui::Context) {
        if self.dirty_since.is_none()
            || self.pending_save.is_some()
            || (self.save_failed && self.last_save_attempt.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        let job = store.prepare_save();
        let (tx, result) = mpsc::channel();
        self.pending_save = Some(PendingSave {
            result,
            revision: self.revision,
        });
        self.last_save_attempt = Instant::now();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job.run());
            ctx.request_repaint();
        });
    }

    fn save_now(&mut self) -> bool {
        // Only explicit flush boundaries wait; ordinary edits never do disk I/O
        // or encryption on the event thread.
        if !self.complete_pending_save(true) {
            return false;
        }
        if self.dirty_since.is_none() {
            return true;
        }
        self.last_save_attempt = Instant::now();
        let Some(store) = &mut self.store else {
            return false;
        };
        match store.save() {
            Ok(()) => {
                self.dirty_since = None;
                self.status = "All changes saved".into();
                if self.save_failed {
                    self.error = None;
                }
                self.save_failed = false;
                true
            }
            Err(error) => {
                self.save_failed = true;
                self.error = Some(format!(
                    "Could not save. Your edits are still in this window. {error:#}"
                ));
                false
            }
        }
    }

    fn lock(&mut self, ctx: &egui::Context) {
        self.register_open_category();
        self.password_table.clear_sensitive();
        if !self.save_now() {
            self.last_activity = Instant::now();
            return;
        }
        self.finish_lock(ctx);
    }

    fn finish_lock(&mut self, ctx: &egui::Context) {
        // Also drain when the user explicitly discards edits: a previous save
        // must finish and release its copied key before the vault is locked.
        self.complete_pending_save(true);
        self.store = None;
        self.dialog = None;
        self.password_editor = None;
        self.dirty_since = None;
        self.error = None;
        self.password_table = password_table::TableState::default();
        self.save_failed = false;
        self.query.zeroize();
        self.category.zeroize();
        self.auth.zeroize();
        self.confirm.zeroize();
        self.auth_focus = true;
        self.status = "Locked".into();
        if let Err(error) = self.clipboard.clear(true) {
            self.error = Some(format!("Locked, but clipboard clearing failed: {error}"));
        }
        // Drop text editor state, including undo history. Framework/GPU allocations are not guaranteed to be wiped.
        ctx.memory_mut(|memory| *memory = egui::Memory::default());
        ctx.clear_animations();
    }

    fn persist_settings(&mut self) {
        if let Err(error) = self.settings.save(&self.settings_path) {
            self.error = Some(format!("Could not save settings: {error:#}"));
        }
    }

    fn copy(&mut self, text: String) {
        match self.clipboard.copy(text) {
            Ok(()) => self.status = "Copied. Clipboard clears in 30 seconds if unchanged.".into(),
            Err(error) => self.error = Some(format!("Could not copy: {error}")),
        }
    }

    fn export_snapshot(&mut self) {
        // Export remains available even when the active vault cannot be saved.
        self.complete_pending_save(true);
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("secure-notepad-backup.json")
            .add_filter("Encrypted vault", &["json"])
            .save_file()
        {
            match self.store.as_mut().unwrap().export(path) {
                Ok(()) => self.status = "Encrypted copy exported, including pending changes".into(),
                Err(error) => self.error = Some(format!("Export failed: {error:#}")),
            }
        }
    }

    fn start_unlock(&mut self, create: bool, ctx: &egui::Context) {
        if self.auth.is_empty() {
            self.error = Some("Enter your master password.".into());
            return;
        }
        if create && (self.auth.chars().count() < 12 || *self.auth != *self.confirm) {
            self.error =
                Some("Use at least 12 characters and make sure both passwords match.".into());
            return;
        }
        let password = std::mem::replace(&mut self.auth, Zeroizing::new(String::new()));
        self.confirm.zeroize();
        // TextEdit retains undo history even for masked fields. Drop it before
        // handing the password to the short-lived derivation worker.
        ctx.memory_mut(|memory| *memory = egui::Memory::default());
        self.error = None;
        let path = self.vault_path.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = if create {
                Store::create(path, &password)
            } else {
                Store::open(path, &password)
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.unlocking = Some(rx);
    }

    fn poll_unlock(&mut self) {
        let Some(receiver) = &self.unlocking else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(store)) => {
                self.store = Some(store);
                self.unlocking = None;
                self.auth_focus = true;
                self.last_activity = Instant::now();
                self.status = "Vault unlocked".into();
                self.error = None;
            }
            Ok(Err(error)) => {
                self.unlocking = None;
                self.auth_focus = true;
                self.error = Some(format!("Could not unlock vault: {error:#}"));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.unlocking = None;
                self.auth_focus = true;
                self.error = Some("Unlock worker stopped unexpectedly. Please try again.".into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    fn auth_ui(&mut self, ui: &mut egui::Ui) {
        let create = !self.vault_path.exists();
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.heading(RichText::new("Secure Notepad").size(30.0));
            ui.label("A private place for your notes and passwords.");
            ui.add_space(30.0);
            ui.heading(if create {
                "Create your vault"
            } else {
                "Unlock your vault"
            });
            ui.add_space(12.0);
            ui.add_enabled_ui(self.unlocking.is_none(), |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut *self.auth)
                        .password(true)
                        .desired_width(330.0)
                        .hint_text("Master password")
                        .id_salt("master"),
                );
                if self.auth_focus {
                    response.request_focus();
                    self.auth_focus = false;
                }
                if create {
                    ui.add(
                        egui::TextEdit::singleline(&mut *self.confirm)
                            .password(true)
                            .desired_width(330.0)
                            .hint_text("Confirm password")
                            .id_salt("confirm"),
                    );
                    ui.label("At least 12 characters. A long, unique passphrase works well.");
                    ui.label("Keep it safe: there is no password recovery.");
                }
                ui.add_space(12.0);
                if style::primary_button(
                    ui,
                    if create {
                        "Create encrypted vault"
                    } else {
                        "Unlock"
                    },
                )
                .clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Enter))
                {
                    self.start_unlock(create, ui.ctx());
                }
            });
            if self.unlocking.is_some() {
                ui.spinner();
                ui.label("Deriving your encryption key…");
            }
            ui.add_space(30.0);
            ui.small(self.vault_path.display().to_string());
            ui.small("Your vault stays local. It always starts locked.");
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Secure Notepad").size(19.0).strong());
            ui.separator();
            for (tab, label) in [
                (Tab::Notes, "Notes"),
                (Tab::Passwords, "Passwords"),
                (Tab::Settings, "Settings"),
            ] {
                if ui.selectable_label(self.tab == tab, label).clicked() {
                    self.register_open_category();
                    if self.tab != tab {
                        self.password_table.clear_sensitive();
                    }
                    self.tab = tab;
                    self.query.zeroize();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Lock").on_hover_text("Ctrl/Cmd+L").clicked() {
                    self.lock(ui.ctx());
                }
                self.save_status(ui);
            });
        });
        ui.separator();
    }

    fn save_status(&self, ui: &mut egui::Ui) {
        let (label, color) = if self.save_failed {
            ("Save failed", ui.visuals().error_fg_color)
        } else if self.dirty_since.is_some() || self.pending_save.is_some() {
            ("Saving…", ui.visuals().warn_fg_color)
        } else if self.status.starts_with("Copied") {
            ("Copied", ui.visuals().hyperlink_color)
        } else {
            ("Saved", ui.visuals().weak_text_color())
        };
        ui.label(RichText::new(format!("● {label}")).color(color))
            .on_hover_text(if self.save_failed {
                self.error.as_deref().unwrap_or("Save failed")
            } else {
                &self.status
            });
    }

    fn notes_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut delete = None;
        let store = self.store.as_mut().unwrap();
        egui::Panel::left("note_list")
            .default_size(220.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Notebooks");
                    if style::primary_button(ui, "+ New").clicked() {
                        self.query.zeroize();
                        let note = Note::new("Untitled");
                        store.data.notes.active_id = note.id.clone();
                        store.data.notes.items.push(note);
                        changed = true;
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search notes…"));
                ui.separator();
                let query = self.query.to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("notes_list")
                    .show(ui, |ui| {
                        for note in &store.data.notes.items {
                            if !query.is_empty()
                                && !note.title.to_lowercase().contains(&query)
                                && !note.content.to_lowercase().contains(&query)
                            {
                                continue;
                            }
                            if ui
                                .selectable_label(
                                    note.id == store.data.notes.active_id,
                                    if note.title.is_empty() {
                                        "Untitled"
                                    } else {
                                        &note.title
                                    },
                                )
                                .clicked()
                            {
                                store.data.notes.active_id = note.id.clone();
                                changed = true;
                            }
                        }
                    });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(note) = store
                .data
                .notes
                .items
                .iter_mut()
                .find(|note| note.id == store.data.notes.active_id)
            {
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut note.title)
                                .id_salt((&note.id, "title"))
                                .font(egui::TextStyle::Heading)
                                .hint_text("Untitled")
                                .desired_width((ui.available_width() - 90.0).max(100.0)),
                        )
                        .changed()
                    {
                        note.updated_at = model::now();
                        changed = true;
                    }
                    if style::danger_button(ui, "Delete…").clicked() {
                        delete = Some(note.id.clone());
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("note_editor_scroll")
                    .show(ui, |ui| {
                        if ui
                            .add_sized(
                                [ui.available_width(), ui.available_height().max(300.0)],
                                egui::TextEdit::multiline(&mut note.content)
                                    .id_salt((&note.id, "content"))
                                    .font(egui::FontId::proportional(16.0))
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Write something worth keeping…"),
                            )
                            .changed()
                        {
                            note.updated_at = model::now();
                            changed = true;
                        }
                    });
            } else {
                ui.centered_and_justified(|ui| ui.label("Create a notebook to start writing."));
            }
        });
        if changed {
            self.changed();
        }
        if let Some(id) = delete {
            self.dialog = Some(Dialog::DeleteNote(id));
        }
    }

    fn passwords_ui(&mut self, ui: &mut egui::Ui) {
        style::passwords(ui);
        if let Some(mut editor) = self.password_editor.take() {
            if !self.password_editor_page(ui, &mut editor) {
                self.password_editor = Some(editor);
            }
            return;
        }
        let data = &self.store.as_ref().unwrap().data;
        let actions = password_table::show(
            ui,
            &data.passwords,
            &data.meta.types,
            &mut self.query,
            &mut self.category,
            &mut self.password_table,
        );
        // Apply this frame's typing before copy, navigation, or category
        // completion, regardless of which row's controls rendered first.
        let (updates, actions): (Vec<_>, Vec<_>) = actions
            .into_iter()
            .partition(|action| matches!(action, password_table::Action::Update { .. }));
        for action in updates.into_iter().chain(actions) {
            use password_table::Action;
            match action {
                Action::Update { id, field, value } => {
                    self.update_password_cell(&id, field, value);
                }
                Action::FinishCategory(id) => self.register_password_category(&id),
                Action::New => {
                    self.finish_password_table_edit();
                    self.password_table.clear_sensitive();
                    self.query.zeroize();
                    let item = PasswordItem {
                        title: "Untitled".into(),
                        kind: self.category.clone(),
                        ..Default::default()
                    };
                    self.store
                        .as_mut()
                        .unwrap()
                        .data
                        .passwords
                        .push(item.clone());
                    self.changed();
                    self.password_editor = Some(Box::new(EntryEditor {
                        item,
                        show_password: false,
                        generator: generator::Options::default(),
                    }));
                }
                Action::ManageCategories => {
                    self.finish_password_table_edit();
                    self.password_table.clear_sensitive();
                    self.dialog = Some(Dialog::Types {
                        selected: String::new(),
                        name: String::new(),
                    })
                }
                Action::Delete(id) => {
                    self.finish_password_table_edit();
                    self.password_table.clear_sensitive();
                    self.dialog = Some(Dialog::DeletePassword(id));
                }
                Action::Edit(id) => {
                    self.finish_password_table_edit();
                    self.password_table.clear_sensitive();
                    if let Some(item) = self
                        .store
                        .as_ref()
                        .unwrap()
                        .data
                        .passwords
                        .iter()
                        .find(|item| item.id == id)
                    {
                        self.password_editor = Some(Box::new(EntryEditor {
                            item: item.clone(),
                            show_password: false,
                            generator: generator::Options::default(),
                        }));
                    }
                }
                Action::CopyUsername(ref id) | Action::CopyPassword(ref id) => {
                    // Resolve only the requested secret, after rendering the rows.
                    if let Some(item) = self
                        .store
                        .as_ref()
                        .unwrap()
                        .data
                        .passwords
                        .iter()
                        .find(|item| &item.id == id)
                    {
                        let text = match &action {
                            Action::CopyUsername(_) => item.username.clone(),
                            _ => item.password.clone(),
                        };
                        self.copy(text);
                    }
                }
            }
        }
    }
    fn register_category(&mut self, name: &str) {
        let name = name.trim();
        if let Some(store) = &mut self.store
            && !name.is_empty()
            && !store.data.meta.types.iter().any(|kind| kind == name)
        {
            store.data.meta.types.push(name.to_owned());
            self.changed();
        }
    }

    fn register_open_category(&mut self) {
        self.finish_password_table_edit();
        if let Some(editor) = &self.password_editor {
            let name = editor.item.kind.clone();
            self.register_category(&name);
        }
    }

    fn finish_password_table_edit(&mut self) {
        if let Some(password_table::Action::FinishCategory(id)) = self.password_table.finish_edit()
        {
            self.register_password_category(&id);
        }
    }

    fn register_password_category(&mut self, id: &str) {
        let category = self.store.as_ref().and_then(|store| {
            store
                .data
                .passwords
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.kind.clone())
        });
        if let Some(category) = category {
            self.register_category(&category);
        }
    }

    fn update_password_cell(
        &mut self,
        id: &str,
        field: password_table::Field,
        mut value: Zeroizing<String>,
    ) {
        use password_table::Field;
        let Some(item) = self
            .store
            .as_mut()
            .and_then(|store| store.data.passwords.iter_mut().find(|item| item.id == id))
        else {
            return;
        };
        // Match the full-page editor's normalization without modifying the
        // actively typed buffer. Passwords and extra information stay verbatim.
        if matches!(field, Field::Title | Field::Category) {
            let normalized = value.trim().to_owned();
            value.zeroize();
            *value = normalized;
            if field == Field::Title && value.is_empty() {
                value.push_str("Untitled");
            }
        }
        let target = match field {
            Field::Title => &mut item.title,
            Field::Username => &mut item.username,
            Field::Password => &mut item.password,
            Field::Category => &mut item.kind,
            Field::Extra => &mut item.extra,
        };
        if *target == *value {
            return;
        }
        target.zeroize();
        *target = std::mem::take(&mut *value);
        item.updated_at = model::now();
        self.changed();
    }

    fn update_entry(&mut self, editor: &mut EntryEditor) {
        editor.item.updated_at = model::now();
        let mut saved = editor.item.clone();
        saved.title = match saved.title.trim() {
            "" => "Untitled".into(),
            title => title.into(),
        };
        saved.kind = saved.kind.trim().to_owned();
        let data = &mut self.store.as_mut().unwrap().data;
        if let Some(item) = data.passwords.iter_mut().find(|item| item.id == saved.id) {
            item.password.zeroize();
            item.extra.zeroize();
            item.username.zeroize();
            *item = saved;
        } else {
            data.passwords.push(saved);
        }
        self.changed();
    }

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut close = false;
        let mut changed = false;
        let mut discard = false;
        let deleting_note = matches!(dialog, Dialog::DeleteNote(_));
        egui::Modal::new(egui::Id::new("vault_dialog")).show(ctx, |ui| {
            style::passwords(ui);
            ui.set_width((ctx.content_rect().width() - 64.0).clamp(280.0, 640.0));
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 96.0).max(160.0))
                .show(ui, |ui| {
            match &mut dialog {
                Dialog::DiscardEdits => {
                    ui.heading("Discard unsaved changes and lock?");
                    ui.label("Changes that have not already been saved will be lost. Export an encrypted copy first if you want to keep them. Any save already in progress will finish before locking.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { close = true; }
                        if style::destructive_button(ui, "Discard changes and lock").clicked() { discard = true; close = true; }
                    });
                }
                Dialog::DeleteNote(id) | Dialog::DeletePassword(id) => {
                    let data = &self.store.as_ref().unwrap().data;
                    let title = if deleting_note {
                        data.notes.items.iter().find(|note| note.id == *id).map(|note| note.title.as_str())
                    } else {
                        data.passwords.iter().find(|item| item.id == *id).map(|item| item.title.as_str())
                    }.filter(|title| !title.is_empty()).unwrap_or("Untitled");
                    let action = if deleting_note { "Delete note" } else { "Delete password" };
                    ui.heading(format!("{action}?"));
                    ui.label(RichText::new(title).strong());
                    ui.label("The item will be removed from this vault. This cannot be undone in the editor.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { close = true; }
                        if style::destructive_button(ui, action).clicked() { close = true; changed = true; }
                    });
                }
                Dialog::Types { selected, name } => {
                    ui.heading("Manage categories");
                    ui.label("Categories are optional groups, such as Work or Personal, that you can filter in the password table.");
                    ui.label("Create a category by typing its name while editing a password. Here you can rename a group or remove one that is no longer used.");
                    ui.add_space(8.0);
                    egui::ComboBox::from_id_salt("manage_categories").selected_text(if selected.is_empty() { "Choose a category" } else { selected.as_str() }).show_ui(ui, |ui| {
                        for kind in &self.store.as_ref().unwrap().data.meta.types {
                            if ui.selectable_value(selected, kind.clone(), kind).clicked() { *name = kind.clone(); }
                        }
                    });
                    ui.label("Category name");
                    ui.add(egui::TextEdit::singleline(name).id_salt("rename_category").hint_text("Select a category to rename").desired_width(f32::INFINITY));
                    let usage = self.store.as_ref().unwrap().data.passwords.iter().filter(|item| item.kind == *selected).count();
                    if !selected.is_empty() {
                        ui.label(format!("Used by {usage} {}", if usage == 1 { "password" } else { "passwords" }));
                    }
                    ui.horizontal(|ui| {
                        let data = &mut self.store.as_mut().unwrap().data;
                        if ui.add_enabled(!selected.is_empty() && !name.trim().is_empty(), egui::Button::new("Rename")).clicked() {
                            let value = name.trim().to_string();
                            if !data.meta.types.contains(&value) || *selected == value {
                                for kind in &mut data.meta.types { if kind == selected { *kind = value.clone(); } }
                                for item in &mut data.passwords { if item.kind == *selected { item.kind = value.clone(); item.updated_at = model::now(); } }
                                if self.category == *selected { self.category = value.clone(); }
                                *selected = value; changed = true;
                            } else { self.error = Some("That category already exists.".into()); }
                        }
                        if ui.add_enabled_ui(!selected.is_empty() && usage == 0, |ui| style::danger_button(ui, "Remove unused category")).inner.on_disabled_hover_text("Only categories with no passwords can be removed.").clicked() {
                            if data.passwords.iter().any(|item| item.kind == *selected) { self.error = Some("This category is still used by a password.".into()); }
                            else {
                                data.meta.types.retain(|kind| kind != selected);
                                if self.category == *selected { self.category.clear(); }
                                selected.clear(); name.clear(); changed = true;
                            }
                        }
                    });
                    if style::primary_button(ui, "Done").clicked() { close = true; }
                }
                Dialog::Import { path, password } => {
                    ui.heading("Replace current vault?");
                    ui.label(path.display().to_string());
                    ui.label("Enter this file’s master password. Its contents will replace your current notes and passwords after validation.");
                    ui.label("A separate encrypted backup of your current vault will be kept.");
                    ui.add(egui::TextEdit::singleline(&mut **password).password(true).hint_text("Imported vault password"));
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { close = true; }
                        if style::destructive_button(ui, "Authenticate and replace").clicked() && self.save_now() {
                            match self.store.as_mut().unwrap().import(path, password) {
                                Ok(()) => { close = true; self.password_editor = None; self.query.zeroize(); self.category.zeroize(); self.password_table = password_table::TableState::default(); self.status = "Imported and authenticated vault".into(); ctx.memory_mut(|memory| *memory = egui::Memory::default()); }
                                Err(error) => self.error = Some(format!("Import failed; current vault kept: {error:#}")),
                            }
                        }
                    });
                }
            }
            if let Some(error) = &self.error { ui.add_space(8.0); ui.colored_label(ui.visuals().error_fg_color, error); }
                });
        });
        if discard {
            self.finish_lock(ctx);
            return;
        }
        if changed {
            let store = self.store.as_mut().unwrap();
            match &dialog {
                Dialog::DeleteNote(id) => {
                    if let Some(note) = store
                        .data
                        .notes
                        .items
                        .iter_mut()
                        .find(|note| note.id == *id)
                    {
                        note.title.zeroize();
                        note.content.zeroize();
                    }
                    store.data.notes.items.retain(|note| note.id != *id);
                    if store.data.notes.active_id == *id {
                        store.data.notes.active_id = store
                            .data
                            .notes
                            .items
                            .first()
                            .map(|note| note.id.clone())
                            .unwrap_or_default();
                    }
                }
                Dialog::DeletePassword(id) => {
                    if let Some(item) = store.data.passwords.iter_mut().find(|item| item.id == *id)
                    {
                        item.password.zeroize();
                        item.extra.zeroize();
                        item.username.zeroize();
                    }
                    store.data.passwords.retain(|item| item.id != *id);
                }
                _ => {}
            }
            self.changed();
        }
        if !close {
            self.dialog = Some(dialog);
        }
    }

    fn tick(&mut self, ctx: &egui::Context) {
        self.poll_unlock();
        self.complete_pending_save(false);
        if self.store.is_some()
            && self.settings.auto_lock_minutes > 0
            && self.last_activity.elapsed()
                >= Duration::from_secs(u64::from(self.settings.auto_lock_minutes) * 60)
        {
            self.lock(ctx);
        }
        self.queue_save(ctx);
        if let Err(error) = self.clipboard.clear(false) {
            self.error = Some(format!("Could not clear clipboard: {error}"));
        }
        ctx.request_repaint_after(if self.pending_save.is_some() {
            Duration::from_millis(32)
        } else {
            Duration::from_secs(1)
        });
    }
}

impl eframe::App for Notepad {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|input| !input.events.is_empty()) {
            self.last_activity = Instant::now();
        }
        if self.store.is_some() {
            if self.dialog.is_none()
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
            {
                self.save_now();
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) {
                self.lock(&ctx);
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.store.is_some() {
                self.toolbar(ui);
            }
            if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
                if self.store.is_some() && self.dirty_since.is_some() {
                    ui.horizontal(|ui| {
                        if ui.button("Export unsaved vault…").clicked() {
                            self.export_snapshot();
                        }
                        if style::danger_button(ui, "Discard edits and lock…").clicked() {
                            self.dialog = Some(Dialog::DiscardEdits);
                        }
                    });
                }
            }
            if self.store.is_none() {
                self.auth_ui(ui);
            } else {
                match self.tab {
                    Tab::Notes => self.notes_ui(ui),
                    Tab::Passwords => self.passwords_ui(ui),
                    Tab::Settings => self.settings_ui(ui),
                }
            }
        });
        if self.store.is_some() {
            self.dialog_ui(&ctx);
        }
        // Queue this frame's edits immediately, while the UI remains interactive.
        self.queue_save(&ctx);
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.settings.window_size = [rect.width(), rect.height()];
        }
        // Handle close after this frame's edits, so a click-and-close cannot bypass saving.
        if ctx.input(|input| input.viewport().close_requested()) {
            self.register_open_category();
            if self.unlocking.is_some() || !self.save_now() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else {
                self.persist_settings();
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.register_open_category();
        self.password_table.clear_sensitive();
        self.save_now();
        let _ = self.clipboard.clear(true);
        self.store = None;
        self.dialog = None;
        self.password_editor = None;
        self.auth.zeroize();
        self.confirm.zeroize();
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
