use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant, SystemTime},
};

use chrono::Timelike;
use eframe::egui::{self, RichText};
use rakshamemo::{
    generator,
    model::{self, PasswordItem},
    platform,
    settings::{Language, Settings},
    store::{BackupVersion, SaveReceipt, Store},
    strength::{self, Strength},
};
use zeroize::{Zeroize, Zeroizing};

use crate::i18n::{self, fill, t};

mod notes_page;
mod ornament;
mod password_editor;
mod password_table;
mod settings_page;
mod style;
mod vault_dialogs;

/// Drop all widget state, including TextEdit undo history that may hold
/// secrets. `Memory::options` holds the app's styles and theme preference, so
/// it is kept; resetting it would fall back to egui's default look.
fn forget_editor_state(ctx: &egui::Context) {
    ctx.memory_mut(|memory| {
        let options = std::mem::take(&mut memory.options);
        *memory = egui::Memory::default();
        memory.options = options;
    });
}

const CLIPBOARD_LIFETIME: Duration = Duration::from_secs(30);
const SAVE_RETRY_DELAY: Duration = Duration::from_secs(2);
/// Autosave after this long without edits...
const AUTOSAVE_IDLE: Duration = Duration::from_millis(800);
/// ...but never hold changes longer than this during continuous typing.
const AUTOSAVE_MAX_DELAY: Duration = Duration::from_secs(5);

/// The ticks below run at least once a minute (the clock's wake-up), so a
/// longer gap means the computer was asleep.
const SLEEP_GAP: Duration = Duration::from_secs(3 * 60);
/// How often to ask Windows whether the session is locked.
const SESSION_POLL: Duration = Duration::from_secs(5);

/// The toolbar clock: weekday, date, and minutes. Hover shows the long date.
fn clock(ui: &mut egui::Ui, clock_24h: bool) {
    let (text, long) = i18n::clock(&chrono::Local::now(), clock_24h);
    ui.label(RichText::new(text).weak()).on_hover_text(long);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Shortcut {
    New,
    Find,
}

/// Session UI state that is not part of the vault. Reset when locking.
#[derive(Default)]
struct Transient {
    /// A keyboard shortcut waiting for the open page to handle it.
    shortcut: Option<Shortcut>,
    note_search: notes_page::SearchCache,
    /// Put the cursor in a newly created note's title.
    focus_note_title: bool,
    /// Move the cursor from a note's title to its body (after Enter).
    focus_note_body: bool,
    /// Strength of the master password being chosen on the create screen.
    master_strength: Option<Strength>,
    /// Wall-clock time of the previous tick, for sleep detection.
    last_tick: Option<SystemTime>,
    session_checked: Option<Instant>,
    /// The emblem's texture, uploaded the first time it is drawn.
    logo: Option<egui::TextureHandle>,
}

/// Time until the clock's minute changes, so it can update without polling.
fn until_next_minute() -> Duration {
    let now = chrono::Local::now();
    let elapsed = u64::from(now.second()) * 1_000 + u64::from(now.timestamp_subsec_millis());
    Duration::from_millis(60_000_u64.saturating_sub(elapsed) + 20)
}

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
    strength: Option<Strength>,
    /// Put the cursor in the title of a newly added entry.
    focus_title: bool,
}

impl EntryEditor {
    fn new(item: PasswordItem) -> Self {
        let mut editor = Self {
            item,
            show_password: false,
            generator: generator::Options::default(),
            strength: None,
            focus_title: false,
        };
        editor.refresh_strength();
        editor
    }

