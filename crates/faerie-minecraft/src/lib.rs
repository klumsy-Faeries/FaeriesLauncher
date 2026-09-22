//! Minecraft-facing services: version metadata, Java runtimes, hardware,
//! installation, and launching.
//!
//! Nothing here hard-codes a Minecraft version — behavior comes from Mojang
//! metadata parsed generically (§2, §48).

pub mod hardware;
pub mod install;
pub mod java;
pub mod java_runtime;
pub mod launch;
pub mod manifest;
pub mod paths;
pub mod process;
pub mod version;

pub use paths::GamePaths;

/// Errors from Minecraft metadata services, installation, and launching.
#[derive(Debug, thiserror::Error)]
pub enum McError {
    #[error("could not reach {url} and no cached copy exists: {reason}")]
    ManifestUnavailable { url: String, reason: String },

    #[error("the version manifest could not be parsed: {0}")]
    ManifestInvalid(String),

    #[error("version `{0}` was not found in the manifest")]
    UnknownVersion(String),

    #[error("version metadata for `{id}` could not be parsed: {reason}")]
    VersionInvalid { id: String, reason: String },

    #[error("this version inherits from `{0}`, whose metadata is missing")]
    MissingParent(String),

    #[error("network error: {0}")]
    Network(#[from] faerie_net::NetError),

    #[error("{count} file(s) failed to download; first: {first}")]
    DownloadsFailed { count: usize, first: String },

    #[error("the install was cancelled")]
    Cancelled,

    #[error("no installed Java satisfies the required major version {required}. \
             Detected: {detected}. Install a matching runtime or set one in the instance's Java settings.")]
    NoSuitableJava { required: u32, detected: String },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("HTTP request to {url} failed: {reason}")]
    Http { url: String, reason: String },
}

impl McError {
    pub fn io(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
