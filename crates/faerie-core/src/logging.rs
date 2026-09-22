use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use crate::DataPaths;

/// Keeps the non-blocking log writer alive; drop it only at process exit.
pub struct LogGuard {
    _file: tracing_appender::non_blocking::WorkerGuard,
}

/// Initialize logging: `logs/launcher.log` plus stderr, both at `level`
/// (one of `error|warn|info|debug|trace`; anything else falls back to `info`).
///
/// Per-domain files (network.log, authentication.log, minecraft.log) are added
/// in the phases that introduce those domains, as additional target-filtered
/// layers here. Secrets never reach this layer: sensitive values are carried
/// in [`crate::Secret`], which redacts itself in all formatting.
///
/// Safe to call more than once (later calls are ignored), which keeps tests
/// and the app from fighting over the global subscriber.
pub fn init(paths: &DataPaths, level: &str) -> LogGuard {
    let level: LevelFilter = level.parse().unwrap_or(LevelFilter::INFO);

    let file_appender = tracing_appender::rolling::never(&paths.logs_dir, "launcher.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(level);
    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(level);

    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .try_init();

    LogGuard { _file: guard }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_to_launcher_log() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::at_root(tmp.path().join("root"));
        paths.ensure_created().unwrap();

        let guard = init(&paths, "info");
        tracing::info!("hello from the test");
        drop(guard); // flush the non-blocking writer

        let body = std::fs::read_to_string(paths.logs_dir.join("launcher.log")).unwrap();
        assert!(body.contains("hello from the test"));
    }
}
