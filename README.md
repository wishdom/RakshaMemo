# RakshaMemo

**Private notes and passwords, saved on your computer.**

RakshaMemo is a desktop app that keeps your notes and account details together in an encrypted vault. Unlock it with one master password. Your data stays on your computer; there is no account to create and no cloud sync.

## Download

Choose your computer below. You do not need any programming tools to use the app.

| Your computer | Download |
| --- | --- |
| Windows 10/11 with an Intel or AMD 64-bit processor | [Download RakshaMemo for Windows (.exe)](https://github.com/wishdom/RakshaMemo/releases/latest/download/RakshaMemo.exe) |
| Mac with Apple silicon (M1, M2, M3, or newer) | [Download RakshaMemo for Apple silicon (.zip)](https://github.com/wishdom/RakshaMemo/releases/latest/download/RakshaMemo-macOS-Apple-Silicon.zip) |
| Mac with an Intel processor | [Download RakshaMemo for Intel Mac (.zip)](https://github.com/wishdom/RakshaMemo/releases/latest/download/RakshaMemo-macOS-Intel.zip) |
| Linux with an Intel or AMD 64-bit processor | [Download RakshaMemo for Linux (.tar.gz)](https://github.com/wishdom/RakshaMemo/releases/latest/download/RakshaMemo-Linux-x86_64.tar.gz) |

On a Mac, open **Apple menu → About This Mac** to check whether it lists an Apple chip or an Intel processor. [Release notes and older versions](https://github.com/wishdom/RakshaMemo/releases) are also available.

## Get started

1. Download the app for your computer. On Windows, open `RakshaMemo.exe`. On macOS, unzip the download, move `RakshaMemo.app` to **Applications**, and open it. On Linux, extract the archive and run `RakshaMemo`.
2. Create a master password of at least **12 characters**. This password unlocks all your notes and saved passwords.
3. Add a note or a password entry. Changes save automatically.
4. Use **Lock** when you are finished. The app also locks after five minutes of inactivity by default; you can change this in **Settings**.

**Keep your master password safe: there is no password recovery.** If you forget it, you cannot unlock your vault or backups made with that password.

<details>
<summary>Help opening the app</summary>

The Windows download runs without an installer. It is not digitally signed, so Windows may show an unknown-publisher notice.

The Mac downloads require **macOS 13 or newer**. They are not notarized by Apple, so macOS may block the first launch. If you trust this download, follow [Apple's instructions for opening an app from an unidentified developer](https://support.apple.com/en-us/102445).

The Linux download is built on Ubuntu 22.04 for 64-bit Intel/AMD computers. It needs a graphical desktop and an XDG desktop portal for file dialogs. From the extracted folder, you can launch it with `./RakshaMemo` in a terminal.

</details>

## What you can do

- Write, search, sort, and pin notes.
- Store account details, organize passwords into categories, and generate new passwords.
- Copy credentials quickly; the app clears its copied value from the clipboard after 30 seconds if it is still there.
- Switch between light and dark themes and **English, French, or Japanese**.
- Export encrypted backups and restore previous saved versions from **Settings**.
- Enable **Launch at sign-in** in Settings to open the locked app when you sign in to your computer.

Useful shortcuts: **Ctrl/Cmd+N** creates an entry, **Ctrl/Cmd+F** searches, and **Ctrl/Cmd+L** locks the app.

## Keep a backup

In **Settings → Export vault…**, save an encrypted copy of your notes and passwords. Keep a copy somewhere separate from this computer, and remember the master password used when you exported it.

**Importing replaces your current data; it does not combine two vaults.** Export your current vault first. Then use **Settings → Import vault…** and enter the imported file's password. That password becomes your master password for the imported vault.

The app also keeps local backup versions, available through **Settings → Restore a previous version**. These are useful for undoing changes, but cannot help if the computer or drive is lost.

## Privacy and security

RakshaMemo encrypts stored notes and passwords using **AES-256-GCM**, with a key derived from your master password using **Argon2id**. It does not send your vault to a server.

Encryption protects the saved vault. Keep your computer secure and lock the app when you leave it. Clipboard history or other apps may retain information you copy, even after RakshaMemo clears the clipboard.

<details>
<summary>Where your data is stored</summary>

Your vault is separate from the downloaded app:

| System | Default vault location |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Wishdom\RakshaMemo\vault.json` |
| macOS | `~/Library/Application Support/RakshaMemo/vault.json` |
| Linux | `~/.local/share/RakshaMemo/vault.json` (or `$XDG_DATA_HOME/RakshaMemo/vault.json` when set) |

Preferences and local backup files are stored alongside the vault. For a separate vault, launch the app with `--vault "/path/to/vault.json"`.

If you enable **Launch at sign-in**, keep the app in a permanent location. After moving it, turn that setting off and back on to update its location.

</details>

<details>
<summary>Build from source and contribute</summary>

RakshaMemo is written in Rust with egui/eframe. Install [Rust](https://rustup.rs/) **1.99 or newer**, then run these commands from the repository directory:

```bash
cargo run --release --locked
```

Windows builds use the MSVC toolchain and Visual Studio Build Tools with **Desktop development with C++**. On macOS, install the Xcode command-line tools (`xcode-select --install`). On Debian/Ubuntu, install the native build dependencies:

```bash
sudo apt-get install build-essential pkg-config libwayland-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev libxi-dev libgl1-mesa-dev libssl-dev
```

Linux needs a graphical desktop and an XDG desktop portal for file dialogs. Install `fonts-noto-cjk` on Debian/Ubuntu for Japanese text if no suitable font is installed.

`cargo build --release --locked` creates the executable in `target/release/`. The `cargo app` shortcut builds and installs it in the repository's `bin/` directory.

Before submitting a change:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Interface translations live in `src/i18n.rs`. Check desktop behavior on the relevant operating system when changing startup, clipboard, display, or file-dialog features.

</details>
