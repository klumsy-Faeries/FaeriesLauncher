use std::path::PathBuf;

use crate::config::ConfigError;

/// Errors produced by `faerie-core`.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("no OS configuration directory could be determined; set the FAERIE_DATA_DIR environment variable")]
    NoConfigDir,

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Config(#[from] ConfigError),
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
