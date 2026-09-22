@echo off
rem Build the interface, then start the launcher.
rem
rem The `custom-protocol` feature is what makes Tauri embed the built
rem interface. Without it the window loads the Vite dev server (devUrl) and
rem shows "localhost refused to connect" unless that server happens to be
rem running - the build profile makes no difference. The first release build
rem takes a few minutes; later runs only rebuild what changed, and start in
rem seconds when nothing did.
rem
rem For fast UI iteration use dev.cmd instead, which starts Vite alongside.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul || set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"
where cargo >nul 2>nul || (
  echo Cargo was not found. Install Rust from https://rustup.rs and try again.
  exit /b 1
)

pushd ui
if not exist node_modules call npm.cmd install --no-audit --no-fund || (popd & exit /b 1)
rem Only rebuilds when interface sources changed: an unconditional build
rem rewrites ui\dist, and Tauri then relinks the launcher (~1.5 min) for
rem nothing. cargo build below is likewise instant when nothing changed.
node scripts\build-if-stale.mjs || (popd & exit /b 1)
popd

echo Starting Faeries Launcher (first build may take a few minutes)...
cargo build --release -p faerie-launcher --features custom-protocol || exit /b 1
rem Tauri's code generator writes its asset cache while the launcher
rem compiles, which would make the very next build relink the launcher again
rem (~1.5 min) for nothing. Settle those timestamps so an unchanged launcher
rem starts at once next time.
node scripts\backdate-codegen-assets.mjs release
start "" /d "%~dp0" "%~dp0target\release\faerie-launcher.exe"
