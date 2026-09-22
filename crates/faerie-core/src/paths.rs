use std::path::PathBuf;

use crate::CoreError;

/// Resolved locations for every launcher-owned file on disk.
///
/// Root resolution order:
/// 1. `FAERIE_DATA_DIR` environment variable (portable installs, tests)
/// 2. `<OS config dir>/FaerieLauncher` (`%APPDATA%\FaerieLauncher` on Windows)
///
/// The first-launch wizard (Phase 5) will let the user pick the root; that
/// choice will be persisted and honored here ahead of the OS default.
#[derive(Debug, Clone)]
pub struct DataPaths {
    pub root: PathBuf,
    /// `config/*.json` — one file per settings group.
    pub config_dir: PathBuf,
    /// Backups of unparseable config files, kept for user recovery.
    pub corrupt_config_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub cache_dir: PathBuf,
    /// User-provided themes; a theme here overrides a built-in of the same name.
    pub themes_dir: PathBuf,
    pub instances_dir: PathBuf,
    /// Shared, deduplicated game data (libraries, assets, versions, java).
    pub data_dir: PathBuf,
}

impl DataPaths {
    pub fn resolve() -> Result<Self, CoreError> {
        let root = match std::env::var_os("FAERIE_DATA_DIR") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::config_dir()
                .ok_or(CoreError::NoConfigDir)?
                .join("FaerieLauncher"),
        };
        Ok(Self::at_root(root))
    }

    pub fn at_root(root: PathBuf) -> Self {
        Self {
            config_dir: root.join("config"),
            corrupt_config_dir: root.join("config").join("corrupt"),
            logs_dir: root.join("logs"),
            cache_dir: root.join("cache"),
            themes_dir: root.join("themes"),
            instances_dir: root.join("instances"),
            data_dir: root.join("data"),
            root,
        }
    }

    /// Create every directory. Idempotent.
    pub fn ensure_created(&self) -> Result<(), CoreError> {
        for dir in [
            &self.root,
            &self.config_dir,
            &self.corrupt_config_dir,
            &self.logs_dir,
            &self.cache_dir,
            &self.themes_dir,
            &self.instances_dir,
            &self.data_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir.clone(), e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_root_derives_children_and_creates_them() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::at_root(tmp.path().join("FaerieLauncher"));
        assert_eq!(paths.config_dir, paths.root.join("config"));
        paths.ensure_created().unwrap();
        assert!(paths.corrupt_config_dir.is_dir());
        assert!(paths.themes_dir.is_dir());
    }
}
