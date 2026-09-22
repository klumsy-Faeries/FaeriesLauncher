//! Theme resolution (§31 of the spec).
//!
//! Built-in themes are embedded at compile time from `themes/` in the repo.
//! A user theme is a folder of the same JSON files under the data directory's
//! `themes/<name>/`; it overrides a built-in of the same name.
//!
//! Token model: every theme file flattens into kebab-case tokens
//! (`colors.json:backgroundAlt` → `color-background-alt`), which the frontend
//! applies as `--fae-<token>` CSS custom properties. Resolution always starts
//! from the complete `smp` token set, so a partial or broken theme can
//! never leave the UI without a value — bad entries fall back per token and
//! are reported in `warnings` instead of failing the whole theme (§41 spirit).
//!
//! Two built-ins ship, one per Faeries world: `smp` (the castle painting,
//! pink glass) and `skyblock` (a pixel sky with pink slot frames, after the
//! Skyblock GUI).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use faerie_core::DataPaths;

/// Artwork compiled into the launcher. SVG stays as source text (escaped
/// into the CSS url), rasters are base64-encoded with their MIME type.
enum BuiltinImage {
    #[allow(dead_code)] // no built-in theme ships an SVG scene right now
    Svg(&'static str),
    Raster {
        mime: &'static str,
        bytes: &'static [u8],
    },
}

struct BuiltinTheme {
    name: &'static str,
    theme: &'static str,
    colors: &'static str,
    typography: &'static str,
    spacing: &'static str,
    layout: &'static str,
    /// Optional scene painted behind the whole window.
    background: Option<BuiltinImage>,
    /// Optional emblem for the home hero, as PNG bytes (`assets/logo.png`).
    logo_png: Option<&'static [u8]>,
    /// Optional small emblem for the sidebar, as PNG bytes (`assets/mark.png`).
    mark_png: Option<&'static [u8]>,
}

/// The theme every resolution starts from; also the default in `ui.theme`.
const BASE: &str = "smp";

static BUILTINS: &[BuiltinTheme] = &[
    BuiltinTheme {
        name: BASE,
        theme: include_str!("../../../themes/smp/theme.json"),
        colors: include_str!("../../../themes/smp/colors.json"),
        typography: include_str!("../../../themes/smp/typography.json"),
        spacing: include_str!("../../../themes/smp/spacing.json"),
        layout: include_str!("../../../themes/smp/layout.json"),
        // JPEG copy of art/background-5296.png: a painted scene has no
        // alpha, and this is a quarter of the PNG's size at window scale.
        background: Some(BuiltinImage::Raster {
            mime: "image/jpeg",
            bytes: include_bytes!("../../../themes/smp/assets/background.jpg"),
        }),
        // Sized copies of art/logo-2500.png: 768px for a 300px hero box at
        // 2x DPI, 256px for the sidebar. Embedding the original would put
        // ~5 MB of base64 into a CSS variable for no visible gain.
        logo_png: Some(include_bytes!("../../../themes/smp/assets/logo.png")),
        mark_png: Some(include_bytes!("../../../themes/smp/assets/mark.png")),
    },
    BuiltinTheme {
        name: "skyblock",
        theme: include_str!("../../../themes/skyblock/theme.json"),
        colors: include_str!("../../../themes/skyblock/colors.json"),
        typography: include_str!("../../../themes/skyblock/typography.json"),
        spacing: include_str!("../../../themes/skyblock/spacing.json"),
        layout: include_str!("../../../themes/skyblock/layout.json"),
        // Pixel sky painted for the theme (flat colours, so PNG stays small).
        background: Some(BuiltinImage::Raster {
            mime: "image/png",
            bytes: include_bytes!("../../../themes/skyblock/assets/background.png"),
        }),
        // The emblem is the same in every Faeries world.
        logo_png: Some(include_bytes!("../../../themes/smp/assets/logo.png")),
        mark_png: Some(include_bytes!("../../../themes/smp/assets/mark.png")),
    },
];

/// Names earlier versions stored in `ui.theme`, and what replaced them:
/// `faerie` was the SMP look's first name, and `dark` was retired when
/// Skyblock took its slot. Resolved silently so an old settings file keeps
/// working without a warning on every start.
const LEGACY_NAMES: [(&str, &str); 2] = [("faerie", BASE), ("dark", BASE)];

/// (token prefix, file name) for each theme file. `theme.json` flattens at
/// the root (`radius-small`, `shadow-card`, `motion-fast`, …).
const THEME_FILES: [(&str, &str); 4] = [
    ("", "theme.json"),
    ("color", "colors.json"),
    ("font", "typography.json"),
    ("space", "spacing.json"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDto {
    /// The theme actually applied (may differ from the request on fallback).
    pub name: String,
    pub source: &'static str,
    pub tokens: BTreeMap<String, String>,
    pub layout: Value,
    pub warnings: Vec<String>,
}

fn builtin(name: &str) -> Option<&'static BuiltinTheme> {
    BUILTINS.iter().find(|b| b.name == name)
}

fn camel_to_kebab(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn flatten_into(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                // `meta` in theme.json describes the theme; it is not a token.
                if prefix.is_empty() && key == "meta" {
                    continue;
                }
                let key = camel_to_kebab(key);
                let child_prefix = if prefix.is_empty() {
                    key
                } else {
                    format!("{prefix}-{key}")
                };
                flatten_into(&child_prefix, child, out);
            }
        }
        Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        Value::Number(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        Value::Bool(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        Value::Array(_) | Value::Null => {}
    }
}

/// Recursively merge `overlay` into `base`. Objects merge key by key;
/// anything else replaces wholesale, so a user can override
/// `sidebar.items` (an array) without having to restate `sidebar.width`.
pub(crate) fn merge_json(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        base_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value.clone(),
    }
}

fn apply_builtin(theme: &BuiltinTheme, tokens: &mut BTreeMap<String, String>) {
    for ((prefix, _), raw) in
        THEME_FILES
            .iter()
            .zip([theme.theme, theme.colors, theme.typography, theme.spacing])
    {
        let value: Value = serde_json::from_str(raw).expect("built-in theme JSON is valid");
        flatten_into(prefix, &value, tokens);
    }
    // A theme with no scene of its own gets a plain background colour.
    let scene = match theme.background {
        Some(BuiltinImage::Svg(svg)) => crate::assets::svg_data_uri(svg),
        Some(BuiltinImage::Raster { mime, bytes }) => crate::assets::raster_data_uri(mime, bytes),
        None => "none".into(),
    };
    tokens.insert("background-image".into(), scene);
    for (slot, png) in [
        ("asset-logo", theme.logo_png),
        ("asset-mark", theme.mark_png),
    ] {
        if let Some(png) = png {
            tokens.insert(
                slot.into(),
                crate::assets::raster_data_uri("image/png", png),
            );
        }
    }
}

fn apply_user_dir(dir: &Path, tokens: &mut BTreeMap<String, String>, warnings: &mut Vec<String>) {
    for (prefix, file_name) in THEME_FILES {
        let path = dir.join(file_name);
        if !path.is_file() {
            continue;
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string()))
        {
            Ok(value) => flatten_into(prefix, &value, tokens),
            Err(e) => warnings.push(format!(
                "{} could not be read ({e}); its tokens keep their previous values",
                path.display()
            )),
        }
    }
}

