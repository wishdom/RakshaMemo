# Secure Notepad

A native Rust desktop app for encrypted notes and passwords. The interface uses egui/eframe; there is no Python, Qt, webview, or JavaScript runtime.

## Features

- Multiple notes with automatic saving, search, rename, and deletion.
- Password entries with search, categories, extra notes, masked display, and clipboard actions.
- Password generation with configurable length and character sets.
- Argon2id key derivation and AES-256-GCM authenticated encryption.
- Manual lock and configurable inactivity lock (five minutes by default).
- Copied credentials expire from the clipboard after 30 seconds if the clipboard still contains the copied value.
- Encrypted export/import, with authentication before an import replaces your current vault.
- Light/dark themes and persistent preferences.
- An optional **Launch at sign-in** setting for opening the app at OS startup.
- A single running instance per vault to avoid concurrent writes.

## Build and run

Install [Rust](https://rustup.rs/) 1.95 or newer. Keep `Cargo.lock` when building; it pins the application's dependency versions.

On Windows, use the MSVC Rust toolchain and install the Visual Studio Build Tools with **Desktop development with C++** if the Rust installer requests them. On macOS, install the Xcode command-line tools with `xcode-select --install`.

For Debian/Ubuntu Linux, install the native build dependencies:

```bash
sudo apt-get install build-essential pkg-config libwayland-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev libxi-dev libgl1-mesa-dev libssl-dev
```

The Linux desktop also needs a working graphics/display session and, for native file dialogs, an XDG desktop portal supported by your desktop environment. See the [eframe documentation](https://docs.rs/eframe/0.36.2/eframe/) for platform details.

For Japanese, Chinese, and Korean text on Linux, install Noto Sans CJK (`fonts-noto-cjk` on Debian/Ubuntu). The app uses an installed system font as a fallback; Windows and macOS also use available system CJK fonts.

From the repository directory:

```bash
cargo run --release --locked
```

Alternatively, run `run.bat` on Windows or `bash run.sh` on Linux/macOS. The scripts build and launch the Rust app; they do not install a Python environment. Initial compilation downloads Rust crates and takes longer than subsequent starts.

To build an executable:

```bash
cargo build --release --locked
```

The result is `target/release/secure-notepad.exe` on Windows or `target/release/secure-notepad` on Linux/macOS. The executable uses the platform's normal system libraries; Rust and Cargo are needed for building, not for launching an already-built executable.

On first launch, create a master password of at least 12 characters. Existing Python vault passwords still work even if they are shorter. Keep your password safe: there is no password recovery.

Notes save automatically after a short pause in typing, with periodic saves during continuous editing. **Save** or `Ctrl/Cmd+S` saves immediately. Password entry dialogs require **Save entry**; inactivity locking discards an unsaved password draft. Save failures are shown in the app, and pending edits stay in memory instead of silently closing or locking.

For a separate test or portable vault, pass a path explicitly:

```bash
cargo run --release --locked -- --vault "/path/to/private/vault.json"
```

The `--vault` option appends `.settings.json` to the full vault filename for preferences; for example, `vault.json.settings.json`. Startup registration is disabled for custom vault paths because OS sign-in opens the default vault.

## Open at startup

Open **Settings** and enable **Launch at sign-in**. The option is off by default, applies to the current OS user, and opens the locked app after you sign in. It does not unlock the vault or run before sign-in.

- **Windows:** registers the executable in the current user's `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` key.
- **Linux:** creates an XDG autostart `.desktop` entry for your desktop session.
- **macOS:** creates a per-user LaunchAgent.

Build the release executable and put it in a permanent location before enabling startup. Registration records that executable's absolute path, so moving or deleting it breaks automatic launch. After moving it, launch the app from its new location and turn startup off and back on. Disable the option before uninstalling. Your OS's startup/login-item controls may also disable an entry.

## Migrating from Python

The Rust app reads the original version-1 encrypted vault format directly, including older vaults that stored one notes field instead of a list. Your master password remains the same, and notes, password entries, categories, and timestamps stay in the vault.

Before upgrading, export an encrypted backup from the Python app and close it. Do not run the Python and Rust versions against the same vault at the same time: the Python version does not participate in the Rust app's instance lock.

The default vault locations match the Python app:

| Platform | Vault location |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Wishdom\Secure Notepad\vault.json` |
| macOS | `~/Library/Application Support/Secure Notepad/vault.json` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/Secure Notepad/vault.json` |

The old README omitted `Wishdom` from the Windows path. If the app offers to create a vault instead of unlocking your existing one, stop and check the data location or import your encrypted backup. OS accounts and overridden data-directory environment variables can change the resolved path.

Rust preferences are stored as `settings.json` next to the default vault. The previous Qt theme and window settings are not migrated. They do not affect the encrypted data.

## Backups and imports

Export encrypts a snapshot of the notes and saved password entries currently in memory, including pending note edits, without changing the active vault file. Unsaved password dialog drafts must be saved as entries first. Keep exported copies somewhere independent of the computer and retain the password that was used for each export.

If saving fails, such as when another program changed the vault on disk, export an encrypted copy to preserve your pending work. You can then explicitly discard local edits and lock, and reopen the vault. The export remains independent of the file that caused the conflict.

**Settings → Import encrypted vault** replaces the current vault; it does not merge entries. The app asks for the imported vault's password and verifies its contents before replacing the current vault. The imported file's password becomes the active vault password.

Each save keeps the previous encrypted file beside the vault as `vault.json.bak`. This is one rolling recovery copy: the next successful save or import overwrites it. Import also keeps a separate `vault.json.pre-import-<uuid>.bak` file that survives later autosaves and uses the previous vault's password. These pre-import copies are not automatically pruned; manage them as encrypted backups.

If the current vault cannot be opened, close the app and copy the vault and backup files to a safe location. Restore a copy of the desired backup as `vault.json`, then unlock with the password used by that backup.

## Security behavior and limits

- The default Argon2id parameters remain three iterations, 64 MiB of memory, two lanes, and a 32-byte key. AES-256-GCM uses a fresh random nonce for each save.
- The unlocked app retains the derived key so autosave does not rerun the expensive password derivation on every edit. Locking releases that key and clears app-owned sensitive buffers where possible.
- This is best-effort memory cleanup, not a guarantee that every copy is erased. GUI text/undo buffers, allocators, OS paging, screenshots, crash dumps, and other processes can retain or expose plaintext while the app is in use.
- Clipboard expiry applies to the app's credential-copy controls. Clipboard history, synchronization, and other software may retain earlier copies; clearing the current clipboard cannot recall them.
- Atomic file replacement protects against partially written vaults. It does not replace independent backups or guarantee survival of disk failure. POSIX vault files use user-only permissions; Windows uses the current user's data directory and its inherited access controls.
- Imports reject oversized vaults (over 32 MiB), unsupported formats, and excessive key-derivation parameters before processing them. External vault changes are detected before saving so a stale window cannot silently overwrite another version.
- The app does not provide cloud synchronization or send vault contents to a server. Encryption protects stored files; it cannot protect an unlocked vault on a compromised computer.
- A forgotten password cannot be recovered. Keep encrypted backups and a separate safe record of your master password.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

CI runs formatting, linting, tests, and a release build on Windows, macOS, and Linux. OS integration and graphical behavior still need a smoke test on the target desktop, particularly startup registration, clipboard access, display scaling, and native file dialogs.
