use eframe::egui;

use super::super::{
    Notepad,
    tests::{fixture, input_frame_sized},
};

fn frame(
    app: &mut Notepad,
    context: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    input_frame_sized(context, egui::vec2(width, 900.0), events, |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.settings_ui(ui));
    })
}

fn text_bounds(output: &egui::FullOutput, text: &str) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(painted) if painted.galley.text() == text => Some(
                egui::Rect::from_min_size(painted.pos, painted.galley.size()),
            ),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Expected visible settings text {text:?}"))
}

fn click_text(app: &mut Notepad, context: &egui::Context, text: &str) {
    frame(app, context, 1100.0, Vec::new());
    let output = frame(app, context, 1100.0, Vec::new());
    let position = text_bounds(&output, text).center();
    for pressed in [true, false] {
        frame(
            app,
            context,
            1100.0,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn displaying_settings_preserves_existing_long_lock_timeout_at_both_widths() {
    let (_directory, mut app, context) = fixture();
    app.settings.auto_lock_minutes = 240;
    for width in [1100.0, 600.0] {
        frame(&mut app, &context, width, Vec::new());
        let output = frame(&mut app, &context, width, Vec::new());
        assert_eq!(app.settings.auto_lock_minutes, 240);
        assert!(
            !app.settings_path.exists(),
            "Rendering must not rewrite preferences"
        );
        for text in [
            "Settings",
            "Theme",
            "Light",
            "Dark",
            "Launch at sign-in",
            "Enabled",
            "Automatic lock",
            // DragValue paints its numeric value and suffix as separate atoms.
            "240",
            " min",
        ] {
            let bounds = text_bounds(&output, text);
            assert!(
                bounds.left() >= 0.0 && bounds.right() <= width,
                "{text:?} overflows at {width}px: {bounds:?}"
            );
        }
    }
}

#[test]
fn clicking_a_theme_option_applies_it_and_persists_preferences() {
    let (_directory, mut app, context) = fixture();
    assert!(app.settings.dark_mode);
    click_text(&mut app, &context, "Light");
    assert!(!app.settings.dark_mode);
    assert_eq!(context.theme(), egui::Theme::Light);
    let saved = rakshamemo::settings::Settings::load(&app.settings_path).unwrap();
    assert!(!saved.dark_mode);
    assert_eq!(saved.auto_lock_minutes, app.settings.auto_lock_minutes);
}

#[test]
fn choosing_a_language_translates_the_page_and_persists_it() {
    use rakshamemo::settings::{Language, Settings};
    let (_directory, mut app, context) = fixture();
    click_text(&mut app, &context, "Français");
    assert_eq!(crate::i18n::language(), Language::French);
    assert_eq!(
        Settings::load(&app.settings_path).unwrap().language,
        Some(Language::French)
    );
    let output = frame(&mut app, &context, 1100.0, Vec::new());
    text_bounds(&output, "Paramètres");
    click_text(&mut app, &context, "日本語");
    let output = frame(&mut app, &context, 1100.0, Vec::new());
    text_bounds(&output, "設定");
    assert_eq!(
        Settings::load(&app.settings_path).unwrap().language,
        Some(Language::Japanese)
    );
}

#[test]
fn custom_vault_startup_control_stays_disabled_when_clicked() {
    let (_directory, mut app, context) = fixture();
    assert!(app.custom_path);
    assert!(!app.startup_enabled);
    let status = app.status.clone();
    click_text(&mut app, &context, "Enabled");
    assert!(!app.startup_enabled);
    assert_eq!(app.status, status);
    assert!(app.error.is_none());
    assert!(!app.settings_path.exists());
}
