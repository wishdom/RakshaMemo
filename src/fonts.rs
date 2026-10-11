//! Use one installed face for proportional Latin and CJK text.
//! Font files stay on the user's computer and are not redistributed.
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;

pub fn install(ctx: &Context) {
    let candidates = candidates();
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("system-cjk".into(), FontData::from_owned(bytes).into());
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

fn candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let fonts =
            PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
                .join("Fonts");
        ["YuGothM.ttc", "msgothic.ttc", "msyh.ttc", "malgun.ttf"]
            .into_iter()
            .map(|name| fonts.join(name))
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
        .map(PathBuf::from)
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
        .map(PathBuf::from)
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
        if !candidates().iter().any(|path| path.is_file()) {
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
}
