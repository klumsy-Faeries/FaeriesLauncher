//! Instance system (§15): every Minecraft installation is an independent
//! workspace under `instances/<id>/` with its own `instance.json`.
//!
//! Design decisions:
//! - the folder name is the instance's stable id; *rename* changes only the
//!   display name, so nothing that references the folder ever breaks;
//! - *delete* moves the folder into `instances/.trash/<id>.<timestamp>/`
//!   instead of destroying it (§13/§41: no silent, irreversible data loss);
//! - `instance.json` carries a `format` number so future launchers can
//!   migrate old instances instead of rejecting them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const INSTANCE_FILE: &str = "instance.json";
pub const CURRENT_FORMAT: u32 = 1;
const TRASH_DIR: &str = ".trash";

/// Directories every instance starts with (§11).
pub mod nbt;
pub mod options;
pub mod servers;

const INSTANCE_SUBDIRS: [&str; 7] = [
    "mods",
    "config",
    "saves",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "logs",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoaderRef {
    /// `fabric`, `quilt`, `forge`, `neoforge`, …
    pub kind: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct JavaSettings {
    /// Explicit java executable; `None` means "let the launcher pick".
    pub path_override: Option<PathBuf>,
    pub min_ram_mb: Option<u32>,
    pub max_ram_mb: Option<u32>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub format: u32,
    pub name: String,
    pub minecraft_version: String,
    #[serde(default)]
    pub loader: Option<LoaderRef>,
    #[serde(default)]
    pub java: JavaSettings,
    pub created_at_secs: u64,
    #[serde(default)]
    pub last_played_at_secs: Option<u64>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub dir: PathBuf,
    #[serde(flatten)]
    pub config: InstanceConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("no instance with id `{0}` exists")]
    NotFound(String),

    #[error("an instance name cannot be empty")]
    EmptyName,

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("instance file {path} could not be parsed: {reason}")]
    Corrupt { path: PathBuf, reason: String },
}

