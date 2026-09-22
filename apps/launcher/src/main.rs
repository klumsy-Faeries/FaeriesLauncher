//! Faeries Launcher — Tauri shell.
//!
//! This crate contains no business logic: it resolves paths, loads config,
//! initializes logging, and exposes `faerie-core` to the webview through a
//! thin IPC command layer plus one forwarded event stream.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod changelog;
mod commands;
mod defaults;
mod launching;
mod locales;
mod mods;
mod perf;
mod selfheal;
mod startup;
mod state;
mod themes;
mod watcher;

use faerie_core::config::ConfigStore;
use faerie_core::{DataPaths, EventBus};
use tauri::{Emitter, Manager};

use crate::state::AppState;

/// Where this process's writes under `AppData` really land.
///
/// A process started from inside an app package — a Store app's terminal,
/// say, and everything it spawns — has its writes under
/// `%APPDATA%` and `%LOCALAPPDATA%` redirected into that package's private
/// `LocalCache` overlay, while its reads see the real folder and the overlay
/// merged. From inside, everything looks normal; a launcher started from
/// Explorer sees only the real folder: no account, no client ID, no
/// instances. That took an evening to find. The package-identity API did
/// not report it (the processes in question had no identity), so the check
/// is empirical: write a marker through the normal path and look for it
/// under the overlays. Returns the overlay the marker turned up in.
#[cfg(windows)]
fn appdata_redirect(config_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let roaming = PathBuf::from(std::env::var_os("APPDATA")?);
    let packages = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Packages");
    // Only AppData is virtualized; a relocated data dir is out of scope.
    let relative = config_dir.strip_prefix(&roaming).ok()?.to_path_buf();
    std::fs::create_dir_all(config_dir).ok()?;
    let marker = format!(".write-probe-{}", std::process::id());
    let probe = config_dir.join(&marker);
    std::fs::write(&probe, b"").ok()?;
    let found = std::fs::read_dir(&packages)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| {
            entry
                .path()
                .join("LocalCache")
                .join("Roaming")
                .join(&relative)
        })
        .find(|overlay| overlay.join(&marker).is_file());
    let _ = std::fs::remove_file(&probe);
    found
}

#[cfg(not(windows))]
fn appdata_redirect(_config_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    None
}

