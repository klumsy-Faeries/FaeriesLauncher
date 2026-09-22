use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value};

use super::{schema, ConfigError, ConfigFile};
use crate::{CoreError, DataPaths};

/// Emitted when a config file could not be parsed and was replaced with
/// defaults. The original file is preserved in `config/corrupt/` (§41:
/// never silently destroy configuration).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryNotice {
    pub file: String,
    pub backup_path: String,
    pub error: String,
}

/// Loads, validates, serves, and persists all settings.
///
/// Unknown keys in config files are preserved verbatim across load/save so a
/// newer launcher's settings survive a temporary downgrade.
pub struct ConfigStore {
    config_dir: PathBuf,
    corrupt_dir: PathBuf,
    files: HashMap<ConfigFile, Map<String, Value>>,
    notices: Vec<RecoveryNotice>,
    warnings: Vec<String>,
    /// Files that were not there at load. A start that cannot see files
    /// which exist has happened; the launcher watches these to recover.
    missing: Vec<ConfigFile>,
}

impl ConfigStore {
    pub fn load(paths: &DataPaths) -> Self {
        let mut store = Self {
            config_dir: paths.config_dir.clone(),
            corrupt_dir: paths.corrupt_config_dir.clone(),
            files: HashMap::new(),
            notices: Vec::new(),
            warnings: Vec::new(),
            missing: Vec::new(),
        };
        for file in ConfigFile::ALL {
            let map = store.load_file(file);
            store.files.insert(file, map);
        }
        store.discard_invalid_values();
        store
    }

