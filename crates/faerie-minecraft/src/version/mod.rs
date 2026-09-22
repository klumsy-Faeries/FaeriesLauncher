//! Minecraft version metadata (the per-version JSON), parsed generically so
//! nothing is hard-coded to a specific Minecraft version (§2, §48).

pub mod args;
pub mod rules;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use args::{Argument, ArgumentSet};
pub use rules::{FeatureSet, Rule, RuleContext};

/// A downloadable artifact with an optional hash and size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Relative path under `libraries/` (maven layout). Absent for the client
    /// jar and other top-level downloads.
    #[serde(default)]
    pub path: Option<String>,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    /// Native/classifier artifacts keyed by classifier (e.g. `natives-windows`).
    #[serde(default)]
    pub classifiers: Option<BTreeMap<String, Artifact>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// Maven coordinates: `group:artifact:version[:classifier]`.
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Maps OS name → classifier key for native jars to extract. May contain
    /// `${arch}`, substituted with `64`/`32`.
    #[serde(default)]
    pub natives: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub extract: Option<Extract>,
    /// Legacy maven base URL, used when `downloads` is absent (older loaders).
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameDownloads {
    #[serde(default)]
    pub client: Option<DownloadEntry>,
    #[serde(default)]
    pub server: Option<DownloadEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionReq {
    #[serde(default)]
    pub component: Option<String>,
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    /// e.g. `-Dlog4j.configurationFile=${path}`.
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

/// A full version JSON. Fields are optional so a child (modded) version that
/// only overrides a few things still parses; [`VersionDetail::resolve_onto`]
/// fills the gaps from the parent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDetail {
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: Option<String>,
    /// Modern argument model.
    #[serde(default)]
    pub arguments: Option<ArgumentSet>,
    /// Legacy single-string arguments (pre-1.13).
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<GameDownloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub java_version: Option<JavaVersionReq>,
    #[serde(default)]
    pub logging: Option<Logging>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

impl VersionDetail {
    pub fn from_json(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }

    /// Merge this (child) version onto its resolved `parent`, producing a
    /// self-contained version. Child values win for scalars; libraries and
    /// arguments are concatenated child-first (§12 loader layering).
    pub fn resolve_onto(mut self, parent: VersionDetail) -> VersionDetail {
        self.inherits_from = None;
        self.main_class = self.main_class.or(parent.main_class);
        self.asset_index = self.asset_index.or(parent.asset_index);
        self.assets = self.assets.or(parent.assets);
        self.downloads = self.downloads.or(parent.downloads);
        self.java_version = self.java_version.or(parent.java_version);
        self.logging = self.logging.or(parent.logging);
        self.kind = self.kind.or(parent.kind);

        // Libraries: child first (loaders expect their overrides to precede
        // vanilla on the classpath), then the parent's.
        let mut libraries = std::mem::take(&mut self.libraries);
        libraries.extend(parent.libraries);
        self.libraries = libraries;

        // Arguments: concatenate modern sets; a legacy parent string becomes
        // additional game arguments so mixed inheritance still works.
        self.arguments = match (self.arguments.take(), parent.arguments) {
            (Some(mut child), Some(parent)) => {
                let mut game = parent.game;
                game.extend(std::mem::take(&mut child.game));
                let mut jvm = parent.jvm;
                jvm.extend(std::mem::take(&mut child.jvm));
                Some(ArgumentSet { game, jvm })
            }
            (Some(child), None) => Some(child),
            (None, parent) => parent,
        };
        self.minecraft_arguments = self.minecraft_arguments.or(parent.minecraft_arguments);
        self
    }
}

/// Compute a maven artifact's relative path from its coordinates, e.g.
/// `net.fabricmc:tiny-mappings-parser:0.3.0` →
/// `net/fabricmc/tiny-mappings-parser/0.3.0/tiny-mappings-parser-0.3.0.jar`.
/// Used when a library has no explicit `downloads.artifact.path` (older
/// loaders), and for resolving native classifiers.
pub fn maven_path(coords: &str) -> Option<String> {
    // group:artifact:version[:classifier][@ext]
    let (coords, ext) = match coords.split_once('@') {
        Some((c, e)) => (c, e),
        None => (coords, "jar"),
    };
    let mut parts = coords.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();

    let group_path = group.replace('.', "/");
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!("{group_path}/{artifact}/{version}/{file}"))
}

/// Resolve the classifier key a library declares for the current OS, applying
/// `${arch}` substitution.
pub fn native_classifier(natives: &BTreeMap<String, String>, os_name: &str) -> Option<String> {
    let raw = natives.get(os_name)?;
    let arch = if std::env::consts::ARCH == "x86" {
        "32"
    } else {
        "64"
    };
    Some(raw.replace("${arch}", arch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_path_with_and_without_classifier() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(
            maven_path("net.minecraftforge:forge:1.20-47.0.0@zip").unwrap(),
            "net/minecraftforge/forge/1.20-47.0.0/forge-1.20-47.0.0.zip"
        );
    }

    #[test]
    fn inheritance_merges_child_over_parent() {
        let parent = VersionDetail::from_json(
            r#"{
                "id": "26.2",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "26",
                "libraries": [{ "name": "vanilla:lib:1" }],
                "arguments": { "game": ["--vanilla"], "jvm": ["-DjvmParent"] }
            }"#,
        )
        .unwrap();
        let child = VersionDetail::from_json(
            r#"{
                "id": "fabric-26.2",
                "inheritsFrom": "26.2",
                "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                "libraries": [{ "name": "fabric:loader:1" }],
                "arguments": { "game": ["--fabric"], "jvm": ["-DjvmChild"] }
            }"#,
        )
        .unwrap();

        let resolved = child.resolve_onto(parent);
        assert_eq!(
            resolved.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        assert_eq!(resolved.assets.as_deref(), Some("26"));
        assert_eq!(resolved.inherits_from, None);
        // Child libraries precede parent libraries.
        assert_eq!(resolved.libraries[0].name, "fabric:loader:1");
        assert_eq!(resolved.libraries[1].name, "vanilla:lib:1");
        // Arguments: parent then child.
        let args = resolved.arguments.unwrap();
        assert_eq!(args.game.len(), 2);
        assert_eq!(args.jvm.len(), 2);
    }

    #[test]
    fn native_classifier_substitutes_arch() {
        let mut natives = BTreeMap::new();
        natives.insert("windows".to_string(), "natives-windows-${arch}".to_string());
        let key = native_classifier(&natives, "windows").unwrap();
        assert!(key == "natives-windows-64" || key == "natives-windows-32");
    }
}
