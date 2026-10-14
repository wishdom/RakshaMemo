#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod fonts;
mod i18n;
mod icon;

use anyhow::{Context, Result, bail};
use rakshamemo::{
    platform,
    settings::{Language, Settings},
};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        rfd::MessageDialog::new()
            .set_title("RakshaMemo")
            .set_description(format!("{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

/// Large enough for the taskbar on high-DPI screens; the OS scales it down.
const WINDOW_ICON_SIZE: u32 = 64;

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (vault_path, custom_path) = match args.next() {
        None => (platform::default_vault_path()?, false),
        Some(arg) if arg == "--vault" => (
            PathBuf::from(args.next().context("--vault requires a file path")?),
            true,
        ),
        Some(_) => bail!("Usage: RakshaMemo [--vault PATH]"),
    };
    if args.next().is_some() {
        bail!("Usage: RakshaMemo [--vault PATH]");
    }
    let vault_path = std::path::absolute(vault_path)?;
    let instance = platform::SingleInstance::acquire(&vault_path)?;
    let settings_path = if custom_path {
        let mut path = vault_path.as_os_str().to_os_string();
        path.push(".settings.json");
        PathBuf::from(path)
    } else {
        platform::default_settings_path()?
    };
    let (mut settings, problem) = match Settings::load(&settings_path) {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(error)),
    };
    // The first launch follows the system language; the choice is then kept
    // in the settings, where the user can change it.
    i18n::set(*settings.language.get_or_insert_with(Language::system));
    let warning = problem
        .map(|error| i18n::fill(i18n::t().could_not_load_settings, &[&format!("{error:#}")]));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(settings.window_size)
            .with_min_inner_size([760.0, 520.0])
            .with_icon(eframe::egui::IconData {
                rgba: icon::rgba(WINDOW_ICON_SIZE),
                width: WINDOW_ICON_SIZE,
                height: WINDOW_ICON_SIZE,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "RakshaMemo",
        options,
        Box::new(move |cc| {
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::Notepad::new(
                cc,
                vault_path,
                settings_path,
                settings,
                warning,
                instance,
                custom_path,
            )))
        }),
    )
    .map_err(|error| anyhow::anyhow!("Could not start the desktop window: {error}"))
}
