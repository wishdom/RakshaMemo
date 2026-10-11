use super::*;

const PASSWORD: &str = "application test passphrase";

/// Keep all persistence inside a temporary directory and bypass Notepad::new,
/// which reads the user's real OS startup registration.
fn fixture() -> (tempfile::TempDir, Notepad, egui::Context) {
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
        error: None,
        status: "Vault unlocked".into(),
        dirty_since: None,
        last_edit: now,
        last_activity: now,
        last_save_attempt: now,
        startup_enabled: false,
        clipboard: Clipboard::new(),
        auth_focus: false,
    };
    (directory, app, egui::Context::default())
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

fn draft() -> Dialog {
    Dialog::Entry(Box::new(EntryEditor {
        item: PasswordItem {
            title: "Unsaved password".into(),
            password: "draft secret".into(),
            ..Default::default()
        },
        show_password: true,
        generator: generator::Options::default(),
    }))
}

fn input_frame(
    context: &egui::Context,
    events: Vec<egui::Event>,
    render: impl FnMut(&mut egui::Ui),
) {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_100.0, 740.0),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        render,
    );
    output.textures_delta.clear();
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
fn locking_flushes_notes_and_clears_drafts_credentials_and_gui_state() {
    let (_directory, mut app, context) = fixture();
    add_pending_note(&mut app, "Keep this note 日本語");
    app.dialog = Some(draft());
    app.query = "private search".into();
    app.category = "private category".into();
    app.auth.push_str("temporary master password");
    app.confirm.push_str("temporary confirmation");
    let private_state_id = egui::Id::new("private editor state");
    context.data_mut(|data| data.insert_temp(private_state_id, "editor history".to_owned()));

    app.lock(&context);

    assert!(app.store.is_none());
    assert!(app.dialog.is_none());
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
    assert!(
        saved.data.passwords.is_empty(),
        "An unsaved dialog is not a saved entry"
    );
}

#[test]
fn a_failed_save_prevents_lock_and_preserves_pending_data() {
    let (_directory, mut app, context) = fixture();
    let original_file = std::fs::read(&app.vault_path).unwrap();
    let backup = app.store.as_ref().unwrap().backup_path();
    // An existing directory cannot be replaced by the atomic backup file.
    std::fs::create_dir(&backup).unwrap();
    add_pending_note(&mut app, "Unsaved but recoverable");
    app.dialog = Some(draft());

    app.lock(&context);

    assert!(app.dirty_since.is_some());
    assert!(matches!(app.dialog, Some(Dialog::Entry(_))));
    assert_eq!(
        app.store.as_ref().unwrap().data.notes.items[0].content,
        "Unsaved but recoverable"
    );
    assert!(app.error.as_deref().unwrap().contains("Could not save"));
    assert_eq!(std::fs::read(&app.vault_path).unwrap(), original_file);
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
    app.dialog = Some(draft());
    app.tick(&context);
    assert!(app.store.is_none());
    assert!(app.dialog.is_none());
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
fn autosave_handles_both_paused_and_continuous_editing() {
    let (_directory, mut app, context) = fixture();
    add_pending_note(&mut app, "Paused typing");
    app.last_edit = Instant::now() - Duration::from_secs(2);
    app.last_save_attempt = Instant::now() - Duration::from_secs(2);
    app.tick(&context);
    assert!(app.dirty_since.is_none());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "Paused typing"
    );

    app.store.as_mut().unwrap().data.notes.items[0].content = "Still typing".into();
    app.changed();
    app.dirty_since = Some(Instant::now() - Duration::from_secs(7));
    app.last_save_attempt = Instant::now() - Duration::from_secs(2);
    app.tick(&context);
    assert!(app.dirty_since.is_none());
    assert_eq!(
        Store::open(&app.vault_path, PASSWORD)
            .unwrap()
            .data
            .notes
            .items[0]
            .content,
        "Still typing"
    );
}

#[test]
fn confirmed_discard_locks_without_overwriting_the_vault() {
    let (_directory, mut app, context) = fixture();
    let original_file = std::fs::read(&app.vault_path).unwrap();
    add_pending_note(&mut app, "Explicitly discarded");
    app.dialog = Some(draft());
    app.finish_lock(&context);
    assert!(app.store.is_none());
    assert!(app.dialog.is_none());
    assert!(app.dirty_since.is_none());
    assert_eq!(std::fs::read(&app.vault_path).unwrap(), original_file);
}

#[test]
fn primary_views_and_password_dialog_render_without_a_desktop_session() {
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
    app.dialog = Some(draft());
    let mut output = context.run_ui(egui::RawInput::default(), |_ui| app.dialog_ui(&context));
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
    assert!(matches!(app.dialog, Some(Dialog::Entry(_))));
    app.lock(&context);
    let mut output = context.run_ui(egui::RawInput::default(), |ui| app.auth_ui(ui));
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
    assert!(app.store.is_none());
    assert!(app.clipboard.device.is_none());
}
