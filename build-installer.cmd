@echo off
rem Build the Windows installer players download: an NSIS setup exe that
rem installs per user (no admin prompt) and puts Faeries Launcher in the
rem Start menu. Output: target\release\bundle\nsis\Faeries Launcher_<version>_x64-setup.exe
rem
rem This rebuilds the interface and the launcher in release mode (a few
rem minutes the first time). Uses npm.cmd / npx.cmd so PowerShell's
rem execution policy is not involved.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul || set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"
where cargo >nul 2>nul || (
  echo Cargo was not found. Install Rust from https://rustup.rs and try again.
  exit /b 1
)

pushd ui
if not exist node_modules call npm.cmd install --no-audit --no-fund || (popd & exit /b 1)
popd
rem The CLI looks for tauri.conf.json in subfolders of the current directory,
rem so it must run from the repository root (the config is apps\launcher\).
rem `--features custom-protocol` matches run.cmd's build exactly, so neither
rem script makes the other relink the launcher.
call "%~dp0ui\node_modules\.bin\tauri.cmd" build --features custom-protocol %* || exit /b 1

rem Settle Tauri's generated-asset timestamps so the next run.cmd start is instant.
node scripts\backdate-codegen-assets.mjs release

echo.
echo Installer(s):
dir /b "%~dp0target\release\bundle\nsis\*.exe"
