//! Recovery from a start that could not see its own files.
//!
//! Built when starts were getting "not found" for config files that were
//! apparently on disk. The real cause turned out to be app-package file
//! virtualization (see `package_identity` in main.rs): the files existed in
//! another process's private overlay, so from this process's point of view
//! they truly were not there. The watcher stays because it is cheap and
//! still covers a file that becomes readable late for any reason: keep
//! checking for the files that were missed, and the moment one is visible,
//! reload settings and tell the window.
//!
//! Only a file whose modification time predates this process counts as
//! "reappeared" — a file the user creates after startup (signing in writes
//! `accounts.json`) is new, not recovered, and must not trigger a reload.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use faerie_core::config::ConfigStore;
use faerie_core::DataPaths;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// How long to keep looking. The observed outage lasted well over ten
/// seconds; a minute costs nothing (one `metadata` call per file per tick).
const WATCH_FOR: Duration = Duration::from_secs(90);
const TICK: Duration = Duration::from_millis(500);

pub fn watch_reappearing_files(
    handle: AppHandle,
    paths: DataPaths,
    started: SystemTime,
    missing: Vec<PathBuf>,
) {
    if missing.is_empty() {
        return;
    }
    tracing::info!(
        "self-heal: watching for {} missing file(s) to reappear: {}",
        missing.len(),
        missing
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(", ")
    );
    tauri::async_runtime::spawn(async move {
        let deadline = std::time::Instant::now() + WATCH_FOR;
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(TICK).await;
            let reappeared: Vec<&PathBuf> = missing
                .iter()
                .filter(|path| {
                    std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .map(|modified| modified < started)
                        .unwrap_or(false)
                })
                .collect();
            if reappeared.is_empty() {
                continue;
            }
            let names: Vec<String> = reappeared
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            tracing::warn!(
                "self-heal: {} reappeared {:?} after start; reloading settings",
                names.join(", "),
                started.elapsed().unwrap_or_default()
            );

            let state = handle.state::<AppState>();
            let fresh = ConfigStore::load(&paths);
            for warning in fresh.warnings() {
                tracing::warn!("config (reload): {warning}");
            }
            *state.config.write().unwrap() = fresh;

            if let Err(e) = handle.emit("faerie://config-reloaded", names) {
                tracing::warn!("self-heal: could not notify the window: {e}");
            }
            return;
        }
        tracing::info!("self-heal: watched files did not reappear; giving up");
    });
}
