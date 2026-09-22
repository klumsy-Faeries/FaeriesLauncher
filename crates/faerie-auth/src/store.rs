//! Account storage (§18, §34).
//!
//! Split by sensitivity: non-secret metadata (name, uuid, which account is
//! active) goes in `config/accounts.json` so it is inspectable; the refresh
//! and access tokens go to the OS credential manager (Windows Credential
//! Manager / macOS Keychain / Secret Service) keyed by account uuid.
//!
//! A keyring failure is never fatal — the launcher keeps working, the user is
//! simply asked to sign in again.
//!
//! Windows Credential Manager caps one credential blob at 2560 bytes, and the
//! keyring stores passwords as UTF-16, so a single entry holds at most 1280
//! characters — less than one Minecraft access token (a JWT of ~2 KB). The
//! secret blob is therefore split across numbered entries:
//!
//! - `<id>`     → the piece count, as a small decimal number
//! - `<id>/<n>` → the pieces, in order
//!
//! macOS and Linux stores have no such cap but use the same layout so there
//! is one code path, and one set of tests, for all three.

use std::path::{Path, PathBuf};

use faerie_core::Secret;
use serde::{Deserialize, Serialize};

use crate::flow::{now_secs, Account, MinecraftProfile};
use crate::AuthError;

const KEYRING_SERVICE: &str = "dev.faerie.launcher";

/// Non-secret account metadata, safe to write to disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub xuid: String,
    /// When the cached Minecraft token expires (unix seconds).
    #[serde(default)]
    pub expires_at_secs: u64,
}

impl AccountRecord {
    pub fn is_expired(&self) -> bool {
        // 60s of slack so we refresh before a launch rather than during.
        self.expires_at_secs <= now_secs() + 60
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AccountsFile {
    accounts: Vec<AccountRecord>,
    active: Option<String>,
}

/// Secrets held for one account.
#[derive(Debug, Serialize, Deserialize)]
struct StoredSecrets {
    minecraft_token: String,
    refresh_token: String,
}

/// Characters per keyring entry: comfortably under the 1280 that fit in a
/// Windows credential, even if a token ever contains non-ASCII.
const CHUNK_CHARS: usize = 1000;

pub struct AccountStore {
    path: PathBuf,
    slots: Box<dyn Slots>,
}

impl AccountStore {
    /// `config_dir` is the launcher's `config/` directory.
    pub fn new(config_dir: &Path) -> Self {
        Self::with_slots(config_dir, Box::new(Keyring))
    }

    fn with_slots(config_dir: &Path, slots: Box<dyn Slots>) -> Self {
        Self {
            path: config_dir.join("accounts.json"),
            slots,
        }
    }

    pub fn list(&self) -> Result<Vec<AccountRecord>, AuthError> {
        Ok(self.read()?.accounts)
    }

    pub fn active_id(&self) -> Result<Option<String>, AuthError> {
        Ok(self.read()?.active)
    }

    pub fn active(&self) -> Result<Option<AccountRecord>, AuthError> {
        let file = self.read()?;
        Ok(file
            .active
            .and_then(|id| file.accounts.into_iter().find(|a| a.id == id)))
    }

    /// Persist an account: metadata to JSON, tokens to the OS keyring. The
    /// newly added account becomes active.
    pub fn upsert(&self, account: &Account) -> Result<AccountRecord, AuthError> {
        let record = AccountRecord {
            id: account.profile.id.clone(),
            name: account.profile.name.clone(),
            xuid: account.profile.xuid.clone(),
            expires_at_secs: account.expires_at_secs,
        };

        let secrets = StoredSecrets {
            minecraft_token: account.minecraft_token.expose().clone(),
            refresh_token: account.refresh_token.expose().clone(),
        };
        let blob = serde_json::to_string(&secrets).expect("secrets serialize");
        write_secret(self.slots.as_ref(), &record.id, &blob)?;

        let mut file = self.read()?;
        match file.accounts.iter_mut().find(|a| a.id == record.id) {
            Some(existing) => *existing = record.clone(),
            None => file.accounts.push(record.clone()),
        }
        file.active = Some(record.id.clone());
        self.write(&file)?;
        Ok(record)
    }

    /// Load an account's tokens from the keyring.
    pub fn secrets_for(&self, id: &str) -> Result<(Secret<String>, Secret<String>), AuthError> {
        let blob = read_secret(self.slots.as_ref(), id)?.ok_or_else(|| {
            AuthError::Keyring("no stored tokens for this account; sign in again".into())
        })?;
        let secrets: StoredSecrets =
            serde_json::from_str(&blob).map_err(|e| AuthError::Keyring(e.to_string()))?;
        Ok((
            Secret::new(secrets.minecraft_token),
            Secret::new(secrets.refresh_token),
        ))
    }

    pub fn set_active(&self, id: &str) -> Result<(), AuthError> {
        let mut file = self.read()?;
        if !file.accounts.iter().any(|a| a.id == id) {
            return Err(AuthError::StoreUnreadable {
                path: self.path.clone(),
                reason: format!("no account with id {id}"),
            });
        }
        file.active = Some(id.to_string());
        self.write(&file)
    }

    /// Remove an account and its stored tokens.
    pub fn remove(&self, id: &str) -> Result<(), AuthError> {
        // Best effort: a missing keyring entry must not block removal.
        delete_secret(self.slots.as_ref(), id);
        let mut file = self.read()?;
        file.accounts.retain(|a| a.id != id);
        if file.active.as_deref() == Some(id) {
            file.active = file.accounts.first().map(|a| a.id.clone());
        }
        self.write(&file)
    }

    fn read(&self) -> Result<AccountsFile, AuthError> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(|e| AuthError::StoreUnreadable {
                path: self.path.clone(),
                reason: e.to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AccountsFile::default()),
            Err(e) => Err(AuthError::StoreUnreadable {
                path: self.path.clone(),
                reason: e.to_string(),
            }),
        }
    }

    fn write(&self, file: &AccountsFile) -> Result<(), AuthError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AuthError::StoreUnreadable {
                path: parent.to_path_buf(),
                reason: e.to_string(),
            })?;
        }
        let body = serde_json::to_string_pretty(file).expect("accounts file serializes");
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, body).map_err(|e| AuthError::StoreUnreadable {
            path: tmp.clone(),
            reason: e.to_string(),
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|e| AuthError::StoreUnreadable {
            path: self.path.clone(),
            reason: e.to_string(),
        })
    }
}

