use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

/// Shared compact desktop styling. Keep motion brief so feedback follows input.
pub(super) fn apply(ctx: &egui::Context, dark: bool) {
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let mut style = (*ctx.style_of(theme)).clone();
    style.visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(20.0)),
        (TextStyle::Body, FontId::proportional(15.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(13.0)),
        (TextStyle::Monospace, FontId::monospace(14.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size = egui::vec2(32.0, 26.0);
    style.spacing.window_margin = egui::Margin::same(14);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.indent = 16.0;
    style.spacing.icon_width = 16.0;
    style.spacing.icon_width_inner = 10.0;
    style.spacing.icon_spacing = 6.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.bar_inner_margin = 4.0;
    style.interaction.interact_radius = 2.0;
    style.interaction.tooltip_delay = 0.25;
    style.animation_time = 0.08;
    style.scroll_animation = egui::style::ScrollAnimation::duration(0.08);

    let color = Color32::from_rgb;
    let (panel, surface, field, border, widget, hovered, active, text, muted, accent, selection) =
        if dark {
            (
                color(20, 25, 33),
                color(27, 34, 44),
                color(16, 21, 29),
                color(48, 61, 78),
                color(36, 46, 60),
                color(43, 62, 88),
                color(30, 64, 102),
                color(226, 233, 242),
                color(170, 186, 205),
                color(122, 185, 255),
                color(33, 71, 113),
            )
        } else {
            (
                color(246, 248, 252),
                Color32::WHITE,
                Color32::WHITE,
                color(204, 213, 225),
                color(237, 241, 247),
                color(225, 235, 250),
                color(210, 228, 254),
                color(31, 44, 63),
                color(76, 93, 115),
                color(35, 95, 173),
                color(215, 232, 254),
            )
        };
    let visuals = &mut style.visuals;
    visuals.panel_fill = panel;
    visuals.window_fill = surface;
    visuals.extreme_bg_color = field;
    visuals.text_edit_bg_color = Some(field);
    visuals.faint_bg_color = if dark { surface } else { widget };
    visuals.code_bg_color = widget;
    visuals.weak_text_color = Some(muted);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = selection;
    visuals.selection.stroke = Stroke::new(1.0, text);
    visuals.text_cursor.stroke = Stroke::new(1.5, accent);
    visuals.window_stroke = Stroke::new(1.0, border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 80 } else { 35 }),
    };
    visuals.popup_shadow = visuals.window_shadow;
    visuals.slider_trailing_fill = true;
    visuals.warn_fg_color = if dark {
        color(245, 186, 97)
    } else {
        color(143, 83, 7)
    };
    visuals.error_fg_color = if dark {
        color(255, 157, 157)
    } else {
        color(177, 40, 47)
    };

    // Control boundaries stay visible without brightening decorative dividers.
    let control_border = if dark {
        color(111, 130, 154)
    } else {
        color(117, 132, 152)
    };
    for (visual, fill, stroke) in [
        (&mut visuals.widgets.noninteractive, surface, border),
        (&mut visuals.widgets.inactive, widget, control_border),
        (&mut visuals.widgets.hovered, hovered, accent),
        (&mut visuals.widgets.active, active, accent),
        (&mut visuals.widgets.open, hovered, accent),
    ] {
        visual.bg_fill = fill;
        visual.weak_bg_fill = fill;
        visual.bg_stroke = Stroke::new(1.0, stroke);
        visual.fg_stroke = Stroke::new(1.0, text);
        visual.corner_radius = CornerRadius::same(4);
        visual.expansion = 0.0;
    }
    visuals.widgets.active.bg_stroke.width = 2.0;
    ctx.set_style_of(theme, style);
    ctx.set_theme(theme);
}

#[derive(Clone, Copy)]
enum ActionStyle {
    Primary,
    Danger,
    Destructive,
}

/// Pages share one readable type size. The page title alone uses Heading;
/// labels, inputs, metadata, and controls all use 15 px.
pub(super) fn page(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    for kind in [
        TextStyle::Body,
        TextStyle::Button,
        TextStyle::Small,
        TextStyle::Monospace,
    ] {
        style.text_styles.insert(kind, FontId::proportional(15.0));
    }
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style.spacing.interact_size.y = 32.0;
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
}

pub(super) fn passwords(ui: &mut egui::Ui) {
    page(ui);
}

/// Scope semantic colors so hover, pressed, disabled, and keyboard focus states
/// keep egui's normal interaction behavior and cannot leak to adjacent controls.
fn action<R>(ui: &mut egui::Ui, kind: ActionStyle, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let dark = ui.visuals().dark_mode;
        let rgb = Color32::from_rgb;
        let danger = ui.visuals().error_fg_color;
        let accent = ui.visuals().hyperlink_color;
        let (text, fills, outline) = match kind {
            ActionStyle::Primary => (
                Color32::WHITE,
                [rgb(35, 95, 173), rgb(29, 80, 148), rgb(24, 66, 126)],
                if dark { accent } else { rgb(24, 66, 126) },
            ),
            ActionStyle::Danger => (
                danger,
                if dark {
                    [rgb(68, 33, 42), rgb(87, 39, 48), rgb(100, 43, 53)]
                } else {
                    [rgb(255, 236, 238), rgb(255, 220, 224), rgb(252, 203, 208)]
                },
                danger,
            ),
            ActionStyle::Destructive => (
                Color32::WHITE,
                [rgb(177, 40, 47), rgb(155, 31, 38), rgb(134, 25, 32)],
                if dark { danger } else { rgb(134, 25, 32) },
            ),
        };
        let visuals = ui.visuals_mut();
        visuals.override_text_color = Some(text);
        for (widget, fill, width) in [
            (&mut visuals.widgets.inactive, fills[0], 1.0),
            (&mut visuals.widgets.hovered, fills[1], 1.5),
            (&mut visuals.widgets.active, fills[2], 2.0),
            (&mut visuals.widgets.open, fills[1], 1.5),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.fg_stroke.color = text;
            widget.bg_stroke = Stroke::new(width, outline);
        }
        contents(ui)
    })
    .inner
}

pub(super) fn primary_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    action(ui, ActionStyle::Primary, |ui| ui.button(label))
}

pub(super) fn danger<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    action(ui, ActionStyle::Danger, contents)
}

pub(super) fn danger_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    danger(ui, |ui| ui.button(label))
}

pub(super) fn destructive_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    action(ui, ActionStyle::Destructive, |ui| ui.button(label))
}
