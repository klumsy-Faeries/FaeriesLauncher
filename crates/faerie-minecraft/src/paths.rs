//! Shared, deduplicated on-disk layout for game data (§11, §15).
//!
//! Libraries, assets, versions, and Java runtimes are shared across every
//! instance; only per-instance state lives under an instance directory.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GamePaths {
    /// `<data>/data` — the shared game-data root.
    pub root: PathBuf,
    pub versions: PathBuf,
    pub libraries: PathBuf,
    pub assets: PathBuf,
    pub java: PathBuf,
}

impl GamePaths {
    /// `data_root` is the launcher's `data/` directory (see faerie-core paths).
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        let root = data_root.into();
        Self {
            versions: root.join("versions"),
            libraries: root.join("libraries"),
            assets: root.join("assets"),
            java: root.join("java"),
            root,
        }
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions.join(id)
    }

    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }

    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }

    /// Per-version natives directory, scoped by OS/arch so switching machines
    /// never mixes native binaries.
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!(
            "natives-{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    }

    pub fn library_path(&self, relative: &str) -> PathBuf {
        self.libraries.join(relative)
    }

    pub fn asset_index_json(&self, id: &str) -> PathBuf {
        self.assets.join("indexes").join(format!("{id}.json"))
    }

    pub fn asset_object(&self, hash: &str) -> PathBuf {
        self.assets.join("objects").join(&hash[..2]).join(hash)
    }

    /// Where a legacy/virtual asset with the given logical path lives.
    pub fn virtual_asset(&self, index_id: &str, logical: &str) -> PathBuf {
        self.assets.join("virtual").join(index_id).join(logical)
    }

    pub fn logging_config(&self, id: &str) -> PathBuf {
        self.assets.join("log_configs").join(id)
    }

    pub fn ensure_base_dirs(&self) -> std::io::Result<()> {
        for dir in [&self.versions, &self.libraries, &self.assets, &self.java] {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::create_dir_all(self.assets.join("objects"))?;
        std::fs::create_dir_all(self.assets.join("indexes"))?;
        Ok(())
    }

    pub fn is_relative_to(root: &Path, child: &Path) -> bool {
        child.starts_with(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_derived_from_data_root() {
        let paths = GamePaths::new("/data");
        assert!(paths
            .version_json("26.2")
            .ends_with("versions/26.2/26.2.json"));
        assert!(paths
            .version_jar("26.2")
            .ends_with("versions/26.2/26.2.jar"));
        assert!(paths
            .asset_object("abcd1234")
            .ends_with("assets/objects/ab/abcd1234"));
        assert!(paths
            .library_path("org/lwjgl/lwjgl/3.3/lwjgl-3.3.jar")
            .ends_with("libraries/org/lwjgl/lwjgl/3.3/lwjgl-3.3.jar"));
    }
}
