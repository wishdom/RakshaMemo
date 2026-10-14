//! Temple ornaments: the emblem, wordmark, frieze, carved card corners,
//! lotus, mandala, and diya lamp. Each is a handful of painted shapes, so they
//! stay sharp at any scale, need no image assets, and cost nothing measurable
//! per frame.

use eframe::egui::{self, Color32, FontId, Stroke};

/// The classical serif used for titles (see `fonts::DISPLAY`), or the
/// interface face when none is installed. Japanese titles use the interface
/// face, whose kana and kanji share its own baseline.
pub(super) fn display_font(ctx: &egui::Context, size: f32) -> FontId {
    if crate::i18n::language() == rakshamemo::settings::Language::Japanese {
        return FontId::proportional(size);
    }
    serif_font(ctx, size)
}

/// The display serif regardless of language, for the Latin wordmark.
fn serif_font(ctx: &egui::Context, size: f32) -> FontId {
    let family = egui::FontFamily::Name(crate::fonts::DISPLAY.into());
    let installed = ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family));
    if installed {
        FontId::new(size, family)
    } else {
        FontId::proportional(size)
    }
}

/// Gold for ornaments, softened so decoration never competes with content.
pub(super) fn gold(ui: &egui::Ui, strength: f32) -> Color32 {
    ui.visuals().hyperlink_color.gamma_multiply(strength)
}

fn diamond(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            center - egui::vec2(0.0, radius),
            center + egui::vec2(radius, 0.0),
            center + egui::vec2(0.0, radius),
            center - egui::vec2(radius, 0.0),
        ],
        color,
        Stroke::NONE,
    ));
}

/// A pointed petal from `base` towards `angle` (radians clockwise from up).
fn petal(base: egui::Pos2, angle: f32, length: f32, width: f32) -> Vec<egui::Pos2> {
    const STEPS: usize = 8;
    let along = egui::vec2(angle.sin(), -angle.cos());
    let across = egui::vec2(-along.y, along.x);
    let point = |t: f32, sign: f32| {
        // Full near the base, tapering to a point at the tip.
        let half = width * (std::f32::consts::PI * t).sin() * (1.0 - 0.35 * t);
        base + along * (length * t) + across * (half * sign)
    };
    let t = |step: usize| step as f32 / STEPS as f32;
    let mut points: Vec<egui::Pos2> = (0..=STEPS).map(|step| point(t(step), 1.0)).collect();
    points.extend((1..STEPS).rev().map(|step| point(t(step), -1.0)));
    points
}

/// A lotus (padma) seen from the front, `radius` tall above `base`. `gap`
/// outlines each petal so overlapping petals stay distinct.
pub(super) fn lotus(
    painter: &egui::Painter,
    base: egui::Pos2,
    radius: f32,
    color: Color32,
    gap: Color32,
) {
    let outline = Stroke::new((radius * 0.07).max(0.8), gap);
    for (degrees, length, width) in [
        (-68.0_f32, 0.62, 0.2),
        (68.0, 0.62, 0.2),
        (-34.0, 0.84, 0.25),
        (34.0, 0.84, 0.25),
        (0.0, 1.0, 0.28),
    ] {
        let points = petal(base, degrees.to_radians(), radius * length, radius * width);
        painter.add(egui::Shape::convex_polygon(points, color, outline));
    }
    let cup = egui::Rect::from_center_size(
        base + egui::vec2(0.0, radius * 0.06),
        egui::vec2(radius * 1.1, radius * 0.14),
    );
    painter.rect_filled(cup, radius * 0.07, color);
}

/// A row of small diamonds on a hairline, like a carved temple frieze, with a
/// lotus at its centre. Used in place of a plain separator under the toolbar.
pub(super) fn frieze(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 12.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let (strong, faint) = (gold(ui, 0.75), gold(ui, 0.35));
    let center = rect.center();
    painter.hline(rect.x_range(), center.y, Stroke::new(1.0, faint));
    const STEP: f32 = 16.0;
    let count = (rect.width() / 2.0 / STEP) as i32;
    for index in (-count..=count).filter(|index| index.abs() > 1) {
        let point = egui::pos2(center.x + index as f32 * STEP, center.y);
        if index % 2 == 0 {
            diamond(painter, point, 3.0, strong);
        } else {
            diamond(painter, point, 1.6, faint);
        }
    }
    let gap = ui.visuals().panel_fill;
    painter.rect_filled(
        egui::Rect::from_center_size(center, egui::vec2(28.0, 12.0)),
        0.0,
        gap,
    );
    lotus(painter, center + egui::vec2(0.0, 5.0), 10.0, strong, gap);
}

