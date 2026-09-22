//! The resource-pack vault shared with the Faeries Pack Vault mod.
//!
//! Layout (see `FORMAT.md` in the mod's repository):
//!
//! ```text
//! <vault>/
//!   packs/<sha1-hex>.zip   content-addressed pack blobs
//!   index.json             metadata + settings
//! ```
//!
//! The mod serves a blob only when a server announces the same SHA-1, so
//! seeding the vault with a pack makes the first join instant — as long as
//! the server still announces that exact build. Fields the launcher does not
//! know are carried through `index.json` untouched; blobs are the source of
//! truth and the index is replaced atomically.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ModError;

const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub max_bytes: u64,
    pub eviction: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            eviction: "lru".into(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub sha1: String,
    pub size: u64,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: 1,
            settings: Settings::default(),
            entries: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// What seeding did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seeded {
    pub sha1: String,
    /// The blob was written now (false: it was already there and verified).
    pub blob_written: bool,
}

pub fn pack_path(dir: &Path, sha1: &str) -> PathBuf {
    dir.join("packs")
        .join(format!("{}.zip", sha1.to_ascii_lowercase()))
}

/// Store `bytes` as a pack blob and record it in the index. `url`, `server`
/// and `display_name` are merged into an existing entry for the same hash.
pub fn seed(
    dir: &Path,
    bytes: &[u8],
    url: Option<&str>,
    server: Option<&str>,
    display_name: Option<&str>,
) -> Result<Seeded, ModError> {
    let sha1 = faerie_net::hash::sha1_of_bytes(bytes);
    let blob = pack_path(dir, &sha1);
    let packs_dir = blob.parent().expect("packs dir");
    std::fs::create_dir_all(packs_dir).map_err(|e| ModError::io(packs_dir, e))?;

    // A blob that exists and matches its name is kept; anything else is
    // (re)written through a temp file so a crash never leaves a half blob.
    let blob_written = match faerie_net::hash::sha1_of_file(&blob) {
        Ok(existing) if existing.eq_ignore_ascii_case(&sha1) => false,
        _ => {
            let tmp = blob.with_extension("zip.tmp");
            std::fs::write(&tmp, bytes).map_err(|e| ModError::io(&tmp, e))?;
            std::fs::rename(&tmp, &blob).map_err(|e| ModError::io(&blob, e))?;
            true
        }
    };

    let mut index = read_index(dir)?;
    let now = rfc3339_now();
    let entry = match index
        .entries
        .iter_mut()
        .find(|e| e.sha1.eq_ignore_ascii_case(&sha1))
    {
        Some(entry) => entry,
        None => {
            index.entries.push(Entry {
                sha1: sha1.clone(),
                size: 0,
                urls: Vec::new(),
                servers: Vec::new(),
                display_name: None,
                first_seen: Some(now.clone()),
                last_used: None,
                last_checked: None,
                extra: Map::new(),
            });
            index.entries.last_mut().expect("just pushed")
        }
    };
    entry.size = bytes.len() as u64;
    if let Some(url) = url {
        if !entry.urls.iter().any(|u| u == url) {
            entry.urls.push(url.to_string());
        }
    }
    if let Some(server) = server {
        if !entry.servers.iter().any(|s| s == server) {
            entry.servers.push(server.to_string());
        }
    }
    if display_name.is_some() {
        entry.display_name = display_name.map(str::to_string);
    }
    if entry.first_seen.is_none() {
        entry.first_seen = Some(now.clone());
    }
    entry.last_checked = Some(now.clone());
    entry.last_used.get_or_insert(now);

    write_index(dir, &index)?;
    Ok(Seeded { sha1, blob_written })
}

pub fn read_index(dir: &Path) -> Result<Index, ModError> {
    let path = dir.join("index.json");
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| ModError::VaultIndex {
            path: path.clone(),
            reason: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
        Err(e) => Err(ModError::io(&path, e)),
    }
}

fn write_index(dir: &Path, index: &Index) -> Result<(), ModError> {
    std::fs::create_dir_all(dir).map_err(|e| ModError::io(dir, e))?;
    let path = dir.join("index.json");
    let tmp = dir.join("index.json.tmp");
    let body = serde_json::to_string_pretty(index).expect("index serializes");
    std::fs::write(&tmp, body).map_err(|e| ModError::io(&tmp, e))?;
    std::fs::rename(&tmp, &path).map_err(|e| ModError::io(&path, e))
}