fn main() {
    let mut boot = startup::Startup::begin();
    let paths = DataPaths::resolve().expect("failed to resolve launcher data directory");
    paths
        .ensure_created()
        .expect("failed to create launcher data directories");

    boot.stage("paths");
    let config = ConfigStore::load(&paths);
    boot.stage("config");
    let level = config
        .get_str("advanced.log_level")
        .unwrap_or_else(|_| "info".into());
    let _log_guard = faerie_core::logging::init(&paths, &level);

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        data_dir = %paths.root.display(),
        "faeries launcher starting"
    );
    for warning in config.warnings() {
        tracing::warn!("config: {warning}");
    }
    for notice in config.notices() {
        tracing::warn!(
            "config file {} was corrupt and reset to defaults (backup: {})",
            notice.file,
            notice.backup_path
        );
    }
    // Twice a cold start has come up with no account and no client id while
    // the files on disk were intact. Record what this process actually sees
    // so the next occurrence explains itself instead of vanishing.
    {
        let inventory: Vec<String> = [
            "launcher.json",
            "ui.json",
            "accounts-settings.json",
            "accounts.json",
        ]
        .iter()
        .map(
            |name| match std::fs::metadata(paths.config_dir.join(name)) {
                Ok(meta) => format!("{name} {} B", meta.len()),
                Err(e) => format!("{name} unreadable ({})", e.kind()),
            },
        )
        .collect();
        let client_id_set = !config
            .get_str("accounts.client_id")
            .unwrap_or_default()
            .trim()
            .is_empty();
        tracing::info!(
            "config dir {}: {} · client id set: {client_id_set}",
            paths.config_dir.display(),
            inventory.join(", ")
        );
        // A launcher started one way sees these files; started another way
        // it does not. Log what this process resolves the folder to, what a
        // listing returns, and where its writes really land, so two starts
        // can be compared line for line.
        match appdata_redirect(&paths.config_dir) {
            Some(overlay) => tracing::warn!(
                "AppData writes are REDIRECTED to {}: this process runs inside an app \
                 package's file-system overlay, and a launcher started from Explorer will not \
                 see anything it saves",
                overlay.display()
            ),
            None => {
                tracing::info!("AppData writes land in the real folder (no app-package overlay)")
            }
        }
        let canonical = std::fs::canonicalize(&paths.config_dir)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("<canonicalize failed: {e}>"));
        let listing = match std::fs::read_dir(&paths.config_dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|e| {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    format!("{} ({size} B)", e.file_name().to_string_lossy())
                })
                .collect::<Vec<_>>()
                .join(", "),
            Err(e) => format!("<read_dir failed: {e}>"),
        };
        tracing::info!("config dir resolves to {canonical}; listing: {listing}");
        tracing::info!(
            "process: pid {}, cwd {}, exe {}",
            std::process::id(),
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
    }
    // Files this start could not see; see selfheal.rs for why that matters.
    let started_at_wall = std::time::SystemTime::now();
    let mut missing_files: Vec<std::path::PathBuf> = config
        .missing_files()
        .iter()
        .map(|file| paths.config_dir.join(file.file_name()))
        .collect();
    let accounts_file = paths.config_dir.join("accounts.json");
    if std::fs::metadata(&accounts_file).is_err() {
        missing_files.push(accounts_file);
    }
    let selfheal_paths = paths.clone();

    boot.stage("logging");
    let themes_dir = paths.themes_dir.clone();
    let bus = EventBus::new();
    let http = faerie_net::build_client();
    let manifest =
        faerie_minecraft::manifest::ManifestService::new(http.clone(), &paths.cache_dir, None);
    let instances = faerie_instances::InstanceStore::new(paths.instances_dir.clone());
    let accounts = faerie_auth::AccountStore::new(&paths.config_dir);

    boot.stage("services");
    boot.report();
    let startup_timings = boot.timings();
    let startup_total_ms = boot.total().as_secs_f64() * 1000.0;

    let state = AppState {
        config: std::sync::RwLock::new(config),
        bus: bus.clone(),
        http,
        manifest,
        instances,
        accounts,
        pending_sign_in: Default::default(),
        running_game: Default::default(),
        play_cancel: Default::default(),
        perf_sampler: std::sync::Mutex::new(sysinfo::System::new()),
        startup_timings,
        startup_total_ms,
        started_at: std::time::Instant::now(),
        paths,
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::settings_schema,
            commands::settings_values,
            commands::set_setting,
            commands::recovery_notices,
            commands::app_info,
            commands::list_themes,
            commands::get_theme,
            commands::open_folder,
            commands::list_locales,
            commands::get_locale,
            commands::list_minecraft_versions,
            commands::detect_java,
            commands::detect_hardware,
            commands::list_instances,
            commands::create_instance,
            commands::rename_instance,
            commands::duplicate_instance,
            commands::delete_instance,
            commands::update_instance_java,
            commands::list_accounts,
            commands::begin_sign_in,
            commands::complete_sign_in,
            commands::cancel_sign_in,
            mods::list_mods,
            mods::add_mods,
            mods::set_mod_enabled,
            mods::remove_mod,
            mods::import_existing_mods,
            mods::create_mod_profile,
            mods::activate_mod_profile,
            mods::delete_mod_profile,
            mods::supported_loaders,
            mods::loader_versions,
            mods::install_loader,
            mods::remove_loader,
            mods::list_mod_presets,
            mods::install_mod_preset,
            changelog::changelog,
            perf::performance_snapshot,
            commands::set_active_account,
            commands::remove_account,
            commands::play_instance,
            commands::cancel_play,
            commands::running_game,
            commands::stop_game,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            selfheal::watch_reappearing_files(
                handle.clone(),
                selfheal_paths,
                started_at_wall,
                missing_files,
            );

            // Theme hot reload (§31): edits to a theme folder reach the
            // running window without a restart. The watcher must outlive
            // setup, so it is handed to Tauri to own.
            let reload_handle = handle.clone();
            if let Some(watch) = watcher::watch_themes(themes_dir, move || {
                if let Err(e) = reload_handle.emit("faerie://themes-changed", ()) {
                    tracing::warn!("could not notify the window of a theme change: {e}");
                }
            }) {
                // `Manager::manage` lives on the handle, not on `&mut App`.
                handle.manage(watch);
            }
            let mut rx = bus.subscribe();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    if let Err(e) = handle.emit("faerie://event", &event) {
                        tracing::warn!("failed to forward event to webview: {e}");
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start the launcher window");
}
