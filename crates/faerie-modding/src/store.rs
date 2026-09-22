//! Mod store and profiles (§13, §14).
//!
//! Mod jars live once in a content-addressed store, `mod-store/<sha1>.jar`.
//! A *profile* is just a manifest naming which hashes are enabled. Applying a
//! profile materializes the instance's `mods/` directory by hard-linking from
//! the store (copying when a hardlink is impossible, e.g. across volumes).
//!
//! Consequences that matter:
//!
//! - switching profiles is a manifest swap plus links — milliseconds, not a
//!   copy of hundreds of megabytes;
//! - ten profiles sharing a 300-mod pack cost one pack of disk;
//! - "disable" flips a flag in a manifest and never deletes a jar (§13);
//! - removing a mod from a profile leaves the store copy intact, so it can
//!   be re-enabled without re-downloading.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scan::{self, DISABLED_SUFFIX};
use crate::ModError;

pub const PROFILES_FILE: &str = "mod-profiles.json";
pub const CURRENT_FORMAT: u32 = 1;

/// One mod inside a profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMod {
    /// SHA-1 of the jar; its name in the store.
    pub sha1: String,
    /// The file name to present in `mods/` (preserved from the original).
    pub file_name: String,
    /// Disabled mods stay in the manifest and the store; they are simply not
    /// materialized (or are materialized with a `.disabled` suffix).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModProfile {
    pub name: String,
    #[serde(default)]
    pub mods: Vec<ProfileMod>,
}

/// The whole profile set for one instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSet {
    pub format: u32,
    pub active: String,
    pub profiles: BTreeMap<String, ModProfile>,
}

impl Default for ProfileSet {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "default".to_string(),
            ModProfile {
                name: "Default".into(),
                mods: Vec::new(),
            },
        );
        Self {
            format: CURRENT_FORMAT,
            active: "default".into(),
            profiles,
        }
    }
}

/// How a mod file was placed into `mods/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMethod {
    Hardlink,
    Copy,
}

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub linked: usize,
    pub copied: usize,
    pub removed: usize,
    pub disabled: usize,
}

/// The content-addressed jar store, shared across every instance.
#[derive(Clone)]
pub struct ModStore {
    root: PathBuf,
}

impl ModStore {
    /// `root` is the store directory, e.g. `<data>/mod-store`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path_for(&self, sha1: &str) -> PathBuf {
        self.root.join(format!("{sha1}.jar"))
    }

    pub fn contains(&self, sha1: &str) -> bool {
        self.path_for(sha1).is_file()
    }

    /// Copy a jar into the store, returning its hash. Re-adding a jar that is
    /// already present is a no-op, so importing the same pack twice costs
    /// nothing.
    pub fn add(&self, source: &Path) -> Result<String, ModError> {
        std::fs::create_dir_all(&self.root).map_err(|e| ModError::io(&self.root, e))?;
        let sha1 = faerie_net::hash::sha1_of_file(source).map_err(|e| ModError::io(source, e))?;
        let dest = self.path_for(&sha1);
        if dest.is_file() {
            return Ok(sha1);
        }
        // Write to a temp name then rename, so a crash never leaves a
        // half-copied jar under a hash that claims to be complete.
        let tmp = self.root.join(format!("{sha1}.tmp"));
        std::fs::copy(source, &tmp).map_err(|e| ModError::io(source, e))?;
        std::fs::rename(&tmp, &dest).map_err(|e| ModError::io(&dest, e))?;
        Ok(sha1)
    }

    /// Delete a jar from the store. Only call this once no profile references
    /// it — [`ModStore::prune`] does that check.
    pub fn remove(&self, sha1: &str) -> Result<(), ModError> {
        let path = self.path_for(sha1);
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| ModError::io(&path, e))?;
        }
        Ok(())
    }

    /// Remove store entries that no live profile references. Returns the
    /// hashes removed. Callers must pass *every* profile set on the system.
    pub fn prune(&self, referenced: &[String]) -> Result<Vec<String>, ModError> {
        let mut removed = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Ok(removed);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("jar") {
                continue;
            }
            if !referenced.iter().any(|r| r == &stem) {
                std::fs::remove_file(&path).map_err(|e| ModError::io(&path, e))?;
                removed.push(stem);
            }
        }
        Ok(removed)
    }
}

