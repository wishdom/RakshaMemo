#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod fonts;

use anyhow::{Context, Result, bail};
use secure_notepad::{platform, settings::Settings};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        rfd::MessageDialog::new()
            .set_title("Secure Notepad")
            .set_description(format!("{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (vault_path, custom_path) = match args.next() {
        None => (platform::default_vault_path()?, false),
        Some(arg) if arg == "--vault" => (
            PathBuf::from(args.next().context("--vault requires a file path")?),
            true,
        ),
        Some(_) => bail!("Usage: secure-notepad [--vault PATH]"),
    };
    if args.next().is_some() {
        bail!("Usage: secure-notepad [--vault PATH]");
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
    let (settings, warning) = match Settings::load(&settings_path) {
        Ok(settings) => (settings, None),
        Err(error) => (
            Settings::default(),
            Some(format!("Could not load settings: {error:#}")),
        ),
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(settings.window_size)
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Secure Notepad",
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
