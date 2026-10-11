use super::*;

const PASSWORD: &str = "application test passphrase";

/// Keep all persistence inside a temporary directory and bypass Notepad::new,
/// which reads the user's real OS startup registration.
pub(super) fn fixture() -> (tempfile::TempDir, Notepad, egui::Context) {
    let directory = tempfile::tempdir().unwrap();
    let vault_path = directory.path().join("vault.json");
    let store = Store::create(&vault_path, PASSWORD).unwrap();
    let instance = platform::SingleInstance::acquire(&vault_path).unwrap();
    let now = Instant::now();
    let app = Notepad {
        vault_path,
        settings_path: directory.path().join("vault.json.settings.json"),
        settings: Settings::default(),
        _instance: instance,
        custom_path: true,
        store: Some(store),
        auth: Zeroizing::new(String::new()),
        confirm: Zeroizing::new(String::new()),
        unlocking: None,
        tab: Tab::Notes,
        query: String::new(),
        category: String::new(),
        dialog: None,
        password_editor: None,
        error: None,
        status: "Vault unlocked".into(),
        dirty_since: None,
        last_edit: now,
        last_activity: now,
        last_save_attempt: now,
        startup_enabled: false,
        clipboard: Clipboard::new(),
        auth_focus: false,
        password_table: password_table::TableState::default(),
        pending_save: None,
        revision: 0,
        save_failed: false,
    };
    let context = egui::Context::default();
    style::apply(&context, app.settings.dark_mode);
    (directory, app, context)
}

fn add_pending_note(app: &mut Notepad, content: &str) {
    let note = Note {
        content: content.into(),
        ..Note::new("Test note")
    };
    let data = &mut app.store.as_mut().unwrap().data;
    data.notes.active_id = note.id.clone();
    data.notes.items.push(note);
    app.changed();
}

fn open_password_entry(app: &mut Notepad) {
    let editor = EntryEditor {
        item: PasswordItem {
            title: "Current password".into(),
            password: "live secret".into(),
            ..Default::default()
        },
        show_password: true,
        generator: generator::Options::default(),
    };
    app.store
        .as_mut()
        .unwrap()
        .data
        .passwords
        .push(editor.item.clone());
    app.changed();
    app.password_editor = Some(Box::new(editor));
}

pub(super) fn input_frame(
    context: &egui::Context,
    events: Vec<egui::Event>,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    input_frame_sized(context, egui::vec2(1_100.0, 740.0), events, render)
}

pub(super) fn input_frame_sized(
    context: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            focused: true,
            ..Default::default()
        },
        render,
    );
    output.textures_delta.clear();
    output
}

fn render_password_page(app: &mut Notepad, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        app.toolbar(ui);
        app.passwords_ui(ui);
    });
}

fn painted_text<'a>(output: &'a egui::FullOutput, value: &str) -> &'a egui::epaint::ClippedShape {
    output
        .shapes
        .iter()
        .find(
            |shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == value),
        )
        .unwrap_or_else(|| panic!("Expected painted text {value:?}"))
}

fn assert_visible_text(output: &egui::FullOutput, value: &str, viewport: egui::Rect) {
    let shape = painted_text(output, value);
    let egui::Shape::Text(text) = &shape.shape else {
        unreachable!();
    };
    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
    assert!(
        viewport.contains_rect(bounds),
        "{value:?} is outside the viewport: {bounds:?}"
    );
    assert!(
        shape.clip_rect.contains_rect(bounds.shrink(0.5)),
        "{value:?} is clipped: {bounds:?}, {:?}",
        shape.clip_rect
    );
}

fn password_field_rect(
    app: &mut Notepad,
    context: &egui::Context,
    size: egui::Vec2,
    output: &egui::FullOutput,
    value: &str,
) -> egui::Rect {
    let egui::Shape::Text(text) = &painted_text(output, value).shape else {
        unreachable!()
    };
    let position = text.pos + egui::vec2(5.0, 8.0);
    for pressed in [true, false] {
        input_frame_sized(
            context,
            size,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| render_password_page(app, ui),
        );
    }
    let focused = context
        .memory(|memory| memory.focused())
        .expect("Field receives keyboard focus");
    context
        .read_response(focused)
        .expect("Focused field has a widget response")
        .rect
}

