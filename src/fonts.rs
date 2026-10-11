//! Use one installed face for proportional Latin and CJK text.
//! Font files stay on the user's computer and are not redistributed.
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;

pub fn install(ctx: &Context) {
    for (path, index) in candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            fonts.font_data.insert("system-cjk".into(), data.into());
            // Mixing egui's Latin face with a CJK fallback gives each script
            // different ascent/line-height metrics. One primary face keeps
            // mixed strings (e.g. Japanese room names with Latin numbers) on
            // the same baseline at every UI size, without font-specific offsets.
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "system-cjk".into());
            // Preserve fixed-width Latin text where Monospace is requested.
            // Egui's remaining built-in faces also retain symbol/emoji fallback.
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("system-cjk".into());
            ctx.set_fonts(fonts);
            break;
        }
    }
}

/// Font files, each with the face to use inside a collection (`.ttc`).
fn candidates() -> Vec<(PathBuf, u32)> {
    #[cfg(target_os = "windows")]
    {
        let fonts =
            PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
                .join("Fonts");
        // Prefer the interface faces (Yu Gothic UI, Microsoft YaHei UI). The
        // document faces at index 0 have a tall descent that draws text about
        // 5 px above centre in buttons and fields, and a mid-line "…".
        [
            ("YuGothM.ttc", 1),
            ("msgothic.ttc", 0),
            ("msyh.ttc", 1),
            ("malgun.ttf", 0),
        ]
        .into_iter()
        .map(|(name, index)| (fonts.join(name), index))
        .collect()
    }
    #[cfg(target_os = "macos")]
    {
        [
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ]
        .into_iter()
        .map(|path| (PathBuf::from(path), 0))
        .collect()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ]
        .into_iter()
        .map(|path| (PathBuf::from(path), 0))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Color32, FontId, RawInput};

    #[test]
    fn installed_cjk_face_keeps_mixed_scripts_on_one_baseline() {
        // No font is bundled: headless systems without an installed CJK face
        // deliberately keep egui's defaults. Exercise real font metrics when
        // the same system font used by the native app is available.
        if !candidates().iter().any(|(path, _)| path.is_file()) {
            return;
        }
        let ctx = Context::default();
        install(&ctx);
        ctx.begin_pass(RawInput::default());
        for size in [15.0, 16.0, 22.0] {
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    "ハイツ102ゴウシツ 日本語漢字/中文 ABCDE012345".into(),
                    FontId::proportional(size),
                    Color32::WHITE,
                )
            });
            let glyphs = &galley.rows[0].glyphs;
            let baseline = glyphs[0].pos.y;
            assert!(
                glyphs
                    .iter()
                    .all(|glyph| (glyph.pos.y - baseline).abs() < 0.1),
                "mixed-script baselines differ at {size}px"
            );
        }
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn installed_face_centres_latin_text_and_keeps_ellipsis_on_the_baseline() {
        if !candidates().iter().any(|(path, _)| path.is_file()) {
            return;
        }
        let ctx = Context::default();
        install(&ctx);
        ctx.begin_pass(RawInput::default());
        // Buttons and fields centre each row, so the drawn ink must sit near
        // the row's middle; "…" belongs on the baseline in Latin labels.
        let ink = |text: &str| {
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout_no_wrap(text.into(), FontId::proportional(15.0), Color32::WHITE)
            });
            let row = &galley.rows[0];
            let (top, bottom) = row
                .glyphs
                .iter()
                .filter(|glyph| !glyph.uv_rect.is_nothing())
                .fold((f32::MAX, f32::MIN), |(top, bottom), glyph| {
                    let glyph_top = glyph.pos.y + glyph.uv_rect.offset.y;
                    (
                        top.min(glyph_top),
                        bottom.max(glyph_top + glyph.uv_rect.size.y),
                    )
                });
            (top, bottom, row.glyphs[0].pos.y, row.height())
        };
        let (top, bottom, _, height) = ink("Lock Hx");
        let offset = (top + bottom) / 2.0 - height / 2.0;
        assert!(offset.abs() <= 1.5, "text is {offset} px off centre");
        let (_, bottom, baseline, _) = ink("…");
        assert!(
            (bottom - baseline).abs() <= 1.5,
            "ellipsis is not on the baseline"
        );
        ctx.end_pass().textures_delta.clear();
    }
}