/// Load an instance's profile set, creating a default one when absent.
pub fn load_profiles(instance_dir: &Path) -> Result<ProfileSet, ModError> {
    let path = instance_dir.join(PROFILES_FILE);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| ModError::BadProfiles {
            path: path.clone(),
            reason: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ProfileSet::default()),
        Err(e) => Err(ModError::io(&path, e)),
    }
}

/// Persist a profile set atomically.
pub fn save_profiles(instance_dir: &Path, set: &ProfileSet) -> Result<(), ModError> {
    std::fs::create_dir_all(instance_dir).map_err(|e| ModError::io(instance_dir, e))?;
    let path = instance_dir.join(PROFILES_FILE);
    let tmp = instance_dir.join(format!("{PROFILES_FILE}.tmp"));
    let body = serde_json::to_string_pretty(set).expect("profile set serializes");
    std::fs::write(&tmp, body).map_err(|e| ModError::io(&tmp, e))?;
    std::fs::rename(&tmp, &path).map_err(|e| ModError::io(&path, e))
}

/// Materialize `profile` into `mods_dir` from the store.
///
/// Files already correct are left alone; anything in `mods_dir` that the
/// profile does not name is removed from the *instance* only — the store copy
/// survives, so nothing is destroyed (§13).
pub fn apply_profile(
    store: &ModStore,
    profile: &ModProfile,
    mods_dir: &Path,
) -> Result<ApplyOutcome, ModError> {
    std::fs::create_dir_all(mods_dir).map_err(|e| ModError::io(mods_dir, e))?;

    let mut wanted: BTreeMap<String, &ProfileMod> = BTreeMap::new();
    for m in &profile.mods {
        let name = if m.enabled {
            m.file_name.clone()
        } else {
            format!("{}{DISABLED_SUFFIX}", m.file_name)
        };
        wanted.insert(name, m);
    }

    let mut outcome = ApplyOutcome {
        linked: 0,
        copied: 0,
        removed: 0,
        disabled: 0,
    };

    // Remove files the profile does not want.
    if let Ok(entries) = std::fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_jar = name.ends_with(".jar") || name.ends_with(".jar.disabled");
            if is_jar && !wanted.contains_key(&name) {
                std::fs::remove_file(&path).map_err(|e| ModError::io(&path, e))?;
                outcome.removed += 1;
            }
        }
    }

    // Link in everything the profile wants.
    for (name, entry) in &wanted {
        let dest = mods_dir.join(name);
        if !entry.enabled {
            outcome.disabled += 1;
        }
        if dest.is_file() {
            continue; // already materialized
        }
        let source = store.path_for(&entry.sha1);
        if !source.is_file() {
            return Err(ModError::MissingFromStore {
                sha1: entry.sha1.clone(),
                file_name: entry.file_name.clone(),
            });
        }
        match std::fs::hard_link(&source, &dest) {
            Ok(()) => outcome.linked += 1,
            // Hardlinks fail across volumes and on some filesystems; a copy
            // is always correct, just less space-efficient.
            Err(_) => {
                std::fs::copy(&source, &dest).map_err(|e| ModError::io(&dest, e))?;
                outcome.copied += 1;
            }
        }
    }

    Ok(outcome)
}

/// Import every jar currently in `mods_dir` into the store and return the
/// profile entries describing them. Used when adopting an existing instance.
pub fn import_existing(store: &ModStore, mods_dir: &Path) -> Result<Vec<ProfileMod>, ModError> {
    let mut imported = Vec::new();
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Ok(imported);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let (file_name, enabled) = match name.strip_suffix(DISABLED_SUFFIX) {
            Some(base) => (base.to_string(), false),
            None => (name.clone(), true),
        };
        if !file_name.ends_with(".jar") {
            continue;
        }
        let sha1 = store.add(&path)?;
        imported.push(ProfileMod {
            sha1,
            file_name,
            enabled,
        });
    }
    imported.sort_by_key(|m| m.file_name.to_lowercase());
    Ok(imported)
}