/// Resolve a theme by name: complete `smp` base, then the named built-in
/// (if any), then the user's folder of the same name (if any).
pub fn load(paths: &DataPaths, name: &str, layout_overrides: &str) -> ThemeDto {
    let mut tokens = BTreeMap::new();
    let mut warnings = Vec::new();

    // A retired name resolves to its replacement, unless the user has made
    // a theme folder of that name, which then means exactly what it says.
    let name = LEGACY_NAMES
        .iter()
        .find(|(old, _)| *old == name && !paths.themes_dir.join(old).is_dir())
        .map(|(_, new)| *new)
        .unwrap_or(name);

    let base = builtin(BASE).expect("the base built-in always exists");
    apply_builtin(base, &mut tokens);
    let mut layout: Value =
        serde_json::from_str(base.layout).expect("built-in layout JSON is valid");
    let mut applied = BASE.to_string();
    let mut source = "builtin";

    if let Some(b) = builtin(name) {
        if b.name != BASE {
            apply_builtin(b, &mut tokens);
            layout = serde_json::from_str(b.layout).expect("built-in layout JSON is valid");
        }
        applied = name.to_string();
    }

    let user_dir = paths.themes_dir.join(name);
    if user_dir.is_dir() {
        apply_user_dir(&user_dir, &mut tokens, &mut warnings);
        // Any artwork the user's theme carries overrides the built-in
        // equivalents, slot by slot (§32).
        for (token, url) in crate::assets::collect(&user_dir, &mut warnings) {
            tokens.insert(token, url);
        }
        let layout_path = user_dir.join("layout.json");
        if layout_path.is_file() {
            match std::fs::read_to_string(&layout_path)
                .map_err(|e| e.to_string())
                .and_then(|raw| serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string()))
            {
                Ok(value) => layout = value,
                Err(e) => warnings.push(format!(
                    "{} could not be read ({e}); using the previous layout",
                    layout_path.display()
                )),
            }
        }
        applied = name.to_string();
        source = "user";
    } else if builtin(name).is_none() {
        warnings.push(format!("theme `{name}` was not found; using `{BASE}`"));
    }

    // The user's own layout tweaks win over whatever the theme declares.
    if !layout_overrides.trim().is_empty() && layout_overrides.trim() != "{}" {
        match serde_json::from_str::<Value>(layout_overrides) {
            Ok(overrides) => merge_json(&mut layout, &overrides),
            Err(e) => warnings.push(format!("layout overrides could not be parsed ({e})")),
        }
    }

    ThemeDto {
        name: applied,
        source,
        tokens,
        layout,
        warnings,
    }
}

