//! Networking for the Faeries Launcher: shared HTTP client and the download
//! manager.
//!
//! Deliberately launcher-agnostic: cancellation is a plain
//! [`tokio_util::sync::CancellationToken`] and progress is a callback, so
//! this crate depends on nothing above it and tests hermetically.

pub mod client;
pub mod download;
pub mod hash;

pub use client::build_client;
pub use download::{
    BatchOutcome, BatchProgress, DownloadConfig, DownloadRequest, Downloader, FailedDownload,
};

/// Errors produced while downloading.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("request to {url} failed: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("server answered {url} with unexpected status {status}")]
    UnexpectedStatus { url: String, status: u16 },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: std::path::PathBuf,
        expected: String,
        actual: String,
    },

    #[error("size mismatch for {path}: expected {expected} bytes, got {actual}")]
    SizeMismatch {
        path: std::path::PathBuf,
        expected: u64,
        actual: u64,
    },

    #[error("download was cancelled")]
    Cancelled,
}
