@echo off
setlocal EnableExtensions
REM Build and run the native Rust application from any working directory.
where cargo >NUL 2>&1
if errorlevel 1 (
  echo Error: Cargo was not found. Install Rust from https://rustup.rs/ and reopen your terminal.
  exit /b 1
)
pushd "%~dp0"
if errorlevel 1 exit /b 1
cargo run --release --locked -- %*
set "APP_EXIT=%ERRORLEVEL%"
popd
exit /b %APP_EXIT%
