//! Localization (§49).
//!
//! `en-US` is embedded so the launcher always has a complete string set.
//! Additional languages are plain JSON files in `<data>/locales/`, which
//! means translating the launcher needs no rebuild and no code change — copy
//! `en-US.json`, translate the values, save it as `de-DE.json`.
//!
//! Every locale is merged *over* English, so a partial translation shows
//! translated strings where they exist and English everywhere else, rather
//! than blank labels or raw keys.

use std::collections::BTreeMap;

use serde::Serialize;

use faerie_core::DataPaths;

pub const FALLBACK: &str = "en-US";
const EMBEDDED_EN_US: &str = include_str!("../../../locales/en-US.json");

pub type Messages = BTreeMap<String, String>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleBundle {
    /// The locale actually served (may differ from the request on fallback).
    pub name: String,
    pub messages: Messages,
    /// How many keys came from the requested locale rather than English.
    pub translated_keys: usize,
    pub total_keys: usize,
    pub warnings: Vec<String>,
}

fn embedded_english() -> Messages {
    serde_json::from_str(EMBEDDED_EN_US).expect("the embedded en-US locale is valid JSON")
}

/// Locales the user can pick: the built-in one plus every `*.json` in the
/// data directory's `locales/` folder.
pub fn list(paths: &DataPaths) -> Vec<String> {
    let mut names = vec![FALLBACK.to_string()];
    let dir = paths.root.join("locales");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut found: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json") {
                    return None;
                }
                path.file_stem().map(|s| s.to_string_lossy().into_owned())
            })
            .filter(|name| name != FALLBACK)
            .collect();
        found.sort();
        names.extend(found);
    }
    names
}

/// Resolve a locale: English as the base, the requested language merged on
/// top. An unknown or unreadable locale degrades to English with a warning
/// rather than an error — a missing translation must never block the UI.
pub fn load(paths: &DataPaths, name: &str) -> LocaleBundle {
    let mut messages = embedded_english();
    let total_keys = messages.len();
    let mut warnings = Vec::new();
    let mut translated_keys = 0;
    let mut applied = FALLBACK.to_string();

    if name != FALLBACK {
        let path = paths.root.join("locales").join(format!("{name}.json"));
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Messages>(&raw) {
                Ok(overrides) => {
                    for (key, value) in overrides {
                        // Keys English does not define are almost always
                        // typos; keeping them would hide the mistake.
                        match messages.entry(key) {
                            std::collections::btree_map::Entry::Occupied(mut slot) => {
                                slot.insert(value);
                                translated_keys += 1;
                            }
                            std::collections::btree_map::Entry::Vacant(slot) => {
                                warnings.push(format!(
                                    "{name}.json has an unknown key `{}`",
                                    slot.key()
                                ));
                            }
                        }
                    }
                    applied = name.to_string();
                }
                Err(e) => warnings.push(format!(
                    "{} could not be parsed ({e}); showing English",
                    path.display()
                )),
            },
            Err(e) => warnings.push(format!(
                "{} could not be read ({e}); showing English",
                path.display()
            )),
        }
    }

    LocaleBundle {
        name: applied,
        messages,
        translated_keys,
        total_keys,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, DataPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::at_root(tmp.path().join("root"));
        paths.ensure_created().unwrap();
        (tmp, paths)
    }

    #[test]
    fn english_is_always_complete() {
        let (_tmp, paths) = temp_paths();
        let bundle = load(&paths, FALLBACK);
        assert_eq!(bundle.name, FALLBACK);
        assert!(
            bundle.total_keys > 100,
            "the shipped locale has real content"
        );
        assert!(bundle.messages.contains_key("nav.home"));
        assert!(bundle.warnings.is_empty());
    }

    #[test]
    fn a_partial_translation_falls_back_per_key() {
        let (_tmp, paths) = temp_paths();
        let dir = paths.root.join("locales");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("de-DE.json"),
            r#"{ "nav.home": "Startseite", "nav.mods": "Mods" }"#,
        )
        .unwrap();

        let bundle = load(&paths, "de-DE");
        assert_eq!(bundle.name, "de-DE");
        assert_eq!(bundle.messages["nav.home"], "Startseite");
        assert_eq!(bundle.translated_keys, 2);
        // Untranslated keys keep their English text, not a blank or a key.
        assert_eq!(bundle.messages["nav.settings"], "Settings");
        assert_eq!(bundle.total_keys, bundle.messages.len());
    }

    #[test]
    fn unknown_keys_are_reported_rather_than_silently_kept() {
        let (_tmp, paths) = temp_paths();
        let dir = paths.root.join("locales");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("xx.json"), r#"{ "nav.hme": "typo" }"#).unwrap();

        let bundle = load(&paths, "xx");
        assert_eq!(bundle.warnings.len(), 1);
        assert!(bundle.warnings[0].contains("nav.hme"));
        assert!(!bundle.messages.contains_key("nav.hme"));
    }

    #[test]
    fn a_broken_locale_degrades_to_english() {
        let (_tmp, paths) = temp_paths();
        let dir = paths.root.join("locales");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();

        let bundle = load(&paths, "broken");
        assert_eq!(bundle.name, FALLBACK, "falls back rather than failing");
        assert_eq!(bundle.messages["nav.home"], "Home");
        assert_eq!(bundle.warnings.len(), 1);
    }

    #[test]
    fn a_missing_locale_degrades_to_english() {
        let (_tmp, paths) = temp_paths();
        let bundle = load(&paths, "fr-FR");
        assert_eq!(bundle.name, FALLBACK);
        assert_eq!(bundle.warnings.len(), 1);
    }

    #[test]
    fn listing_finds_user_locales_and_always_offers_english() {
        let (_tmp, paths) = temp_paths();
        assert_eq!(list(&paths), vec![FALLBACK]);

        let dir = paths.root.join("locales");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("de-DE.json"), "{}").unwrap();
        std::fs::write(dir.join("es-ES.json"), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();

        assert_eq!(list(&paths), vec![FALLBACK, "de-DE", "es-ES"]);
    }
}
