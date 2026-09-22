//! Forge and NeoForge adapter.
//!
//! **Status: version discovery works; installation is not yet implemented.**
//!
//! Unlike Fabric and Quilt, Forge and NeoForge do not publish a ready-to-use
//! version JSON. They ship an *installer jar* containing `install_profile.json`,
//! which declares a chain of **processors**: Java programs (jar splitters,
//! mapping mergers, binary patchers) that must be executed in order, with
//! `[maven:coordinate]` tokens resolved to local paths, to produce the patched
//! client the loader then launches.
//!
//! That pipeline is a subsystem in its own right — it downloads a tool set,
//! runs several JVM processes, and its steps change between Forge versions.
//! Implementing it half-way would produce instances that install without
//! error and then fail at launch, which is worse than a clear refusal. So
//! [`ForgeAdapter::install`] returns [`ModError::LoaderInstallUnsupported`]
//! with an explanation, while `versions_for` works fully so the rest of the
//! system (UI, instance config, compatibility checks against Forge mods) is
//! real and exercised.

use std::path::Path;

use super::{InstalledLoader, LoaderAdapter, LoaderVersion};
use crate::scan::LoaderKind;
use crate::ModError;

pub const FORGE_MAVEN_META: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
pub const NEOFORGE_MAVEN_META: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";

pub struct ForgeAdapter {
    client: reqwest::Client,
    kind: LoaderKind,
    metadata_url: String,
}

impl ForgeAdapter {
    pub fn forge(client: reqwest::Client) -> Self {
        Self {
            client,
            kind: LoaderKind::Forge,
            metadata_url: FORGE_MAVEN_META.to_string(),
        }
    }

    pub fn neoforge(client: reqwest::Client) -> Self {
        Self {
            client,
            kind: LoaderKind::NeoForge,
            metadata_url: NEOFORGE_MAVEN_META.to_string(),
        }
    }

    pub fn with_metadata_url(mut self, url: impl Into<String>) -> Self {
        self.metadata_url = url.into();
        self
    }
}

/// Pull `<version>` values out of a Maven `maven-metadata.xml`.
///
/// A tiny hand-rolled extraction rather than an XML dependency: the document
/// is a flat, machine-generated list and this is the only XML in the project.
pub(crate) fn parse_maven_versions(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        versions.push(after[..end].trim().to_string());
        rest = &after[end..];
    }
    versions
}

/// Forge versions are `<mc>-<forge>` (e.g. `1.20.4-49.0.30`); NeoForge drops
/// the Minecraft prefix and encodes it in its own number (`20.4.230`).
fn matches_minecraft(kind: LoaderKind, version: &str, minecraft_version: &str) -> bool {
    match kind {
        LoaderKind::Forge => version
            .split_once('-')
            .map(|(mc, _)| mc == minecraft_version)
            .unwrap_or(false),
        LoaderKind::NeoForge => {
            // 1.20.4 -> "20.4", 1.21 -> "21.0"
            let mut parts = minecraft_version.split('.');
            let (Some(_major), Some(minor)) = (parts.next(), parts.next()) else {
                return false;
            };
            let patch = parts.next().unwrap_or("0");
            let prefix = format!("{minor}.{patch}.");
            version.starts_with(&prefix)
        }
        _ => false,
    }
}

#[async_trait::async_trait]
impl LoaderAdapter for ForgeAdapter {
    fn kind(&self) -> LoaderKind {
        self.kind
    }

    async fn versions_for(&self, minecraft_version: &str) -> Result<Vec<LoaderVersion>, ModError> {
        let response = self
            .client
            .get(&self.metadata_url)
            .send()
            .await
            .map_err(|e| ModError::Http {
                url: self.metadata_url.clone(),
                reason: e.to_string(),
            })?;
        if !response.status().is_success() {
            return Err(ModError::Http {
                url: self.metadata_url.clone(),
                reason: format!("status {}", response.status()),
            });
        }
        let xml = response.text().await.map_err(|e| ModError::Http {
            url: self.metadata_url.clone(),
            reason: e.to_string(),
        })?;

        let mut versions: Vec<LoaderVersion> = parse_maven_versions(&xml)
            .into_iter()
            .filter(|v| matches_minecraft(self.kind, v, minecraft_version))
            .map(|v| LoaderVersion {
                stable: !v.contains("beta") && !v.contains("alpha"),
                version_id: format!("{}-{v}", self.kind.id()),
                version: v,
            })
            .collect();
        versions.reverse(); // maven lists oldest first
        Ok(versions)
    }

    async fn install(
        &self,
        _minecraft_version: &str,
        _loader_version: &str,
        _versions_dir: &Path,
    ) -> Result<InstalledLoader, ModError> {
        Err(ModError::LoaderInstallUnsupported {
            loader: self.kind.display(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.minecraftforge</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <versions>
      <version>1.20.1-47.2.0</version>
      <version>1.20.4-49.0.30</version>
      <version>1.20.4-49.0.31</version>
      <version>1.21-51.0.10</version>
    </versions>
  </versioning>
</metadata>"#;

    #[test]
    fn maven_versions_are_extracted_in_document_order() {
        let versions = parse_maven_versions(SAMPLE);
        assert_eq!(versions.len(), 4);
        assert_eq!(versions[0], "1.20.1-47.2.0");
        assert_eq!(versions[3], "1.21-51.0.10");
    }

    #[test]
    fn forge_versions_filter_by_minecraft_prefix() {
        assert!(matches_minecraft(
            LoaderKind::Forge,
            "1.20.4-49.0.30",
            "1.20.4"
        ));
        assert!(!matches_minecraft(
            LoaderKind::Forge,
            "1.20.1-47.2.0",
            "1.20.4"
        ));
        // 1.20.4 must not match a 1.20.40-style version.
        assert!(!matches_minecraft(
            LoaderKind::Forge,
            "1.20.40-1.0",
            "1.20.4"
        ));
    }

    #[test]
    fn neoforge_versions_encode_minecraft_in_their_own_numbering() {
        assert!(matches_minecraft(
            LoaderKind::NeoForge,
            "20.4.230",
            "1.20.4"
        ));
        assert!(!matches_minecraft(
            LoaderKind::NeoForge,
            "20.2.88",
            "1.20.4"
        ));
        // 1.21 has no patch component, so NeoForge uses 21.0.x.
        assert!(matches_minecraft(LoaderKind::NeoForge, "21.0.5", "1.21"));
    }

    #[tokio::test]
    async fn install_refuses_clearly_rather_than_half_working() {
        let adapter = ForgeAdapter::neoforge(reqwest::Client::new());
        let err = adapter
            .install("1.20.4", "20.4.230", Path::new("."))
            .await
            .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("NeoForge"), "names the loader: {text}");
        assert!(
            text.contains("not yet"),
            "states it is unimplemented: {text}"
        );
    }
}
