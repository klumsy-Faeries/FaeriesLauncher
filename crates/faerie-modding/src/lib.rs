//! Modding: mod discovery, compatibility checking, storage, and loaders.
//!
//! Nothing above this crate branches on which mod loader is in use; that
//! knowledge lives in [`loader`] behind one trait (§12).

pub mod compat;
pub mod loader;
pub mod modrinth;
pub mod presets;
pub mod scan;
pub mod store;
pub mod vault;
pub mod version_range;

pub use compat::{check, CompatReport, Environment, Issue, Severity};
pub use scan::{LoaderKind, ModMetadata, ScannedMod};
pub use store::{ModProfile, ModStore, ProfileSet};

#[derive(Debug, thiserror::Error)]
pub enum ModError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not a readable jar: {reason}")]
    BadJar {
        path: std::path::PathBuf,
        reason: String,
    },

    #[error("mod profiles at {path} could not be parsed: {reason}")]
    BadProfiles {
        path: std::path::PathBuf,
        reason: String,
    },

    #[error(
        "{file_name} is listed in this profile but missing from the mod store \
         (hash {sha1}). Re-add the mod file to restore it."
    )]
    MissingFromStore { sha1: String, file_name: String },

    #[error("HTTP request to {url} failed: {reason}")]
    Http { url: String, reason: String },

    #[error("{loader} metadata could not be understood: {reason}")]
    BadLoaderMetadata {
        loader: &'static str,
        reason: String,
    },

    #[error(
        "installing {loader} is not yet supported. {loader} requires running its \
         official installer's processor pipeline, which this launcher does not \
         implement yet. Fabric and Quilt instances work today; {loader} mods can \
         still be scanned and checked."
    )]
    LoaderInstallUnsupported { loader: &'static str },

    #[error("no adapter exists for loader `{0}`")]
    UnknownLoader(String),

    #[error("pack vault index at {path} could not be parsed: {reason}")]
    VaultIndex {
        path: std::path::PathBuf,
        reason: String,
    },
}

impl ModError {
    pub fn io(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
