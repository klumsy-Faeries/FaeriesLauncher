//! Mojang version manifest: fetch, cache, serve offline.
//!
//! Caching rules (§7: no unnecessary network traffic; §42: graceful offline):
//! - a cache younger than [`FRESH_WINDOW`] is served without any request;
//! - otherwise we revalidate with `If-None-Match` (ETag) — a 304 costs
//!   almost nothing;
//! - if the network is down and a cache exists, the cache is served marked
//!   stale; only "no network AND no cache" is an error.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::McError;

pub const DEFAULT_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Serve a cache younger than this without touching the network.
const FRESH_WINDOW: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestPointers,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestPointers {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    /// Mojang calls this `type`; the UI, like every other IPC payload,
    /// reads camelCase `kind`. Renaming only on deserialize keeps both.
    #[serde(rename(deserialize = "type"))]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

/// Where the served manifest came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManifestSource {
    /// Downloaded (or revalidated via ETag) just now.
    Network,
    /// Cache within the freshness window — no request was made.
    CacheFresh,
    /// Network unreachable; cache served as a fallback.
    CacheStale,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestResult {
    pub manifest: VersionManifest,
    pub source: ManifestSource,
}

/// Sidecar metadata for the cached manifest body.
#[derive(Debug, Serialize, Deserialize)]
struct CacheMeta {
    etag: Option<String>,
    fetched_at_secs: u64,
}

#[derive(Clone)]
pub struct ManifestService {
    client: reqwest::Client,
    url: String,
    body_path: PathBuf,
    meta_path: PathBuf,
}

impl ManifestService {
    pub fn new(client: reqwest::Client, cache_dir: &Path, url: Option<String>) -> Self {
        Self {
            client,
            url: url.unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_string()),
            body_path: cache_dir.join("version_manifest_v2.json"),
            meta_path: cache_dir.join("version_manifest_v2.meta.json"),
        }
    }

    /// Fetch the manifest. `force` skips the freshness window (but still
    /// revalidates with the ETag, so an unchanged manifest is cheap).
    pub async fn fetch(&self, force: bool) -> Result<ManifestResult, McError> {
        let cached = self.read_cache().await;

        if !force {
            if let Some((manifest, meta)) = &cached {
                if cache_age(meta) < FRESH_WINDOW {
                    return Ok(ManifestResult {
                        manifest: manifest.clone(),
                        source: ManifestSource::CacheFresh,
                    });
                }
            }
        }

        let mut request = self.client.get(&self.url);
        if let Some((_, meta)) = &cached {
            if let Some(etag) = &meta.etag {
                request = request.header(reqwest::header::IF_NONE_MATCH, etag.clone());
            }
        }

        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => {
                // Offline path: serve what we have, clearly marked stale.
                if let Some((manifest, _)) = cached {
                    tracing::warn!(target: "faerie_net", "manifest fetch failed, serving cache: {e}");
                    return Ok(ManifestResult {
                        manifest,
                        source: ManifestSource::CacheStale,
                    });
                }
                return Err(McError::ManifestUnavailable {
                    url: self.url.clone(),
                    reason: e.to_string(),
                });
            }
        };

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some((manifest, meta)) = cached {
                // Refresh the timestamp so the freshness window restarts.
                self.write_meta(&CacheMeta {
                    etag: meta.etag,
                    fetched_at_secs: now_secs(),
                })
                .await;
                return Ok(ManifestResult {
                    manifest,
                    source: ManifestSource::Network,
                });
            }
            // 304 without a cache should not happen; fall through to error.
            return Err(McError::ManifestUnavailable {
                url: self.url.clone(),
                reason: "server said Not Modified but no cache exists".into(),
            });
        }

        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = response
            .text()
            .await
            .map_err(|e| McError::ManifestUnavailable {
                url: self.url.clone(),
                reason: e.to_string(),
            })?;
        let manifest: VersionManifest =
            serde_json::from_str(&body).map_err(|e| McError::ManifestInvalid(e.to_string()))?;

        self.write_cache(&body, etag).await;
        Ok(ManifestResult {
            manifest,
            source: ManifestSource::Network,
        })
    }

    async fn read_cache(&self) -> Option<(VersionManifest, CacheMeta)> {
        let body = tokio::fs::read_to_string(&self.body_path).await.ok()?;
        let manifest = serde_json::from_str(&body).ok()?;
        let meta = match tokio::fs::read_to_string(&self.meta_path).await {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or(CacheMeta {
                etag: None,
                fetched_at_secs: 0,
            }),
            Err(_) => CacheMeta {
                etag: None,
                fetched_at_secs: 0,
            },
        };
        Some((manifest, meta))
    }

    async fn write_cache(&self, body: &str, etag: Option<String>) {
        if let Some(parent) = self.body_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        if let Err(e) = tokio::fs::write(&self.body_path, body).await {
            tracing::warn!(target: "faerie_net", "could not cache manifest: {e}");
        }
        self.write_meta(&CacheMeta {
            etag,
            fetched_at_secs: now_secs(),
        })
        .await;
    }

    async fn write_meta(&self, meta: &CacheMeta) {
        let raw = serde_json::to_string(meta).expect("meta serializes");
        if let Err(e) = tokio::fs::write(&self.meta_path, raw).await {
            tracing::warn!(target: "faerie_net", "could not write manifest meta: {e}");
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_age(meta: &CacheMeta) -> Duration {
    Duration::from_secs(now_secs().saturating_sub(meta.fetched_at_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_v2_manifest_shape() {
        let raw = r#"{
            "latest": { "release": "26.2", "snapshot": "26w35a" },
            "versions": [
                {
                    "id": "26.2",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/abc/26.2.json",
                    "time": "2026-07-20T10:00:00+00:00",
                    "releaseTime": "2026-07-20T10:00:00+00:00",
                    "sha1": "abc123",
                    "complianceLevel": 1
                },
                {
                    "id": "1.8.9",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/def/1.8.9.json",
                    "releaseTime": "2015-12-08T00:00:00+00:00",
                    "sha1": "def456"
                }
            ]
        }"#;
        let manifest: VersionManifest = serde_json::from_str(raw).unwrap();
        assert_eq!(manifest.latest.release, "26.2");
        assert_eq!(manifest.versions.len(), 2);
        assert_eq!(manifest.versions[0].kind, "release");
        // The webview filters on `kind`; a `type` leak here empties every
        // version list in the UI while the mock keeps working.
        let wire = serde_json::to_value(&manifest.versions[0]).unwrap();
        assert_eq!(wire["kind"], "release");
        assert!(wire.get("type").is_none());
        assert_eq!(manifest.versions[1].id, "1.8.9");
        assert_eq!(
            manifest.versions[1].release_time,
            "2015-12-08T00:00:00+00:00"
        );
    }
}