    /// Scored only when the password changes; the title and username count
    /// against it, since passwords based on them are easy to guess.
    fn refresh_strength(&mut self) {
        self.strength = strength::estimate(
            &self.item.password,
            &[&self.item.title, &self.item.username],
        );
    }
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
    ChangePassword(Box<vault_dialogs::PasswordChange>),
    Versions {
        versions: Vec<BackupVersion>,
        selected: Option<usize>,
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
            if !force && time.elapsed() < CLIPBOARD_LIFETIME {
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
    fn expires_in(&self) -> Option<Duration> {
        self.copied
            .as_ref()
            .map(|(_, time)| CLIPBOARD_LIFETIME.saturating_sub(time.elapsed()))
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
    transient: Transient,
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
                warning = Some(fill(t().could_not_read_startup, &[&format!("{error:#}")]));
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
            status: t().locked.into(),
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
            transient: Transient::default(),
        }
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.dirty_since.get_or_insert_with(Instant::now);
        self.last_edit = Instant::now();
        self.status = t().saving_changes.into();
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
                    self.status = t().all_changes_saved.into();
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
                self.error = Some(fill(t().could_not_save, &[&i18n::describe(&error)]));
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
            || (self.save_failed && self.last_save_attempt.elapsed() < SAVE_RETRY_DELAY)
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
        // Edits made from now on belong to the next save, so the maximum
        // autosave delay restarts here.
        self.dirty_since = Some(Instant::now());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job.run());
            ctx.request_repaint();
        });
    }

    /// Time left before unsaved edits should be written, or `None` when there
    /// is nothing to save. Typing defers the save; a background window saves now.
    fn autosave_wait(&self, focused: bool) -> Option<Duration> {
        let dirty_since = self.dirty_since?;
        if !focused {
            return Some(Duration::ZERO);
        }
        Some(
            AUTOSAVE_IDLE
                .saturating_sub(self.last_edit.elapsed())
                .min(AUTOSAVE_MAX_DELAY.saturating_sub(dirty_since.elapsed())),
        )
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(true));
        if self
            .autosave_wait(focused)
            .is_some_and(|wait| wait.is_zero())
        {
            self.queue_save(ctx);
        }
    }

    fn set_dark_mode(&mut self, ctx: &egui::Context, dark: bool) {
        if dark != self.settings.dark_mode {
            self.settings.dark_mode = dark;
            style::apply(ctx, dark);
            self.persist_settings();
        }
    }

    /// Pages call this every frame with the selected language; only an actual
    /// change is applied and saved.
    fn set_language(&mut self, language: Language) {
        if language != i18n::language() {
            self.settings.language = Some(language);
            i18n::set(language);
            self.persist_settings();
        }
    }

    /// A compact language picker for the header, beside the theme toggle.
    fn language_menu(&mut self, ui: &mut egui::Ui) {
        let mut language = i18n::language();
        egui::ComboBox::from_id_salt("header_language")
            .selected_text(language.code())
            .width(54.0)
            .show_ui(ui, |ui| {
                for option in Language::ALL {
                    ui.selectable_value(&mut language, option, option.native_name());
                }
            })
            .response
            .on_hover_text(t().language);
        self.set_language(language);
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
                self.status = t().all_changes_saved.into();
                if self.save_failed {
                    self.error = None;
                }
                self.save_failed = false;
                true
            }
            Err(error) => {
                self.save_failed = true;
                self.error = Some(fill(t().could_not_save, &[&i18n::describe(&error)]));
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
        self.transient = Transient::default();
        self.status = t().locked.into();
        if let Err(error) = self.clipboard.clear(true) {
            self.error = Some(fill(t().locked_but_clipboard_failed, &[&error]));
        }
        // Drop text editor state, including undo history. Framework/GPU allocations are not guaranteed to be wiped.
        forget_editor_state(ctx);
        ctx.clear_animations();
    }

    fn persist_settings(&mut self) {
        if let Err(error) = self.settings.save(&self.settings_path) {
            self.error = Some(fill(t().could_not_save_settings, &[&format!("{error:#}")]));
        }
    }

    fn copy(&mut self, text: String) {
        match self.clipboard.copy(text) {
            Ok(()) => self.status = t().copied_message.into(),
            Err(error) => self.error = Some(fill(t().could_not_copy, &[&error])),
        }
    }

    fn export_snapshot(&mut self) {
        // Export remains available even when the active vault cannot be saved.
        self.complete_pending_save(true);
        let destination = rfd::FileDialog::new()
            .set_file_name("rakshamemo-backup.json")
            .add_filter(t().encrypted_vault_filter, &["json"])
            .save_file();
        self.after_blocking_dialog();
        if let Some(path) = destination {
            match self.store.as_mut().unwrap().export(path) {
                Ok(()) => self.status = t().exported.into(),
                Err(error) => self.error = Some(fill(t().export_failed, &[&format!("{error:#}")])),
            }
        }
    }

    /// Native file dialogs block the event loop. Forget the last tick so the
    /// pause is not mistaken for the computer sleeping.
    fn after_blocking_dialog(&mut self) {
        self.transient.last_tick = None;
    }

    /// Whether the user has stepped away: the window is minimized, the
    /// computer slept, or the Windows session is locked. Always updates the
    /// tick bookkeeping, even while the vault is locked.
    fn away(&mut self, ctx: &egui::Context) -> bool {
        let now = SystemTime::now();
        let slept = self
            .transient
            .last_tick
            .and_then(|last| now.duration_since(last).ok())
            .is_some_and(|gap| gap >= SLEEP_GAP);
        self.transient.last_tick = Some(now);
        let minimized = ctx.input(|input| input.viewport().minimized == Some(true));
        let poll = self
            .transient
            .session_checked
            .is_none_or(|checked| checked.elapsed() >= SESSION_POLL);
        let session_locked = poll && {
            self.transient.session_checked = Some(Instant::now());
            platform::session_locked()
        };
        minimized || slept || session_locked
    }

    fn start_unlock(&mut self, create: bool, ctx: &egui::Context) {
        if self.auth.is_empty() {
            self.error = Some(t().enter_master_password.into());
            return;
        }
        if create && (self.auth.chars().count() < 12 || *self.auth != *self.confirm) {
            self.error = Some(t().create_password_rules.into());
            return;
        }
        let password = std::mem::replace(&mut self.auth, Zeroizing::new(String::new()));
        self.confirm.zeroize();
        self.transient.master_strength = None;
        // TextEdit retains undo history even for masked fields. Drop it before
        // handing the password to the short-lived derivation worker.
        forget_editor_state(ctx);
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
                self.status = t().vault_unlocked.into();
                self.error = None;
            }
            Ok(Err(error)) => {
                self.unlocking = None;
                self.auth_focus = true;
                self.error = Some(fill(t().could_not_unlock, &[&i18n::describe(&error)]));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.unlocking = None;
                self.auth_focus = true;
                self.error = Some(t().unlock_worker_stopped.into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    fn auth_ui(&mut self, ui: &mut egui::Ui) {
        let create = !self.vault_path.exists();
        style::page(ui);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if style::theme_toggle(ui, self.settings.dark_mode).clicked() {
                    self.set_dark_mode(ui.ctx(), !self.settings.dark_mode);
                }
                self.language_menu(ui);
                clock(ui, self.settings.clock_24h);
            });
        });
        // Scrolls only when the window is too short for the form and its messages.
        egui::ScrollArea::vertical()
            .id_salt("unlock_page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // The lamp glow and mandala sit behind the form; their slot is
                // reserved now and filled once the form's position is known.
                let backdrop = ui.painter().add(egui::Shape::Noop);
                ui.vertical_centered(|ui| {
                    // Approximate content height (emblem, title, card, and footer) for centering.
                    let content_height = if create { 650.0 } else { 530.0 };
                    ui.add_space(((ui.available_height() - content_height) / 2.0).max(16.0));
                    ornament::logo(ui, &mut self.transient.logo, 72.0);
                    ui.add_space(4.0);
                    ornament::wordmark(ui, 34.0);
                    ui.weak(t().tagline);
                    ui.add_space(20.0);
                    let card = ornament::card(ui, 24, |ui| {
                        ui.set_width(340.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            self.auth_form(ui, create);
                        });
                    })
                    .response
                    .rect;
                    // A lotus crowns the card, set into its top edge.
                    let gap = ui.visuals().window_fill;
                    let top = card.center_top();
                    ui.painter().circle_filled(top, 13.0, gap);
                    ui.painter().circle_stroke(
                        top,
                        13.0,
                        egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
                    );
                    ornament::lotus(
                        ui.painter(),
                        top + egui::vec2(0.0, 6.0),
                        13.0,
                        ornament::gold(ui, 0.95),
                        gap,
                    );
                    let center = card.center();
                    let glow = ornament::gold(ui, if ui.visuals().dark_mode { 0.16 } else { 0.22 });
                    let lines =
                        ornament::gold(ui, if ui.visuals().dark_mode { 0.12 } else { 0.18 });
                    let mut shapes = vec![ornament::glow(center, 380.0, glow)];
                    shapes.extend(ornament::mandala(center, 250.0, lines));
                    ui.painter().set(backdrop, egui::Shape::Vec(shapes));
                    ui.add_space(16.0);
                    let path = self.vault_path.display().to_string();
                    ui.scope(|ui| {
                        ui.set_max_width(420.0);
                        ui.add(egui::Label::new(RichText::new(&path).small().weak()).truncate())
                            .on_hover_text(path);
                    });
                    ui.weak(t().vault_stays_local);
                });
            });
    }

    fn auth_form(&mut self, ui: &mut egui::Ui, create: bool) {
        let text = t();
        ui.heading(if create {
            text.create_vault_title
        } else {
            text.unlock_vault_title
        });
        ui.weak(if create {
            text.create_vault_intro
        } else {
            text.unlock_vault_intro
        });
        ui.add_space(12.0);
        fn field(text: &mut String) -> egui::TextEdit<'_> {
            egui::TextEdit::singleline(text)
                .password(true)
                .desired_width(f32::INFINITY)
                .min_size(egui::vec2(0.0, 36.0))
                .margin(egui::Margin::symmetric(10, 8))
        }
        ui.add_enabled_ui(self.unlocking.is_none(), |ui| {
            let response = ui.add(
                field(&mut self.auth)
                    .hint_text(text.master_password_hint)
                    .id_salt("master"),
            );
            if self.auth_focus {
                response.request_focus();
                self.auth_focus = false;
            }
            if create && response.changed() {
                self.transient.master_strength = strength::estimate(&self.auth, &[]);
            }
            if create {
                ui.add(
                    field(&mut self.confirm)
                        .hint_text(text.confirm_password_hint)
                        .id_salt("confirm"),
                );
                let length = self.auth.chars().count();
                if let Some(strength) = &self.transient.master_strength
                    && length >= 12
                {
                    style::strength_meter(ui, strength);
                }
                let (hint, color) = if length >= 12 {
                    (String::new(), ui.visuals().weak_text_color())
                } else {
                    (
                        fill(text.at_least_12, &[&length]),
                        if length == 0 {
                            ui.visuals().weak_text_color()
                        } else {
                            ui.visuals().warn_fg_color
                        },
                    )
                };
                if !hint.is_empty() {
                    ui.label(RichText::new(hint).color(color));
                }
                if !self.confirm.is_empty() && *self.confirm != *self.auth {
                    ui.colored_label(ui.visuals().warn_fg_color, text.passwords_do_not_match);
                }
                ui.weak(text.no_recovery);
            }
            ui.add_space(8.0);
            let label = if create {
                text.create_vault_button
            } else {
                text.unlock_button
            };
            let clicked = style::primary(ui, |ui| {
                ui.add_sized([ui.available_width(), 36.0], egui::Button::new(label))
            })
            .clicked();
            if clicked
                || (self.unlocking.is_none()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter)))
            {
                self.start_unlock(create, ui.ctx());
            }
        });
        if self.unlocking.is_some() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(text.deriving_key);
            });
        }
        if let Some(error) = &self.error {
            ui.add_space(4.0);
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    fn error_banner(&mut self, ui: &mut egui::Ui) {
        let Some(error) = self.error.clone() else {
            return;
        };
        let color = ui.visuals().error_fg_color;
        egui::Frame::new()
            .fill(color.gamma_multiply(0.12))
            .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.6)))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 90.0).max(120.0);
                    ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                        ui.add(egui::Label::new(RichText::new(error).color(color)).wrap());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t().dismiss).clicked() {
                            self.error = None;
                        }
                    });
                });
                if self.dirty_since.is_some() {
                    ui.horizontal(|ui| {
                        if ui.button(t().export_unsaved).clicked() {
                            self.export_snapshot();
                        }
                        if style::danger_button(ui, t().discard_and_lock_ellipsis).clicked() {
                            self.dialog = Some(Dialog::DiscardEdits);
                        }
                    });
                }
            });
        ui.add_space(6.0);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ornament::logo(ui, &mut self.transient.logo, 26.0);
            ornament::wordmark(ui, 21.0);
            ui.add_space(10.0);
            for (tab, label) in [
                (Tab::Notes, t().tab_notes),
                (Tab::Passwords, t().tab_passwords),
                (Tab::Settings, t().tab_settings),
            ] {
                if style::nav_tab(ui, self.tab == tab, label).clicked() {
                    self.register_open_category();
                    if self.tab != tab {
                        self.password_table.clear_sensitive();
                    }
                    self.tab = tab;
                    self.query.zeroize();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t().lock).on_hover_text("Ctrl/Cmd+L").clicked() {
                    self.lock(ui.ctx());
                }
                if style::theme_toggle(ui, self.settings.dark_mode).clicked() {
                    self.set_dark_mode(ui.ctx(), !self.settings.dark_mode);
                }
                self.language_menu(ui);
                // A fixed slot keeps the clock still as the status text changes.
                ui.allocate_ui_with_layout(
                    egui::vec2(110.0, ui.spacing().interact_size.y),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| self.save_status(ui),
                );
                // The clock is the first thing to go in a narrow window.
                if ui.available_width() > 220.0 {
                    ui.separator();
                    clock(ui, self.settings.clock_24h);
                }
            });
        });
        ornament::frieze(ui);
    }

    fn save_status(&self, ui: &mut egui::Ui) {
        let text = t();
        let (label, color) = if self.save_failed {
            (text.status_save_failed, ui.visuals().error_fg_color)
        } else if self.dirty_since.is_some() || self.pending_save.is_some() {
            (text.status_saving, ui.visuals().warn_fg_color)
        } else if self.clipboard.copied.is_some() {
            // Until the copied value expires from the clipboard.
            (text.status_copied, ui.visuals().hyperlink_color)
        } else {
            (text.status_saved, ui.visuals().weak_text_color())
        };
        ui.label(RichText::new(format!("● {label}")).color(color))
            .on_hover_text(if self.save_failed {
                self.error.as_deref().unwrap_or(text.status_save_failed)
            } else {
                &self.status
            });
    }

    fn new_password(&mut self) {
        self.finish_password_table_edit();
        self.password_table.clear_sensitive();
        self.query.zeroize();
        let item = PasswordItem {
            title: t().untitled.into(),
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
        // The table lists it as "Untitled"; the editor starts empty, with the
        // cursor in the title, so typing replaces the placeholder.
        let mut editor = EntryEditor::new(PasswordItem {
            title: String::new(),
            ..item
        });
        editor.focus_title = true;
        self.password_editor = Some(Box::new(editor));
    }

    fn passwords_ui(&mut self, ui: &mut egui::Ui) {
        style::page(ui);
        let shortcut = self.transient.shortcut.take();
        if let Some(mut editor) = self.password_editor.take() {
            let back = self.password_editor_page(ui, &mut editor);
            if shortcut.is_some() && !back {
                self.register_category(&editor.item.kind);
            }
            match shortcut {
                None if !back => self.password_editor = Some(editor),
                None => {}
                Some(Shortcut::New) => self.new_password(),
                // Leave the editor; the table focuses its search next frame.
                Some(Shortcut::Find) => {
                    self.transient.shortcut = Some(Shortcut::Find);
                    ui.ctx().request_repaint();
                }
            }
            return;
        }
        if shortcut == Some(Shortcut::New) {
            self.new_password();
            ui.ctx().request_repaint();
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
            shortcut == Some(Shortcut::Find),
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
                Action::New => self.new_password(),
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
                        self.password_editor = Some(Box::new(EntryEditor::new(item.clone())));
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
                value.push_str(t().untitled);
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
            "" => t().untitled.into(),
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
        let mut next = None;
        let deleting_note = matches!(dialog, Dialog::DeleteNote(_));
        // Escape cancels any dialog. The password-change dialog handles it
        // itself, so it can refuse while its key derivation is running.
        if !matches!(dialog, Dialog::ChangePassword(_))
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            return;
        }
        let text = t();
        egui::Modal::new(egui::Id::new("vault_dialog")).show(ctx, |ui| {
            style::page(ui);
            ui.set_width((ctx.content_rect().width() - 64.0).clamp(280.0, 640.0));
            let max_height = (ctx.content_rect().height() - 96.0).max(160.0);
            // Without a minimum, the scroll area keeps the height it opened
            // with when content grows (for example the strength meter).
            egui::ScrollArea::vertical()
                .max_height(max_height)
                .min_scrolled_height(max_height)
                .show(ui, |ui| {
                    match &mut dialog {
                        Dialog::DiscardEdits => {
                            ui.heading(text.discard_title);
                            ui.label(text.discard_body);
                            ui.horizontal(|ui| {
                                if ui.button(text.cancel).clicked() {
                                    close = true;
                                }
                                if style::destructive_button(ui, text.discard_and_lock).clicked() {
                                    discard = true;
                                    close = true;
                                }
                            });
                        }
                        Dialog::DeleteNote(id) | Dialog::DeletePassword(id) => {
                            let data = &self.store.as_ref().unwrap().data;
                            let title = if deleting_note {
                                data.notes
                                    .items
                                    .iter()
                                    .find(|note| note.id == *id)
                                    .map(|note| note.title.as_str())
                            } else {
                                data.passwords
                                    .iter()
                                    .find(|item| item.id == *id)
                                    .map(|item| item.title.as_str())
                            }
                            .filter(|title| !title.is_empty())
                            .unwrap_or(text.untitled);
                            let (heading, action) = if deleting_note {
                                (text.delete_note_title, text.delete_note)
                            } else {
                                (text.delete_password_title, text.delete_password)
                            };
                            ui.heading(heading);
                            ui.label(RichText::new(title).strong());
                            ui.label(text.delete_body);
                            ui.horizontal(|ui| {
                                if ui.button(text.cancel).clicked() {
                                    close = true;
                                }
                                if style::destructive_button(ui, action).clicked() {
                                    close = true;
                                    changed = true;
                                }
                            });
                        }
                        Dialog::Types { selected, name } => {
                            ui.heading(text.manage_categories);
                            ui.label(text.manage_categories_body);
                            ui.label(text.manage_categories_how);
                            ui.add_space(8.0);
                            egui::ComboBox::from_id_salt("manage_categories")
                                .selected_text(if selected.is_empty() {
                                    text.choose_category
                                } else {
                                    selected.as_str()
                                })
                                .show_ui(ui, |ui| {
                                    for kind in &self.store.as_ref().unwrap().data.meta.types {
                                        if ui
                                            .selectable_value(selected, kind.clone(), kind)
                                            .clicked()
                                        {
                                            *name = kind.clone();
                                        }
                                    }
                                });
                            ui.label(text.category_name);
                            ui.add(
                                egui::TextEdit::singleline(name)
                                    .id_salt("rename_category")
                                    .hint_text(text.rename_hint)
                                    .desired_width(f32::INFINITY),
                            );
                            let usage = self
                                .store
                                .as_ref()
                                .unwrap()
                                .data
                                .passwords
                                .iter()
                                .filter(|item| item.kind == *selected)
                                .count();
                            if !selected.is_empty() {
                                ui.label(i18n::plural(usage, text.used_by));
                            }
                            ui.horizontal(|ui| {
                                let data = &mut self.store.as_mut().unwrap().data;
                                if ui
                                    .add_enabled(
                                        !selected.is_empty() && !name.trim().is_empty(),
                                        egui::Button::new(text.rename),
                                    )
                                    .clicked()
                                {
                                    let value = name.trim().to_string();
                                    if !data.meta.types.contains(&value) || *selected == value {
                                        for kind in &mut data.meta.types {
                                            if kind == selected {
                                                *kind = value.clone();
                                            }
                                        }
                                        for item in &mut data.passwords {
                                            if item.kind == *selected {
                                                item.kind = value.clone();
                                                item.updated_at = model::now();
                                            }
                                        }
                                        if self.category == *selected {
                                            self.category = value.clone();
                                        }
                                        *selected = value;
                                        changed = true;
                                    } else {
                                        self.error = Some(text.category_exists.into());
                                    }
                                }
                                if ui
                                    .add_enabled_ui(!selected.is_empty() && usage == 0, |ui| {
                                        style::danger_button(ui, text.remove_unused_category)
                                    })
                                    .inner
                                    .on_disabled_hover_text(text.remove_unused_hover)
                                    .clicked()
                                {
                                    if data.passwords.iter().any(|item| item.kind == *selected) {
                                        self.error = Some(text.category_in_use.into());
                                    } else {
                                        data.meta.types.retain(|kind| kind != selected);
                                        if self.category == *selected {
                                            self.category.clear();
                                        }
                                        selected.clear();
                                        name.clear();
                                        changed = true;
                                    }
                                }
                            });
                            if style::primary_button(ui, text.done).clicked() {
                                close = true;
                            }
                        }
                        Dialog::ChangePassword(change) => {
                            close = self.change_password_ui(ui, change)
                        }
                        Dialog::Versions { versions, selected } => {
                            (close, next) = vault_dialogs::versions_ui(ui, versions, selected);
                        }
                        Dialog::Import { path, password } => {
                            ui.heading(text.import_title);
                            ui.label(path.display().to_string());
                            ui.label(text.import_body);
                            ui.label(text.import_backup_kept);
                            ui.add(
                                egui::TextEdit::singleline(&mut **password)
                                    .password(true)
                                    .hint_text(text.import_password_hint),
                            );
                            ui.horizontal(|ui| {
                                if ui.button(text.cancel).clicked() {
                                    close = true;
                                }
                                if style::destructive_button(ui, text.authenticate_and_replace)
                                    .clicked()
                                    && self.save_now()
                                {
                                    match self.store.as_mut().unwrap().import(path, password) {
                                        Ok(()) => {
                                            close = true;
                                            self.password_editor = None;
                                            self.query.zeroize();
                                            self.category.zeroize();
                                            self.password_table =
                                                password_table::TableState::default();
                                            self.status = text.imported.into();
                                            self.transient.note_search = Default::default();
                                            forget_editor_state(ctx);
                                        }
                                        Err(error) => {
                                            self.error = Some(fill(
                                                text.import_failed,
                                                &[&i18n::describe(&error)],
                                            ))
                                        }
                                    }
                                }
                            });
                        }
                    }
                    if let Some(error) = &self.error {
                        ui.add_space(8.0);
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                });
        });
        if discard {
            self.finish_lock(ctx);
            return;
        }
        if next.is_some() {
            self.dialog = next;
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
        if self.away(ctx) && self.store.is_some() && self.settings.lock_when_away {
            self.lock(ctx);
        }
        if self.store.is_some()
            && self.settings.auto_lock_minutes > 0
            && self.last_activity.elapsed()
                >= Duration::from_secs(u64::from(self.settings.auto_lock_minutes) * 60)
        {
            self.lock(ctx);
        }
        self.autosave(ctx);
        if let Err(error) = self.clipboard.clear(false) {
            self.error = Some(fill(t().could_not_clear_clipboard, &[&error]));
        }
        // Background workers request a repaint when they finish, so only
        // timers need a wake-up. An idle window otherwise stays asleep.
        let auto_lock = (self.store.is_some() && self.settings.auto_lock_minutes > 0).then(|| {
            Duration::from_secs(u64::from(self.settings.auto_lock_minutes) * 60)
                .saturating_sub(self.last_activity.elapsed())
        });
        let save_retry = (self.save_failed && self.dirty_since.is_some())
            .then(|| SAVE_RETRY_DELAY.saturating_sub(self.last_save_attempt.elapsed()));
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(true));
        let autosave = self
            .pending_save
            .is_none()
            .then(|| self.autosave_wait(focused))
            .flatten();
        if let Some(delay) = [
            auto_lock,
            save_retry,
            autosave,
            self.clipboard.expires_in(),
            Some(until_next_minute()),
        ]
        .into_iter()
        .flatten()
        .min()
        {
            // A floor avoids a busy loop if a deadline action keeps failing.
            ctx.request_repaint_after(delay.max(Duration::from_millis(250)));
        }
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
            if self.dialog.is_none() {
                let shortcut = ctx.input_mut(|input| {
                    if input.consume_key(egui::Modifiers::COMMAND, egui::Key::N) {
                        Some(Shortcut::New)
                    } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::F) {
                        Some(Shortcut::Find)
                    } else {
                        None
                    }
                });
                if shortcut.is_some() {
                    self.transient.shortcut = shortcut;
                }
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.store.is_some() {
                self.toolbar(ui);
            }
            // The unlock card shows its own errors next to the password field.
            if self.store.is_some() {
                self.error_banner(ui);
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
        // Settings has no list to search or add to.
        if self.store.is_none() || self.tab == Tab::Settings {
            self.transient.shortcut = None;
        }
        // Start a background save once typing pauses; the UI never waits on it.
        self.autosave(&ctx);
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