/// Current time as RFC 3339 UTC with second precision, e.g.
/// `2026-09-09T22:50:07Z`. Written without a date crate: the vault format
/// only needs something the mod's `Instant.parse` accepts.
pub fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339_from_unix(secs)
}

fn rfc3339_from_unix(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (Howard Hinnant), proleptic Gregorian, UTC.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_matches_known_instants() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        // 2026-09-09T22:50:07Z
        assert_eq!(rfc3339_from_unix(1_788_994_207), "2026-09-09T22:50:07Z");
        // Leap day.
        assert_eq!(rfc3339_from_unix(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn seeds_a_fresh_vault_with_blob_and_index() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = b"PK\x03\x04 pretend pack";
        let seeded = seed(
            tmp.path(),
            bytes,
            Some("https://hermes.example/pack/x"),
            Some("mc.example.com"),
            Some("Example pack"),
        )
        .unwrap();
        assert!(seeded.blob_written);
        assert_eq!(seeded.sha1, faerie_net::hash::sha1_of_bytes(bytes));
        assert_eq!(
            std::fs::read(pack_path(tmp.path(), &seeded.sha1)).unwrap(),
            bytes
        );

        let index = read_index(tmp.path()).unwrap();
        assert_eq!(index.version, 1);
        assert_eq!(index.settings.max_bytes, DEFAULT_MAX_BYTES);
        assert_eq!(index.entries.len(), 1);
        let entry = &index.entries[0];
        assert_eq!(entry.size, bytes.len() as u64);
        assert_eq!(entry.urls, ["https://hermes.example/pack/x"]);
        assert_eq!(entry.servers, ["mc.example.com"]);
        assert_eq!(entry.display_name.as_deref(), Some("Example pack"));
        assert!(entry.first_seen.as_deref().unwrap().ends_with('Z'));
        // The mod reads snake_case keys.
        let raw = std::fs::read_to_string(tmp.path().join("index.json")).unwrap();
        assert!(raw.contains("\"first_seen\"") && raw.contains("\"max_bytes\""));
    }

    #[test]
    fn merges_into_an_existing_index_without_losing_anything() {
        let tmp = tempfile::tempdir().unwrap();
        // An index the mod wrote: another pack, a custom budget, and a key
        // this launcher does not know about.
        std::fs::write(
            tmp.path().join("index.json"),
            r#"{"version":1,"settings":{"max_bytes":123,"eviction":"lru","future":true},
                "entries":[{"sha1":"aaaa","size":1,"urls":["u"],"servers":["s"],"note":"keep me"}]}"#,
        )
        .unwrap();
        let bytes = b"PK\x03\x04 another";
        seed(tmp.path(), bytes, None, Some("mc.example.com"), None).unwrap();

        let index = read_index(tmp.path()).unwrap();
        assert_eq!(index.settings.max_bytes, 123, "the mod's budget is kept");
        assert_eq!(index.settings.extra["future"], true);
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.entries[0].sha1, "aaaa");
        assert_eq!(index.entries[0].extra["note"], "keep me");
        assert_eq!(index.entries[1].servers, ["mc.example.com"]);
        assert!(index.entries[1].urls.is_empty());
    }

    #[test]
    fn reseeding_is_idempotent_and_keeps_a_verified_blob() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = b"PK\x03\x04 same";
        let first = seed(tmp.path(), bytes, Some("u1"), None, None).unwrap();
        let second = seed(tmp.path(), bytes, Some("u2"), Some("srv"), None).unwrap();
        assert!(first.blob_written && !second.blob_written);
        let index = read_index(tmp.path()).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].urls, ["u1", "u2"]);
        assert_eq!(index.entries[0].servers, ["srv"]);
    }

    #[test]
    fn a_corrupt_blob_is_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = b"PK\x03\x04 good";
        let sha1 = faerie_net::hash::sha1_of_bytes(bytes);
        std::fs::create_dir_all(tmp.path().join("packs")).unwrap();
        std::fs::write(pack_path(tmp.path(), &sha1), b"garbage").unwrap();
        let seeded = seed(tmp.path(), bytes, None, None, None).unwrap();
        assert!(seeded.blob_written);
        assert_eq!(std::fs::read(pack_path(tmp.path(), &sha1)).unwrap(), bytes);
    }
}