fn io_at(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> InstanceError {
    let path = path.into();
    move |source| InstanceError::Io { path, source }
}

/// CRUD over the instances directory. Stateless: the filesystem is the
/// source of truth, so external edits are picked up on the next list.
#[derive(Clone)]
pub struct InstanceStore {
    root: PathBuf,
}

impl InstanceStore {
    pub fn new(instances_dir: PathBuf) -> Self {
        Self {
            root: instances_dir,
        }
    }

    /// Every readable instance, sorted by creation time (newest first).
    /// Unreadable ones are reported separately, never silently dropped.
    pub fn list(&self) -> (Vec<Instance>, Vec<String>) {
        let mut instances = Vec::new();
        let mut problems = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return (instances, problems);
        };
        for entry in entries.flatten() {
            // `file_type` comes from the directory iteration itself, so it
            // costs no extra syscall — unlike `path.is_dir()`, which stats.
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir || entry.file_name() == TRASH_DIR {
                continue;
            }
            // Read straight away rather than stat-ing first: a missing
            // instance.json simply means "not an instance folder", which the
            // read tells us for free.
            match self.read_instance(&entry.path()) {
                Ok(Some(instance)) => instances.push(instance),
                Ok(None) => {}
                Err(e) => problems.push(e.to_string()),
            }
        }
        instances.sort_by_key(|i| std::cmp::Reverse(i.config.created_at_secs));
        (instances, problems)
    }

    pub fn get(&self, id: &str) -> Result<Instance, InstanceError> {
        let dir = self.dir_of(id);
        self.read_instance(&dir)?
            .ok_or_else(|| InstanceError::NotFound(id.to_string()))
    }

    pub fn create(&self, name: &str, minecraft_version: &str) -> Result<Instance, InstanceError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(InstanceError::EmptyName);
        }
        let id = self.unique_id(name);
        let dir = self.dir_of(&id);
        std::fs::create_dir_all(&dir).map_err(io_at(&dir))?;
        for sub in INSTANCE_SUBDIRS {
            std::fs::create_dir_all(dir.join(sub)).map_err(io_at(dir.join(sub)))?;
        }
        let config = InstanceConfig {
            format: CURRENT_FORMAT,
            name: name.to_string(),
            minecraft_version: minecraft_version.to_string(),
            loader: None,
            java: JavaSettings::default(),
            created_at_secs: now_secs(),
            last_played_at_secs: None,
            notes: String::new(),
        };
        let instance = Instance { id, dir, config };
        self.save(&instance)?;
        Ok(instance)
    }

    /// Change the display name. The folder (id) stays stable on purpose.
    pub fn rename(&self, id: &str, new_name: &str) -> Result<Instance, InstanceError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(InstanceError::EmptyName);
        }
        let mut instance = self.get(id)?;
        instance.config.name = new_name.to_string();
        self.save(&instance)?;
        Ok(instance)
    }

    /// Full copy under a new id: mods, configs, worlds — everything.
    pub fn duplicate(&self, id: &str, new_name: &str) -> Result<Instance, InstanceError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(InstanceError::EmptyName);
        }
        let source = self.get(id)?;
        let new_id = self.unique_id(new_name);
        let dest = self.dir_of(&new_id);
        copy_dir(&source.dir, &dest)?;
        let mut copy = Instance {
            id: new_id,
            dir: dest,
            config: source.config,
        };
        copy.config.name = new_name.to_string();
        copy.config.created_at_secs = now_secs();
        copy.config.last_played_at_secs = None;
        self.save(&copy)?;
        Ok(copy)
    }

    /// Move the instance into `.trash/<id>.<timestamp>` — recoverable by
    /// hand until the user empties it. Returns the trash location.
    pub fn delete(&self, id: &str) -> Result<PathBuf, InstanceError> {
        let instance = self.get(id)?;
        let trash_root = self.root.join(TRASH_DIR);
        std::fs::create_dir_all(&trash_root).map_err(io_at(&trash_root))?;
        let target = trash_root.join(format!("{id}.{}", now_secs()));
        std::fs::rename(&instance.dir, &target).map_err(io_at(&instance.dir))?;
        tracing::info!("instance {id} moved to trash at {}", target.display());
        Ok(target)
    }

    /// Persist an instance's configuration (atomic write).
    pub fn save(&self, instance: &Instance) -> Result<(), InstanceError> {
        let path = instance.dir.join(INSTANCE_FILE);
        let tmp = instance.dir.join(format!("{INSTANCE_FILE}.tmp"));
        let body = serde_json::to_string_pretty(&instance.config).expect("config serializes");
        std::fs::write(&tmp, body).map_err(io_at(&tmp))?;
        std::fs::rename(&tmp, &path).map_err(io_at(&path))?;
        Ok(())
    }

    /// Update the stored config through a closure and persist the result.
    pub fn update<F>(&self, id: &str, mutate: F) -> Result<Instance, InstanceError>
    where
        F: FnOnce(&mut InstanceConfig),
    {
        let mut instance = self.get(id)?;
        mutate(&mut instance.config);
        self.save(&instance)?;
        Ok(instance)
    }

    fn dir_of(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// `Ok(None)` when the folder holds no `instance.json` — that is a
    /// plain directory the user put here, not a fault to report.
    fn read_instance(&self, dir: &Path) -> Result<Option<Instance>, InstanceError> {
        let path = dir.join(INSTANCE_FILE);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_at(&path)(e)),
        };
        let config: InstanceConfig =
            serde_json::from_str(&raw).map_err(|e| InstanceError::Corrupt {
                path: path.clone(),
                reason: e.to_string(),
            })?;
        Ok(Some(Instance {
            id: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dir: dir.to_path_buf(),
            config,
        }))
    }

    /// Derive a filesystem-safe unique id from a display name.
    fn unique_id(&self, name: &str) -> String {
        let base = slugify(name);
        let existing: HashSet<String> = std::fs::read_dir(&self.root)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        if !existing.contains(&base) {
            return base;
        }
        for n in 2.. {
            let candidate = format!("{base}-{n}");
            if !existing.contains(&candidate) {
                return candidate;
            }
        }
        unreachable!("the counter loop always finds a free id")
    }
}

fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_dash = true; // suppress leading dashes
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "instance".to_string()
    } else {
        slug
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), InstanceError> {
    std::fs::create_dir_all(to).map_err(io_at(to))?;
    for entry in std::fs::read_dir(from).map_err(io_at(from))?.flatten() {
        let source = entry.path();
        let dest = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &dest)?;
        } else {
            std::fs::copy(&source, &dest).map_err(io_at(&source))?;
        }
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, InstanceStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path().join("instances"));
        std::fs::create_dir_all(tmp.path().join("instances")).unwrap();
        (tmp, store)
    }

    #[test]
    fn create_builds_the_standard_layout_and_lists() {
        let (_tmp, store) = store();
        let created = store.create("Fabric 26.2", "26.2").unwrap();
        assert_eq!(created.id, "fabric-26-2");
        for sub in INSTANCE_SUBDIRS {
            assert!(created.dir.join(sub).is_dir(), "missing {sub}");
        }

        let (instances, problems) = store.list();
        assert!(problems.is_empty());
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].config.name, "Fabric 26.2");
        assert_eq!(instances[0].config.minecraft_version, "26.2");
    }

    #[test]
    fn duplicate_names_get_distinct_ids() {
        let (_tmp, store) = store();
        let a = store.create("My Pack", "26.2").unwrap();
        let b = store.create("My Pack", "26.2").unwrap();
        let c = store.create("My Pack", "26.2").unwrap();
        assert_eq!(a.id, "my-pack");
        assert_eq!(b.id, "my-pack-2");
        assert_eq!(c.id, "my-pack-3");
    }

    #[test]
    fn rename_changes_display_name_but_not_the_folder() {
        let (_tmp, store) = store();
        let created = store.create("Old Name", "26.2").unwrap();
        let renamed = store.rename(&created.id, "New Name").unwrap();
        assert_eq!(renamed.id, "old-name");
        assert_eq!(renamed.config.name, "New Name");
        assert!(store.get("old-name").is_ok());
    }

    #[test]
    fn duplicate_copies_files_and_resets_history() {
        let (_tmp, store) = store();
        let original = store.create("Base", "26.2").unwrap();
        std::fs::write(original.dir.join("mods").join("sodium.jar"), b"jar bytes").unwrap();
        std::fs::write(original.dir.join("options.txt"), b"fov:90").unwrap();

        let copy = store.duplicate(&original.id, "Base Copy").unwrap();
        assert_eq!(copy.id, "base-copy");
        assert_eq!(
            std::fs::read(copy.dir.join("mods").join("sodium.jar")).unwrap(),
            b"jar bytes"
        );
        assert_eq!(
            std::fs::read(copy.dir.join("options.txt")).unwrap(),
            b"fov:90"
        );
        assert_eq!(copy.config.last_played_at_secs, None);
    }

    #[test]
    fn delete_moves_to_trash_not_oblivion() {
        let (_tmp, store) = store();
        let created = store.create("Doomed", "26.2").unwrap();
        std::fs::write(created.dir.join("saves").join("keep-me"), b"world").unwrap();

        let trash = store.delete(&created.id).unwrap();
        assert!(store.get(&created.id).is_err());
        assert!(
            trash.join("saves").join("keep-me").is_file(),
            "data survives in trash"
        );

        let (instances, _) = store.list();
        assert!(
            instances.is_empty(),
            "trash must not show up as an instance"
        );
    }

    #[test]
    fn update_persists_jvm_settings() {
        let (_tmp, store) = store();
        let created = store.create("Tuned", "26.2").unwrap();
        store
            .update(&created.id, |config| {
                config.java.max_ram_mb = Some(6144);
                config.java.extra_args = vec!["-XX:+UseG1GC".into()];
            })
            .unwrap();
        let reloaded = store.get(&created.id).unwrap();
        assert_eq!(reloaded.config.java.max_ram_mb, Some(6144));
        assert_eq!(
            reloaded.config.java.extra_args,
            vec!["-XX:+UseG1GC".to_string()]
        );
    }

    #[test]
    fn broken_instance_is_reported_not_hidden() {
        let (_tmp, store) = store();
        store.create("Good", "26.2").unwrap();
        let bad_dir = store.root.join("bad");
        std::fs::create_dir_all(&bad_dir).unwrap();
        std::fs::write(bad_dir.join(INSTANCE_FILE), "{ nope").unwrap();

        let (instances, problems) = store.list();
        assert_eq!(instances.len(), 1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("could not be parsed"));
    }

    #[test]
    fn empty_names_are_rejected_everywhere() {
        let (_tmp, store) = store();
        assert!(matches!(
            store.create("  ", "26.2"),
            Err(InstanceError::EmptyName)
        ));
        let created = store.create("Ok", "26.2").unwrap();
        assert!(matches!(
            store.rename(&created.id, ""),
            Err(InstanceError::EmptyName)
        ));
        assert!(matches!(
            store.duplicate(&created.id, " "),
            Err(InstanceError::EmptyName)
        ));
    }

    #[test]
    fn slugify_handles_awkward_names() {
        assert_eq!(slugify("Fabric 26.2"), "fabric-26-2");
        assert_eq!(slugify("  ✨ Sparkle Pack!! "), "sparkle-pack");
        assert_eq!(slugify("日本語"), "instance");
        assert_eq!(slugify("---"), "instance");
    }
}
