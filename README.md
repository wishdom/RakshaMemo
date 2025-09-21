# Secure Notepad

An encrypted desktop notepad and password manager with a professional UI, dark/light themes, and crash-resilient autosave for notes.

## Features

- **Encrypted vault** using Argon2id KDF and AES-256-GCM
- **Master password** setup/unlock flow (your password is never stored)
- **Notes** with fast autosave and save-on-exit for crash resilience
- **Password manager** with add/edit/delete, masked display, and copy-to-clipboard
- **Light/Dark theme** toggle and settings persistence (theme, window geometry)
- **Atomic writes** and restrictive file permissions for the vault file

## Tech Stack

- Python 3.10+
- PySide6 for the UI
- cryptography + argon2-cffi for encryption
- platformdirs for cross-platform data paths

## Install

Create and activate a virtual environment (recommended) and install dependencies.

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
```

## Run

```bash
python main.py
```

On first launch you will be prompted to create a master password. Keep it safe—there is no recovery if it is forgotten.

## Data Location

The encrypted vault is stored in your user data directory, typically:

- Linux: `~/.local/share/Secure Notepad/vault.json`
- macOS: `~/Library/Application Support/Secure Notepad/vault.json`
- Windows: `%LOCALAPPDATA%/Secure Notepad/vault.json`

File permissions are set to user-only (0600) on POSIX systems.

## Security Notes

- Argon2id parameters are tuned for interactive use (time cost 3, memory 64 MiB, parallelism 2).
- AES-GCM provides confidentiality and integrity; any tampering will fail decryption.
- Clipboard operations are convenient but may be captured by other software on the system.
- There is no cloud sync or network access; your data stays local.

## Troubleshooting

- If you forget the master password, the vault cannot be recovered. You may delete the vault file to reset the app (this will permanently erase your data).
- If the UI appears without theme, ensure PySide6 installed correctly.

## Development

Project entry point: `main.py`

Key modules:

- `secure_notepad/crypto/crypto.py` – key derivation and AES-GCM
- `secure_notepad/storage/store.py` – encrypted storage and CRUD APIs
- `secure_notepad/ui/` – Qt UI (main window, tabs, dialogs, theming)