/// All selectable theme names: built-ins plus user theme folders.
pub fn list(paths: &DataPaths) -> Vec<String> {
    let mut names: Vec<String> = BUILTINS.iter().map(|b| b.name.to_string()).collect();
    if let Ok(entries) = std::fs::read_dir(&paths.themes_dir) {
        let mut user: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| !names.contains(name))
            .collect();
        user.sort();
        names.extend(user);
    }
    names
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

    /// Read a value straight out of a built-in theme's own JSON.
    ///
    /// Tests assert against this rather than literal colours: these tests
    /// are about *resolution mechanics*, so retuning a palette should not
    /// break them.
    fn builtin_value(theme: &str, file: &str, key: &str) -> String {
        let b = builtin(theme).expect("built-in exists");
        let raw = match file {
            "theme" => b.theme,
            "colors" => b.colors,
            "typography" => b.typography,
            "spacing" => b.spacing,
            other => panic!("unknown theme file {other}"),
        };
        let value: Value = serde_json::from_str(raw).unwrap();
        let mut node = &value;
        for part in key.split('.') {
            node = &node[part];
        }
        node.as_str().expect("string value").to_string()
    }

    #[test]
    fn builtin_smp_flattens_to_expected_tokens() {
        let (_tmp, paths) = temp_paths();
        let theme = load(&paths, "smp", "");
        assert_eq!(theme.source, "builtin");
        // Keys are kebab-cased and prefixed per file; values come through
        // verbatim from the theme's own JSON.
        assert_eq!(
            theme.tokens["color-background"],
            builtin_value("smp", "colors", "background")
        );
        assert_eq!(
            theme.tokens["color-background-alt"],
            builtin_value("smp", "colors", "backgroundAlt"),
            "camelCase keys become kebab-case tokens"
        );
        assert_eq!(
            theme.tokens["font-size-base"],
            builtin_value("smp", "typography", "size.base")
        );
        assert_eq!(
            theme.tokens["radius-medium"],
            builtin_value("smp", "theme", "radius.medium"),
            "theme.json flattens at the root, with no prefix"
        );
        assert_eq!(
            theme.tokens["space-md"],
            builtin_value("smp", "spacing", "md")
        );
        assert!(theme.warnings.is_empty());
        assert!(theme.layout["sidebar"]["enabled"].as_bool().unwrap());
    }

    #[test]
    fn both_builtins_ship_their_own_background_scene() {
        let (_tmp, paths) = temp_paths();
        let mut scenes = Vec::new();
        for name in ["smp", "skyblock"] {
            let theme = load(&paths, name, "");
            let background = theme.tokens["background-image"].clone();
            assert!(
                background.starts_with("url(\"data:image/"),
                "{name}: the scene is embedded as a data URI: {background:.40}"
            );
            // Whatever the format, nothing inside may terminate the CSS url().
            let inner = background
                .trim_start_matches("url(\"")
                .trim_end_matches("\")");
            assert!(!inner.contains('<') && !inner.contains('>') && !inner.contains('"'));
            scenes.push(background);
        }
        assert_ne!(scenes[0], scenes[1], "skyblock paints its own sky");
        // Both worlds share the emblem.
        assert_eq!(
            load(&paths, "smp", "").tokens["asset-logo"],
            load(&paths, "skyblock", "").tokens["asset-logo"]
        );
    }

    #[test]
    fn retired_names_resolve_to_smp_without_a_warning() {
        let (_tmp, paths) = temp_paths();
        for old in ["faerie", "dark"] {
            let theme = load(&paths, old, "");
            assert_eq!(theme.name, "smp", "{old}");
            assert!(theme.warnings.is_empty(), "{old}: {:?}", theme.warnings);
        }
        // A user folder that happens to use a retired name is that folder.
        let dir = paths.themes_dir.join("faerie");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("colors.json"), r##"{ "background": "#abcdef" }"##).unwrap();
        let theme = load(&paths, "faerie", "");
        assert_eq!(theme.name, "faerie");
        assert_eq!(theme.tokens["color-background"], "#abcdef");
    }

    #[test]
    fn a_user_theme_can_supply_its_own_background() {
        let (_tmp, paths) = temp_paths();
        let dir = paths.themes_dir.join("meadow");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(
            dir.join("assets").join("background.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="4" height="4"/></svg>"#,
        )
        .unwrap();

        let theme = load(&paths, "meadow", "");
        let background = &theme.tokens["background-image"];
        assert!(background.contains("data:image/svg+xml"));
        assert!(background.contains("rect"), "the user's art is used");
        assert!(
            !background.contains("castle") && !background.contains("vignette"),
            "it replaces the built-in scene rather than layering on it"
        );
    }

    #[test]
    fn skyblock_builtin_overrides_colors_but_keeps_full_token_set() {
        let (_tmp, paths) = temp_paths();
        let smp = load(&paths, "smp", "");
        let skyblock = load(&paths, "skyblock", "");
        assert_eq!(
            skyblock.tokens["color-background"],
            builtin_value("skyblock", "colors", "background")
        );
        assert_ne!(
            skyblock.tokens["color-background"],
            smp.tokens["color-background"]
        );
        assert_eq!(skyblock.tokens.len(), smp.tokens.len());
    }

    #[test]
    fn unknown_theme_falls_back_to_smp_with_warning() {
        let (_tmp, paths) = temp_paths();
        let theme = load(&paths, "sparkles", "");
        assert_eq!(theme.name, "smp");
        assert_eq!(theme.warnings.len(), 1);
        assert_eq!(
            theme.tokens["color-background"],
            builtin_value("smp", "colors", "background")
        );
    }

    #[test]
    fn user_theme_overrides_tokens_and_is_listed() {
        let (_tmp, paths) = temp_paths();
        let dir = paths.themes_dir.join("candy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("colors.json"), r##"{ "background": "#123456" }"##).unwrap();

        let theme = load(&paths, "candy", "");
        assert_eq!(theme.source, "user");
        assert_eq!(theme.tokens["color-background"], "#123456");
        // Everything the user theme does not define keeps the smp base.
        assert_eq!(
            theme.tokens["color-primary"],
            builtin_value("smp", "colors", "primary")
        );

        assert_eq!(list(&paths), vec!["smp", "skyblock", "candy"]);
    }

    #[test]
    fn broken_user_file_warns_and_keeps_base_tokens() {
        let (_tmp, paths) = temp_paths();
        let dir = paths.themes_dir.join("broken");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("colors.json"), "not json").unwrap();

        let theme = load(&paths, "broken", "");
        assert_eq!(theme.warnings.len(), 1);
        assert_eq!(
            theme.tokens["color-background"],
            builtin_value("smp", "colors", "background")
        );
    }
}
