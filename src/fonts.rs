//! Use an installed CJK fallback so existing non-Latin notes remain readable.
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
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("system-cjk".into());
            }
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
