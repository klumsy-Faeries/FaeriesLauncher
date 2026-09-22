//! Quilt adapter.
//!
//! Quilt forked Fabric's meta service, so the endpoints and payload shapes
//! are the same; only the base URL and the version-id prefix differ. The
//! duplication is deliberate: Quilt is free to diverge, and when it does,
//! only this file changes.

use std::path::Path;

use serde::Deserialize;

use super::{InstalledLoader, LoaderAdapter, LoaderVersion};
use crate::scan::LoaderKind;
use crate::ModError;

pub const DEFAULT_META_BASE: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
}

pub struct QuiltAdapter {
    client: reqwest::Client,
    base: String,
}

impl QuiltAdapter {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base: DEFAULT_META_BASE.to_string(),
        }
    }

    pub fn with_base(client: reqwest::Client, base: impl Into<String>) -> Self {
        Self {
            client,
            base: base.into(),
        }
    }

    pub(crate) fn version_id(minecraft_version: &str, loader_version: &str) -> String {
        format!("quilt-loader-{loader_version}-{minecraft_version}")
    }

    async fn get_text(&self, url: &str) -> Result<String, ModError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ModError::Http {
                url: url.to_string(),
                reason: e.to_string(),
            })?;
        if !response.status().is_success() {
            return Err(ModError::Http {
                url: url.to_string(),
                reason: format!("status {}", response.status()),
            });
        }
        response.text().await.map_err(|e| ModError::Http {
            url: url.to_string(),
            reason: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl LoaderAdapter for QuiltAdapter {
    fn kind(&self) -> LoaderKind {
        LoaderKind::Quilt
    }

    async fn versions_for(&self, minecraft_version: &str) -> Result<Vec<LoaderVersion>, ModError> {
        let url = format!("{}/versions/loader/{minecraft_version}", self.base);
        let body = self.get_text(&url).await?;
        let entries: Vec<LoaderEntry> =
            serde_json::from_str(&body).map_err(|e| ModError::BadLoaderMetadata {
                loader: "quilt",
                reason: e.to_string(),
            })?;
        Ok(entries
            .into_iter()
            .map(|e| LoaderVersion {
                // Quilt marks pre-releases in the version string itself.
                stable: !e.loader.version.contains("beta") && !e.loader.version.contains("pre"),
                version_id: Self::version_id(minecraft_version, &e.loader.version),
                version: e.loader.version,
            })
            .collect())
    }

    async fn install(
        &self,
        minecraft_version: &str,
        loader_version: &str,
        versions_dir: &Path,
    ) -> Result<InstalledLoader, ModError> {
        let url = format!(
            "{}/versions/loader/{minecraft_version}/{loader_version}/profile/json",
            self.base
        );
        let profile = self.get_text(&url).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&profile).map_err(|e| ModError::BadLoaderMetadata {
                loader: "quilt",
                reason: e.to_string(),
            })?;
        let version_id = parsed
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| Self::version_id(minecraft_version, loader_version));

        let dir = versions_dir.join(&version_id);
        std::fs::create_dir_all(&dir).map_err(|e| ModError::io(&dir, e))?;
        let path = dir.join(format!("{version_id}.json"));
        std::fs::write(&path, &profile).map_err(|e| ModError::io(&path, e))?;

        Ok(InstalledLoader {
            version_id,
            kind: LoaderKind::Quilt,
            loader_version: loader_version.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_id_matches_quilts_naming() {
        assert_eq!(
            QuiltAdapter::version_id("1.20.4", "0.23.1"),
            "quilt-loader-0.23.1-1.20.4"
        );
    }
}