fn focus_password_text(app: &mut Notepad, context: &egui::Context, value: &str) -> egui::Id {
    // Let the page settle its layout before locating the field's actual text.
    input_frame(context, Vec::new(), |ui| app.passwords_ui(ui));
    let output = input_frame(context, Vec::new(), |ui| app.passwords_ui(ui));
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == value => {
                Some(text.pos + egui::vec2(5.0, 8.0))
            }
            _ => None,
        })
        .expect("The editable field should be visible on the page");
    for pressed in [true, false] {
        input_frame(
            context,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| app.passwords_ui(ui),
        );
    }
    context
        .memory(|memory| memory.focused())
        .expect("Clicking the field should focus its editor")
}

#[test]
fn setup_and_unlock_fields_accept_text_after_requesting_keyboard_focus() {
    let (directory, mut app, context) = fixture();
    app.lock(&context);
    for create in [false, true] {
        if create {
            app.vault_path = directory.path().join("new-vault.json");
            app.auth.zeroize();
            app.auth_focus = true;
        }
        input_frame(&context, Vec::new(), |ui| app.auth_ui(ui));
        assert!(context.memory(|memory| memory.focused()).is_some());
        assert!(app.auth.is_empty());

        input_frame(
            &context,
            vec![egui::Event::Text("Synthetic password 日本語 🔐".into())],
            |ui| app.auth_ui(ui),
        );

        assert_eq!(app.auth.as_str(), "Synthetic password 日本語 🔐");
        assert!(app.confirm.is_empty());
        assert!(
            app.unlocking.is_none(),
            "Typing alone must not submit the form"
        );
    }
}

#[test]
fn clicking_a_note_editor_accepts_text_and_marks_the_note_for_autosave() {
    let (_directory, mut app, context) = fixture();
    add_pending_note(&mut app, "Original note");
    assert!(app.save_now());
    input_frame(&context, Vec::new(), |ui| app.notes_ui(ui));
    // At the fixed viewport size, this lies inside the multiline editor below
    // the title and to the right of the notebooks panel.
    let position = egui::pos2(450.0, 140.0);
    for pressed in [true, false] {
        input_frame(
            &context,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| app.notes_ui(ui),
        );
    }
    assert!(context.memory(|memory| memory.focused()).is_some());
    input_frame(
        &context,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("Edited through keyboard input 日本語 🔐".into()),
        ],
        |ui| app.notes_ui(ui),
    );
    assert_eq!(
        app.store.as_ref().unwrap().data.notes.items[0].content,
        "Edited through keyboard input 日本語 🔐"
    );
    assert!(app.query.is_empty());
    assert!(app.dirty_since.is_some());
    assert!(app.save_now());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "Edited through keyboard input 日本語 🔐"
    );
}

#[test]
fn locking_drains_background_save_and_flushes_newer_edits_then_clears_gui_state() {
    let (_directory, mut app, context) = fixture();
    add_pending_note(&mut app, "Keep this note 日本語");
    app.queue_save(&context);
    assert!(app.pending_save.is_some());
    open_password_entry(&mut app);
    app.query = "private search".into();
    app.category = "private category".into();
    app.auth.push_str("temporary master password");
    app.confirm.push_str("temporary confirmation");
    let private_state_id = egui::Id::new("private editor state");
    context.data_mut(|data| data.insert_temp(private_state_id, "editor history".to_owned()));

    app.lock(&context);

    assert!(app.store.is_none());
    assert!(app.dialog.is_none());
    assert!(app.password_editor.is_none());
    assert!(app.pending_save.is_none());
    assert!(app.dirty_since.is_none());
    assert!(app.auth.is_empty() && app.confirm.is_empty());
    assert!(app.query.is_empty() && app.category.is_empty());
    assert!(app.auth_focus);
    assert!(
        context
            .data(|data| data.get_temp::<String>(private_state_id))
            .is_none()
    );
    let saved = Store::open(&app.vault_path, PASSWORD).unwrap();
    assert_eq!(saved.data.notes.items[0].content, "Keep this note 日本語");
    assert_eq!(saved.data.passwords[0].password, "live secret");
}

