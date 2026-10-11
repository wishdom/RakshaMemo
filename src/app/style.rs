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
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size = egui::vec2(32.0, 30.0);
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
    visuals.selection.stroke = Stroke::new(1.0, if dark { accent } else { color(24, 66, 126) });
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
        color(84, 101, 124)
    } else {
        color(160, 172, 189)
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
        visual.corner_radius = CornerRadius::same(6);
        visual.expansion = 0.0;
    }
    // Dividers and frames stay quiet; only interactive states get the accent.
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
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

/// Scope semantic colors so hover, pressed, disabled, and keyboard focus states
/// keep egui's normal interaction behavior and cannot leak to adjacent controls.
fn action<R>(ui: &mut egui::Ui, kind: ActionStyle, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let dark = ui.visuals().dark_mode;
        let rgb = Color32::from_rgb;
        let danger = ui.visuals().error_fg_color;
        let accent = ui.visuals().hyperlink_color;
        let (text, fills, outlines) = match kind {
            ActionStyle::Primary => {
                let fills = if dark {
                    [rgb(43, 108, 196), rgb(56, 124, 214), rgb(36, 92, 168)]
                } else {
                    [rgb(35, 95, 173), rgb(29, 84, 156), rgb(24, 66, 126)]
                };
                let ring = if dark { accent } else { rgb(16, 46, 92) };
                (Color32::WHITE, fills, [fills[0], fills[1], ring])
            }
            ActionStyle::Danger => {
                let fills = if dark {
                    [rgb(68, 33, 42), rgb(87, 39, 48), rgb(100, 43, 53)]
                } else {
                    [rgb(255, 236, 238), rgb(255, 220, 224), rgb(252, 203, 208)]
                };
                let soft = danger.gamma_multiply(0.55);
                (danger, fills, [soft, danger, danger])
            }
            ActionStyle::Destructive => {
                let fills = [rgb(177, 40, 47), rgb(155, 31, 38), rgb(134, 25, 32)];
                let ring = if dark { danger } else { rgb(96, 16, 22) };
                (Color32::WHITE, fills, [fills[0], fills[1], ring])
            }
        };
        let visuals = ui.visuals_mut();
        visuals.override_text_color = Some(text);
        for (widget, fill, outline, width) in [
            (&mut visuals.widgets.inactive, fills[0], outlines[0], 1.0),
            (&mut visuals.widgets.hovered, fills[1], outlines[1], 1.0),
            (&mut visuals.widgets.active, fills[2], outlines[2], 2.0),
            (&mut visuals.widgets.open, fills[1], outlines[1], 1.0),
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
    primary(ui, |ui| ui.button(label))
}

pub(super) fn primary<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    action(ui, ActionStyle::Primary, contents)
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

/// A top-level navigation tab: a quiet pill that fills when selected,
/// instead of a bordered button.
pub(super) fn nav_tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let galley = egui::WidgetText::from(egui::RichText::new(label).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let padding = egui::vec2(14.0, 6.0);
    let size = egui::vec2(
        galley.size().x + padding.x * 2.0,
        ui.spacing()
            .interact_size
            .y
            .max(galley.size().y + padding.y * 2.0),
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if selected {
            Some(visuals.selection.bg_fill)
        } else if response.hovered() {
            Some(visuals.widgets.hovered.weak_bg_fill)
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().rect_filled(rect, 8.0, fill);
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                8.0,
                Stroke::new(1.5, visuals.widgets.hovered.bg_stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        let color = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        let position = rect.center() - galley.size() / 2.0;
        ui.painter().galley(position, galley, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A square secondary button with a painted sun (switch to light) or moon
/// (switch to dark). Icons are drawn so they never depend on font coverage.
pub(super) fn theme_toggle(ui: &mut egui::Ui, dark: bool) -> egui::Response {
    let side = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let label = if dark {
        "Switch to light mode"
    } else {
        "Switch to dark mode"
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let (fill, stroke, radius) = (visuals.bg_fill, visuals.bg_stroke, visuals.corner_radius);
        let color = visuals.fg_stroke.color;
        let painter = ui.painter();
        painter.rect(rect, radius, fill, stroke, egui::StrokeKind::Inside);
        let center = rect.center();
        if dark {
            let line = Stroke::new(1.5, color);
            painter.circle_stroke(center, 3.5, line);
            for step in 0..8 {
                let angle = step as f32 * std::f32::consts::FRAC_PI_4;
                let direction = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment([center + direction * 6.0, center + direction * 8.0], line);
            }
        } else {
            // A crescent: a disc with an offset disc in the button's fill.
            painter.circle_filled(center, 6.5, color);
            painter.circle_filled(center + egui::vec2(3.5, -3.0), 5.5, fill);
        }
    }
    response
        .on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Green for "strong", tuned for contrast on each theme's panels.
pub(super) fn success_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(110, 200, 140)
    } else {
        Color32::from_rgb(22, 120, 60)
    }
}

/// A four-segment bar, rating, crack-time estimate, and zxcvbn's advice.
pub(super) fn strength_meter(ui: &mut egui::Ui, strength: &rakshamemo::strength::Strength) {
    let color = match strength.score {
        0 | 1 => ui.visuals().error_fg_color,
        2 => ui.visuals().warn_fg_color,
        _ => success_color(ui),
    };
    let width = ui.available_width().min(360.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
    let gap = 4.0;
    let segment = (rect.width() - gap * 3.0) / 4.0;
    let filled = strength.score.max(1);
    for index in 0..4u8 {
        let left = rect.left() + f32::from(index) * (segment + gap);
        let bar = egui::Rect::from_min_size(egui::pos2(left, rect.top()), egui::vec2(segment, 6.0));
        let fill = if index < filled {
            color
        } else {
            ui.visuals().widgets.inactive.bg_fill
        };
        ui.painter().rect_filled(bar, 3.0, fill);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(strength.label).color(color).strong());
        ui.weak(format!("· {} to crack offline", strength.crack_time));
    });
    for advice in [&strength.warning, &strength.suggestion]
        .into_iter()
        .flatten()
    {
        ui.add(egui::Label::new(egui::RichText::new(advice).weak()).wrap());
    }
}

/// Paint a five-pointed star, filled or outlined, centred on `center`.
pub(super) fn paint_star(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    color: Color32,
    filled: bool,
) {
    let points: Vec<egui::Pos2> = (0..10)
        .map(|index| {
            let angle = std::f32::consts::PI * (index as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            let distance = if index % 2 == 0 {
                radius
            } else {
                radius * 0.45
            };
            center + egui::vec2(angle.cos(), angle.sin()) * distance
        })
        .collect();
    if filled {
        // The star is not convex but every point sees the centre, so a fan
        // of triangles from the centre fills it exactly.
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(center, color);
        for point in &points {
            mesh.colored_vertex(*point, color);
        }
        for index in 0..10u32 {
            mesh.add_triangle(0, index + 1, (index + 1) % 10 + 1);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    // The outline also anti-aliases the filled edges.
    painter.add(egui::Shape::closed_line(points, Stroke::new(1.3, color)));
}

/// A square secondary button showing a star: filled when `on`.
pub(super) fn star_button(ui: &mut egui::Ui, on: bool, label: &str) -> egui::Response {
    let side = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let color = if on {
            ui.visuals().warn_fg_color
        } else {
            visuals.fg_stroke.color
        };
        paint_star(ui.painter(), rect.center(), 7.5, color, on);
    }
    response
        .on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
