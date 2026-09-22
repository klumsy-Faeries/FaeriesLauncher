@echo off
rem Development mode: Vite dev server + launcher window with hot reload.
rem Uses npm.cmd so PowerShell's execution policy is not involved.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul || set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"
where cargo >nul 2>nul || (
  echo Cargo was not found. Install Rust from https://rustup.rs and try again.
  exit /b 1
)

if not exist "%~dp0ui\node_modules" (
  pushd ui
  call npm.cmd install --no-audit --no-fund || (popd & exit /b 1)
  popd
)

rem The Tauri CLI looks for tauri.conf.json in subfolders of the current
rem directory, so it must run from the repository root (the config lives in
rem apps\launcher\). The locally installed CLI is used directly: no npx lookup.
call "%~dp0ui\node_modules\.bin\tauri.cmd" dev %*
