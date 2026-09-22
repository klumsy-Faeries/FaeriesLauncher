//! Download manager (§17 of the spec): parallel, resumable, verified.
//!
//! Behavior per file:
//! - if the destination already exists and matches its expected hash/size,
//!   it is skipped (never re-downloaded);
//! - data streams into `<dest>.part`; an interrupted transfer resumes from
//!   the partial file's length with an HTTP `Range` request;
//! - completed files are SHA-1 verified before an atomic rename into place;
//! - transient failures retry with exponential backoff; permanent failures
//!   (404, disk errors) do not;
//! - cancellation stops promptly and leaves `.part` files for later resume.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::hash::sha1_of_file;
use crate::NetError;

/// One file to download.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub dest: PathBuf,
    /// Expected SHA-1 (lowercase hex). Verified after download; also used to
    /// validate an already-present destination file.
    pub sha1: Option<String>,
    /// Expected size in bytes; used for progress totals and validation when
    /// no hash is available.
    pub size: Option<u64>,
    /// Human-readable name for progress/error messages.
    pub label: String,
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadConfig {
    /// Concurrent transfers (§17: configurable, be polite to servers).
    pub concurrency: usize,
    /// Retry attempts after the first failure of a file.
    pub retries: u32,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            retries: 3,
        }
    }
}

/// Progress snapshot handed to the batch progress callback (~4x/second).
#[derive(Debug, Clone, Copy)]
pub struct BatchProgress {
    pub files_done: usize,
    pub files_total: usize,
    /// Bytes accounted for, including full credit for skipped files.
    pub bytes_done: u64,
    /// Known only when every request declared a size.
    pub bytes_total: Option<u64>,
    pub bytes_per_sec: u64,
}

impl BatchProgress {
    /// Overall fraction in `0.0..=1.0` (byte-based when sizes are known).
    pub fn fraction(&self) -> f32 {
        match self.bytes_total {
            Some(total) if total > 0 => (self.bytes_done as f32 / total as f32).clamp(0.0, 1.0),
            _ if self.files_total > 0 => self.files_done as f32 / self.files_total as f32,
            _ => 1.0,
        }
    }
}

#[derive(Debug)]
pub struct FailedDownload {
    pub url: String,
    pub label: String,
    pub error: NetError,
}

/// Result of a whole batch.
#[derive(Debug, Default)]
pub struct BatchOutcome {
    pub completed: usize,
    pub skipped: usize,
    pub failed: Vec<FailedDownload>,
    pub cancelled: bool,
    /// Bytes actually transferred over the network this run.
    pub bytes_network: u64,
}

impl BatchOutcome {
    pub fn is_success(&self) -> bool {
        !self.cancelled && self.failed.is_empty()
    }
}

struct Counters {
    /// Progress bytes (downloads + credit for skipped/valid files).
    bytes_done: AtomicU64,
    /// Bytes that actually crossed the network.
    bytes_network: AtomicU64,
}

enum FileOutcome {
    Downloaded,
    Skipped,
}

pub struct Downloader {
    client: reqwest::Client,
    config: DownloadConfig,
}

impl Downloader {
    pub fn new(client: reqwest::Client, config: DownloadConfig) -> Self {
        Self { client, config }
    }

