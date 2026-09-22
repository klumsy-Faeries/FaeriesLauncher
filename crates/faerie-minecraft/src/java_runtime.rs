//! Provision Java runtimes from Mojang's own runtime manifest (§10: don't
//! bundle huge runtimes; fetch the exact builds vanilla uses, on demand).

use std::collections::BTreeMap;

use faerie_net::{DownloadRequest, Downloader};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{GamePaths, McError};

pub const RUNTIME_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// `all.json`: platform → component name → list of available runtimes.
type RuntimeManifest = BTreeMap<String, BTreeMap<String, Vec<RuntimeEntry>>>;

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    url: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

/// The per-runtime file listing.
#[derive(Debug, Deserialize)]
struct RuntimeFiles {
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    Directory,
    File {
        downloads: FileDownloads,
        /// Only consulted on Unix, where we must restore the +x bit.
        #[serde(default)]
        #[cfg_attr(not(unix), allow(dead_code))]
        executable: bool,
    },
    /// Symlinks appear only in the mac/linux runtimes; we skip them.
    Link {
        #[allow(dead_code)]
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct FileDownloads {
    raw: RawDownload,
}

#[derive(Debug, Deserialize)]
struct RawDownload {
    url: String,
    sha1: String,
    size: u64,
}

/// Mojang's platform key for the current machine.
pub fn current_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "x86") => "windows-x86",
        ("windows", "aarch64") => "windows-arm64",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("linux", "x86") => "linux-i386",
        _ => "linux",
    }
}

pub struct RuntimeProvisioner {
    client: reqwest::Client,
    downloader: Downloader,
    paths: GamePaths,
}

impl RuntimeProvisioner {
    pub fn new(client: reqwest::Client, downloader: Downloader, paths: GamePaths) -> Self {
        Self {
            client,
            downloader,
            paths,
        }
    }

    /// Download the runtime `component` (e.g. `java-runtime-delta`) for this
    /// platform into `<data>/java/<component>`, returning the java executable.
    pub async fn provision(
        &self,
        component: &str,
        manifest_url: Option<&str>,
        cancel: &CancellationToken,
        mut on_progress: impl FnMut(usize, usize),
    ) -> Result<std::path::PathBuf, McError> {
        let url = manifest_url.unwrap_or(RUNTIME_MANIFEST_URL);
        let manifest: RuntimeManifest = self.fetch_json(url).await?;
        let platform = current_platform();
        let entry = manifest
            .get(platform)
            .and_then(|components| components.get(component))
            .and_then(|entries| entries.first())
            .ok_or_else(|| McError::VersionInvalid {
                id: component.to_string(),
                reason: format!("no `{component}` runtime is published for {platform}"),
            })?;
        tracing::info!(
            "provisioning java runtime {} ({})",
            entry.version.name,
            component
        );

        let files: RuntimeFiles = self.fetch_json(&entry.manifest.url).await?;
        let target = self.paths.java.join(component);

        let mut batch = Vec::new();
        for (relative, file) in &files.files {
            match file {
                RuntimeFile::Directory => {
                    let dir = target.join(relative);
                    tokio::fs::create_dir_all(&dir)
                        .await
                        .map_err(|e| McError::io(&dir, e))?;
                }
                RuntimeFile::File { downloads, .. } => {
                    batch.push(DownloadRequest {
                        url: downloads.raw.url.clone(),
                        dest: target.join(relative),
                        sha1: Some(downloads.raw.sha1.clone()),
                        size: Some(downloads.raw.size),
                        label: relative.clone(),
                    });
                }
                RuntimeFile::Link { .. } => {
                    // Symlinks appear only in the mac/linux runtimes; skipped
                    // on Windows and recreated as copies where needed later.
                }
            }
        }

        let total = batch.len();
        let outcome = self
            .downloader
            .fetch_batch(batch, cancel, |p| on_progress(p.files_done, total))
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for (relative, file) in &files.files {
                if let RuntimeFile::File {
                    executable: true, ..
                } = file
                {
                    let path = target.join(relative);
                    if let Ok(meta) = std::fs::metadata(&path) {
                        let mut perms = meta.permissions();
                        perms.set_mode(perms.mode() | 0o755);
                        let _ = std::fs::set_permissions(&path, perms);
                    }
                }
            }
        }

        let exe = if cfg!(windows) {
            target.join("bin").join("java.exe")
        } else {
            target.join("bin").join("java")
        };
        Ok(exe)
    }

    async fn fetch_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, McError> {
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
        response.json().await.map_err(|e| McError::Http {
            url: url.to_string(),
            reason: e.to_string(),
        })
    }
}

/// Fallback mapping from a required Java major version to Mojang's runtime
/// component name.
///
/// Only used when the version metadata does not name its own component —
/// `javaVersion.component` is authoritative and always preferred (§2:
/// versions are data, not assumptions). Newer majors than we know about fall
/// through to the newest component we do know, which is better than failing.
pub fn component_for_major(major: u32) -> &'static str {
    match major {
        0..=8 => "jre-legacy",
        9..=16 => "java-runtime-alpha",
        17..=20 => "java-runtime-gamma",
        21..=24 => "java-runtime-delta",
        _ => "java-runtime-epsilon",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_key_is_known() {
        let platform = current_platform();
        assert!([
            "windows-x64",
            "windows-x86",
            "windows-arm64",
            "mac-os",
            "mac-os-arm64",
            "linux",
            "linux-i386"
        ]
        .contains(&platform));
    }

    #[test]
    fn components_map_by_major() {
        assert_eq!(component_for_major(8), "jre-legacy");
        assert_eq!(component_for_major(17), "java-runtime-gamma");
        assert_eq!(component_for_major(21), "java-runtime-delta");
        // Minecraft 26.2 requires Java 25 (component java-runtime-epsilon).
        assert_eq!(component_for_major(25), "java-runtime-epsilon");
        assert_eq!(component_for_major(99), "java-runtime-epsilon");
    }

    #[test]
    fn runtime_manifest_shape_parses() {
        let raw = r#"{
            "windows-x64": {
                "java-runtime-delta": [
                    { "availability": { "group": 1, "progress": 100 },
                      "manifest": { "sha1": "abc", "size": 1, "url": "http://x/files.json" },
                      "version": { "name": "21.0.3", "released": "2024-04-16" } }
                ],
                "jre-legacy": []
            }
        }"#;
        let manifest: RuntimeManifest = serde_json::from_str(raw).unwrap();
        let entry = &manifest["windows-x64"]["java-runtime-delta"][0];
        assert_eq!(entry.version.name, "21.0.3");
        assert_eq!(entry.manifest.url, "http://x/files.json");
    }

    #[test]
    fn runtime_files_shape_parses() {
        let raw = r#"{
            "files": {
                "bin": { "type": "directory" },
                "bin/java.exe": {
                    "type": "file",
                    "executable": true,
                    "downloads": { "raw": { "sha1": "d", "size": 42, "url": "http://x/java.exe" } }
                },
                "lib/link": { "type": "link", "target": "../other" }
            }
        }"#;
        let files: RuntimeFiles = serde_json::from_str(raw).unwrap();
        assert_eq!(files.files.len(), 3);
        assert!(matches!(files.files["bin"], RuntimeFile::Directory));
        assert!(matches!(
            files.files["bin/java.exe"],
            RuntimeFile::File { .. }
        ));
    }
}