#[test]
fn a_failed_background_save_prevents_lock_and_preserves_pending_data() {
    let (_directory, mut app, context) = fixture();
    let original_file = std::fs::read(&app.vault_path).unwrap();
    let backup = app.store.as_ref().unwrap().backup_path();
    // An existing directory cannot be replaced by the atomic backup file.
    std::fs::create_dir(&backup).unwrap();
    add_pending_note(&mut app, "Unsaved but recoverable");
    open_password_entry(&mut app);
    app.queue_save(&context);
    assert!(app.pending_save.is_some());

    app.lock(&context);

    assert!(app.dirty_since.is_some());
    assert!(app.pending_save.is_none());
    assert!(app.save_failed);
    assert!(app.password_editor.is_some());
    assert_eq!(
        app.store.as_ref().unwrap().data.notes.items[0].content,
        "Unsaved but recoverable"
    );
    assert!(app.error.as_deref().unwrap().contains("Could not save"));
    assert_eq!(std::fs::read(&app.vault_path).unwrap(), original_file);
    // Failed background writes wait briefly before retrying, instead of spawning
    // another failing worker on every paint frame.
    app.queue_save(&context);
    assert!(app.pending_save.is_none());
    // Repairing the cause lets the same pending state save and lock normally.
    std::fs::remove_dir(backup).unwrap();
    app.lock(&context);
    assert!(app.store.is_none());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "Unsaved but recoverable"
    );
}

#[test]
fn idle_lock_runs_without_input_and_respects_the_disabled_setting() {
    let (_directory, mut app, context) = fixture();
    app.settings.auto_lock_minutes = 0;
    app.last_activity = Instant::now() - Duration::from_secs(3_600);
    app.tick(&context);
    assert!(app.store.is_some());

    add_pending_note(&mut app, "Saved by inactivity lock");
    app.settings.auto_lock_minutes = 1;
    open_password_entry(&mut app);
    app.tick(&context);
    assert!(app.store.is_none());
    assert!(app.dialog.is_none());
    assert!(app.password_editor.is_none());
    assert!(app.dirty_since.is_none());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "Saved by inactivity lock"
    );
}

#[test]
fn autosave_queues_immediately_and_an_older_completion_keeps_newer_edits_dirty() {
    let (_directory, mut app, context) = fixture();
    add_pending_note(&mut app, "First edit");
    app.tick(&context);
    assert!(app.pending_save.is_some(), "No typing pause is required");
    let first_revision = app.pending_save.as_ref().unwrap().revision;
    app.store.as_mut().unwrap().data.notes.items[0].content = "Newer edit".into();
    app.changed();
    app.queue_save(&context);
    assert_eq!(app.pending_save.as_ref().unwrap().revision, first_revision);

    assert!(app.complete_pending_save(true));
    assert!(app.dirty_since.is_some());
    assert_eq!(
        app.store.as_ref().unwrap().data.notes.items[0].content,
        "Newer edit"
    );
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "First edit"
    );

    app.tick(&context);
    assert!(app.pending_save.is_some());
    assert!(app.complete_pending_save(true));
    assert!(app.dirty_since.is_none());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "Newer edit"
    );
}

#[test]
fn typing_in_a_password_editor_autosaves_and_keeps_undo_local_to_the_entry() {
    let (_directory, mut app, context) = fixture();
    open_password_entry(&mut app);
    assert!(app.save_now());
    let first_field = focus_password_text(&mut app, &context, "Current password");
    input_frame(
        &context,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("Edited account 日本語".into()),
        ],
        |ui| app.passwords_ui(ui),
    );
    assert_eq!(
        app.store.as_ref().unwrap().data.passwords[0].title,
        "Edited account 日本語"
    );
    assert!(app.password_editor.is_some());
    assert!(app.dirty_since.is_some());
    app.queue_save(&context);
    assert!(app.complete_pending_save(true));
    assert!(app.dirty_since.is_none());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .passwords[0]
            .title,
        "Edited account 日本語"
    );
    assert!(app.password_editor.is_some());

    // Open a different entry without clearing egui's memory. Its undo shortcut
    // must not pull the first account's title into this account and autosave it.
    open_password_entry(&mut app);
    let second_field = focus_password_text(&mut app, &context, "Current password");
    assert_ne!(first_field, second_field);
    input_frame(
        &context,
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: Some(egui::Key::Z),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        |ui| app.passwords_ui(ui),
    );
    let data = &app.store.as_ref().unwrap().data;
    assert_eq!(data.passwords[0].title, "Edited account 日本語");
    assert_eq!(data.passwords[1].title, "Current password");
}

