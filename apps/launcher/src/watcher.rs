//! Theme hot reload.
//!
//! Watches the user themes directory and tells the webview when something
//! under it changes, so editing a theme's JSON or artwork updates the
//! running launcher without a restart.
//!
//! Two properties matter here:
//!
//! - **Debounced.** A single save often produces several filesystem events
//!   (write, attribute change, rename of a temp file). Emitting a reload per
//!   event would re-read and re-embed every asset several times.
//! - **Idle-free.** `notify` uses the OS notification API, so nothing polls;
//!   an untouched themes directory costs nothing (§7).

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Filesystem events arriving closer together than this are treated as one
/// edit. Long enough to coalesce an editor's save, short enough to feel live.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Start watching `themes_dir`, calling `on_change` once per settled edit.
///
/// Returns the watcher, which must be kept alive for watching to continue —
/// dropping it stops the watch. Returns `None` when a watch could not be
/// established (the directory is missing, or the platform refused), in which
/// case the launcher simply works without hot reload.
pub fn watch_themes(
    themes_dir: PathBuf,
    on_change: impl Fn() + Send + 'static,
) -> Option<RecommendedWatcher> {
    if !themes_dir.is_dir() {
        return None;
    }

    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |result| {
        // Only successful events are interesting; a failed one is logged by
        // the receiver side, which has somewhere to log to.
        let _ = tx.send(result);
    }) {
        Ok(watcher) => watcher,
        Err(e) => {
            tracing::warn!("theme hot reload unavailable: {e}");
            return None;
        }
    };

    if let Err(e) = watcher.watch(&themes_dir, RecursiveMode::Recursive) {
        tracing::warn!(
            "could not watch {} for theme changes: {e}",
            themes_dir.display()
        );
        return None;
    }

    std::thread::spawn(move || {
        let mut pending: Option<Instant> = None;
        loop {
            // Wait for an event, or wake up to flush a pending one.
            let timeout = match pending {
                Some(_) => DEBOUNCE,
                None => Duration::from_secs(3600),
            };
            match rx.recv_timeout(timeout) {
                Ok(Ok(event)) if is_interesting(&event) => pending = Some(Instant::now()),
                Ok(Ok(_)) => {}
                Ok(Err(e)) => tracing::debug!("theme watch error: {e}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                // The watcher was dropped; the thread's work is done.
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }

            if let Some(at) = pending {
                if at.elapsed() >= DEBOUNCE {
                    pending = None;
                    tracing::info!("theme files changed; reloading");
                    on_change();
                }
            }
        }
    });

    tracing::info!("watching {} for theme changes", themes_dir.display());
    Some(watcher)
}

/// Ignore events that cannot change how a theme looks, so editor scratch
/// files do not trigger reloads.
fn is_interesting(event: &notify::Event) -> bool {
    use notify::EventKind;
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        // Editors write alongside the real file; those writes are noise.
        !(name.ends_with('~')
            || name.ends_with(".tmp")
            || name.ends_with(".swp")
            || name.starts_with(".#"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_directory_disables_hot_reload_rather_than_failing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        assert!(watch_themes(missing, || {}).is_none());
    }

    #[test]
    fn editor_scratch_files_are_ignored() {
        let event = |name: &str| notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Any,
            )),
            paths: vec![PathBuf::from(name)],
            attrs: Default::default(),
        };
        assert!(is_interesting(&event("colors.json")));
        assert!(!is_interesting(&event("colors.json~")));
        assert!(!is_interesting(&event("colors.json.tmp")));
        assert!(!is_interesting(&event(".#colors.json")));
    }

    #[test]
    fn reads_are_not_changes() {
        let event = notify::Event {
            kind: notify::EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("colors.json")],
            attrs: Default::default(),
        };
        assert!(!is_interesting(&event));
    }

    #[test]
    fn a_real_edit_fires_the_callback_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let tmp = tempfile::tempdir().unwrap();
        let themes = tmp.path().join("themes");
        std::fs::create_dir_all(themes.join("mytheme")).unwrap();

        let hits = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&hits);
        let watcher = watch_themes(themes.clone(), move || {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watch established");

        // Several rapid writes should coalesce into a single reload.
        for i in 0..4 {
            std::fs::write(
                themes.join("mytheme").join("colors.json"),
                format!("{{ \"background\": \"#{i}{i}{i}\" }}"),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }

        // Allow the debounce to settle.
        std::thread::sleep(DEBOUNCE * 4);
        drop(watcher);

        let count = hits.load(Ordering::SeqCst);
        assert!(count >= 1, "the edit should have fired a reload");
        assert!(count <= 2, "rapid writes should coalesce, got {count}");
    }
}