/// Carved corner brackets inside a card, each ending in a small diamond.
fn corner_brackets(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    const INSET: f32 = 6.0;
    const ARM: f32 = 14.0;
    for (corner, dx, dy) in [
        (rect.left_top(), 1.0, 1.0),
        (rect.right_top(), -1.0, 1.0),
        (rect.left_bottom(), 1.0, -1.0),
        (rect.right_bottom(), -1.0, -1.0),
    ] {
        let point = corner + egui::vec2(dx, dy) * INSET;
        painter.add(egui::Shape::line(
            vec![
                point + egui::vec2(0.0, dy * ARM),
                point,
                point + egui::vec2(dx * ARM, 0.0),
            ],
            Stroke::new(1.4, color),
        ));
        diamond(painter, point + egui::vec2(dx, dy) * 4.5, 1.8, color);
    }
}

/// A raised card with carved corners: the frame for settings sections and
/// the unlock form.
pub(super) fn card<R>(
    ui: &mut egui::Ui,
    margin: i8,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let response = egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(ui.visuals().window_stroke)
        .corner_radius(8)
        .inner_margin(margin)
        .show(ui, contents);
    corner_brackets(ui.painter(), response.response.rect, gold(ui, 0.6));
    response
}

/// A page or section title in the display face, after a gold diamond.
pub(super) fn title(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    const INDENT: f32 = 16.0;
    let galley =
        egui::WidgetText::from(egui::RichText::new(text).font(display_font(ui.ctx(), size)))
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Heading,
            );
    let (rect, response) = ui.allocate_exact_size(
        galley.size() + egui::vec2(INDENT, 0.0),
        egui::Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    if ui.is_rect_visible(rect) {
        // Centre the diamond on the first letter's ink (a capital), whatever
        // the face's ascent and line gap.
        let middle = galley
            .rows
            .first()
            .and_then(|row| Some((row.pos.y, row.glyphs.first()?)))
            .filter(|(_, glyph)| !glyph.uv_rect.is_nothing())
            .map_or(galley.size().y / 2.0, |(row, glyph)| {
                row + glyph.pos.y + glyph.uv_rect.offset.y + glyph.uv_rect.size.y / 2.0
            });
        let painter = ui.painter();
        diamond(
            painter,
            egui::pos2(rect.left() + 4.0, rect.top() + middle),
            4.0,
            gold(ui, 0.9),
        );
        let color = ui.visuals().strong_text_color();
        painter.galley(rect.left_top() + egui::vec2(INDENT, 0.0), galley, color);
    }
    response
}

/// A soft disc of light that fades to nothing at `radius`.
pub(super) fn glow(center: egui::Pos2, radius: f32, color: Color32) -> egui::Shape {
    const SEGMENTS: u32 = 48;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color);
    for step in 0..SEGMENTS {
        let angle = std::f32::consts::TAU * step as f32 / SEGMENTS as f32;
        mesh.colored_vertex(
            center + egui::vec2(angle.cos(), angle.sin()) * radius,
            Color32::TRANSPARENT,
        );
    }
    for step in 0..SEGMENTS {
        mesh.add_triangle(0, step + 1, (step + 1) % SEGMENTS + 1);
    }
    egui::Shape::mesh(mesh)
}

/// A faint mandala: concentric rings, two rings of petals, and a circle of
/// beads. Drawn behind the unlock form.
pub(super) fn mandala(center: egui::Pos2, radius: f32, color: Color32) -> Vec<egui::Shape> {
    let line = Stroke::new(1.0, color);
    let mut shapes = Vec::new();
    for ring in [1.0, 0.8, 0.52, 0.3] {
        shapes.push(egui::Shape::circle_stroke(center, radius * ring, line));
    }
    for (count, inner, length, width) in [(16, 0.52, 0.27, 0.07), (8, 0.3, 0.21, 0.08)] {
        for index in 0..count {
            let angle = std::f32::consts::TAU * (index as f32 + 0.5) / count as f32;
            let base = center + egui::vec2(angle.sin(), -angle.cos()) * (radius * inner);
            shapes.push(egui::Shape::closed_line(
                petal(base, angle, radius * length, radius * width),
                line,
            ));
        }
    }
    for index in 0..32 {
        let angle = std::f32::consts::TAU * index as f32 / 32.0;
        let point = center + egui::vec2(angle.cos(), angle.sin()) * (radius * 0.9);
        shapes.push(egui::Shape::circle_filled(point, 2.0, color));
    }
    shapes
}

