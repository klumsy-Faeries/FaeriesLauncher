//! Theme asset system (§32).
//!
//! A theme is one shareable folder. Any artwork it carries lives under
//! `assets/` and is embedded as a data URI, so themes install by copying a
//! directory — no asset registration step, nothing to rebuild.
//!
//! ```text
//! themes/mytheme/
//! └── assets/
//!     ├── background.png     →  --fae-background-image
//!     ├── logo.png           →  --fae-asset-logo
//!     ├── mark.svg           →  --fae-asset-mark
//!     └── icons/
//!         ├── home.svg       →  --fae-icon-home
//!         └── mods.png       →  --fae-icon-mods
//! ```
//!
//! Every slot is optional; the UI falls back to its built-in drawing when a
//! token is absent, so a theme can replace one icon without supplying all of
//! them.

use std::collections::BTreeMap;
use std::path::Path;

/// Image formats a theme may supply, in preference order.
const IMAGE_FORMATS: [(&str, &str); 6] = [
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("webp", "image/webp"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
];

/// Named single-file slots and the token each produces.
///
/// `background` keeps its historical token name because the stylesheet
/// paints it directly; the rest use the `asset-` prefix.
const SLOTS: [(&str, &str); 3] = [
    ("background", "background-image"),
    ("logo", "asset-logo"),
    ("mark", "asset-mark"),
];

/// Refuse absurdly large embeds: a data URI this big would bloat every
/// theme load for no visual benefit, and almost certainly means a mistake.
const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024;

/// Percent-encode SVG source for `url("data:image/svg+xml,…")`.
/// Only characters that would break the CSS token are escaped, which keeps
/// the result far smaller than base64.
pub fn svg_data_uri(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 64);
    out.push_str("url(\"data:image/svg+xml,");
    for c in svg.chars() {
        match c {
            '"' => out.push_str("%22"),
            '#' => out.push_str("%23"),
            '%' => out.push_str("%25"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '\n' | '\r' | '\t' => out.push(' '),
            other => out.push(other),
        }
    }
    out.push_str("\")");
    out
}

/// A raster image as a CSS `url(...)` data URI.
pub fn raster_data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("url(\"data:{mime};base64,{}\")", base64(bytes))
}

/// Minimal base64 for embedding raster assets.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Turn one image file into a CSS `url(...)` value.
fn file_to_url(path: &Path, mime: &str, warnings: &mut Vec<String>) -> Option<String> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_ASSET_BYTES => {
            warnings.push(format!(
                "{} is {} MB; theme assets are limited to {} MB and this one was skipped",
                path.display(),
                meta.len() / (1024 * 1024),
                MAX_ASSET_BYTES / (1024 * 1024)
            ));
            return None;
        }
        Ok(_) => {}
        Err(e) => {
            warnings.push(format!("{} could not be read ({e})", path.display()));
            return None;
        }
    }

    if mime == "image/svg+xml" {
        return match std::fs::read_to_string(path) {
            Ok(svg) => Some(svg_data_uri(&svg)),
            Err(e) => {
                warnings.push(format!("{} could not be read ({e})", path.display()));
                None
            }
        };
    }
    match std::fs::read(path) {
        Ok(bytes) => Some(raster_data_uri(mime, &bytes)),
        Err(e) => {
            warnings.push(format!("{} could not be read ({e})", path.display()));
            None
        }
    }
}

/// Find `<dir>/<stem>.<ext>` for the first supported extension.
fn find_image(dir: &Path, stem: &str, warnings: &mut Vec<String>) -> Option<String> {
    for (ext, mime) in IMAGE_FORMATS {
        let path = dir.join(format!("{stem}.{ext}"));
        if path.is_file() {
            return file_to_url(&path, mime, warnings);
        }
    }
    None
}

/// Collect every asset token a theme folder provides.
///
/// Returns token name → CSS `url(...)` value. Missing slots are simply
/// absent, which is what lets a theme override one icon and inherit the
/// rest.
pub fn collect(theme_dir: &Path, warnings: &mut Vec<String>) -> BTreeMap<String, String> {
    let mut tokens = BTreeMap::new();
    let assets = theme_dir.join("assets");
    if !assets.is_dir() {
        return tokens;
    }

    for (stem, token) in SLOTS {
        if let Some(url) = find_image(&assets, stem, warnings) {
            tokens.insert(token.to_string(), url);
        }
    }

    // Per-item icons, named after the thing they replace (`home`, `mods`…).
    let icons = assets.join("icons");
    if let Ok(entries) = std::fs::read_dir(&icons) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_lowercase()) else {
                continue;
            };
            let Some((_, mime)) = IMAGE_FORMATS.iter().find(|(e, _)| *e == ext) else {
                continue; // not an image we can embed
            };
            if let Some(url) = file_to_url(&path, mime, warnings) {
                tokens.insert(format!("icon-{}", stem.to_lowercase()), url);
            }
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (relative, bytes) in files {
            let path = tmp.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
        }
        tmp
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn svg_escapes_only_what_breaks_the_css_url() {
        let uri = svg_data_uri("<svg fill=\"#fff\">100%</svg>");
        assert!(!uri.trim_start_matches("url(\"").contains('<'));
        assert!(uri.contains("%3C"));
        assert!(uri.contains("%23fff"), "# is escaped");
        assert!(uri.contains("100%25"), "% is escaped");
    }

    #[test]
    fn collects_named_slots_and_icons() {
        let tmp = theme_with(&[
            ("assets/background.svg", b"<svg/>"),
            ("assets/logo.png", b"\x89PNG fake"),
            ("assets/icons/home.svg", b"<svg id=\"home\"/>"),
            ("assets/icons/Mods.png", b"\x89PNG fake"),
            ("assets/icons/notes.txt", b"ignored"),
        ]);
        let mut warnings = Vec::new();
        let tokens = collect(tmp.path(), &mut warnings);

        assert!(tokens["background-image"].contains("image/svg+xml"));
        assert!(tokens["asset-logo"].contains("image/png;base64"));
        assert!(tokens["icon-home"].contains("home"));
        assert!(
            tokens.contains_key("icon-mods"),
            "icon names are lower-cased: {:?}",
            tokens.keys().collect::<Vec<_>>()
        );
        assert!(!tokens.contains_key("icon-notes"), "non-images are skipped");
        assert!(
            !tokens.contains_key("asset-mark"),
            "absent slots stay absent"
        );
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
    }

    #[test]
    fn svg_wins_over_other_formats_for_the_same_slot() {
        let tmp = theme_with(&[
            ("assets/logo.png", b"\x89PNG fake"),
            ("assets/logo.svg", b"<svg id=\"vector\"/>"),
        ]);
        let mut warnings = Vec::new();
        let tokens = collect(tmp.path(), &mut warnings);
        assert!(
            tokens["asset-logo"].contains("vector"),
            "prefers the vector"
        );
    }

    #[test]
    fn a_theme_without_assets_yields_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut warnings = Vec::new();
        assert!(collect(tmp.path(), &mut warnings).is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn oversized_assets_are_skipped_with_an_explanation() {
        let big = vec![0u8; (MAX_ASSET_BYTES + 1) as usize];
        let tmp = theme_with(&[("assets/background.png", &big)]);
        let mut warnings = Vec::new();
        let tokens = collect(tmp.path(), &mut warnings);
        assert!(!tokens.contains_key("background-image"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("limited to"), "got: {}", warnings[0]);
    }
}