#[test]
fn password_editor_shows_all_core_fields_without_scrolling_at_standard_size() {
    let (_directory, mut app, context) = fixture();
    app.tab = Tab::Passwords;
    open_password_entry(&mut app);
    let editor = app.password_editor.as_mut().unwrap();
    editor.item.username = "synthetic@example.test".into();
    editor.item.kind = "Synthetic category".into();
    editor.item.extra = "https://example.test\nRecovery details remain readable.".into();
    let size = egui::vec2(1_100.0, 740.0);
    for _ in 0..2 {
        input_frame_sized(&context, size, Vec::new(), |ui| {
            render_password_page(&mut app, ui)
        });
    }
    let output = input_frame_sized(&context, size, Vec::new(), |ui| {
        render_password_page(&mut app, ui)
    });
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    for value in [
        "Back to passwords",
        "Title",
        "Username / email",
        "Password",
        "Category (optional)",
        "Extra information",
        "Current password",
        "synthetic@example.test",
        "live secret",
        "Synthetic category",
        "https://example.test\nRecovery details remain readable.",
    ] {
        assert_visible_text(&output, value, viewport);
        let egui::Shape::Text(text) = &painted_text(&output, value).shape else {
            unreachable!()
        };
        for section in &text.galley.job.sections {
            assert_eq!(
                section.format.font_id.size, 15.0,
                "Inconsistent font size for {value:?}"
            );
            assert_eq!(
                section.format.font_id.family,
                egui::FontFamily::Proportional
            );
        }
    }
    let title = password_field_rect(&mut app, &context, size, &output, "Current password");
    let extra = password_field_rect(
        &mut app,
        &context,
        size,
        &output,
        "https://example.test\nRecovery details remain readable.",
    );
    assert!(
        title.width() > size.x * 0.38,
        "Account inputs should fill their column: {title:?}"
    );
    assert!(
        extra.width() > size.x * 0.38,
        "Extra information should fill its column"
    );
    assert!(
        extra.height() > 280.0,
        "Extra information needs a spacious editor"
    );
    assert!(
        app.dialog.is_none(),
        "Password editing must use the page instead of a modal"
    );
}

#[test]
fn narrow_password_editor_uses_the_page_width_and_scrolls_below_fixed_navigation() {
    let (_directory, mut app, context) = fixture();
    app.tab = Tab::Passwords;
    open_password_entry(&mut app);
    app.password_editor.as_mut().unwrap().item.extra =
        "Extra details at the bottom of the page".into();
    let size = egui::vec2(760.0, 520.0);
    for _ in 0..2 {
        input_frame_sized(&context, size, Vec::new(), |ui| {
            render_password_page(&mut app, ui)
        });
    }
    let output = input_frame_sized(&context, size, Vec::new(), |ui| {
        render_password_page(&mut app, ui)
    });
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    assert_visible_text(&output, "Back to passwords", viewport);
    assert_visible_text(&output, "Current password", viewport);
    let title = password_field_rect(&mut app, &context, size, &output, "Current password");
    assert!(
        title.width() > size.x * 0.85,
        "Narrow input bounds: {title:?}"
    );
    let pointer = egui::pos2(700.0, 420.0);
    for _ in 0..16 {
        input_frame_sized(
            &context,
            size,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -100.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| render_password_page(&mut app, ui),
        );
    }
    let output = input_frame_sized(&context, size, Vec::new(), |ui| {
        render_password_page(&mut app, ui)
    });
    assert_visible_text(&output, "Back to passwords", viewport);
    assert_visible_text(&output, "Extra details at the bottom of the page", viewport);
    let extra = password_field_rect(
        &mut app,
        &context,
        size,
        &output,
        "Extra details at the bottom of the page",
    );
    assert!(
        extra.width() > size.x * 0.85,
        "Narrow extra information bounds: {extra:?}"
    );
}

