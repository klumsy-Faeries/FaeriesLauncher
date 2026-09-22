//! Version installation: resolve metadata, then download and verify every
//! file a version needs (client jar, libraries, natives, assets, logging
//! config) through the Phase-2 download manager, and extract natives.

use std::collections::BTreeMap;
use std::path::PathBuf;

use faerie_net::{DownloadConfig, DownloadRequest, Downloader};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::manifest::VersionManifest;
use crate::version::rules::{FeatureSet, RuleContext};
use crate::version::{self, VersionDetail};
use crate::{GamePaths, McError};

const RESOURCES_BASE: &str = "https://resources.download.minecraft.net";

/// Progress for the whole install (download phase). `phase` names the current
/// stage for display; the byte/file counters come straight from the batch.
#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub phase: &'static str,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
    pub bytes_per_sec: u64,
}

/// A fully-resolved, installed version ready to launch.
#[derive(Debug, Clone)]
pub struct InstalledVersion {
    /// The requested version id.
    pub id: String,
    /// The id of the version whose `downloads.client` supplies the jar (the
    /// vanilla root for modded versions).
    pub client_jar_id: String,
    pub detail: VersionDetail,
    /// Required Java major version, if the metadata declares one.
    pub required_java_major: Option<u32>,
    /// Mojang runtime component this version asks for (e.g.
    /// `java-runtime-epsilon`). Authoritative over any major-version guess.
    pub required_java_component: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: BTreeMap<String, AssetObject>,
    #[serde(default)]
    map_to_resources: bool,
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub struct Installer {
    client: reqwest::Client,
    downloader: Downloader,
    paths: GamePaths,
}

impl Installer {
    pub fn new(client: reqwest::Client, paths: GamePaths, downloads: DownloadConfig) -> Self {
        Self {
            downloader: Downloader::new(client.clone(), downloads),
            client,
            paths,
        }
    }

    /// Resolve, download, verify, and prepare everything `id` needs.
    pub async fn install(
        &self,
        id: &str,
        manifest: &VersionManifest,
        cancel: &CancellationToken,
        mut on_progress: impl FnMut(InstallProgress),
    ) -> Result<InstalledVersion, McError> {
        self.paths
            .ensure_base_dirs()
            .map_err(|e| McError::io(&self.paths.root, e))?;

        let (detail, client_jar_id) = self.resolve(id, manifest).await?;
        let ctx = RuleContext::current(FeatureSet::default());

        // Phase 1: the asset index must exist before we can enumerate objects.
        let asset_index = self.fetch_asset_index(&detail, cancel).await?;

        // Phase 2: one unified batch — client jar, libraries, natives,
        // logging config, and every asset object.
        let mut batch = Vec::new();
        let mut native_jars = Vec::new();
        self.plan_client_jar(&detail, &client_jar_id, &mut batch);
        self.plan_libraries(&detail, &ctx, &mut batch, &mut native_jars);
        self.plan_logging(&detail, &mut batch);
        self.plan_assets(&asset_index, &mut batch);

        let files_total = batch.len();
        let outcome = self
            .downloader
            .fetch_batch(batch, cancel, |p| {
                on_progress(InstallProgress {
                    phase: "downloading",
                    files_done: p.files_done,
                    files_total,
                    bytes_done: p.bytes_done,
                    bytes_total: p.bytes_total,
                    bytes_per_sec: p.bytes_per_sec,
                });
            })
            .await;

        if outcome.cancelled {
            return Err(McError::Cancelled);
        }
        if let Some(first) = outcome.failed.first() {
            return Err(McError::DownloadsFailed {
                count: outcome.failed.len(),
                first: format!("{}: {}", first.label, first.error),
            });
        }

        // Post-download: extract natives and materialize virtual assets.
        on_progress(InstallProgress {
            phase: "extracting natives",
            files_done: files_total,
            files_total,
            bytes_done: outcome.bytes_network,
            bytes_total: None,
            bytes_per_sec: 0,
        });
        self.extract_natives(&detail, native_jars).await?;
        if asset_index.map_to_resources || asset_index.is_virtual {
            self.materialize_virtual_assets(&detail, &asset_index)
                .await?;
        }

        let required_java_major = detail.java_version.as_ref().map(|j| j.major_version);
        let required_java_component = detail
            .java_version
            .as_ref()
            .and_then(|j| j.component.clone());
        Ok(InstalledVersion {
            id: id.to_string(),
            client_jar_id,
            detail,
            required_java_major,
            required_java_component,
        })
    }