    fn load_file(&mut self, file: ConfigFile) -> Map<String, Value> {
        let path = self.config_dir.join(file.file_name());
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            // Missing file simply means "all defaults".
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.missing.push(file);
                return Map::new();
            }
            Err(e) => {
                self.warnings.push(format!(
                    "could not read {}: {e}; using defaults for this session",
                    path.display()
                ));
                return Map::new();
            }
        };
        match serde_json::from_str::<Map<String, Value>>(&raw) {
            Ok(map) => map,
            Err(e) => {
                let backup = self.backup_corrupt(&path);
                self.notices.push(RecoveryNotice {
                    file: file.file_name().to_string(),
                    backup_path: backup
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    error: e.to_string(),
                });
                Map::new()
            }
        }
    }

    /// Copy an unparseable file into `config/corrupt/<name>.<unix-secs>.json`.
    fn backup_corrupt(&self, path: &Path) -> Option<PathBuf> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".into());
        let name = path.file_stem()?.to_string_lossy();
        let backup = self.corrupt_dir.join(format!("{name}.{secs}.json"));
        std::fs::create_dir_all(&self.corrupt_dir).ok()?;
        std::fs::copy(path, &backup).ok()?;
        Some(backup)
    }

    /// Drop stored values that fail validation so the default applies instead.
    fn discard_invalid_values(&mut self) {
        for def in schema::settings() {
            let Some(map) = self.files.get_mut(&def.file) else {
                continue;
            };
            if let Some(value) = map.get(def.key()) {
                if let Err(e) = def.validate(value) {
                    self.warnings
                        .push(format!("{e}; falling back to default {}", def.default));
                    map.remove(def.key());
                }
            }
        }
    }

    /// Effective value of one setting (stored value or default).
    pub fn get(&self, id: &str) -> Result<Value, ConfigError> {
        let def = schema::setting(id).ok_or_else(|| ConfigError::UnknownSetting(id.into()))?;
        Ok(self
            .files
            .get(&def.file)
            .and_then(|m| m.get(def.key()))
            .cloned()
            .unwrap_or_else(|| def.default.clone()))
    }

    pub fn get_str(&self, id: &str) -> Result<String, ConfigError> {
        Ok(self.get(id)?.as_str().unwrap_or_default().to_string())
    }

    pub fn get_bool(&self, id: &str) -> Result<bool, ConfigError> {
        Ok(self.get(id)?.as_bool().unwrap_or_default())
    }

    pub fn get_u64(&self, id: &str) -> Result<u64, ConfigError> {
        Ok(self.get(id)?.as_u64().unwrap_or_default())
    }

    /// Effective values of every registered setting, keyed by full id.
    pub fn values(&self) -> HashMap<String, Value> {
        schema::settings()
            .iter()
            .map(|def| (def.id.to_string(), self.get(def.id).expect("registered id")))
            .collect()
    }

    /// Validate and stage a new value. Returns the definition so the caller
    /// can persist the right file and honor `restart_required`.
    pub fn set(
        &mut self,
        id: &str,
        value: Value,
    ) -> Result<&'static schema::SettingDef, ConfigError> {
        let def = schema::setting(id).ok_or_else(|| ConfigError::UnknownSetting(id.into()))?;
        def.validate(&value)?;
        self.files
            .entry(def.file)
            .or_default()
            .insert(def.key().to_string(), value);
        Ok(def)
    }

    /// Files that did not exist when the store was loaded.
    pub fn missing_files(&self) -> &[ConfigFile] {
        &self.missing
    }

    /// Atomically persist one config file (write temp, then rename over).
    ///
    /// Read-modify-write: whatever the file holds on disk right now is the
    /// base and this store's values go on top. If the file could not be
    /// seen at load but is there now, its other keys survive instead of
    /// being replaced by defaults.
    pub fn save_file(&self, file: ConfigFile) -> Result<(), CoreError> {
        std::fs::create_dir_all(&self.config_dir)
            .map_err(|e| CoreError::io(self.config_dir.clone(), e))?;
        let path = self.config_dir.join(file.file_name());
        let tmp = self.config_dir.join(format!("{}.tmp", file.file_name()));
        let mut map: Map<String, Value> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        if let Some(ours) = self.files.get(&file) {
            for (key, value) in ours {
                map.insert(key.clone(), value.clone());
            }
        }
        let body = serde_json::to_string_pretty(&map).expect("config maps are valid JSON");
        std::fs::write(&tmp, body).map_err(|e| CoreError::io(tmp.clone(), e))?;
        std::fs::rename(&tmp, &path).map_err(|e| CoreError::io(path, e))
    }

    pub fn save_all(&self) -> Result<(), CoreError> {
        for file in ConfigFile::ALL {
            self.save_file(file)?;
        }
        Ok(())
    }

    /// Corruption-recovery notices collected during load, for the UI.
    pub fn notices(&self) -> &[RecoveryNotice] {
        &self.notices
    }

    /// Human-readable warnings (invalid values that fell back to defaults).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn temp_paths() -> (tempfile::TempDir, DataPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::at_root(tmp.path().join("root"));
        paths.ensure_created().unwrap();
        (tmp, paths)
    }

    #[test]
    fn defaults_apply_when_no_files_exist() {
        let (_tmp, paths) = temp_paths();
        let store = ConfigStore::load(&paths);
        assert_eq!(store.get_str("ui.theme").unwrap(), "smp");
        assert_eq!(store.get_u64("downloads.concurrency").unwrap(), 4);
        assert!(store.notices().is_empty());
    }

    #[test]
    fn set_save_reload_roundtrip_preserves_unknown_keys() {
        let (_tmp, paths) = temp_paths();
        std::fs::write(
            paths.config_dir.join("ui.json"),
            r#"{ "future_setting": [1, 2, 3] }"#,
        )
        .unwrap();

        let mut store = ConfigStore::load(&paths);
        let def = store.set("ui.theme", json!("dark")).unwrap();
        assert!(!def.restart_required);
        store.save_file(def.file).unwrap();

        let reloaded = ConfigStore::load(&paths);
        assert_eq!(reloaded.get_str("ui.theme").unwrap(), "dark");

        let raw: Map<String, Value> = serde_json::from_str(
            &std::fs::read_to_string(paths.config_dir.join("ui.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(raw.get("future_setting"), Some(&json!([1, 2, 3])));
    }

    #[test]
    fn saving_keeps_keys_the_store_never_saw() {
        let (_tmp, paths) = temp_paths();
        // Loaded while ui.json is invisible: the store has no idea it holds
        // a theme choice.
        let mut store = ConfigStore::load(&paths);
        assert!(store.missing_files().contains(&ConfigFile::Ui));
        std::fs::write(
            paths.config_dir.join("ui.json"),
            r#"{"theme":"dark","font_scale":110}"#,
        )
        .unwrap();

        store
            .set("ui.reduced_motion", serde_json::json!(true))
            .unwrap();
        store.save_file(ConfigFile::Ui).unwrap();

        let written: Map<String, Value> = serde_json::from_str(
            &std::fs::read_to_string(paths.config_dir.join("ui.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(written["theme"], "dark", "the on-disk theme survives");
        assert_eq!(written["font_scale"], 110);
        assert_eq!(written["reduced_motion"], true, "our change lands");
    }

    #[test]
    fn set_rejects_invalid_values_and_unknown_ids() {
        let (_tmp, paths) = temp_paths();
        let mut store = ConfigStore::load(&paths);
        assert!(matches!(
            store.set("ui.font_scale", json!(1000)),
            Err(ConfigError::OutOfRange { .. })
        ));
        assert!(matches!(
            store.set("ui.reduced_motion", json!("yes")),
            Err(ConfigError::TypeMismatch { .. })
        ));
        assert!(matches!(
            store.set("nope.nothing", json!(1)),
            Err(ConfigError::UnknownSetting(_))
        ));
    }

    #[test]
    fn corrupt_file_is_backed_up_and_replaced_with_defaults() {
        let (_tmp, paths) = temp_paths();
        std::fs::write(paths.config_dir.join("launcher.json"), "{ not json !!").unwrap();

        let store = ConfigStore::load(&paths);
        assert_eq!(store.get_str("launcher.language").unwrap(), "en-US");
        assert_eq!(store.notices().len(), 1);
        let notice = &store.notices()[0];
        assert_eq!(notice.file, "launcher.json");
        assert!(Path::new(&notice.backup_path).is_file());
        let backup_body = std::fs::read_to_string(&notice.backup_path).unwrap();
        assert_eq!(backup_body, "{ not json !!");
    }

    #[test]
    fn invalid_stored_value_falls_back_to_default_with_warning() {
        let (_tmp, paths) = temp_paths();
        std::fs::write(
            paths.config_dir.join("ui.json"),
            r#"{ "font_scale": 9999 }"#,
        )
        .unwrap();

        let store = ConfigStore::load(&paths);
        assert_eq!(store.get_u64("ui.font_scale").unwrap(), 100);
        assert_eq!(store.warnings().len(), 1);
        assert!(
            store.notices().is_empty(),
            "invalid value is not corruption"
        );
    }

    #[test]
    fn values_reports_every_registered_setting() {
        let (_tmp, paths) = temp_paths();
        let store = ConfigStore::load(&paths);
        let values = store.values();
        assert_eq!(values.len(), schema::settings().len());
        assert_eq!(values["advanced.log_level"], json!("info"));
    }
}
