@echo off
REM Cross-platform-friendly Windows runner for Secure Notepad
REM - Creates a virtualenv in .\.venv if missing
REM - Installs requirements
REM - Launches the app
setlocal EnableExtensions EnableDelayedExpansion

set "SCRIPT_DIR=%~dp0"
set "VENV_DIR=%SCRIPT_DIR%.venv"
set "REQ_FILE=%SCRIPT_DIR%requirements.txt"
set "MAIN_PY=%SCRIPT_DIR%main.py"

REM Choose a Python executable
where py >NUL 2>&1
if %ERRORLEVEL%==0 (
  set "PY=py"
) else (
  where python >NUL 2>&1
  if %ERRORLEVEL%==0 (
    set "PY=python"
  ) else (
    echo Error: Python not found. Please install Python 3.10+ and retry.
    exit /b 1
  )
)

REM Create venv if needed
if not exist "%VENV_DIR%\Scripts\python.exe" (
  echo [setup] Creating virtual environment at %VENV_DIR%
  %PY% -m venv "%VENV_DIR%"
)

REM Upgrade pip (best-effort)
call "%VENV_DIR%\Scripts\python.exe" -m pip install --upgrade pip >NUL 2>NUL

REM Install requirements
if exist "%REQ_FILE%" (
  echo [setup] Installing dependencies from requirements.txt
  call "%VENV_DIR%\Scripts\python.exe" -m pip install -r "%REQ_FILE%"
  if %ERRORLEVEL% NEQ 0 (
    echo [error] Dependency installation failed with exit code %ERRORLEVEL%.
    echo.
    pause
    exit /b %ERRORLEVEL%
  )
)

REM Launch app
call "%VENV_DIR%\Scripts\python.exe" "%MAIN_PY%"
set "APP_EXIT=%ERRORLEVEL%"
if %APP_EXIT% NEQ 0 (
  echo [error] Application exited with code %APP_EXIT%.
  echo.
  echo If this window closed too quickly before, you can now review the error above.
  pause
)
exit /b %APP_EXIT%
