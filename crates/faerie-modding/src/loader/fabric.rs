//! Fabric adapter.
//!
//! Fabric's meta service publishes both the loader list and a ready-made
//! *profile JSON* per (game, loader) pair. That profile uses `inheritsFrom`
//! to layer onto vanilla, which the Phase 3 version pipeline already
//! resolves — so installing Fabric is: fetch JSON, write it to the versions
//! directory, and let the normal installer take it from there.

use std::path::Path;

use serde::Deserialize;

use super::{InstalledLoader, LoaderAdapter, LoaderVersion};
use crate::scan::LoaderKind;
use crate::ModError;

pub const DEFAULT_META_BASE: &str = "https://meta.fabricmc.net/v2";

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
    #[serde(default)]
    stable: bool,
}

pub struct FabricAdapter {
    client: reqwest::Client,
    base: String,
}

impl FabricAdapter {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base: DEFAULT_META_BASE.to_string(),
        }
    }

    /// Point the adapter at a different meta service (used by tests).
    pub fn with_base(client: reqwest::Client, base: impl Into<String>) -> Self {
        Self {
            client,
            base: base.into(),
        }
    }

    pub(crate) fn version_id(minecraft_version: &str, loader_version: &str) -> String {
        format!("fabric-loader-{loader_version}-{minecraft_version}")
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
impl LoaderAdapter for FabricAdapter {
    fn kind(&self) -> LoaderKind {
        LoaderKind::Fabric
    }

    async fn versions_for(&self, minecraft_version: &str) -> Result<Vec<LoaderVersion>, ModError> {
        let url = format!("{}/versions/loader/{minecraft_version}", self.base);
        let body = self.get_text(&url).await?;
        let entries: Vec<LoaderEntry> =
            serde_json::from_str(&body).map_err(|e| ModError::BadLoaderMetadata {
                loader: "fabric",
                reason: e.to_string(),
            })?;
        Ok(entries
            .into_iter()
            .map(|e| LoaderVersion {
                version_id: Self::version_id(minecraft_version, &e.loader.version),
                version: e.loader.version,
                stable: e.loader.stable,
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
        // Sanity-check before writing: a truncated or error body must not be
        // cached as if it were a valid version.
        let parsed: serde_json::Value =
            serde_json::from_str(&profile).map_err(|e| ModError::BadLoaderMetadata {
                loader: "fabric",
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
            kind: LoaderKind::Fabric,
            loader_version: loader_version.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_id_matches_fabrics_own_naming() {
        assert_eq!(
            FabricAdapter::version_id("1.20.4", "0.15.7"),
            "fabric-loader-0.15.7-1.20.4"
        );
    }

    #[test]
    fn launch_id_is_the_loader_version_for_installable_loaders() {
        use crate::loader::installed_version_id;
        use crate::scan::LoaderKind;
        assert_eq!(
            installed_version_id(LoaderKind::Fabric, "26.2", "0.19.5").as_deref(),
            Some("fabric-loader-0.19.5-26.2")
        );
        assert_eq!(
            installed_version_id(LoaderKind::Quilt, "26.2", "0.30.0").as_deref(),
            Some("quilt-loader-0.30.0-26.2")
        );
        assert!(installed_version_id(LoaderKind::Forge, "26.2", "1").is_none());
    }
}