/// One password slot per user name: the OS keyring, or a map in tests.
trait Slots: Send + Sync {
    fn get(&self, user: &str) -> Result<Option<String>, AuthError>;
    fn set(&self, user: &str, value: &str) -> Result<(), AuthError>;
    /// Best effort; a missing slot is not an error.
    fn delete(&self, user: &str);
}

struct Keyring;

impl Slots for Keyring {
    fn get(&self, user: &str) -> Result<Option<String>, AuthError> {
        match keyring_entry(user)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn set(&self, user: &str, value: &str) -> Result<(), AuthError> {
        keyring_entry(user)?
            .set_password(value)
            .map_err(keyring_error)
    }

    fn delete(&self, user: &str) {
        if let Ok(entry) = keyring_entry(user) {
            let _ = entry.delete_credential();
        }
    }
}

fn keyring_entry(user: &str) -> Result<keyring::Entry, AuthError> {
    keyring::Entry::new(KEYRING_SERVICE, user).map_err(keyring_error)
}

fn keyring_error(e: keyring::Error) -> AuthError {
    AuthError::Keyring(e.to_string())
}

fn piece_slot(id: &str, index: usize) -> String {
    format!("{id}/{index}")
}

fn piece_count(slots: &dyn Slots, id: &str) -> Result<Option<usize>, AuthError> {
    match slots.get(id)? {
        None => Ok(None),
        Some(raw) => raw.trim().parse().map(Some).map_err(|_| {
            AuthError::Keyring("stored tokens are in an unrecognised layout; sign in again".into())
        }),
    }
}

/// Store `blob` for `id`, replacing anything previously stored.
fn write_secret(slots: &dyn Slots, id: &str, blob: &str) -> Result<(), AuthError> {
    // Clear stale pieces first so a shorter blob leaves no orphans behind.
    delete_secret(slots, id);
    let pieces: Vec<String> = blob
        .chars()
        .collect::<Vec<_>>()
        .chunks(CHUNK_CHARS)
        .map(|piece| piece.iter().collect())
        .collect();
    for (index, piece) in pieces.iter().enumerate() {
        slots.set(&piece_slot(id, index), piece)?;
    }
    // The count goes last: until it exists the pieces are invisible, so an
    // interrupted write reads as "nothing stored" rather than as garbage.
    slots.set(id, &pieces.len().to_string())
}

fn read_secret(slots: &dyn Slots, id: &str) -> Result<Option<String>, AuthError> {
    let Some(count) = piece_count(slots, id)? else {
        return Ok(None);
    };
    let mut blob = String::new();
    for index in 0..count {
        let piece = slots.get(&piece_slot(id, index))?.ok_or_else(|| {
            AuthError::Keyring(format!(
                "stored tokens are incomplete (piece {index} of {count} is missing); sign in again"
            ))
        })?;
        blob.push_str(&piece);
    }
    Ok(Some(blob))
}

/// Best effort: removes the pieces and the count. An index that is not a
/// number (the pre-chunking layout) is simply discarded.
fn delete_secret(slots: &dyn Slots, id: &str) {
    let count = piece_count(slots, id).ok().flatten().unwrap_or(0);
    for index in 0..count {
        slots.delete(&piece_slot(id, index));
    }
    slots.delete(id);
}

/// Build an [`Account`] from a stored record plus keyring secrets.
pub fn account_from_parts(
    record: &AccountRecord,
    minecraft_token: Secret<String>,
    refresh_token: Secret<String>,
) -> Account {
    Account {
        profile: MinecraftProfile {
            id: record.id.clone(),
            name: record.name.clone(),
            xuid: record.xuid.clone(),
        },
        minecraft_token,
        expires_at_secs: record.expires_at_secs,
        refresh_token,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct MemorySlots(Mutex<HashMap<String, String>>);

    impl Slots for MemorySlots {
        fn get(&self, user: &str) -> Result<Option<String>, AuthError> {
            Ok(self.0.lock().unwrap().get(user).cloned())
        }
        fn set(&self, user: &str, value: &str) -> Result<(), AuthError> {
            // Mirror the Windows limit so a regression fails here, not on a
            // user's machine: 2560 bytes of UTF-16.
            let bytes = value.encode_utf16().count() * 2;
            assert!(
                bytes <= 2560,
                "slot {user} holds {bytes} UTF-16 bytes, over the limit"
            );
            self.0.lock().unwrap().insert(user.into(), value.into());
            Ok(())
        }
        fn delete(&self, user: &str) {
            self.0.lock().unwrap().remove(user);
        }
    }

    /// Lets a test look at the slots behind a store.
    struct Shared(Arc<MemorySlots>);

    impl Slots for Shared {
        fn get(&self, user: &str) -> Result<Option<String>, AuthError> {
            self.0.get(user)
        }
        fn set(&self, user: &str, value: &str) -> Result<(), AuthError> {
            self.0.set(user, value)
        }
        fn delete(&self, user: &str) {
            self.0.delete(user)
        }
    }

    fn memory_store(dir: &Path) -> (AccountStore, Arc<MemorySlots>) {
        let slots = Arc::new(MemorySlots::default());
        let store = AccountStore::with_slots(dir, Box::new(Shared(slots.clone())));
        (store, slots)
    }

    fn account(id: &str, token_len: usize) -> Account {
        Account {
            profile: MinecraftProfile {
                id: id.into(),
                name: "Faerie".into(),
                xuid: "555".into(),
            },
            minecraft_token: Secret::new("m".repeat(token_len)),
            expires_at_secs: now_secs() + 3600,
            refresh_token: Secret::new("r".repeat(token_len)),
        }
    }

    #[test]
    fn tokens_larger_than_one_credential_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, slots) = memory_store(tmp.path());

        // Real tokens are ~2 KB each; together they are far over the
        // 1280-character Windows limit that broke the first live sign-in.
        let saved = account("uuid-big", 2200);
        store.upsert(&saved).unwrap();

        let (mc, refresh) = store.secrets_for("uuid-big").unwrap();
        assert_eq!(mc.expose(), saved.minecraft_token.expose());
        assert_eq!(refresh.expose(), saved.refresh_token.expose());

        // 4400 token chars plus JSON framing: five pieces, the index says
        // so, and none of them is over the chunk size.
        assert_eq!(slots.get("uuid-big").unwrap().as_deref(), Some("5"));
        for index in 0..5 {
            let piece = slots.get(&piece_slot("uuid-big", index)).unwrap().unwrap();
            assert!(piece.chars().count() <= CHUNK_CHARS);
        }
        assert!(slots.get(&piece_slot("uuid-big", 5)).unwrap().is_none());
    }

