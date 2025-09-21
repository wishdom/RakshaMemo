#!/usr/bin/env bash
# Cross-platform-friendly Linux/macOS runner for Secure Notepad
# - Creates a virtualenv in ./.venv if missing
# - Installs requirements
# - Launches the app
set -euo pipefail

# Resolve repository root (directory containing this script)
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
VENV_DIR="$SCRIPT_DIR/.venv"
REQ_FILE="$SCRIPT_DIR/requirements.txt"
MAIN_PY="$SCRIPT_DIR/main.py"

# Choose a Python executable
if command -v python3 >/dev/null 2>&1; then
  PY=python3
elif command -v python >/dev/null 2>&1; then
  PY=python
else
  echo "Error: Python not found. Please install Python 3.10+ and retry." >&2
  exit 1
fi

# Create venv if needed
if [ ! -f "$VENV_DIR/bin/python" ]; then
  echo "[setup] Creating virtual environment at $VENV_DIR"
  "$PY" -m venv "$VENV_DIR"
fi

# Activate venv
# shellcheck disable=SC1091
source "$VENV_DIR/bin/activate"

# Install requirements
if [ -f "$REQ_FILE" ]; then
  echo "[setup] Installing dependencies from requirements.txt"
  python -m pip install --upgrade pip >/dev/null 2>&1 || true
  python -m pip install -r "$REQ_FILE"
fi

# Launch app
exec python "$MAIN_PY"