    /// Resolve a version and its inheritance chain, returning the merged
    /// metadata and the id of the root that owns the client jar.
    async fn resolve(
        &self,
        id: &str,
        manifest: &VersionManifest,
    ) -> Result<(VersionDetail, String), McError> {
        let detail = self.load_version(id, manifest).await?;
        match detail.inherits_from.clone() {
            None => Ok((detail, id.to_string())),
            Some(parent_id) => {
                let (parent, root_id) = Box::pin(self.resolve(&parent_id, manifest)).await?;
                Ok((detail.resolve_onto(parent), root_id))
            }
        }
    }

    /// Load one version's JSON: from disk if already installed, otherwise from
    /// the manifest entry (verified and cached).
    async fn load_version(
        &self,
        id: &str,
        manifest: &VersionManifest,
    ) -> Result<VersionDetail, McError> {
        let path = self.paths.version_json(id);
        if let Ok(raw) = tokio::fs::read_to_string(&path).await {
            if let Ok(detail) = VersionDetail::from_json(&raw) {
                return Ok(detail);
            }
            // Corrupt cache: fall through and re-fetch.
        }

        let summary = manifest
            .versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| McError::UnknownVersion(id.to_string()))?;
        let raw = self.fetch_text(&summary.url).await?;
        let detail = VersionDetail::from_json(&raw).map_err(|e| McError::VersionInvalid {
            id: id.to_string(),
            reason: e.to_string(),
        })?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| McError::io(parent, e))?;
        }
        tokio::fs::write(&path, &raw)
            .await
            .map_err(|e| McError::io(&path, e))?;
        Ok(detail)
    }

    async fn fetch_asset_index(
        &self,
        detail: &VersionDetail,
        cancel: &CancellationToken,
    ) -> Result<AssetIndex, McError> {
        let index_ref = detail
            .asset_index
            .as_ref()
            .ok_or_else(|| McError::VersionInvalid {
                id: detail.id.clone(),
                reason: "no assetIndex in version metadata".into(),
            })?;
        let dest = self.paths.asset_index_json(&index_ref.id);
        let request = DownloadRequest {
            url: index_ref.url.clone(),
            dest: dest.clone(),
            sha1: index_ref.sha1.clone(),
            size: index_ref.size,
            label: format!("asset index {}", index_ref.id),
        };
        let outcome = self
            .downloader
            .fetch_batch(vec![request], cancel, |_| {})
            .await;
        if outcome.cancelled {
            return Err(McError::Cancelled);
        }
        if let Some(first) = outcome.failed.first() {
            return Err(McError::DownloadsFailed {
                count: 1,
                first: format!("{}: {}", first.label, first.error),
            });
        }
        let raw = tokio::fs::read_to_string(&dest)
            .await
            .map_err(|e| McError::io(&dest, e))?;
        serde_json::from_str(&raw).map_err(|e| McError::VersionInvalid {
            id: index_ref.id.clone(),
            reason: format!("asset index: {e}"),
        })
    }

    fn plan_client_jar(
        &self,
        detail: &VersionDetail,
        client_jar_id: &str,
        batch: &mut Vec<DownloadRequest>,
    ) {
        if let Some(client) = detail.downloads.as_ref().and_then(|d| d.client.as_ref()) {
            batch.push(DownloadRequest {
                url: client.url.clone(),
                dest: self.paths.version_jar(client_jar_id),
                sha1: client.sha1.clone(),
                size: client.size,
                label: format!("client {client_jar_id}"),
            });
        }
    }

    fn plan_libraries(
        &self,
        detail: &VersionDetail,
        ctx: &RuleContext,
        batch: &mut Vec<DownloadRequest>,
        native_jars: &mut Vec<PathBuf>,
    ) {
        for lib in &detail.libraries {
            if !version::rules::allowed(&lib.rules, ctx) {
                continue;
            }

            // Main artifact (may be absent for pure-native library entries).
            if let Some(request) = self.library_artifact_request(lib) {
                batch.push(request);
            }

            // Native classifier for this OS, if declared.
            if let Some(natives) = &lib.natives {
                if let Some(classifier) = version::native_classifier(natives, ctx.os_name) {
                    if let Some(artifact) = lib
                        .downloads
                        .as_ref()
                        .and_then(|d| d.classifiers.as_ref())
                        .and_then(|c| c.get(&classifier))
                    {
                        if let Some(rel) = artifact
                            .path
                            .clone()
                            .or_else(|| version::maven_path(&format!("{}:{classifier}", lib.name)))
                        {
                            let dest = self.paths.library_path(&rel);
                            native_jars.push(dest.clone());
                            batch.push(DownloadRequest {
                                url: artifact.url.clone(),
                                dest,
                                sha1: artifact.sha1.clone(),
                                size: artifact.size,
                                label: format!("native {}", lib.name),
                            });
                        }
                    }
                }
            }
        }
    }

    fn library_artifact_request(&self, lib: &version::Library) -> Option<DownloadRequest> {
        if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let rel = artifact
                .path
                .clone()
                .or_else(|| version::maven_path(&lib.name))?;
            return Some(DownloadRequest {
                url: artifact.url.clone(),
                dest: self.paths.library_path(&rel),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
                label: lib.name.clone(),
            });
        }
        // No explicit download: synthesize from maven coordinates + base url.
        let rel = version::maven_path(&lib.name)?;
        let base = lib.url.clone()?;
        let url = format!("{}/{rel}", base.trim_end_matches('/'));
        Some(DownloadRequest {
            url,
            dest: self.paths.library_path(&rel),
            sha1: None,
            size: None,
            label: lib.name.clone(),
        })
    }

    fn plan_logging(&self, detail: &VersionDetail, batch: &mut Vec<DownloadRequest>) {
        if let Some(client) = detail.logging.as_ref().and_then(|l| l.client.as_ref()) {
            batch.push(DownloadRequest {
                url: client.file.url.clone(),
                dest: self.paths.logging_config(&client.file.id),
                sha1: client.file.sha1.clone(),
                size: client.file.size,
                label: format!("logging config {}", client.file.id),
            });
        }
    }

    fn plan_assets(&self, index: &AssetIndex, batch: &mut Vec<DownloadRequest>) {
        for (name, object) in &index.objects {
            let hash = &object.hash;
            batch.push(DownloadRequest {
                url: format!("{RESOURCES_BASE}/{}/{hash}", &hash[..2]),
                dest: self.paths.asset_object(hash),
                sha1: Some(hash.clone()),
                size: Some(object.size),
                label: format!("asset {name}"),
            });
        }
    }

    async fn extract_natives(
        &self,
        detail: &VersionDetail,
        native_jars: Vec<PathBuf>,
    ) -> Result<(), McError> {
        if native_jars.is_empty() {
            return Ok(());
        }
        let target = self.paths.natives_dir(&detail.id);
        tokio::fs::create_dir_all(&target)
            .await
            .map_err(|e| McError::io(&target, e))?;

        // Gather per-library exclusion globs.
        let excludes: Vec<Vec<String>> = detail
            .libraries
            .iter()
            .filter_map(|l| l.extract.as_ref().map(|e| e.exclude.clone()))
            .collect();
        let flat_excludes: Vec<String> = excludes.into_iter().flatten().collect();

        let target_clone = target.clone();
        tokio::task::spawn_blocking(move || {
            extract_native_jars(&native_jars, &target_clone, &flat_excludes)
        })
        .await
        .expect("native extraction task never panics")
        .map_err(|e| McError::io(&target, e))
    }

    async fn materialize_virtual_assets(
        &self,
        detail: &VersionDetail,
        index: &AssetIndex,
    ) -> Result<(), McError> {
        let index_id = detail
            .assets
            .clone()
            .or_else(|| detail.asset_index.as_ref().map(|a| a.id.clone()))
            .unwrap_or_else(|| "legacy".into());
        for (logical, object) in &index.objects {
            let source = self.paths.asset_object(&object.hash);
            let dest = self.paths.virtual_asset(&index_id, logical);
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| McError::io(parent, e))?;
            }
            // Copy only if missing; virtual trees can be large.
            if tokio::fs::metadata(&dest).await.is_err() {
                tokio::fs::copy(&source, &dest)
                    .await
                    .map_err(|e| McError::io(&dest, e))?;
            }
        }
        Ok(())
    }

    async fn fetch_text(&self, url: &str) -> Result<String, McError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| McError::Http {
                url: url.to_string(),
                reason: e.to_string(),
            })?;
        if !response.status().is_success() {
            return Err(McError::Http {
                url: url.to_string(),
                reason: format!("status {}", response.status()),
            });
        }
        response.text().await.map_err(|e| McError::Http {
            url: url.to_string(),
            reason: e.to_string(),
        })
    }
}

/// Extract native jars into `target`, skipping `META-INF` and any path
/// matching an exclusion prefix. Blocking; call via `spawn_blocking`.
fn extract_native_jars(
    jars: &[PathBuf],
    target: &std::path::Path,
    excludes: &[String],
) -> std::io::Result<()> {
    for jar in jars {
        let file = std::fs::File::open(jar)?;
        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("skipping unreadable native jar {}: {e}", jar.display());
                continue;
            }
        };
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(name) = entry.enclosed_name() else {
                continue; // reject path-traversal entries
            };
            let name_str = name.to_string_lossy().replace('\\', "/");
            if entry.is_dir() || name_str.starts_with("META-INF/") {
                continue;
            }
            if excludes
                .iter()
                .any(|ex| name_str.starts_with(ex.trim_end_matches('/')))
            {
                continue;
            }
            // Flatten to the file name: native loaders look in the dir root.
            let file_name = match name.file_name() {
                Some(n) => n,
                None => continue,
            };
            let dest = target.join(file_name);
            let mut out = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}
