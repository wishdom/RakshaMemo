use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "RakshaMemo";

pub fn default_vault_path() -> Result<PathBuf> {
    Ok(data_directory()?.join("vault.json"))
}

pub fn default_settings_path() -> Result<PathBuf> {
    Ok(data_directory()?.join("settings.json"))
}

fn data_directory() -> Result<PathBuf> {
    let base = dirs::data_local_dir().context("Could not locate your user data directory")?;
    #[cfg(target_os = "windows")]
    let base = base.join("Wishdom");
    Ok(base.join(APP_NAME))
}

/// Whether the user's Windows session is locked (Win+L or the lock screen).
/// Other platforms report `false`; the app detects sleep separately.
pub fn session_locked() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::RemoteDesktop::{
            WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSFreeMemory,
            WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
        };
        let mut buffer = std::ptr::null_mut();
        let mut bytes = 0u32;
        // SAFETY: on success Windows allocates `buffer` (a WTSINFOEXW of
        // `bytes` bytes), which is read only after a size check and then
        // released with WTSFreeMemory exactly once.
        unsafe {
            if WTSQuerySessionInformationW(
                WTS_CURRENT_SERVER_HANDLE,
                WTS_CURRENT_SESSION,
                WTSSessionInfoEx,
                &mut buffer,
                &mut bytes,
            ) == 0
                || buffer.is_null()
            {
                return false;
            }
            let locked = bytes as usize >= std::mem::size_of::<WTSINFOEXW>() && {
                let info = &*(buffer as *const WTSINFOEXW);
                info.Level == 1
                    && info.Data.WTSInfoExLevel1.SessionFlags as u32 == WTS_SESSIONSTATE_LOCK
            };
            WTSFreeMemory(buffer.cast());
            locked
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Keep this guard alive for as long as the vault can be edited. The lock file
/// stays on disk; deleting it would permit another process to lock a new inode.
#[derive(Debug)]
pub struct SingleInstance {
    _file: File,
}

impl SingleInstance {
    pub fn acquire(vault_path: &Path) -> Result<Self> {
        let parent = vault_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut directory = fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory
            .create(parent)
            .context("Could not create the vault directory")?;
        let absolute_vault = if vault_path.try_exists()? {
            vault_path.canonicalize()?
        } else {
            parent.canonicalize()?.join(
                vault_path
                    .file_name()
                    .context("The vault path has no file name")?,
            )
        };
        let mut lock_name = absolute_vault.as_os_str().to_os_string();
        lock_name.push(".lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(PathBuf::from(lock_name))
            .context("Could not open the vault lock file")?;
        FileExt::try_lock_exclusive(&file).context(
            "Could not lock this vault. Another RakshaMemo window may already have it open. Close that window and try again.",
        )?;
        Ok(Self { _file: file })
    }
}

/// Opt-in, per-user registration for the next OS login. These functions never
/// store a master password or launch an unlocked vault.
pub mod startup {
    use anyhow::{Context, Result, ensure};
    use std::path::Path;

    /// Whether an app startup registration exists, including one pointing to an
    /// older executable location. This allows stale registrations to be disabled.
    /// OS policy or the OS startup UI can additionally suppress launch.
    pub fn is_enabled() -> Result<bool> {
        backend::is_enabled()
    }

    pub fn set_enabled(enabled: bool) -> Result<()> {
        backend::set_enabled(enabled)
    }

    fn executable_text(executable: &Path) -> Result<&str> {
        ensure!(
            executable.is_absolute(),
            "Startup requires an absolute executable path"
        );
        let text = executable
            .to_str()
            .context("This executable path cannot be represented in the startup configuration")?;
        ensure!(
            !text.chars().any(char::is_control),
            "Startup does not support control characters in the executable path"
        );
        Ok(text)
    }

    #[cfg(any(target_os = "windows", test))]
    fn windows_command(executable: &Path) -> Result<String> {
        let text = executable_text(executable)?;
        ensure!(
            !text.contains('"'),
            "Executable path contains a quotation mark"
        );
        let command = format!("\"{text}\"");
        ensure!(
            command.encode_utf16().count() <= 260,
            "Windows startup supports command lines up to 260 characters; move the application to a shorter path"
        );
        Ok(command)
    }

    #[cfg(any(target_os = "linux", test))]
    fn desktop_entry(executable: &Path) -> Result<String> {
        let text = executable_text(executable)?;
        ensure!(
            !text.contains('='),
            "Linux startup does not support '=' in an executable path"
        );
        // Desktop files decode string escapes, then Exec quoting, then field
        // codes. Shell-like characters must survive both escape layers.
        let mut quoted = String::from("\"");
        for character in text.chars() {
            match character {
                '\\' => quoted.push_str(r"\\\\"),
                '"' => quoted.push_str("\\\\\""),
                '$' => quoted.push_str(r"\\$"),
                '`' => quoted.push_str(r"\\`"),
                '%' => quoted.push_str("%%"),
                other => quoted.push(other),
            }
        }
        quoted.push('"');
        Ok(format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName=RakshaMemo\nComment=Open your encrypted vault at login\nExec={quoted}\nTerminal=false\nHidden=false\nX-GNOME-Autostart-enabled=true\n"
        ))
    }

    #[cfg(any(target_os = "macos", test))]
    fn launch_agent(executable: &Path) -> Result<String> {
        let text = executable_text(executable)?;
        let escaped = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;");
        Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\"><dict>\n\
             <key>Label</key><string>com.wishdom.rakshamemo</string>\n\
             <key>ProgramArguments</key><array><string>{escaped}</string></array>\n\
             <key>RunAtLoad</key><true/>\n\
             <key>LimitLoadToSessionType</key><string>Aqua</string>\n\
             </dict></plist>\n"
        ))
    }

    #[cfg(target_os = "windows")]
    mod backend {
        use super::*;
        use std::io::ErrorKind;
        use winreg::{RegKey, enums::*};

        const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
        const VALUE_NAME: &str = "RakshaMemo";

        pub(super) fn is_enabled() -> Result<bool> {
            registry_is_enabled(&RegKey::predef(HKEY_CURRENT_USER), RUN_KEY)
        }

        fn registry_is_enabled(root: &RegKey, subkey: &str) -> Result<bool> {
            let key = match root.open_subkey_with_flags(subkey, KEY_READ) {
                Ok(key) => key,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error).context("Could not read startup preferences"),
            };
            match key.get_raw_value(VALUE_NAME) {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error).context("Could not read the startup registration"),
            }
        }

        pub(super) fn set_enabled(enabled: bool) -> Result<()> {
            let current_user = RegKey::predef(HKEY_CURRENT_USER);
            let executable = enabled.then(std::env::current_exe).transpose()?;
            set_registry_enabled(&current_user, RUN_KEY, executable.as_deref())
        }

        fn set_registry_enabled(
            root: &RegKey,
            subkey: &str,
            executable: Option<&Path>,
        ) -> Result<()> {
            if let Some(executable) = executable {
                let command = windows_command(executable)?;
                let (key, _) = root
                    .create_subkey_with_flags(subkey, KEY_SET_VALUE)
                    .context("Could not open your startup preferences")?;
                key.set_value(VALUE_NAME, &command)
                    .context("Could not enable launch at login")?;
            } else {
                let key = match root.open_subkey_with_flags(subkey, KEY_SET_VALUE) {
                    Ok(key) => key,
                    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
                    Err(error) => return Err(error).context("Could not open startup preferences"),
                };
                match key.delete_value(VALUE_NAME) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(error).context("Could not disable launch at login"),
                }
            }
            Ok(())
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            // The production backend is exercised under an isolated scratch key.
            // No test reads or changes the user's actual Run registration.
            struct TestRegistry {
                path: String,
                root: RegKey,
            }

            impl TestRegistry {
                fn new() -> Self {
                    let path = format!(r"Software\RakshaMemoTest-{}", uuid::Uuid::new_v4());
                    let (root, _) = RegKey::predef(HKEY_CURRENT_USER)
                        .create_subkey(&path)
                        .unwrap();
                    Self { path, root }
                }
            }

            impl Drop for TestRegistry {
                fn drop(&mut self) {
                    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.path);
                }
            }

            #[test]
            fn registry_enable_and_disable_preserve_unrelated_entries() {
                let registry = TestRegistry::new();
                let subkey = "StartupTest";
                let executable = Path::new(r"C:\Applications\RakshaMemo\RakshaMemo.exe");
                assert!(!registry_is_enabled(&registry.root, subkey).unwrap());
                set_registry_enabled(&registry.root, subkey, None).unwrap();
                assert!(registry.root.open_subkey(subkey).is_err());

                set_registry_enabled(&registry.root, subkey, Some(executable)).unwrap();
                assert!(registry_is_enabled(&registry.root, subkey).unwrap());
                let key = registry
                    .root
                    .open_subkey_with_flags(subkey, KEY_ALL_ACCESS)
                    .unwrap();
                let command: String = key.get_value(VALUE_NAME).unwrap();
                assert_eq!(command, r#""C:\Applications\RakshaMemo\RakshaMemo.exe""#);
                assert_eq!(key.get_raw_value(VALUE_NAME).unwrap().vtype, REG_SZ);
                key.set_value("Other application", &"preserved").unwrap();

                // Both operations are idempotent and never delete another app.
                set_registry_enabled(&registry.root, subkey, Some(executable)).unwrap();
                set_registry_enabled(&registry.root, subkey, None).unwrap();
                set_registry_enabled(&registry.root, subkey, None).unwrap();
                assert!(!registry_is_enabled(&registry.root, subkey).unwrap());
                assert_eq!(
                    key.get_value::<String, _>("Other application").unwrap(),
                    "preserved"
                );
                assert_eq!(key.enum_values().count(), 1);
            }

            #[test]
            fn registry_detects_and_replaces_stale_registration() {
                let registry = TestRegistry::new();
                let subkey = "StartupTest";
                let old = Path::new(r"C:\Old location\RakshaMemo.exe");
                let new = Path::new(r"C:\New location\RakshaMemo.exe");
                set_registry_enabled(&registry.root, subkey, Some(old)).unwrap();
                assert!(registry_is_enabled(&registry.root, subkey).unwrap());
                set_registry_enabled(&registry.root, subkey, Some(new)).unwrap();
                assert!(registry_is_enabled(&registry.root, subkey).unwrap());
                let key = registry
                    .root
                    .open_subkey_with_flags(subkey, KEY_ALL_ACCESS)
                    .unwrap();
                assert_eq!(
                    key.get_value::<String, _>(VALUE_NAME).unwrap(),
                    windows_command(new).unwrap()
                );

                // An invalid replacement cannot destroy a valid registration.
                assert!(
                    set_registry_enabled(&registry.root, subkey, Some(Path::new("relative.exe")))
                        .is_err()
                );
                assert!(registry_is_enabled(&registry.root, subkey).unwrap());
                assert_eq!(
                    key.get_value::<String, _>(VALUE_NAME).unwrap(),
                    windows_command(new).unwrap()
                );

                // Even a malformed external edit can be disabled from the app.
                key.set_value(VALUE_NAME, &42_u32).unwrap();
                assert!(registry_is_enabled(&registry.root, subkey).unwrap());
                set_registry_enabled(&registry.root, subkey, None).unwrap();
                assert!(!registry_is_enabled(&registry.root, subkey).unwrap());
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    mod backend {
        use super::*;
        use std::{fs, io::ErrorKind, path::PathBuf};

        fn entry_path() -> Result<PathBuf> {
            const ID: &str = "com.wishdom.rakshamemo";
            #[cfg(target_os = "linux")]
            {
                Ok(dirs::config_dir()
                    .context("Could not locate your configuration directory")?
                    .join(format!("autostart/{ID}.desktop")))
            }
            #[cfg(target_os = "macos")]
            {
                Ok(dirs::home_dir()
                    .context("Could not locate your home directory")?
                    .join(format!("Library/LaunchAgents/{ID}.plist")))
            }
        }

        fn entry_contents() -> Result<String> {
            let executable = std::env::current_exe()?;
            #[cfg(target_os = "linux")]
            return desktop_entry(&executable);
            #[cfg(target_os = "macos")]
            return launch_agent(&executable);
        }

        pub(super) fn is_enabled() -> Result<bool> {
            entry_path()?
                .try_exists()
                .context("Could not read the startup registration")
        }

        pub(super) fn set_enabled(enabled: bool) -> Result<()> {
            if enabled {
                return crate::settings::write_atomic(&entry_path()?, entry_contents()?.as_bytes())
                    .context("Could not enable launch at login");
            }
            match fs::remove_file(entry_path()?) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error).context("Could not disable launch at login"),
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    mod backend {
        use super::*;

        pub(super) fn is_enabled() -> Result<bool> {
            Ok(false)
        }

        pub(super) fn set_enabled(_enabled: bool) -> Result<()> {
            anyhow::bail!("Launch at login is supported on Windows, macOS and Linux")
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn path_with_name(name: &str) -> std::path::PathBuf {
            std::env::temp_dir().join(name)
        }

        #[test]
        fn windows_command_quotes_spaces_and_does_not_invoke_a_shell() {
            let executable = path_with_name("RakshaMemo & app.exe");
            assert_eq!(
                windows_command(&executable).unwrap(),
                format!("\"{}\"", executable.display())
            );
            assert!(windows_command(&path_with_name("bad\"name.exe")).is_err());
            assert!(windows_command(&path_with_name(&"x".repeat(270))).is_err());
        }

        #[test]
        fn desktop_entry_escapes_both_parsing_layers_and_field_codes() {
            let executable = path_with_name("a b\"$`%\\app");
            let entry = desktop_entry(&executable).unwrap();
            let exec = entry
                .lines()
                .find(|line| line.starts_with("Exec="))
                .unwrap();
            assert!(exec.starts_with("Exec=\""));
            assert!(exec.ends_with('"'));
            assert!(exec.contains(r#"a b\\"\\$\\`%%\\\\app"#));
            assert_eq!(
                entry
                    .lines()
                    .filter(|line| line.starts_with("Exec="))
                    .count(),
                1
            );
        }

        #[test]
        fn launch_agent_escapes_xml_and_only_runs_once_at_login() {
            let entry = launch_agent(&path_with_name("A&B <c> 'd' \"e\"")).unwrap();
            assert!(entry.contains("A&amp;B &lt;c&gt; &apos;d&apos; &quot;e&quot;"));
            assert!(entry.contains("<key>RunAtLoad</key><true/>"));
            assert!(!entry.contains("KeepAlive"));
        }

        #[test]
        fn malformed_startup_paths_are_rejected_before_writing() {
            for name in ["foo\nExec=bad", "foo\rbar", "foo\tbar"] {
                let path = path_with_name(name);
                assert!(windows_command(&path).is_err());
                assert!(desktop_entry(&path).is_err());
                assert!(launch_agent(&path).is_err());
            }
            assert!(desktop_entry(Path::new("relative/path")).is_err());
            assert!(desktop_entry(&path_with_name("foo=bar")).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paths_share_one_data_folder_named_for_the_app() {
        let base = dirs::data_local_dir().unwrap();
        #[cfg(target_os = "windows")]
        let base = base.join("Wishdom");
        let vault = default_vault_path().unwrap();
        assert_eq!(vault, base.join(APP_NAME).join("vault.json"));
        assert_eq!(
            default_settings_path().unwrap(),
            vault.with_file_name("settings.json")
        );
    }

    #[test]
    fn session_query_succeeds_without_side_effects() {
        // The value depends on the machine; this checks the call is safe.
        let _ = session_locked();
    }

    #[test]
    fn lock_excludes_second_editor_and_releases_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.json");
        let first = SingleInstance::acquire(&path).unwrap();
        assert!(SingleInstance::acquire(&path).is_err());
        // Locking must neither create nor truncate the vault itself.
        assert!(!path.exists());
        fs::write(&path, "encrypted content").unwrap();
        assert!(SingleInstance::acquire(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "encrypted content");
        drop(first);
        let _second = SingleInstance::acquire(&path).unwrap();
        assert!(directory.path().join("vault.json.lock").is_file());
    }

    #[test]
    fn separate_vaults_have_independent_locks() {
        let directory = tempfile::tempdir().unwrap();
        let _first = SingleInstance::acquire(&directory.path().join("first.json")).unwrap();
        let _second = SingleInstance::acquire(&directory.path().join("second.json")).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn new_vault_directories_and_lock_file_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("new/private");
        let _instance = SingleInstance::acquire(&parent.join("vault.json")).unwrap();
        for created in [directory.path().join("new"), parent.clone()] {
            let mode = fs::metadata(created).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let mode = fs::metadata(parent.join("vault.json.lock"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