/// Which mod each jar in the profile is, keyed by SHA-1. A jar whose
/// metadata cannot be read (no descriptor, or one the scanner only warns
/// about) is left out: with no id, nothing can supersede it, and two
/// unknowns must never be taken for the same mod. One scan per jar, so
/// callers adding many jars compute this once.
pub fn mod_ids(store: &ModStore, profile: &ModProfile) -> HashMap<String, String> {
    profile
        .mods
        .iter()
        .filter_map(|m| {
            let scanned = scan::scan_jar(&store.path_for(&m.sha1)).ok()?;
            let id = scanned.metadata.mod_id;
            (!id.is_empty()).then(|| (m.sha1.clone(), id))
        })
        .collect()
}

/// Forget every entry of `profile` that is another build of `mod_id` (any
/// jar but `keep`), and return the file names dropped. The store keeps those
/// jars; only the profile forgets them, because a loader refuses to start
/// with two builds of one mod. `ids` comes from [`mod_ids`] and may be stale
/// for jars no longer in the profile; those are simply not visited.
pub fn supersede(
    profile: &mut ModProfile,
    ids: &HashMap<String, String>,
    keep: &str,
    mod_id: &str,
) -> Vec<String> {
    let mut dropped = Vec::new();
    profile.mods.retain(|m| {
        let same_mod = m.sha1 != keep && ids.get(&m.sha1).is_some_and(|id| id == mod_id);
        if same_mod {
            dropped.push(m.file_name.clone());
        }
        !same_mod
    });
    dropped
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    /// A jar holding only a `fabric.mod.json` with the given text.
    fn jar_with_descriptor(dir: &Path, name: &str, descriptor: &str) -> PathBuf {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(descriptor.as_bytes()).unwrap();
        write_jar(dir, name, &writer.finish().unwrap().into_inner())
    }

    /// A minimal Fabric mod jar: just the descriptor the scanner reads.
    fn fabric_jar(dir: &Path, name: &str, id: &str, version: &str) -> PathBuf {
        jar_with_descriptor(
            dir,
            name,
            &format!(r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","name":"{id}"}}"#),
        )
    }

    #[test]
    fn a_newer_build_supersedes_the_older_one_in_the_profile_only() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let src = tmp.path().join("src");
        let old = store
            .add(&fabric_jar(&src, "sodium-0.9.1.jar", "sodium", "0.9.1"))
            .unwrap();
        let lithium = store
            .add(&fabric_jar(&src, "lithium-0.25.3.jar", "lithium", "0.25.3"))
            .unwrap();
        let new = store
            .add(&fabric_jar(&src, "sodium-0.9.2.jar", "sodium", "0.9.2"))
            .unwrap();
        let junk = store
            .add(&write_jar(&src, "not-a-mod.jar", b"no descriptor"))
            .unwrap();
        // Scans "successfully" with a warning and an empty id; two such jars
        // must not be taken for builds of the same mod.
        let broken = store
            .add(&jar_with_descriptor(&src, "broken.jar", "{ not json"))
            .unwrap();
        let broken_too = store
            .add(&jar_with_descriptor(
                &src,
                "broken-too.jar",
                "{ not json either",
            ))
            .unwrap();
        let mut profile = ModProfile {
            name: "default".into(),
            mods: vec![
                ProfileMod {
                    sha1: old.clone(),
                    file_name: "sodium-0.9.1.jar".into(),
                    enabled: true,
                },
                ProfileMod {
                    sha1: lithium.clone(),
                    file_name: "lithium-0.25.3.jar".into(),
                    enabled: false,
                },
                ProfileMod {
                    sha1: junk.clone(),
                    file_name: "not-a-mod.jar".into(),
                    enabled: true,
                },
                ProfileMod {
                    sha1: broken.clone(),
                    file_name: "broken.jar".into(),
                    enabled: true,
                },
            ],
        };

        let ids = mod_ids(&store, &profile);
        assert_eq!(ids.get(&old).map(String::as_str), Some("sodium"));
        assert_eq!(ids.get(&lithium).map(String::as_str), Some("lithium"));
        assert!(!ids.contains_key(&junk), "unreadable jars have no id");
        assert!(
            !ids.contains_key(&broken),
            "a malformed descriptor yields no id"
        );
        assert!(
            supersede(&mut profile, &ids, &broken_too, "").is_empty(),
            "an unknown id supersedes nothing"
        );

        let dropped = supersede(&mut profile, &ids, &new, "sodium");
        assert_eq!(dropped, ["sodium-0.9.1.jar"]);
        let left: Vec<&str> = profile.mods.iter().map(|m| m.file_name.as_str()).collect();
        assert_eq!(left, ["lithium-0.25.3.jar", "not-a-mod.jar", "broken.jar"]);
        assert!(
            store.contains(&old),
            "superseding never deletes from the store"
        );

        // Nothing else claims to be lithium, and the same hash is never dropped.
        assert!(supersede(&mut profile, &ids, &lithium, "lithium").is_empty());
        assert_eq!(profile.mods.len(), 3);
    }

    fn write_jar(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn adding_the_same_jar_twice_stores_one_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let a = write_jar(&tmp.path().join("src"), "sodium.jar", b"jar bytes");
        let b = write_jar(&tmp.path().join("src2"), "sodium-copy.jar", b"jar bytes");

        let hash_a = store.add(&a).unwrap();
        let hash_b = store.add(&b).unwrap();
        assert_eq!(hash_a, hash_b, "identical content shares one hash");

        let count = std::fs::read_dir(tmp.path().join("mod-store"))
            .unwrap()
            .count();
        assert_eq!(count, 1, "deduplicated to a single stored jar");
    }

    #[test]
    fn apply_materializes_enabled_and_suffixes_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let src = tmp.path().join("src");
        let sodium = store
            .add(&write_jar(&src, "sodium.jar", b"sodium"))
            .unwrap();
        let lithium = store
            .add(&write_jar(&src, "lithium.jar", b"lithium"))
            .unwrap();

        let profile = ModProfile {
            name: "Performance".into(),
            mods: vec![
                ProfileMod {
                    sha1: sodium,
                    file_name: "sodium.jar".into(),
                    enabled: true,
                },
                ProfileMod {
                    sha1: lithium,
                    file_name: "lithium.jar".into(),
                    enabled: false,
                },
            ],
        };
        let mods_dir = tmp.path().join("instance/mods");
        let outcome = apply_profile(&store, &profile, &mods_dir).unwrap();

        assert!(mods_dir.join("sodium.jar").is_file());
        assert!(mods_dir.join("lithium.jar.disabled").is_file());
        assert!(!mods_dir.join("lithium.jar").exists());
        assert_eq!(outcome.disabled, 1);
        assert_eq!(outcome.linked + outcome.copied, 2);
    }

    #[test]
    fn switching_profiles_swaps_files_without_touching_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let src = tmp.path().join("src");
        let sodium = store
            .add(&write_jar(&src, "sodium.jar", b"sodium"))
            .unwrap();
        let create = store
            .add(&write_jar(&src, "create.jar", b"create"))
            .unwrap();
        let mods_dir = tmp.path().join("instance/mods");

        let performance = ModProfile {
            name: "Performance".into(),
            mods: vec![ProfileMod {
                sha1: sodium.clone(),
                file_name: "sodium.jar".into(),
                enabled: true,
            }],
        };
        let building = ModProfile {
            name: "Building".into(),
            mods: vec![ProfileMod {
                sha1: create.clone(),
                file_name: "create.jar".into(),
                enabled: true,
            }],
        };

        apply_profile(&store, &performance, &mods_dir).unwrap();
        assert!(mods_dir.join("sodium.jar").is_file());

        let outcome = apply_profile(&store, &building, &mods_dir).unwrap();
        assert!(mods_dir.join("create.jar").is_file());
        assert!(
            !mods_dir.join("sodium.jar").exists(),
            "old profile unlinked"
        );
        assert_eq!(outcome.removed, 1);

        // Crucially, the store still has both: switching back is free.
        assert!(store.contains(&sodium));
        assert!(store.contains(&create));
        apply_profile(&store, &performance, &mods_dir).unwrap();
        assert!(mods_dir.join("sodium.jar").is_file());
    }

    #[test]
    fn applying_twice_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let sha1 = store
            .add(&write_jar(&tmp.path().join("src"), "a.jar", b"a"))
            .unwrap();
        let profile = ModProfile {
            name: "P".into(),
            mods: vec![ProfileMod {
                sha1,
                file_name: "a.jar".into(),
                enabled: true,
            }],
        };
        let mods_dir = tmp.path().join("mods");
        apply_profile(&store, &profile, &mods_dir).unwrap();
        let second = apply_profile(&store, &profile, &mods_dir).unwrap();
        assert_eq!(second.linked + second.copied, 0, "nothing to redo");
        assert_eq!(second.removed, 0);
    }

    #[test]
    fn a_missing_store_entry_is_a_clear_error() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let profile = ModProfile {
            name: "P".into(),
            mods: vec![ProfileMod {
                sha1: "0000000000000000000000000000000000000000".into(),
                file_name: "ghost.jar".into(),
                enabled: true,
            }],
        };
        let err = apply_profile(&store, &profile, &tmp.path().join("mods")).unwrap_err();
        assert!(err.to_string().contains("ghost.jar"));
    }

    #[test]
    fn import_adopts_existing_mods_including_disabled_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let mods_dir = tmp.path().join("instance/mods");
        write_jar(&mods_dir, "sodium.jar", b"sodium");
        write_jar(&mods_dir, "lithium.jar.disabled", b"lithium");
        write_jar(&mods_dir, "notes.txt", b"ignore me");

        let imported = import_existing(&store, &mods_dir).unwrap();
        assert_eq!(imported.len(), 2, "only jars are imported");
        let lithium = imported
            .iter()
            .find(|m| m.file_name == "lithium.jar")
            .unwrap();
        assert!(!lithium.enabled, "disabled state is preserved on import");
        assert!(store.contains(&lithium.sha1));
    }

    #[test]
    fn profiles_round_trip_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let instance = tmp.path().join("instance");
        let mut set = ProfileSet::default();
        set.profiles.insert(
            "shaders".into(),
            ModProfile {
                name: "Shaders".into(),
                mods: vec![ProfileMod {
                    sha1: "abc".into(),
                    file_name: "iris.jar".into(),
                    enabled: true,
                }],
            },
        );
        set.active = "shaders".into();
        save_profiles(&instance, &set).unwrap();

        let loaded = load_profiles(&instance).unwrap();
        assert_eq!(loaded.active, "shaders");
        assert_eq!(loaded.profiles["shaders"].mods[0].file_name, "iris.jar");
    }

    #[test]
    fn missing_profile_file_yields_a_usable_default() {
        let tmp = tempfile::tempdir().unwrap();
        let set = load_profiles(tmp.path()).unwrap();
        assert_eq!(set.active, "default");
        assert!(set.profiles.contains_key("default"));
    }

    #[test]
    fn prune_removes_only_unreferenced_jars() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModStore::new(tmp.path().join("mod-store"));
        let src = tmp.path().join("src");
        let keep = store.add(&write_jar(&src, "keep.jar", b"keep")).unwrap();
        let drop = store.add(&write_jar(&src, "drop.jar", b"drop")).unwrap();

        let removed = store.prune(std::slice::from_ref(&keep)).unwrap();
        assert_eq!(removed, vec![drop.clone()]);
        assert!(store.contains(&keep));
        assert!(!store.contains(&drop));
    }
}