#[test]
fn returning_to_password_table_keeps_live_edits_and_registers_the_final_category() {
    let (_directory, mut app, context) = fixture();
    app.tab = Tab::Passwords;
    open_password_entry(&mut app);
    let field = focus_password_text(&mut app, &context, "Current password");
    assert!(context.memory(|memory| memory.has_focus(field)));
    input_frame(
        &context,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("Account edited before returning".into()),
        ],
        |ui| app.passwords_ui(ui),
    );
    let mut editor = app.password_editor.take().unwrap();
    editor.item.kind = "Finished category".into();
    app.update_entry(&mut editor);
    app.password_editor = Some(editor);
    let output = input_frame(&context, Vec::new(), |ui| app.passwords_ui(ui));
    let egui::Shape::Text(back) = &painted_text(&output, "Back to passwords").shape else {
        unreachable!()
    };
    let position = back.pos + back.galley.size() * 0.5;
    for pressed in [true, false] {
        input_frame(
            &context,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| app.passwords_ui(ui),
        );
    }
    assert!(app.password_editor.is_none());
    app.queue_save(&context);
    assert!(app.complete_pending_save(true));
    let saved = Store::open(&app.vault_path, PASSWORD).unwrap();
    assert_eq!(
        saved.data.passwords[0].title,
        "Account edited before returning"
    );
    assert_eq!(saved.data.passwords[0].kind, "Finished category");
    assert!(
        saved
            .data
            .meta
            .types
            .iter()
            .any(|kind| kind == "Finished category")
    );
}

#[test]
fn live_entry_changes_preserve_empty_titles_without_creating_partial_categories() {
    let (_directory, mut app, context) = fixture();
    open_password_entry(&mut app);
    let Some(mut editor) = app.password_editor.take() else {
        unreachable!();
    };
    editor.item.title = "  ".into();
    editor.item.password = "newly generated secret".into();
    for category in ["P", "Pr", "  Private category  "] {
        editor.item.kind = category.into();
        app.update_entry(&mut editor);
    }
    let data = &app.store.as_ref().unwrap().data;
    assert_eq!(data.passwords.len(), 1);
    assert_eq!(data.passwords[0].title, "Untitled");
    assert_eq!(data.passwords[0].kind, "Private category");
    for category in ["P", "Pr", "Private category"] {
        assert!(!data.meta.types.iter().any(|kind| kind == category));
    }
    app.password_editor = Some(editor);
    // Locking also finalizes a category whose field never lost keyboard focus.
    app.lock(&context);
    let saved = Store::open(&app.vault_path, PASSWORD).unwrap();
    assert_eq!(saved.data.passwords[0].title, "Untitled");
    assert_eq!(saved.data.passwords[0].password, "newly generated secret");
    assert!(
        saved
            .data
            .meta
            .types
            .iter()
            .any(|kind| kind == "Private category")
    );
    assert!(
        !saved
            .data
            .meta
            .types
            .iter()
            .any(|kind| kind == "P" || kind == "Pr")
    );
}

#[test]
fn confirmed_discard_locks_without_overwriting_the_vault() {
    let (_directory, mut app, context) = fixture();
    let original_file = std::fs::read(&app.vault_path).unwrap();
    add_pending_note(&mut app, "Explicitly discarded");
    open_password_entry(&mut app);
    app.finish_lock(&context);
    assert!(app.store.is_none());
    assert!(app.dialog.is_none());
    assert!(app.password_editor.is_none());
    assert!(app.dirty_since.is_none());
    assert_eq!(std::fs::read(&app.vault_path).unwrap(), original_file);
}

#[test]
fn primary_views_and_password_editor_render_without_a_desktop_session() {
    let (_directory, mut app, context) = fixture();
    add_pending_note(&mut app, "A note with Unicode 🔐");
    app.store
        .as_mut()
        .unwrap()
        .data
        .passwords
        .push(PasswordItem {
            title: "Example account".into(),
            username: "person@example.test".into(),
            password: "masked value".into(),
            ..Default::default()
        });
    for tab in [Tab::Notes, Tab::Passwords, Tab::Settings] {
        let mut output = context.run_ui(egui::RawInput::default(), |ui| match tab {
            Tab::Notes => app.notes_ui(ui),
            Tab::Passwords => app.passwords_ui(ui),
            Tab::Settings => app.settings_ui(ui),
        });
        assert!(!output.shapes.is_empty());
        // This smoke test intentionally has no GPU backend to upload textures.
        output.textures_delta.clear();
    }
    open_password_entry(&mut app);
    let mut output = context.run_ui(egui::RawInput::default(), |ui| app.passwords_ui(ui));
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
    assert!(app.password_editor.is_some());
    app.lock(&context);
    let mut output = context.run_ui(egui::RawInput::default(), |ui| app.auth_ui(ui));
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
    assert!(app.store.is_none());
    assert!(app.clipboard.device.is_none());
}

