//! Central configuration system (§19 of the spec).
//!
//! Every setting is declared exactly once in [`schema::settings`] with its
//! type, default, bounds, and restart requirement. The settings UI, JSON
//! persistence, and validation are all driven from that registry — there is
//! one obvious place to add or change a setting.

mod schema;
mod store;

pub use schema::{setting, settings, ConfigFile, SettingDef, SettingKind};
pub use store::{ConfigStore, RecoveryNotice};

/// Validation and lookup failures for settings.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ConfigError {
    #[error("unknown setting `{0}`")]
    UnknownSetting(String),

    #[error("setting `{id}` expects a {expected}, got `{got}`")]
    TypeMismatch {
        id: String,
        expected: &'static str,
        got: String,
    },

    #[error("setting `{id}` must be between {min} and {max}, got {got}")]
    OutOfRange {
        id: String,
        min: u64,
        max: u64,
        got: u64,
    },

    #[error("setting `{id}` must be one of {options:?}, got `{got}`")]
    InvalidChoice {
        id: String,
        options: &'static [&'static str],
        got: String,
    },
}