    #[test]
    fn removing_an_account_removes_every_piece() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, slots) = memory_store(tmp.path());
        store.upsert(&account("uuid-gone", 2200)).unwrap();
        store.remove("uuid-gone").unwrap();

        assert!(slots.0.lock().unwrap().is_empty(), "pieces left behind");
        assert!(matches!(
            store.secrets_for("uuid-gone"),
            Err(AuthError::Keyring(_))
        ));
    }

    #[test]
    fn rewriting_a_shorter_secret_leaves_no_orphaned_pieces() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, slots) = memory_store(tmp.path());
        store.upsert(&account("uuid-shrink", 2200)).unwrap(); // five pieces
        store.upsert(&account("uuid-shrink", 10)).unwrap(); // one piece

        assert_eq!(slots.get("uuid-shrink").unwrap().as_deref(), Some("1"));
        assert!(slots.get(&piece_slot("uuid-shrink", 1)).unwrap().is_none());
        let (mc, _) = store.secrets_for("uuid-shrink").unwrap();
        assert_eq!(mc.expose(), &"m".repeat(10));
    }

    #[test]
    fn pre_chunking_entry_is_reported_and_replaced_on_next_sign_in() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, slots) = memory_store(tmp.path());
        // The layout before chunking: the JSON blob in the index slot.
        slots
            .set("uuid-old", "{\"minecraft_token\":\"x\"}")
            .unwrap();

        assert!(matches!(
            store.secrets_for("uuid-old"),
            Err(AuthError::Keyring(_))
        ));
        store.upsert(&account("uuid-old", 10)).unwrap();
        assert!(store.secrets_for("uuid-old").is_ok());
    }

    #[test]
    fn empty_store_reads_as_no_accounts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path());
        assert!(store.list().unwrap().is_empty());
        assert!(store.active().unwrap().is_none());
    }

    #[test]
    fn accounts_file_never_contains_tokens() {
        // Writing the metadata file must not include secret material, even
        // though upsert() also writes to the keyring.
        let file = AccountsFile {
            accounts: vec![AccountRecord {
                id: "uuid-1".into(),
                name: "Faerie".into(),
                xuid: "555".into(),
                expires_at_secs: 42,
            }],
            active: Some("uuid-1".into()),
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("Faerie"));
        assert!(!json.to_lowercase().contains("token"));
    }

    #[test]
    fn expiry_uses_a_refresh_margin() {
        let fresh = AccountRecord {
            id: "a".into(),
            name: "n".into(),
            xuid: String::new(),
            expires_at_secs: now_secs() + 3600,
        };
        let stale = AccountRecord {
            expires_at_secs: now_secs() + 10, // inside the 60s margin
            ..fresh.clone()
        };
        assert!(!fresh.is_expired());
        assert!(stale.is_expired());
    }

    #[test]
    fn corrupt_accounts_file_is_reported_not_swallowed() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("accounts.json"), "{ broken").unwrap();
        let store = AccountStore::new(tmp.path());
        assert!(matches!(
            store.list(),
            Err(AuthError::StoreUnreadable { .. })
        ));
    }
}
