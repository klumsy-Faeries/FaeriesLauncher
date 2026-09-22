//! Edits to an instance's `options.txt` that the launcher makes on the
//! player's behalf. The file is Minecraft's own: every line the launcher
//! does not understand is preserved byte for byte, and the game's line
//! endings are kept.

use std::path::Path;

use crate::{io_at, InstanceError};

const RESOURCE_PACKS_KEY: &str = "resourcePacks:";

/// The `resourcePacks` entry for a zip in the instance's `resourcepacks/`.
pub fn file_pack_entry(file_name: &str) -> String {
    format!("file/{file_name}")
}

/// Enable a resource pack in `<instance_dir>/options.txt`. New entries go
/// last, which the game treats as the top of the stack, so the pack wins
/// over whatever was already enabled. Creates the file when the game has
/// not written one yet. Returns whether anything changed.
pub fn enable_resource_pack(instance_dir: &Path, entry: &str) -> Result<bool, InstanceError> {
    let path = instance_dir.join("options.txt");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(io_at(&path)(e)),
    };
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    let mut found = false;
    for line in lines.iter_mut() {
        let Some(raw) = line.strip_prefix(RESOURCE_PACKS_KEY) else {
            continue;
        };
        found = true;
        let mut packs: Vec<String> =
            serde_json::from_str(raw.trim()).map_err(|e| InstanceError::Corrupt {
                path: path.clone(),
                reason: format!("resourcePacks is not a JSON list: {e}"),
            })?;
        if packs.iter().any(|p| p == entry) {
            return Ok(false);
        }
        packs.push(entry.to_string());
        *line = format!(
            "{RESOURCE_PACKS_KEY}{}",
            serde_json::to_string(&packs).expect("strings serialize")
        );
    }
    if !found {
        lines.push(format!(
            "{RESOURCE_PACKS_KEY}{}",
            serde_json::to_string(&[entry]).expect("strings serialize")
        ));
    }

    let mut out = lines.join(newline);
    out.push_str(newline);
    std::fs::write(&path, out).map_err(io_at(&path))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_file_when_the_game_never_wrote_one() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(enable_resource_pack(tmp.path(), "file/faeries.zip").unwrap());
        let text = std::fs::read_to_string(tmp.path().join("options.txt")).unwrap();
        assert_eq!(text, "resourcePacks:[\"file/faeries.zip\"]\n");
    }

    #[test]
    fn appends_to_the_top_of_an_existing_stack_and_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        // What the game wrote after a run with the Faeries Theme enabled,
        // with Windows line endings.
        let original = "version:4903\r\nresourcePacks:[\"faeries-theme:faeries-menu\"]\r\nincompatibleResourcePacks:[]\r\nlang:en_us\r\n";
        std::fs::write(tmp.path().join("options.txt"), original).unwrap();

        assert!(enable_resource_pack(tmp.path(), "file/faeries.zip").unwrap());
        let text = std::fs::read_to_string(tmp.path().join("options.txt")).unwrap();
        assert_eq!(
            text,
            "version:4903\r\nresourcePacks:[\"faeries-theme:faeries-menu\",\"file/faeries.zip\"]\r\nincompatibleResourcePacks:[]\r\nlang:en_us\r\n",
            "the theme's menu pack stays, ours goes on top, nothing else moves"
        );
    }

    #[test]
    fn is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(enable_resource_pack(tmp.path(), "file/a.zip").unwrap());
        assert!(!enable_resource_pack(tmp.path(), "file/a.zip").unwrap());
        let text = std::fs::read_to_string(tmp.path().join("options.txt")).unwrap();
        assert_eq!(text.matches("a.zip").count(), 1);
    }

    #[test]
    fn a_mangled_list_is_reported_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("options.txt"), "resourcePacks:[oops\n").unwrap();
        let err = enable_resource_pack(tmp.path(), "file/a.zip").unwrap_err();
        assert!(matches!(err, InstanceError::Corrupt { .. }), "{err}");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("options.txt")).unwrap(),
            "resourcePacks:[oops\n",
            "the player's file is left alone"
        );
    }

    #[test]
    fn entry_names_follow_the_games_convention() {
        assert_eq!(file_pack_entry("Faeries Pack.zip"), "file/Faeries Pack.zip");
    }
}
