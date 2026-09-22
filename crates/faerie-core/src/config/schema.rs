use std::sync::OnceLock;

use serde::Serialize;
use serde_json::{json, Value};

use super::ConfigError;

/// Which JSON file under `config/` a setting is persisted in.
///
/// A file only exists here once it has at least one real, consumed setting —
/// no speculative empty config files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigFile {
    Launcher,
    Ui,
    Downloads,
    Java,
    Accounts,
    Shortcuts,
    Advanced,
}

impl ConfigFile {
    pub const ALL: [ConfigFile; 7] = [
        ConfigFile::Launcher,
        ConfigFile::Ui,
        ConfigFile::Downloads,
        ConfigFile::Java,
        ConfigFile::Accounts,
        ConfigFile::Shortcuts,
        ConfigFile::Advanced,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            ConfigFile::Launcher => "launcher.json",
            ConfigFile::Ui => "ui.json",
            ConfigFile::Downloads => "downloads.json",
            ConfigFile::Shortcuts => "shortcuts.json",
            ConfigFile::Java => "java.json",
            ConfigFile::Accounts => "accounts-settings.json",
            ConfigFile::Advanced => "advanced.json",
        }
    }

    /// The setting-id prefix for this file (`ui` in `ui.theme`).
    pub fn prefix(self) -> &'static str {
        match self {
            ConfigFile::Launcher => "launcher",
            ConfigFile::Ui => "ui",
            ConfigFile::Downloads => "downloads",
            ConfigFile::Shortcuts => "shortcuts",
            ConfigFile::Java => "java",
            ConfigFile::Accounts => "accounts",
            ConfigFile::Advanced => "advanced",
        }
    }
}

/// The value type and constraints of a setting.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SettingKind {
    Bool,
    Uint { min: u64, max: u64 },
    Text,
    Choice { options: &'static [&'static str] },
}

/// A single setting declaration.
///
/// `id` is `<file-prefix>.<key>` and doubles as the i18n key base:
/// the UI looks up `setting.<id>.name` and `setting.<id>.description`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingDef {
    pub id: &'static str,
    #[serde(skip)]
    pub file: ConfigFile,
    #[serde(flatten)]
    pub kind: SettingKind,
    pub default: Value,
    pub restart_required: bool,
}

impl SettingDef {
    /// The JSON key inside the file (`theme` for `ui.theme`).
    pub fn key(&self) -> &'static str {
        self.id
            .split_once('.')
            .map(|(_, key)| key)
            .unwrap_or(self.id)
    }

    /// Validate a candidate value against this setting's kind and bounds.
    pub fn validate(&self, value: &Value) -> Result<(), ConfigError> {
        let mismatch = |expected: &'static str| ConfigError::TypeMismatch {
            id: self.id.to_string(),
            expected,
            got: value.to_string(),
        };
        match &self.kind {
            SettingKind::Bool => value.as_bool().map(|_| ()).ok_or(mismatch("boolean")),
            SettingKind::Uint { min, max } => {
                let n = value.as_u64().ok_or(mismatch("whole number"))?;
                if n < *min || n > *max {
                    return Err(ConfigError::OutOfRange {
                        id: self.id.to_string(),
                        min: *min,
                        max: *max,
                        got: n,
                    });
                }
                Ok(())
            }
            SettingKind::Text => value.as_str().map(|_| ()).ok_or(mismatch("string")),
            SettingKind::Choice { options } => {
                let s = value.as_str().ok_or(mismatch("string"))?;
                if !options.contains(&s) {
                    return Err(ConfigError::InvalidChoice {
                        id: self.id.to_string(),
                        options,
                        got: s.to_string(),
                    });
                }
                Ok(())
            }
        }
    }
}

