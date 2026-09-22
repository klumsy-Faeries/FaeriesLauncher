//! Modrinth lookups: which build of a mod fits an instance.
//!
//! Only the two read endpoints the presets need are wrapped. Downloads go
//! through `faerie-net` like every other file, so they get the same retry,
//! resume, and hash verification as the game itself.

use serde::{Deserialize, Serialize};

use crate::scan::LoaderKind;
use crate::ModError;

pub const DEFAULT_BASE: &str = "https://api.modrinth.com/v2";

/// Release channel Modrinth tags a version with. Ordered so `Release` is
/// the greatest: selection prefers a release over any pre-release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Alpha,
    Beta,
    Release,
}

/// A resolved, downloadable build of one mod.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub project_id: String,
    /// The slug or id the build was looked up by.
    pub project: String,
    pub version: String,
    pub channel: Channel,
    pub file_name: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    /// Project ids this build declares as required dependencies.
    pub required: Vec<String>,
}

pub struct ModrinthClient {
    client: reqwest::Client,
    base: String,
}

impl ModrinthClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self::with_base(client, DEFAULT_BASE)
    }

    /// Point at a different API root (test server).
    pub fn with_base(client: reqwest::Client, base: impl Into<String>) -> Self {
        Self {
            client,
            base: base.into().trim_end_matches('/').to_string(),
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, String)],
    ) -> Result<T, ModError> {
        let response = self
            .client
            .get(url)
            .query(query)
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
        let body = response.text().await.map_err(|e| ModError::Http {
            url: url.to_string(),
            reason: e.to_string(),
        })?;
        serde_json::from_str(&body).map_err(|e| ModError::BadLoaderMetadata {
            loader: "Modrinth",
            reason: e.to_string(),
        })
    }

    /// The newest build of `project` (slug or id) for a Minecraft version
    /// and loader. A release always wins; a pre-release is used only when
    /// `prerelease_ok` and no release exists. `None` means the project has
    /// no build for that combination at all.
    pub async fn resolve(
        &self,
        project: &str,
        game_version: &str,
        loader: LoaderKind,
        prerelease_ok: bool,
    ) -> Result<Option<ModFile>, ModError> {
        let url = format!("{}/project/{project}/version", self.base);
        let mut versions: Vec<Version> = self
            .get_json(
                &url,
                &[
                    ("game_versions", format!("[\"{game_version}\"]")),
                    ("loaders", format!("[\"{}\"]", loader.id())),
                ],
            )
            .await?;

        // Newest of the best channel first. Modrinth already returns newest
        // first, but sorting here keeps the choice independent of that.
        versions.sort_by(|a, b| {
            b.version_type
                .cmp(&a.version_type)
                .then_with(|| b.date_published.cmp(&a.date_published))
        });
        let Some(best) = versions.into_iter().next() else {
            return Ok(None);
        };
        if best.version_type != Channel::Release && !prerelease_ok {
            return Ok(None);
        }
        let file = best
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| best.files.first())
            .ok_or_else(|| ModError::BadLoaderMetadata {
                loader: "Modrinth",
                reason: format!(
                    "version {} of {project} lists no files",
                    best.version_number
                ),
            })?;
        let sha1 = file
            .hashes
            .sha1
            .clone()
            .ok_or_else(|| ModError::BadLoaderMetadata {
                loader: "Modrinth",
                reason: format!("{} has no sha1 hash to verify against", file.filename),
            })?;
        Ok(Some(ModFile {
            project_id: best.project_id,
            project: project.to_string(),
            version: best.version_number,
            channel: best.version_type,
            file_name: file.filename.clone(),
            url: file.url.clone(),
            sha1,
            size: file.size,
            required: best
                .dependencies
                .into_iter()
                .filter(|d| d.dependency_type == "required")
                .filter_map(|d| d.project_id)
                .collect(),
        }))
    }

    /// The slug of a project referenced by id (dependencies are declared by id).
    pub async fn slug_of(&self, project_id: &str) -> Result<String, ModError> {
        let url = format!("{}/project/{project_id}", self.base);
        let project: Project = self.get_json(&url, &[]).await?;
        Ok(project.slug)
    }
}

#[derive(Deserialize)]
struct Version {
    project_id: String,
    version_number: String,
    version_type: Channel,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    files: Vec<File>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct File {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    hashes: Hashes,
}

#[derive(Deserialize, Default)]
struct Hashes {
    sha1: Option<String>,
}

#[derive(Deserialize)]
struct Dependency {
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct Project {
    slug: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_order_release_highest() {
        assert!(Channel::Release > Channel::Beta);
        assert!(Channel::Beta > Channel::Alpha);
    }

    #[test]
    fn channel_parses_modrinth_names() {
        assert_eq!(
            serde_json::from_str::<Channel>("\"release\"").unwrap(),
            Channel::Release
        );
        assert_eq!(
            serde_json::from_str::<Channel>("\"beta\"").unwrap(),
            Channel::Beta
        );
    }
}