fn render_open_tab(app: &mut Notepad, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        app.toolbar(ui);
        match app.tab {
            Tab::Passwords => app.passwords_ui(ui),
            Tab::Settings => app.settings_ui(ui),
            Tab::Notes => app.notes_ui(ui),
        }
    });
}

fn click_visible_text(app: &mut Notepad, context: &egui::Context, value: &str) {
    input_frame(context, Vec::new(), |ui| render_open_tab(app, ui));
    let output = input_frame(context, Vec::new(), |ui| render_open_tab(app, ui));
    let egui::Shape::Text(text) = &painted_text(&output, value).shape else {
        unreachable!()
    };
    let position = text.pos + text.galley.size() * 0.5;
    for pressed in [true, false] {
        input_frame(
            context,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| render_open_tab(app, ui),
        );
    }
    input_frame(context, Vec::new(), |ui| render_open_tab(app, ui));
}

fn replace_focused_text(app: &mut Notepad, context: &egui::Context, value: &str) {
    input_frame(
        context,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text(value.into()),
        ],
        |ui| render_open_tab(app, ui),
    );
}

#[test]
fn inline_password_changes_reach_encrypted_storage_before_finishing_the_cell() {
    let (_directory, mut app, context) = fixture();
    app.tab = Tab::Passwords;
    app.store
        .as_mut()
        .unwrap()
        .data
        .passwords
        .push(PasswordItem {
            id: "inline-account".into(),
            title: "Inline account".into(),
            password: "original secret".into(),
            ..Default::default()
        });
    click_visible_text(&mut app, &context, "••••••••");
    replace_focused_text(&mut app, &context, "  new secret 日本語  ");
    assert!(
        app.password_editor.is_none(),
        "The cell must not open a full-page editor"
    );
    assert_eq!(
        app.store.as_ref().unwrap().data.passwords[0].password,
        "  new secret 日本語  "
    );
    assert!(app.dirty_since.is_some());
    app.queue_save(&context);
    assert!(app.complete_pending_save(true));
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .passwords[0]
            .password,
        "  new secret 日本語  "
    );

    // Enter only ends editing: the encrypted save has already completed.
    input_frame(
        &context,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        |ui| render_open_tab(&mut app, ui),
    );
    assert!(app.dirty_since.is_none());
    app.lock(&context);
    assert!(app.store.is_none());
}

#[test]
fn leaving_an_inline_category_edit_registers_only_the_finished_category() {
    let (_directory, mut app, context) = fixture();
    app.tab = Tab::Passwords;
    app.store
        .as_mut()
        .unwrap()
        .data
        .passwords
        .push(PasswordItem {
            id: "inline-category".into(),
            title: "Category account".into(),
            kind: "Original group".into(),
            ..Default::default()
        });
    click_visible_text(&mut app, &context, "Original group");
    replace_focused_text(&mut app, &context, "P");
    replace_focused_text(&mut app, &context, "  Private group  ");
    let data = &app.store.as_ref().unwrap().data;
    assert_eq!(data.passwords[0].kind, "Private group");
    assert!(
        !data
            .meta
            .types
            .iter()
            .any(|kind| kind == "P" || kind == "Private group")
    );
    click_visible_text(&mut app, &context, "Settings");
    assert!(app.tab == Tab::Settings);
    assert!(
        app.store
            .as_ref()
            .unwrap()
            .data
            .meta
            .types
            .iter()
            .any(|kind| kind == "Private group")
    );
    assert!(
        !app.store
            .as_ref()
            .unwrap()
            .data
            .meta
            .types
            .iter()
            .any(|kind| kind == "P")
    );
    assert!(app.save_now());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .passwords[0]
            .kind,
        "Private group"
    );
}
