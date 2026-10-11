use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, RichText};
use secure_notepad::{
    generator,
    model::{self, Note, PasswordItem},
    platform,
    settings::Settings,
    store::Store,
};
use zeroize::{Zeroize, Zeroizing};

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
    Entry(Box<EntryEditor>),
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
    error: Option<String>,
    status: String,
    dirty_since: Option<Instant>,
    last_edit: Instant,
    last_activity: Instant,
    last_save_attempt: Instant,
    startup_enabled: bool,
    clipboard: Clipboard,
    auth_focus: bool,
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
        cc.egui_ctx.set_visuals(if settings.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
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
            error: warning,
            status: "Locked".into(),
            dirty_since: None,
            last_edit: Instant::now(),
            last_activity: Instant::now(),
            last_save_attempt: Instant::now(),
            startup_enabled,
            clipboard: Clipboard::new(),
            auth_focus: true,
        }
    }

    fn changed(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
        self.last_edit = Instant::now();
        self.status = "Unsaved changes".into();
    }

    fn save_now(&mut self) -> bool {
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
                self.status = "All changes saved and encrypted".into();
                true
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not save. Your edits are still in this window. {error:#}"
                ));
                false
            }
        }
    }

    fn lock(&mut self, ctx: &egui::Context) {
        if !self.save_now() {
            self.last_activity = Instant::now();
            return;
        }
        self.finish_lock(ctx);
    }

    fn finish_lock(&mut self, ctx: &egui::Context) {
        self.store = None;
        self.dialog = None;
        self.dirty_since = None;
        self.error = None;
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
                if ui
                    .button(if create {
                        "Create encrypted vault"
                    } else {
                        "Unlock"
                    })
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
            ui.heading("Secure Notepad");
            ui.separator();
            for (tab, label) in [
                (Tab::Notes, "Notes"),
                (Tab::Passwords, "Passwords"),
                (Tab::Settings, "Settings"),
            ] {
                if ui.selectable_label(self.tab == tab, label).clicked() {
                    self.tab = tab;
                    self.query.zeroize();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Lock").on_hover_text("Ctrl/Cmd+L").clicked() {
                    self.lock(ui.ctx());
                }
                if ui.button("Save").on_hover_text("Ctrl/Cmd+S").clicked() {
                    self.save_now();
                }
            });
        });
        ui.separator();
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
                    if ui.button("+ New").clicked() {
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
                                .font(egui::TextStyle::Heading)
                                .hint_text("Untitled")
                                .desired_width((ui.available_width() - 90.0).max(100.0)),
                        )
                        .changed()
                    {
                        note.updated_at = model::now();
                        changed = true;
                    }
                    if ui.button("Delete").clicked() {
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
        let mut copy = None;
        let mut dialog = None;
        let store = self.store.as_ref().unwrap();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search title, username, category, or notes…")
                    .desired_width(340.0),
            );
            egui::ComboBox::from_id_salt("filter")
                .selected_text(if self.category.is_empty() {
                    "All categories"
                } else {
                    &self.category
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category, String::new(), "All categories");
                    for kind in &store.data.meta.types {
                        ui.selectable_value(&mut self.category, kind.clone(), kind);
                    }
                });
            if ui.button("+ Password").clicked() {
                let item = PasswordItem {
                    id: uuid::Uuid::new_v4().simple().to_string(),
                    kind: self.category.clone(),
                    ..Default::default()
                };
                dialog = Some(Dialog::Entry(Box::new(EntryEditor {
                    item,
                    show_password: false,
                    generator: generator::Options::default(),
                })));
            }
            if ui.button("Categories").clicked() {
                dialog = Some(Dialog::Types {
                    selected: String::new(),
                    name: String::new(),
                });
            }
        });
        ui.add_space(10.0);
        let query = self.query.to_lowercase();
        let mut count = 0;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for item in &store.data.passwords {
                if !self.category.is_empty() && self.category != item.kind {
                    continue;
                }
                if !query.is_empty()
                    && ![&item.title, &item.username, &item.kind, &item.extra]
                        .iter()
                        .any(|text| text.to_lowercase().contains(&query))
                {
                    continue;
                }
                count += 1;
                ui.push_id(&item.id, |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width((ui.available_width() - 10.0).max(200.0));
                        ui.horizontal(|ui| {
                            ui.strong(&item.title);
                            if !item.kind.is_empty() {
                                ui.weak(format!("  {}", item.kind));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Delete").clicked() {
                                        dialog = Some(Dialog::DeletePassword(item.id.clone()));
                                    }
                                    if ui.button("Edit").clicked() {
                                        dialog = Some(Dialog::Entry(Box::new(EntryEditor {
                                            item: item.clone(),
                                            show_password: false,
                                            generator: generator::Options::default(),
                                        })));
                                    }
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(&item.username);
                            if ui.small_button("Copy username").clicked() {
                                copy = Some(item.username.clone());
                            }
                            ui.label("  ••••••••••••");
                            if ui.small_button("Copy password").clicked() {
                                copy = Some(item.password.clone());
                            }
                        });
                        ui.small(format!("Updated {}", item.updated_at));
                    });
                    ui.add_space(5.0);
                });
            }
        });
        ui.weak(format!("{count} password(s)"));
        if let Some(text) = copy {
            self.copy(text);
        }
        if let Some(dialog) = dialog {
            self.dialog = Some(dialog);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preferences");
        ui.add_space(10.0);
        if ui
            .checkbox(&mut self.settings.dark_mode, "Dark theme")
            .changed()
        {
            ui.ctx().set_visuals(if self.settings.dark_mode {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            self.persist_settings();
        }
        let mut startup = self.startup_enabled;
        if ui
            .add_enabled(
                !self.custom_path,
                egui::Checkbox::new(&mut startup, "Launch at sign-in"),
            )
            .on_hover_text(
                "Open Secure Notepad when you sign in to your OS. The vault remains locked.",
            )
            .changed()
        {
            match platform::startup::set_enabled(startup) {
                Ok(()) => {
                    self.startup_enabled = startup;
                    self.status = "Startup preference updated".into();
                }
                Err(error) => {
                    self.error = Some(format!("Could not update startup preference: {error:#}"))
                }
            }
        }
        ui.small(if self.custom_path {
            "Startup is available when using the default vault location."
        } else {
            "Keep the executable in a permanent location. If you move it, turn startup off and on again."
        });
        ui.horizontal(|ui| {
            ui.label("Lock after inactivity:");
            if ui
                .add(
                    egui::DragValue::new(&mut self.settings.auto_lock_minutes)
                        .range(0..=120)
                        .suffix(" minutes"),
                )
                .changed()
            {
                self.persist_settings();
            }
            ui.weak("0 = never");
        });
        ui.small(
            "Copy buttons clear the clipboard after 30 seconds if its contents are unchanged.",
        );
        ui.add_space(20.0);
        ui.separator();
        ui.heading("Vault and backups");
        ui.label(self.vault_path.display().to_string());
        if let Some(store) = &self.store {
            ui.small(format!("Created: {}", store.data.meta.created_at));
            ui.small(format!("Modified: {}", store.data.meta.modified_at));
            ui.small(format!(
                "Previous encrypted version: {}",
                store.backup_path().display()
            ));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Export encrypted vault…").clicked() {
                self.export_snapshot();
            }
            if ui.button("Import encrypted vault…").clicked()
                && self.save_now()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Encrypted vault", &["json", "bak"])
                    .pick_file()
            {
                self.dialog = Some(Dialog::Import {
                    path,
                    password: Zeroizing::new(String::new()),
                });
            }
        });
        ui.small("Imports replace this vault after successful authentication. The previous vault is backed up.");
        ui.add_space(20.0);
        ui.weak("Secure Notepad 2.0 • Rust • Argon2id + AES-256-GCM");
        ui.small("Ctrl/Cmd+S: save    Ctrl/Cmd+L: lock");
    }

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut close = false;
        let mut changed = false;
        let mut discard = false;
        egui::Modal::new(egui::Id::new("vault_dialog")).show(ctx, |ui| {
            ui.set_width(480.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 80.0).max(280.0))
                .show(ui, |ui| {
            match &mut dialog {
                Dialog::DiscardEdits => {
                    ui.heading("Discard unsaved changes and lock?");
                    ui.label("Your changes in this window will be lost. Export an encrypted copy first if you want to keep them. The vault on disk will stay as it is.");
                    ui.horizontal(|ui| {
                        if ui.button("Discard changes and lock").clicked() { discard = true; close = true; }
                        if ui.button("Cancel").clicked() { close = true; }
                    });
                }
                Dialog::Entry(editor) => {
                    ui.heading("Password entry");
                    ui.label("Title"); ui.text_edit_singleline(&mut editor.item.title);
                    ui.label("Username / email"); ui.text_edit_singleline(&mut editor.item.username);
                    ui.label("Category (choose or type a new name)");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut editor.item.kind);
                        egui::ComboBox::from_id_salt("entry_category").selected_text("Choose").show_ui(ui, |ui| {
                            for kind in &self.store.as_ref().unwrap().data.meta.types { ui.selectable_value(&mut editor.item.kind, kind.clone(), kind); }
                        });
                    });
                    ui.label("Password");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut editor.item.password).password(!editor.show_password).desired_width(280.0));
                        ui.checkbox(&mut editor.show_password, "Show");
                        if ui.button("Copy").clicked() { self.copy(editor.item.password.clone()); }
                    });
                    ui.collapsing("Generate a random password", |ui| {
                        ui.add(egui::Slider::new(&mut editor.generator.length, 8..=128).text("Length"));
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut editor.generator.lowercase, "a–z");
                            ui.checkbox(&mut editor.generator.uppercase, "A–Z");
                            ui.checkbox(&mut editor.generator.digits, "0–9");
                            ui.checkbox(&mut editor.generator.symbols, "Symbols");
                        });
                        if ui.button("Generate").clicked() {
                            match generator::generate(&editor.generator) {
                                Ok(password) => { editor.item.password.zeroize(); editor.item.password = password; },
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
                    });
                    ui.label("Extra information");
                    ui.add(egui::TextEdit::multiline(&mut editor.item.extra).desired_rows(3).desired_width(f32::INFINITY));
                    ui.small("Save this entry to keep changes. Locking discards this draft.");
                    ui.horizontal(|ui| {
                        if ui.button("Save entry").clicked() {
                            if editor.item.title.trim().is_empty() { self.error = Some("A title is required.".into()); }
                            else {
                                editor.item.title = editor.item.title.trim().to_owned();
                                editor.item.kind = editor.item.kind.trim().to_owned();
                                editor.item.updated_at = model::now();
                                let data = &mut self.store.as_mut().unwrap().data;
                                if !editor.item.kind.is_empty() && !data.meta.types.contains(&editor.item.kind) { data.meta.types.push(editor.item.kind.clone()); }
                                if let Some(item) = data.passwords.iter_mut().find(|item| item.id == editor.item.id) {
                                    item.password.zeroize(); *item = editor.item.clone();
                                } else { data.passwords.push(editor.item.clone()); }
                                changed = true; close = true;
                            }
                        }
                        if ui.button("Cancel").clicked() { close = true; }
                    });
                }
                Dialog::DeleteNote(id) | Dialog::DeletePassword(id) => {
                    ui.heading("Delete this item?");
                    ui.label("The item will be removed from this vault. This cannot be undone in the editor.");
                    ui.horizontal(|ui| {
                        if ui.button("Delete permanently").clicked() { close = true; changed = true; }
                        if ui.button("Cancel").clicked() { close = true; }
                    });
                    let _ = id;
                }
                Dialog::Types { selected, name } => {
                    ui.heading("Password categories");
                    egui::ComboBox::from_id_salt("manage_categories").selected_text(if selected.is_empty() { "Choose a category" } else { selected.as_str() }).show_ui(ui, |ui| {
                        for kind in &self.store.as_ref().unwrap().data.meta.types {
                            if ui.selectable_value(selected, kind.clone(), kind).clicked() { *name = kind.clone(); }
                        }
                    });
                    ui.add(egui::TextEdit::singleline(name).hint_text("Category name"));
                    ui.horizontal(|ui| {
                        let data = &mut self.store.as_mut().unwrap().data;
                        if ui.button("Add").clicked() && !name.trim().is_empty() {
                            let value = name.trim().to_string();
                            if !data.meta.types.contains(&value) { data.meta.types.push(value); changed = true; }
                        }
                        if ui.add_enabled(!selected.is_empty() && !name.trim().is_empty(), egui::Button::new("Rename")).clicked() {
                            let value = name.trim().to_string();
                            if !data.meta.types.contains(&value) || *selected == value {
                                for kind in &mut data.meta.types { if kind == selected { *kind = value.clone(); } }
                                for item in &mut data.passwords { if item.kind == *selected { item.kind = value.clone(); item.updated_at = model::now(); } }
                                if self.category == *selected { self.category = value.clone(); }
                                *selected = value; changed = true;
                            } else { self.error = Some("That category already exists.".into()); }
                        }
                        if ui.add_enabled(!selected.is_empty(), egui::Button::new("Delete unused")).clicked() {
                            if data.passwords.iter().any(|item| item.kind == *selected) { self.error = Some("This category is still used by a password.".into()); }
                            else {
                                data.meta.types.retain(|kind| kind != selected);
                                if self.category == *selected { self.category.clear(); }
                                selected.clear(); name.clear(); changed = true;
                            }
                        }
                    });
                    if ui.button("Done").clicked() { close = true; }
                }
                Dialog::Import { path, password } => {
                    ui.heading("Replace current vault?");
                    ui.label(path.display().to_string());
                    ui.label("Enter this file’s master password. Its contents will replace your current notes and passwords after validation.");
                    ui.label("A separate encrypted backup of your current vault will be kept.");
                    ui.add(egui::TextEdit::singleline(&mut **password).password(true).hint_text("Imported vault password"));
                    ui.horizontal(|ui| {
                        if ui.button("Authenticate and replace").clicked() && self.save_now() {
                            match self.store.as_mut().unwrap().import(path, password) {
                                Ok(()) => { close = true; self.query.zeroize(); self.category.zeroize(); self.status = "Imported and authenticated vault".into(); ctx.memory_mut(|memory| *memory = egui::Memory::default()); }
                                Err(error) => self.error = Some(format!("Import failed; current vault kept: {error:#}")),
                            }
                        }
                        if ui.button("Cancel").clicked() { close = true; }
                    });
                }
            }
            if let Some(error) = &self.error { ui.add_space(8.0); ui.colored_label(Color32::from_rgb(220, 80, 70), error); }
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
            self.save_now();
        }
        if !close {
            self.dialog = Some(dialog);
        }
    }

    fn tick(&mut self, ctx: &egui::Context) {
        self.poll_unlock();
        if self.store.is_some()
            && self.settings.auto_lock_minutes > 0
            && self.last_activity.elapsed()
                >= Duration::from_secs(u64::from(self.settings.auto_lock_minutes) * 60)
        {
            self.lock(ctx);
        }
        if let Some(since) = self.dirty_since
            && (self.last_edit.elapsed() > Duration::from_millis(600)
                || since.elapsed() > Duration::from_secs(5))
            && self.last_save_attempt.elapsed() > Duration::from_secs(1)
        {
            self.save_now();
        }
        if let Err(error) = self.clipboard.clear(false) {
            self.error = Some(format!("Could not clear clipboard: {error}"));
        }
        ctx.request_repaint_after(Duration::from_millis(250));
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
                    ui.colored_label(Color32::from_rgb(220, 80, 70), error);
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
                if self.store.is_some() && self.dirty_since.is_some() {
                    ui.horizontal(|ui| {
                        if ui.button("Export unsaved vault…").clicked() {
                            self.export_snapshot();
                        }
                        if ui.button("Discard edits and lock…").clicked() {
                            self.dialog = Some(Dialog::DiscardEdits);
                        }
                    });
                }
            }
            if self.store.is_none() {
                self.auth_ui(ui);
            } else {
                ui.label(RichText::new(&self.status).small().weak());
                ui.add_space(5.0);
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
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.settings.window_size = [rect.width(), rect.height()];
        }
        // Handle close after this frame's edits, so a click-and-close cannot bypass saving.
        if ctx.input(|input| input.viewport().close_requested()) {
            if self.unlocking.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if matches!(self.dialog, Some(Dialog::Entry(_))) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.error = Some("Save or cancel the password entry before closing.".into());
            } else if !self.save_now() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else {
                self.persist_settings();
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.clipboard.clear(true);
        self.store = None;
        self.dialog = None;
        self.auth.zeroize();
        self.confirm.zeroize();
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
