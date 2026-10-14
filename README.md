# RakshaMemo

*Rakṣā* is Sanskrit for "protection". RakshaMemo is a native Rust desktop app for encrypted notes and passwords. The interface uses egui/eframe; there is no Python, Qt, webview, or JavaScript runtime.

The look takes its cue from Indian temple architecture. The icon is a gold lotus mandala, the flower seen from above, with a keyhole in its seed pod. Dark mode is a lamp-lit shrine at night (indigo stone, ivory text, temple gold); light mode is sandalwood and palm-leaf paper with brass accents. Primary actions are saffron. A carved frieze runs under the toolbar, cards have carved corners, a mandala sits behind the unlock form, and empty pages show a diya (oil lamp). Every ornament is drawn in code, so there are no image assets and nothing blurs at high DPI.

## Features

- Multiple notes with automatic saving, search, rename, deletion, pinning, and sorting (last edited, title, or newest first).
- A compact password table with search, categories, extra notes, masked passwords, and clipboard actions.
- Automatic saving for notes and password edits, with encryption and disk writes performed in the background.
- Password generation with configurable length and character sets, and a strength meter (zxcvbn) for entries and master passwords.
- Argon2id key derivation and AES-256-GCM authenticated encryption.
- Manual lock, configurable inactivity lock (five minutes by default), and locking when the window is minimized, the computer sleeps, or (on Windows) the session is locked.
- Master password changes, and periodic encrypted backup versions you can restore from Settings.
- Copied credentials expire from the clipboard after 30 seconds if the clipboard still contains the copied value.
- Encrypted export/import, with authentication before an import replaces your current vault.
- Temple-inspired light and dark themes with a one-click toolbar switch, a date and time display (24- or 12-hour), and persistent preferences.
- English, French, and Japanese. The first launch follows your system language; switch at any time from the header (beside the theme button) or **Settings → Appearance**.
- Keyboard shortcuts: **Ctrl/Cmd+N** new note or password, **Ctrl/Cmd+F** search, **Ctrl/Cmd+L** lock, **Ctrl/Cmd+S** save now, **Escape** closes dialogs.
- An optional **Launch at sign-in** setting for opening the app at OS startup.
- A single running instance per vault to avoid concurrent writes.

## Build and run