/// A diya, the small clay oil lamp, with its flame: the empty-state picture.
pub(super) fn diya(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, width * 0.8), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let unit = width / 100.0;
    let at = |x: f32, y: f32| rect.left_top() + egui::vec2(x, y) * unit;
    let flame_outer = if ui.visuals().dark_mode {
        Color32::from_rgb(240, 150, 56)
    } else {
        Color32::from_rgb(214, 112, 30)
    };
    let flame_inner = Color32::from_rgb(255, 226, 156);
    let brass = gold(ui, 1.0);
    // Opaque darker brass for the oil, the foot, and the incised dots.
    let shade = {
        let [r, g, b, _] = brass.to_array();
        let scale = |value: u8| (f32::from(value) * 0.62) as u8;
        Color32::from_rgb(scale(r), scale(g), scale(b))
    };
    painter.add(glow(
        at(82.0, 36.0),
        30.0 * unit,
        flame_outer.gamma_multiply(0.3),
    ));
    // The foot, then the bowl (a shallow half-ellipse) and its pinched spout.
    painter.add(egui::Shape::convex_polygon(
        vec![
            at(38.0, 70.0),
            at(58.0, 70.0),
            at(63.0, 80.0),
            at(33.0, 80.0),
        ],
        shade,
        Stroke::NONE,
    ));
    let bowl: Vec<egui::Pos2> = (0..=20)
        .rev()
        .map(|step| {
            let angle = std::f32::consts::PI * step as f32 / 20.0;
            at(48.0 - 34.0 * angle.cos(), 56.0 + 18.0 * angle.sin())
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(bowl, brass, Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(
        vec![at(70.0, 56.0), at(88.0, 49.0), at(78.0, 62.0)],
        brass,
        Stroke::NONE,
    ));
    // The oil's surface, seen just from above the rim.
    let oil: Vec<egui::Pos2> = (0..24)
        .map(|step| {
            let angle = std::f32::consts::TAU * step as f32 / 24.0;
            at(46.0 + 30.0 * angle.cos(), 56.0 + 3.5 * angle.sin())
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(oil, shade, Stroke::NONE));
    for x in [30.0, 39.0, 48.0, 57.0, 66.0] {
        painter.circle_filled(at(x, 64.0), 1.6 * unit, shade);
    }
    // The flame: a teardrop rising from the wick at the spout.
    let flame = |height: f32, half: f32| -> Vec<egui::Pos2> {
        (0..24)
            .map(|step| {
                let angle = std::f32::consts::TAU * step as f32 / 24.0;
                // y runs from 0 at the wick to 1 at the tip.
                let y = (1.0 - angle.cos()) / 2.0;
                let x = angle.sin() * half * (1.0 - y).sqrt();
                at(86.0 + x, 47.0 - height * y)
            })
            .collect()
    };
    painter.add(egui::Shape::convex_polygon(
        flame(30.0, 9.0),
        flame_outer,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        flame(16.0, 5.0),
        flame_inner,
        Stroke::NONE,
    ));
}

/// The app's emblem (the window icon) at `size` points. The pixels are drawn
/// once per process, and the texture once per window, kept in `cache`.
pub(super) fn logo(ui: &mut egui::Ui, cache: &mut Option<egui::TextureHandle>, size: f32) {
    const PIXELS: usize = 128;
    static RGBA: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let texture = cache.get_or_insert_with(|| {
        let rgba = RGBA.get_or_init(|| crate::icon::rgba(PIXELS as u32));
        ui.ctx().load_texture(
            "rakshamemo-logo",
            egui::ColorImage::from_rgba_unmultiplied([PIXELS; 2], rgba),
            egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
        )
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// "Raksha" in the text color and "Memo" in gold, in the display face.
pub(super) fn wordmark(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let font = serif_font(ui.ctx(), size);
    let mut job = egui::text::LayoutJob::default();
    for (text, color) in [
        ("Raksha", ui.visuals().strong_text_color()),
        ("Memo", ui.visuals().hyperlink_color),
    ] {
        job.append(text, 0.0, egui::TextFormat::simple(font.clone(), color));
    }
    ui.label(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn petals_are_closed_and_end_at_their_tip() {
        let base = egui::pos2(0.0, 0.0);
        let points = petal(base, 0.0, 10.0, 3.0);
        assert_eq!(points.len(), 16);
        assert!(points.iter().any(|point| (point.y + 10.0).abs() < 1e-4));
        assert!(points.iter().all(|point| point.y <= 1e-4));
        // A petal pointing right lies to the right of its base.
        let right = petal(base, std::f32::consts::FRAC_PI_2, 10.0, 3.0);
        assert!(right.iter().all(|point| point.x >= -1e-4));
    }

    #[test]
    fn titles_fall_back_to_the_interface_face_without_the_display_font() {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        assert_eq!(
            display_font(&ctx, 20.0).family,
            egui::FontFamily::Proportional
        );
        ctx.end_pass().textures_delta.clear();
    }
}