/// The complete setting registry. Add new settings here and nowhere else.
pub fn settings() -> &'static [SettingDef] {
    static REGISTRY: OnceLock<Vec<SettingDef>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        vec![
            SettingDef {
                // Text, not Choice: locales are discovered at runtime from
                // the data directory, so a fixed option list would reject a
                // language the user added themselves (§49).
                id: "launcher.language",
                file: ConfigFile::Launcher,
                kind: SettingKind::Text,
                default: json!("en-US"),
                restart_required: false,
            },
            SettingDef {
                // Set by the first-launch wizard when it finishes (§37).
                // Until then the launcher opens on the setup flow.
                id: "launcher.setup_complete",
                file: ConfigFile::Launcher,
                kind: SettingKind::Bool,
                default: json!(false),
                restart_required: false,
            },
            SettingDef {
                id: "ui.theme",
                file: ConfigFile::Ui,
                kind: SettingKind::Text,
                default: json!("smp"),
                restart_required: false,
            },
            SettingDef {
                // JSON object merged over the active theme's layout.json.
                // Stored in config rather than the theme folder so the
                // user's arrangement survives a theme switch and never
                // writes into a built-in theme (§6).
                id: "ui.layout_overrides",
                file: ConfigFile::Ui,
                kind: SettingKind::Text,
                default: json!("{}"),
                restart_required: false,
            },
            SettingDef {
                id: "ui.font_scale",
                file: ConfigFile::Ui,
                kind: SettingKind::Uint { min: 50, max: 200 },
                default: json!(100),
                restart_required: false,
            },
            SettingDef {
                id: "ui.reduced_motion",
                file: ConfigFile::Ui,
                kind: SettingKind::Bool,
                default: json!(false),
                restart_required: false,
            },
            SettingDef {
                id: "downloads.concurrency",
                file: ConfigFile::Downloads,
                kind: SettingKind::Uint { min: 1, max: 16 },
                default: json!(4),
                restart_required: false,
            },
            SettingDef {
                id: "downloads.retries",
                file: ConfigFile::Downloads,
                kind: SettingKind::Uint { min: 0, max: 10 },
                default: json!(3),
                restart_required: false,
            },
            SettingDef {
                // 0 means "derive from detected hardware" (§9: recommend,
                // never force), hence the min of 0 rather than 512.
                id: "java.default_max_ram_mb",
                file: ConfigFile::Java,
                kind: SettingKind::Uint { min: 0, max: 65536 },
                default: json!(0),
                restart_required: false,
            },
            SettingDef {
                id: "java.default_min_ram_mb",
                file: ConfigFile::Java,
                kind: SettingKind::Uint { min: 0, max: 65536 },
                default: json!(512),
                restart_required: false,
            },
            SettingDef {
                id: "java.default_args",
                file: ConfigFile::Java,
                kind: SettingKind::Text,
                default: json!("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions"),
                restart_required: false,
            },
            SettingDef {
                id: "accounts.client_id",
                file: ConfigFile::Accounts,
                kind: SettingKind::Text,
                default: json!(""),
                restart_required: false,
            },
            SettingDef {
                id: "shortcuts.palette",
                file: ConfigFile::Shortcuts,
                kind: SettingKind::Text,
                default: json!("Ctrl+K"),
                restart_required: false,
            },
            SettingDef {
                id: "shortcuts.launch",
                file: ConfigFile::Shortcuts,
                kind: SettingKind::Text,
                default: json!("Ctrl+L"),
                restart_required: false,
            },
            SettingDef {
                id: "shortcuts.instances",
                file: ConfigFile::Shortcuts,
                kind: SettingKind::Text,
                default: json!("Ctrl+I"),
                restart_required: false,
            },
            SettingDef {
                id: "shortcuts.mods",
                file: ConfigFile::Shortcuts,
                kind: SettingKind::Text,
                default: json!("Ctrl+M"),
                restart_required: false,
            },
            SettingDef {
                id: "shortcuts.settings",
                file: ConfigFile::Shortcuts,
                kind: SettingKind::Text,
                default: json!("Ctrl+,"),
                restart_required: false,
            },
            SettingDef {
                id: "shortcuts.refresh",
                file: ConfigFile::Shortcuts,
                kind: SettingKind::Text,
                default: json!("Ctrl+Shift+R"),
                restart_required: false,
            },
            SettingDef {
                id: "advanced.log_level",
                file: ConfigFile::Advanced,
                kind: SettingKind::Choice {
                    options: &["error", "warn", "info", "debug", "trace"],
                },
                default: json!("info"),
                restart_required: true,
            },
        ]
    })
}

/// Look up a setting definition by full id.
pub fn setting(id: &str) -> Option<&'static SettingDef> {
    settings().iter().find(|def| def.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_match_their_file_prefix_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for def in settings() {
            assert!(seen.insert(def.id), "duplicate setting id {}", def.id);
            let (prefix, _) = def.id.split_once('.').expect("id must be file.key");
            assert_eq!(prefix, def.file.prefix(), "id {} in wrong file", def.id);
            def.validate(&def.default)
                .expect("default must pass its own validation");
        }
    }

    #[test]
    fn validation_rejects_bad_values() {
        let scale = setting("ui.font_scale").unwrap();
        assert!(matches!(
            scale.validate(&json!(300)),
            Err(ConfigError::OutOfRange { .. })
        ));
        assert!(matches!(
            scale.validate(&json!("big")),
            Err(ConfigError::TypeMismatch { .. })
        ));

        let level = setting("advanced.log_level").unwrap();
        assert!(matches!(
            level.validate(&json!("verbose")),
            Err(ConfigError::InvalidChoice { .. })
        ));
        assert!(level.validate(&json!("debug")).is_ok());
    }
}
