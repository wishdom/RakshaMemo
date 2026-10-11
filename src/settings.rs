//! Nonsecret preferences. Startup registration is queried from the OS separately.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Settings {
    pub dark_mode: bool,
    /// Zero disables idle locking; otherwise this is a number of minutes.
    pub auto_lock_minutes: u32,
    pub window_size: [f32; 2],
    /// Lock when the window is minimized, the computer sleeps, or (on
    /// Windows) the user locks their session.
    pub lock_when_away: bool,
    pub clock_24h: bool,
    pub note_sort: NoteSort,
}

/// Pinned notes always come first; this orders the rest.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteSort {
    #[default]
    RecentlyEdited,
    Title,
    Created,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark_mode: true,
            auto_lock_minutes: 5,
            window_size: [1100.0, 740.0],
            lock_when_away: true,
            clock_24h: true,
            note_sort: NoteSort::default(),
        }
    }
}

impl Settings {
    /// Missing settings are normal on first launch. Other errors are returned so
    /// the UI can explain that it is using defaults instead of silently resetting.
    pub fn load(path: &Path) -> Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error).context("Could not read preferences"),
        };
        const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("Could not read preferences")?;
        ensure!(
            bytes.len() as u64 <= MAX_SETTINGS_BYTES,
            "Preferences file is unexpectedly large"
        );
        let settings: Self =
            serde_json::from_slice(&bytes).context("Preferences file is not valid JSON")?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        write_atomic(path, &serde_json::to_vec_pretty(self)?).context("Could not save preferences")
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.auto_lock_minutes <= 24 * 60,
            "Idle lock preference must be between 0 and 1440 minutes"
        );
        ensure!(
            self.window_size
                .iter()
                .all(|dimension| dimension.is_finite() && (320.0..=16384.0).contains(dimension)),
            "Saved window dimensions are invalid"
        );
        Ok(())
    }
}

/// Create the temporary file beside its destination for an atomic replacement.
/// The data and (on Unix) directory entry are synced before returning success.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).context("Could not create the preferences directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .context("Could not create a temporary preferences file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_partial_preferences_use_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        fs::write(&path, r#"{"dark_mode": false}"#).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert!(!settings.dark_mode);
        assert_eq!(settings.auto_lock_minutes, 5);
        assert_eq!(settings.window_size, [1100.0, 740.0]);
    }

    #[test]
    fn damaged_preferences_are_reported_and_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        for contents in [
            "{truncated",
            r#"{"window_size":[-5,800]}"#,
            r#"{"auto_lock_minutes":4294967295}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert!(Settings::load(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
    }

    #[test]
    fn preferences_replace_existing_file_and_reject_invalid_updates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/settings.json");
        let mut settings = Settings::default();
        settings.save(&path).unwrap();
        settings.dark_mode = false;
        settings.auto_lock_minutes = 0;
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        let saved = fs::read(&path).unwrap();
        settings.window_size[0] = f32::NAN;
        assert!(settings.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
}