    /// Download a batch. Progress is reported through `on_progress` from a
    /// single driver loop (individual transfers still run concurrently).
    pub async fn fetch_batch(
        &self,
        requests: Vec<DownloadRequest>,
        cancel: &CancellationToken,
        mut on_progress: impl FnMut(&BatchProgress),
    ) -> BatchOutcome {
        let files_total = requests.len();
        let bytes_total = requests
            .iter()
            .map(|r| r.size)
            .collect::<Option<Vec<_>>>()
            .map(|sizes| sizes.iter().sum());
        let counters = Arc::new(Counters {
            bytes_done: AtomicU64::new(0),
            bytes_network: AtomicU64::new(0),
        });

        let mut jobs = futures_util::stream::iter(requests.into_iter().map(|request| {
            let counters = Arc::clone(&counters);
            let cancel = cancel.clone();
            let client = self.client.clone();
            let config = self.config;
            async move { fetch_one(client, config, request, counters, cancel).await }
        }))
        .buffer_unordered(self.config.concurrency.max(1));

        let mut outcome = BatchOutcome::default();
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut files_done = 0usize;
        let mut last_bytes = 0u64;
        let mut last_tick = tokio::time::Instant::now();

        let snapshot = |files_done: usize, counters: &Counters, speed: u64| BatchProgress {
            files_done,
            files_total,
            bytes_done: counters.bytes_done.load(Ordering::Relaxed),
            bytes_total,
            bytes_per_sec: speed,
        };

        loop {
            tokio::select! {
                item = jobs.next() => match item {
                    Some(Ok(FileOutcome::Downloaded)) => {
                        files_done += 1;
                        outcome.completed += 1;
                    }
                    Some(Ok(FileOutcome::Skipped)) => {
                        files_done += 1;
                        outcome.skipped += 1;
                    }
                    Some(Err(failed)) => {
                        if matches!(failed.error, NetError::Cancelled) {
                            outcome.cancelled = true;
                            break;
                        }
                        files_done += 1;
                        tracing::warn!(target: "faerie_net", "download failed: {} ({})", failed.label, failed.error);
                        outcome.failed.push(*failed);
                    }
                    None => break,
                },
                _ = ticker.tick() => {
                    let now = tokio::time::Instant::now();
                    let bytes = counters.bytes_network.load(Ordering::Relaxed);
                    let dt = now.duration_since(last_tick).as_secs_f64().max(0.001);
                    let speed = ((bytes.saturating_sub(last_bytes)) as f64 / dt) as u64;
                    last_bytes = bytes;
                    last_tick = now;
                    on_progress(&snapshot(files_done, &counters, speed));
                }
                _ = cancel.cancelled() => {
                    outcome.cancelled = true;
                    break;
                }
            }
        }

        outcome.bytes_network = counters.bytes_network.load(Ordering::Relaxed);
        on_progress(&snapshot(files_done, &counters, 0));
        outcome
    }
}

fn part_path(dest: &std::path::Path) -> PathBuf {
    let mut os = dest.as_os_str().to_owned();
    os.push(".part");
    PathBuf::from(os)
}

/// Errors worth retrying: transient network trouble and corrupt transfers.
/// Client errors (404/403) and local disk errors are permanent.
fn is_retryable(error: &NetError) -> bool {
    match error {
        NetError::Http { .. } => true,
        NetError::UnexpectedStatus { status, .. } => *status >= 500,
        NetError::HashMismatch { .. } | NetError::SizeMismatch { .. } => true,
        NetError::Io { .. } | NetError::Cancelled => false,
    }
}

async fn fetch_one(
    client: reqwest::Client,
    config: DownloadConfig,
    request: DownloadRequest,
    counters: Arc<Counters>,
    cancel: CancellationToken,
) -> Result<FileOutcome, Box<FailedDownload>> {
    // Boxed error: the Err path is rare and FailedDownload is large.
    let fail = |error: NetError| {
        Box::new(FailedDownload {
            url: request.url.clone(),
            label: request.label.clone(),
            error,
        })
    };

    // Already present and valid → done without touching the network.
    match existing_is_valid(&request).await {
        Ok(Some(len)) => {
            counters.bytes_done.fetch_add(len, Ordering::Relaxed);
            return Ok(FileOutcome::Skipped);
        }
        Ok(None) => {}
        Err(e) => return Err(fail(e)),
    }

    if let Some(parent) = request.dest.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return Err(fail(NetError::Io {
                path: parent.to_path_buf(),
                source: e,
            }));
        }
    }

    let mut attempt = 0u32;
    loop {
        if cancel.is_cancelled() {
            return Err(fail(NetError::Cancelled));
        }
        match attempt_download(&client, &request, &counters, &cancel).await {
            Ok(()) => return Ok(FileOutcome::Downloaded),
            Err(e) if is_retryable(&e) && attempt < config.retries => {
                attempt += 1;
                tracing::debug!(target: "faerie_net", "retrying {} (attempt {attempt}): {e}", request.label);
                let backoff = Duration::from_millis(500u64.saturating_mul(1 << attempt.min(4)));
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = cancel.cancelled() => return Err(fail(NetError::Cancelled)),
                }
            }
            Err(e) => return Err(fail(e)),
        }
    }
}

