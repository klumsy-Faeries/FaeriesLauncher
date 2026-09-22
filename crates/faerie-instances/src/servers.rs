//! The in-game multiplayer server list (`servers.dat`), so an instance can
//! start with a server already on it. Existing entries — including tags the
//! launcher knows nothing about, such as icons — are kept exactly.

use std::path::Path;

use crate::nbt::{self, Tag};
use crate::{io_at, InstanceError};

/// A server to add to an instance's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEntry {
    pub name: &'static str,
    pub address: &'static str,
    /// "Server Resource Packs: Enabled" — the game applies the server's pack
    /// without asking on every join.
    pub accept_packs: bool,
}

/// Add `server` to `<instance_dir>/servers.dat` unless an entry with the
/// same address is already there (the player's own settings for it win).
/// Creates the file when the game has not written one. Returns whether
/// anything changed.
pub fn ensure_server(instance_dir: &Path, server: &ServerEntry) -> Result<bool, InstanceError> {
    let path = instance_dir.join("servers.dat");
    let (name, mut root) = match std::fs::read(&path) {
        Ok(bytes) => nbt::read_root(&bytes).map_err(|e| InstanceError::Corrupt {
            path: path.clone(),
            reason: e.to_string(),
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            Vec::new(),
            Tag::Compound(vec![(b"servers".to_vec(), Tag::List(0, Vec::new()))]),
        ),
        Err(e) => return Err(io_at(&path)(e)),
    };

    let existed = root.get("servers").is_some();
    if !existed {
        root.set("servers", Tag::List(0, Vec::new()));
    }
    let Some(Tag::List(_, entries)) = root.get_mut("servers") else {
        return Err(InstanceError::Corrupt {
            path,
            reason: "`servers` is not a list".into(),
        });
    };

    let wanted = server.address.trim().to_ascii_lowercase();
    let present = entries.iter().any(|entry| {
        entry
            .get("ip")
            .and_then(Tag::as_str_bytes)
            .map(|ip| String::from_utf8_lossy(ip).trim().to_ascii_lowercase() == wanted)
            .unwrap_or(false)
    });
    if present {
        return Ok(false);
    }

    let mut compound = vec![
        (
            b"name".to_vec(),
            Tag::String(server.name.as_bytes().to_vec()),
        ),
        (
            b"ip".to_vec(),
            Tag::String(server.address.as_bytes().to_vec()),
        ),
    ];
    if server.accept_packs {
        compound.push((b"acceptTextures".to_vec(), Tag::Byte(1)));
    }
    compound.push((b"hidden".to_vec(), Tag::Byte(0)));
    entries.push(Tag::Compound(compound));

    // The game keeps `servers.dat_old` as its own backup; do the same before
    // touching a file it wrote.
    if path.is_file() {
        let backup = instance_dir.join("servers.dat_old");
        std::fs::copy(&path, &backup).map_err(io_at(&backup))?;
    }
    let tmp = instance_dir.join("servers.dat.tmp");
    std::fs::write(&tmp, nbt::write_root(&name, &root)).map_err(io_at(&tmp))?;
    std::fs::rename(&tmp, &path).map_err(io_at(&path))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAERIES: ServerEntry = ServerEntry {
        name: "Faeries SMP",
        address: "faeriessmp.com",
        accept_packs: true,
    };

    fn entries(dir: &Path) -> Vec<Tag> {
        let bytes = std::fs::read(dir.join("servers.dat")).unwrap();
        let (_, root) = nbt::read_root(&bytes).unwrap();
        match root.get("servers") {
            Some(Tag::List(_, items)) => items.clone(),
            other => panic!("no servers list: {other:?}"),
        }
    }

    #[test]
    fn creates_the_list_with_the_games_field_layout() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(ensure_server(tmp.path(), &FAERIES).unwrap());
        let list = entries(tmp.path());
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0],
            Tag::Compound(vec![
                (b"name".to_vec(), Tag::String(b"Faeries SMP".to_vec())),
                (b"ip".to_vec(), Tag::String(b"faeriessmp.com".to_vec())),
                (b"acceptTextures".to_vec(), Tag::Byte(1)),
                (b"hidden".to_vec(), Tag::Byte(0)),
            ])
        );
        assert!(
            !tmp.path().join("servers.dat_old").exists(),
            "nothing to back up"
        );
    }

    #[test]
    fn keeps_the_players_servers_and_their_icons() {
        let tmp = tempfile::tempdir().unwrap();
        let theirs = Tag::Compound(vec![
            (b"name".to_vec(), Tag::String(b"Friend".to_vec())),
            (b"ip".to_vec(), Tag::String(b"play.friend.net".to_vec())),
            (b"icon".to_vec(), Tag::String(b"iVBORw0KGgo=".to_vec())),
            (b"acceptTextures".to_vec(), Tag::Byte(0)),
        ]);
        let root = Tag::Compound(vec![(
            b"servers".to_vec(),
            Tag::List(10, vec![theirs.clone()]),
        )]);
        std::fs::write(tmp.path().join("servers.dat"), nbt::write_root(b"", &root)).unwrap();

        assert!(ensure_server(tmp.path(), &FAERIES).unwrap());
        let list = entries(tmp.path());
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], theirs, "their entry is untouched, icon and all");
        assert_eq!(
            list[1].get("ip").and_then(Tag::as_str_bytes),
            Some(&b"faeriessmp.com"[..])
        );
        assert!(
            tmp.path().join("servers.dat_old").is_file(),
            "backup like the game"
        );
    }

    #[test]
    fn does_not_duplicate_or_override_an_existing_entry() {
        let tmp = tempfile::tempdir().unwrap();
        // The player already has it, with packs set to prompt and a nickname.
        let theirs = Tag::Compound(vec![
            (b"name".to_vec(), Tag::String(b"faeries <3".to_vec())),
            (b"ip".to_vec(), Tag::String(b"FaeriesSMP.com".to_vec())),
            (b"acceptTextures".to_vec(), Tag::Byte(0)),
        ]);
        let root = Tag::Compound(vec![(
            b"servers".to_vec(),
            Tag::List(10, vec![theirs.clone()]),
        )]);
        std::fs::write(tmp.path().join("servers.dat"), nbt::write_root(b"", &root)).unwrap();

        assert!(!ensure_server(tmp.path(), &FAERIES).unwrap());
        assert_eq!(entries(tmp.path()), vec![theirs]);
    }

    #[test]
    fn a_broken_file_is_reported_and_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("servers.dat"), b"\x0a\x00\x00\x09").unwrap();
        let err = ensure_server(tmp.path(), &FAERIES).unwrap_err();
        assert!(matches!(err, InstanceError::Corrupt { .. }), "{err}");
        assert_eq!(
            std::fs::read(tmp.path().join("servers.dat")).unwrap(),
            b"\x0a\x00\x00\x09"
        );
    }
}