Install [Rust](https://rustup.rs/) 1.95 or newer. Keep `Cargo.lock` when building; it pins the application's dependency versions.

On Windows, use the MSVC Rust toolchain and install the Visual Studio Build Tools with **Desktop development with C++** if the Rust installer requests them. On macOS, install the Xcode command-line tools with `xcode-select --install`.

For Debian/Ubuntu Linux, install the native build dependencies:

```bash
sudo apt-get install build-essential pkg-config libwayland-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev libxi-dev libgl1-mesa-dev libssl-dev
```

Titles use an installed classical serif when one is available (Palatino Linotype on Windows, Georgia on macOS, Noto Serif or DejaVu Serif on Linux) and the interface font otherwise.

The Linux desktop also needs a working graphics/display session and, for native file dialogs, an XDG desktop portal supported by your desktop environment. See the [eframe documentation](https://docs.rs/eframe/0.36.2/eframe/) for platform details.

For Japanese, Chinese, and Korean text on Linux, install Noto Sans CJK (`fonts-noto-cjk` on Debian/Ubuntu). The app uses an installed CJK font for both Latin and CJK interface text; Windows and macOS also use available system CJK fonts.

From the repository directory:

```bash
cargo run --release --locked
```

The first build downloads Rust crates and takes longer than subsequent starts.

To build the app into this folder:

```bash
cargo app
```

This builds the release version and installs it as `bin/RakshaMemo.exe` on Windows (with its icon and version details embedded) or `bin/RakshaMemo` on Linux/macOS. Run it from there; rebuilding replaces it in place, so close the app before running `cargo app` again. The `bin` folder is ignored by git. `cargo app` is an alias defined in `.cargo/config.toml` for `cargo install --path . --root . --locked --force`.

`cargo build --release --locked` also works and leaves the executable in `target/release/` (or under `CARGO_TARGET_DIR`, if set). The executable uses the platform's normal system libraries; Rust and Cargo are needed for building, not for launching an already-built executable.

On first launch, create a master password of at least 12 characters. Existing Python vault passwords still work even if they are shorter. Keep your password safe: there is no password recovery.

Changes are saved automatically once you pause typing for a moment (at most every five seconds during continuous typing, and immediately when you switch to another window, lock, or close). Encryption and disk writes run in the background; edits made during a save are included in the next one. The save indicator shows when everything has reached disk. There is no **Save** button; **Ctrl/Cmd+S** saves at once.

Password entries appear in a compact table, including a preview of **Extra information**. Click a title, username, password, category, or extra-information value to edit it directly; every change saves automatically. **Enter** or **Escape** finishes a cell without reverting saved changes. Extra information expands into a multiline editor: **Enter** inserts a new line, and **Ctrl/Cmd+Enter** finishes. Rows stay in place while you type. Each password has an eye button to show or hide it; leaving the page or locking hides revealed values again.

Search covers titles, usernames, categories, and extra information. Use **Columns** to show or hide optional metadata. Use an entry's **Edit** button for the full-page editor; **Back to passwords** returns to the table. Account details and a large extra-information field appear side by side on wider windows, with a single scrolling page on smaller windows.

Categories are optional groups for filtering accounts. Type a category while editing an entry to create it; choose **Manage categories…** inside the category filter to rename groups or remove unused ones. Adding a password creates and saves an **Untitled** entry immediately, ready to fill in. Delete an unwanted entry from the table. Save failures are shown in the app, and pending edits stay in memory instead of silently closing or locking.

**Settings** groups appearance, startup/security, and vault backups in one scrollable page. Preferences save automatically. Automatic locking accepts 0–1440 minutes; 0 disables it. An installed CJK font is used for both Latin and CJK interface text where available, keeping mixed-script text on one baseline.

Notes can be pinned with the star button (or by right-clicking a note in the list) to keep them at the top. **Enter** in a note's title moves to its body.

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

Build the app with `cargo app` and start it from `bin/` (or copy it to another permanent location) before enabling startup. Registration records that executable's absolute path, so moving or deleting it breaks automatic launch. After moving it, launch the app from its new location and turn startup off and back on. Disable the option before uninstalling. Your OS's startup/login-item controls may also disable an entry.

## Migrating from Python

The Rust app reads the original version-1 encrypted vault format directly, including older vaults that stored one notes field instead of a list. Your master password remains the same, and notes, password entries, categories, and timestamps stay in the vault.

Before upgrading, export an encrypted backup from the Python app and close it. Do not run the Python and Rust versions against the same vault at the same time: the Python version does not participate in the Rust app's instance lock.

The vault is kept in a folder named after the app:

| Platform | Vault location |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Wishdom\RakshaMemo\vault.json` |
| macOS | `~/Library/Application Support/RakshaMemo/vault.json` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/RakshaMemo/vault.json` |

The Python app stored its vault in a differently named folder, which RakshaMemo does not read. When upgrading from it, use **Settings → Import vault…** with an encrypted export, or move the vault and its backup files into the folder above while both apps are closed. If the app offers to create a vault instead of unlocking your existing one, stop and check the data location or import your encrypted backup. OS accounts and overridden data-directory environment variables can change the resolved path.

Rust preferences are stored as `settings.json` next to the default vault. The previous Qt theme and window settings are not migrated. They do not affect the encrypted data.

## Backups and imports

Export encrypts a snapshot of the notes and password entries currently in memory, including edits waiting to finish saving, without changing the active vault file. Keep exported copies somewhere independent of the computer and retain the password that was used for each export.

If saving fails, such as when another program changed the vault on disk, export an encrypted copy to preserve your pending work. You can then explicitly discard local edits and lock, and reopen the vault. The export remains independent of the file that caused the conflict.

**Settings → Import vault…** replaces the current vault; it does not merge entries. The app asks for the imported vault's password and verifies its contents before replacing the current vault. The imported file's password becomes the active vault password.

Each save keeps the previous encrypted file beside the vault as `vault.json.bak`. This is one rolling recovery copy: the next successful save or import overwrites it. Saves also keep an encrypted version in `vault.json.versions/` at most every 30 minutes, retaining the latest 20. **Settings → Restore a previous version** lists these; restoring asks for the password that version was saved with and works like an import, so your current vault is backed up first.

**Settings → Change password…** re-encrypts the vault after checking the current password. By default it also deletes the rolling backup and saved versions, because they still open with the old password; permanent pre-import backups are kept. Import also keeps a separate `vault.json.pre-import-<uuid>.bak` file that survives later autosaves and uses the previous vault's password. These pre-import copies are not automatically pruned; manage them as encrypted backups.

If the current vault cannot be opened, close the app and copy the vault and backup files to a safe location. Restore a copy of the desired backup as `vault.json`, then unlock with the password used by that backup.

## Security behavior and limits

- The default Argon2id parameters remain three iterations, 64 MiB of memory, two lanes, and a 32-byte key. AES-256-GCM uses a fresh random nonce for each save.
- The unlocked app retains the derived key so autosave does not rerun the expensive password derivation on every edit. Locking releases that key and clears app-owned sensitive buffers where possible.
- This is best-effort memory cleanup, not a guarantee that every copy is erased. GUI text/undo buffers, allocators, OS paging, screenshots, crash dumps, and other processes can retain or expose plaintext while the app is in use.
- Clipboard expiry applies to the app's credential-copy controls. Clipboard history, synchronization, and other software may retain earlier copies; clearing the current clipboard cannot recall them.
- Atomic file replacement protects against partially written vaults. It does not replace independent backups or guarantee survival of disk failure. POSIX vault files use user-only permissions; Windows uses the current user's data directory and its inherited access controls.
- Imports reject oversized vaults (over 32 MiB), unsupported formats, and excessive key-derivation parameters before processing them. External vault changes are detected before saving so a stale window cannot silently overwrite another version.
- The app does not provide cloud synchronization or send vault contents to a server. Encryption protects stored files; it cannot protect an unlocked vault on a compromised computer.
- Password strength estimates use zxcvbn and only analyse the first 100 characters. Its intermediate results contain password fragments, so only the score and advice are kept, and only recalculated when the password changes.
- Session-lock detection uses the Windows WTS API. On macOS and Linux, RakshaMemo detects sleep (a gap in its once-a-minute timer) and minimizing, but not a screen lock without sleep.
- A forgotten password cannot be recovered. Keep encrypted backups and a separate safe record of your master password.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

CI runs formatting, linting, tests, and a release build on Windows, macOS, and Linux. OS integration and graphical behavior still need a smoke test on the target desktop, particularly startup registration, clipboard access, display scaling, and native file dialogs.

Interface text lives in `src/i18n.rs`, one table per language. Every language must provide every string, or the build fails.

### Checking for dependency updates

`scripts/check-updates.ps1` lists the direct dependencies that have newer releases on crates.io, and changes nothing:

```text
Update now with "cargo update":
  serde_json     1.0.152   -> 1.0.154

Major versions (edit Cargo.toml; code may need changes):
  rand           0.8.8     -> 0.10.3
```

```powershell
powershell -ExecutionPolicy Bypass -File scripts\check-updates.ps1   # Windows
pwsh scripts/check-updates.ps1                                       # PowerShell 7 on any OS
```

It needs cargo and internet access.