/// `Ok(Some(len))` when the destination exists and passes validation.
async fn existing_is_valid(request: &DownloadRequest) -> Result<Option<u64>, NetError> {
    let Ok(meta) = tokio::fs::metadata(&request.dest).await else {
        return Ok(None);
    };
    let len = meta.len();
    match &request.sha1 {
        Some(expected) => {
            let expected = expected.clone();
            let path = request.dest.clone();
            let actual = tokio::task::spawn_blocking(move || sha1_of_file(&path))
                .await
                .expect("hash task never panics")
                .map_err(|e| NetError::Io {
                    path: request.dest.clone(),
                    source: e,
                })?;
            Ok((actual == expected).then_some(len))
        }
        None => Ok((request.size.is_none() || request.size == Some(len)).then_some(len)),
    }
}

async fn attempt_download(
    client: &reqwest::Client,
    request: &DownloadRequest,
    counters: &Counters,
    cancel: &CancellationToken,
) -> Result<(), NetError> {
    let part = part_path(&request.dest);
    let resume_from = tokio::fs::metadata(&part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let io_err = |path: &PathBuf| {
        let path = path.clone();
        move |source: std::io::Error| NetError::Io {
            path: path.clone(),
            source,
        }
    };
    let http_err = |source: reqwest::Error| NetError::Http {
        url: request.url.clone(),
        source,
    };

    let mut req = client.get(&request.url);
    if resume_from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let mut response = req.send().await.map_err(http_err)?;

    match response.status().as_u16() {
        // Server honored the range: append to the partial file.
        206 => {
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&part)
                .await
                .map_err(io_err(&part))?;
            stream_body(
                &mut response,
                &mut file,
                counters,
                cancel,
                http_err,
                io_err(&part),
            )
            .await?;
            file.flush().await.map_err(io_err(&part))?;
        }
        // Full body (fresh download, or the server ignored our range).
        200 => {
            let mut file = tokio::fs::File::create(&part)
                .await
                .map_err(io_err(&part))?;
            stream_body(
                &mut response,
                &mut file,
                counters,
                cancel,
                http_err,
                io_err(&part),
            )
            .await?;
            file.flush().await.map_err(io_err(&part))?;
        }
        // Our partial file is already at (or past) the full length; verify it.
        416 => {}
        status => {
            return Err(NetError::UnexpectedStatus {
                url: request.url.clone(),
                status,
            })
        }
    }

    // Verify before the file is allowed to exist under its real name.
    let final_len = tokio::fs::metadata(&part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if let Some(expected) = &request.sha1 {
        let path = part.clone();
        let actual = tokio::task::spawn_blocking(move || sha1_of_file(&path))
            .await
            .expect("hash task never panics")
            .map_err(io_err(&part))?;
        if &actual != expected {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(NetError::HashMismatch {
                path: request.dest.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    } else if let Some(size) = request.size {
        if final_len != size {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(NetError::SizeMismatch {
                path: request.dest.clone(),
                expected: size,
                actual: final_len,
            });
        }
    }

    tokio::fs::rename(&part, &request.dest)
        .await
        .map_err(io_err(&request.dest))?;
    Ok(())
}

async fn stream_body(
    response: &mut reqwest::Response,
    file: &mut tokio::fs::File,
    counters: &Counters,
    cancel: &CancellationToken,
    http_err: impl Fn(reqwest::Error) -> NetError,
    io_err: impl Fn(std::io::Error) -> NetError,
) -> Result<(), NetError> {
    while let Some(chunk) = response.chunk().await.map_err(&http_err)? {
        if cancel.is_cancelled() {
            let _ = file.flush().await; // keep bytes for resume
            return Err(NetError::Cancelled);
        }
        file.write_all(&chunk).await.map_err(&io_err)?;
        let n = chunk.len() as u64;
        counters.bytes_done.fetch_add(n, Ordering::Relaxed);
        counters.bytes_network.fetch_add(n, Ordering::Relaxed);
    }
    Ok(())
}
