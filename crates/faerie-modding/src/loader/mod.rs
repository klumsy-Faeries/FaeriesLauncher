//! Mod loader adapters (§12): one trait, several ecosystems.
//!
//! The launcher never branches on "which loader is this" outside this module.
//! Adding a loader means adding an implementation here and registering it —
//! nothing above needs to change.
//!
//! Fabric and Quilt both publish ready-made version JSONs that layer over
//! vanilla through `inheritsFrom`, which the Phase 3 version pipeline already
//! resolves. Forge and NeoForge instead ship an *installer* that must be run
//! to produce a patched version; see [`forge`] for where that stands.

pub mod fabric;
pub mod forge;
pub mod quilt;

use crate::scan::LoaderKind;
use crate::ModError;

/// A loader version offered for a given Minecraft version.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    /// Marked stable by the upstream project.
    pub stable: bool,
    /// The version id this loader install will create, e.g.
    /// `fabric-loader-0.15.7-1.20.4`.
    pub version_id: String,
}

/// What a loader install produced: a version id the Phase 3 pipeline can
/// install and launch, having written its JSON into the versions directory.
#[derive(Debug, Clone)]
pub struct InstalledLoader {
    pub version_id: String,
    pub kind: LoaderKind,
    pub loader_version: String,
}

/// The interface every loader implements.
#[async_trait::async_trait]
pub trait LoaderAdapter: Send + Sync {
    fn kind(&self) -> LoaderKind;

    /// Loader versions available for a Minecraft version, newest first.
    async fn versions_for(&self, minecraft_version: &str) -> Result<Vec<LoaderVersion>, ModError>;

    /// Write the loader's version JSON into `versions_dir` so the Minecraft
    /// installer can resolve and install it like any other version.
    async fn install(
        &self,
        minecraft_version: &str,
        loader_version: &str,
        versions_dir: &std::path::Path,
    ) -> Result<InstalledLoader, ModError>;
}

/// Build the adapter for a loader kind.
pub fn adapter_for(kind: LoaderKind, client: reqwest::Client) -> Option<Box<dyn LoaderAdapter>> {
    match kind {
        LoaderKind::Fabric => Some(Box::new(fabric::FabricAdapter::new(client))),
        LoaderKind::Quilt => Some(Box::new(quilt::QuiltAdapter::new(client))),
        LoaderKind::Forge => Some(Box::new(forge::ForgeAdapter::forge(client))),
        LoaderKind::NeoForge => Some(Box::new(forge::ForgeAdapter::neoforge(client))),
        LoaderKind::Unknown => None,
    }
}

/// The version id a loader install creates for a game version — the id to
/// install and launch instead of the vanilla one, e.g.
/// `fabric-loader-0.15.7-1.20.4`. `None` for loaders whose install is not
/// implemented: they never wrote a version JSON, so there is nothing to launch.
pub fn installed_version_id(
    kind: LoaderKind,
    minecraft_version: &str,
    loader_version: &str,
) -> Option<String> {
    match kind {
        LoaderKind::Fabric => Some(fabric::FabricAdapter::version_id(
            minecraft_version,
            loader_version,
        )),
        LoaderKind::Quilt => Some(quilt::QuiltAdapter::version_id(
            minecraft_version,
            loader_version,
        )),
        LoaderKind::Forge | LoaderKind::NeoForge | LoaderKind::Unknown => None,
    }
}

/// Every loader the launcher can install, for populating UI choosers.
pub fn supported_loaders() -> [LoaderKind; 4] {
    [
        LoaderKind::Fabric,
        LoaderKind::Quilt,
        LoaderKind::NeoForge,
        LoaderKind::Forge,
    ]
}
