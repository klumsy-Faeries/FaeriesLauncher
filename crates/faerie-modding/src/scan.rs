//! Mod metadata extraction (§12): read what a jar declares about itself.
//!
//! Four descriptor formats cover the ecosystems we support:
//!
//! | File | Loader | Format |
//! |---|---|---|
//! | `fabric.mod.json` | Fabric | JSON |
//! | `quilt.mod.json` | Quilt | JSON |
//! | `META-INF/mods.toml` | Forge | TOML |
//! | `META-INF/neoforge.mods.toml` | NeoForge | TOML |
//!
//! A jar with none of these is still reported (as [`LoaderKind::Unknown`])
//! rather than hidden: an unrecognized jar in `mods/` is something the user
//! should see, not something we should silently ignore.
//!
//! Jars bundle other jars ("Jar-in-Jar"): Fabric and Quilt list them in the
//! descriptor, Forge and NeoForge in `META-INF/jarjar/metadata.json`. The
//! loader extracts and loads those too, so they satisfy dependencies exactly
//! like a jar in `mods/` — Fabric API is forty-odd of them, and Sodium
//! carries its own copies of the modules it needs. They are read here,
//! recursively, and reported as [`NestedMod`]s on the jar that carries them.

use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::version_range::VersionReq;
use crate::ModError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    Unknown,
}

impl LoaderKind {
    pub fn id(self) -> &'static str {
        match self {
            LoaderKind::Fabric => "fabric",
            LoaderKind::Quilt => "quilt",
            LoaderKind::Forge => "forge",
            LoaderKind::NeoForge => "neoforge",
            LoaderKind::Unknown => "unknown",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id.to_ascii_lowercase().as_str() {
            "fabric" => LoaderKind::Fabric,
            "quilt" => LoaderKind::Quilt,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            _ => return None,
        })
    }

    pub fn display(self) -> &'static str {
        match self {
            LoaderKind::Fabric => "Fabric",
            LoaderKind::Quilt => "Quilt",
            LoaderKind::Forge => "Forge",
            LoaderKind::NeoForge => "NeoForge",
            LoaderKind::Unknown => "Unknown",
        }
    }

    /// Quilt loads Fabric mods, so a Fabric mod is fine on a Quilt install.
    pub fn accepts_mods_for(self, mod_loader: LoaderKind) -> bool {
        self == mod_loader || (self == LoaderKind::Quilt && mod_loader == LoaderKind::Fabric)
    }
}

/// How strongly a mod needs another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    /// Must be present and satisfied, or the game will not start.
    Required,
    /// Used if present.
    Optional,
    /// Must NOT be present (Fabric `breaks`, Forge `incompatible`).
    Breaks,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub mod_id: String,
    pub kind: DependencyKind,
    pub requirement: VersionReq,
    /// The requirement exactly as the mod wrote it, for error messages.
    pub raw: String,
}

/// What a mod jar declares about itself.
#[derive(Debug, Clone)]
pub struct ModMetadata {
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub loader: LoaderKind,
    pub description: String,
    pub authors: Vec<String>,
    pub dependencies: Vec<Dependency>,
    /// Mod ids this jar also provides (aliases), e.g. Fabric `provides`.
    pub provides: Vec<String>,
}

/// A mod bundled inside another jar, loaded along with it.
#[derive(Debug, Clone)]
pub struct NestedMod {
    /// Where it sits in the carrying jar, e.g. `META-INF/jars/fabric-api-base-2.0.4.jar`.
    pub file: String,
    pub mod_id: String,
    pub version: String,
    /// Ids it also provides (aliases), like a top-level mod's `provides`.
    pub provides: Vec<String>,
}

/// A jar on disk plus whatever we could learn from it.
#[derive(Debug, Clone)]
pub struct ScannedMod {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    /// `true` when the file ends in `.disabled` (§13: disabling never deletes).
    pub disabled: bool,
    pub metadata: ModMetadata,
    /// Mods bundled inside this jar, at any depth.
    pub nested: Vec<NestedMod>,
    /// Non-fatal problems encountered while reading (malformed descriptor…).
    pub warnings: Vec<String>,
}

impl ScannedMod {
    /// Effective ids this jar satisfies: its own plus anything it provides,
    /// including what its bundled jars bring.
    pub fn provided_ids(&self) -> impl Iterator<Item = &str> {
        self.provided_versions().map(|(id, _)| id)
    }

    /// Every `(id, version)` this jar puts into the game: the mod itself and
    /// its aliases at the jar's version, then each bundled mod and its
    /// aliases at that bundled mod's own version.
    pub fn provided_versions(&self) -> impl Iterator<Item = (&str, &str)> {
        let own = std::iter::once(self.metadata.mod_id.as_str())
            .chain(self.metadata.provides.iter().map(String::as_str))
            .map(move |id| (id, self.metadata.version.as_str()));
        let bundled = self.nested.iter().flat_map(|n| {
            std::iter::once(n.mod_id.as_str())
                .chain(n.provides.iter().map(String::as_str))
                .map(move |id| (id, n.version.as_str()))
        });
        own.chain(bundled)
    }
}

pub const DISABLED_SUFFIX: &str = ".disabled";

/// How deep bundled jars may nest. Fabric API nests one level; a bundled
/// mod that bundles a library makes two. Deeper is a mistake or a loop.
const MAX_NESTING: usize = 3;

/// A descriptor plus the bundled jars it lists.
struct Descriptor {
    metadata: ModMetadata,
    /// Paths inside the jar, as written in the descriptor.
    nested_paths: Vec<String>,
}

/// Read one jar. Returns `Ok` even for jars with no recognizable descriptor;
/// only unreadable files are errors.
pub fn scan_jar(path: &Path) -> Result<ScannedMod, ModError> {
    let file = std::fs::File::open(path).map_err(|e| ModError::io(path, e))?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let disabled = file_name.ends_with(DISABLED_SUFFIX);

    let mut warnings = Vec::new();
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ModError::BadJar {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let (metadata, nested) = match read_descriptor(&mut archive, &mut warnings) {
        Some(descriptor) => {
            let mut nested = Vec::new();
            collect_nested(
                &mut archive,
                &descriptor.nested_paths,
                1,
                &mut nested,
                &mut warnings,
            );
            (descriptor.metadata, nested)
        }
        None => (unknown_metadata(&file_name), Vec::new()),
    };

    Ok(ScannedMod {
        path: path.to_path_buf(),
        file_name,
        size,
        disabled,
        metadata,
        nested,
        warnings,
    })
}

fn read_descriptor<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    warnings: &mut Vec<String>,
) -> Option<Descriptor> {
    // Order matters: a Quilt jar may also ship fabric.mod.json for
    // compatibility, and the Quilt descriptor is the authoritative one.
    for (entry, parse) in [
        (
            "quilt.mod.json",
            parse_quilt as fn(&str) -> Result<Descriptor, String>,
        ),
        ("fabric.mod.json", parse_fabric),
        ("META-INF/neoforge.mods.toml", parse_neoforge),
        ("META-INF/mods.toml", parse_forge),
    ] {
        let raw = {
            let Ok(mut file) = archive.by_name(entry) else {
                continue;
            };
            let mut raw = String::new();
            if file.read_to_string(&mut raw).is_err() {
                warnings.push(format!("{entry} is not valid UTF-8"));
                continue;
            }
            raw
        };
        match parse(&raw) {
            Ok(mut descriptor) => {
                if matches!(
                    descriptor.metadata.loader,
                    LoaderKind::Forge | LoaderKind::NeoForge
                ) {
                    descriptor
                        .nested_paths
                        .extend(jarjar_paths(archive, warnings));
                }
                return Some(descriptor);
            }
            Err(e) => warnings.push(format!("{entry} could not be parsed: {e}")),
        }
    }
    None
}

/// Forge and NeoForge list bundled jars in a file of their own.
fn jarjar_paths<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    const FILE: &str = "META-INF/jarjar/metadata.json";

    #[derive(Deserialize)]
    struct JarJar {
        #[serde(default)]
        jars: Vec<JarJarEntry>,
    }
    #[derive(Deserialize)]
    struct JarJarEntry {
        path: String,
    }

    let Ok(mut file) = archive.by_name(FILE) else {
        return Vec::new();
    };
    let mut raw = String::new();
    if file.read_to_string(&mut raw).is_err() {
        warnings.push(format!("{FILE} is not valid UTF-8"));
        return Vec::new();
    }
    match serde_json::from_str::<JarJar>(&raw) {
        Ok(list) => list.jars.into_iter().map(|e| e.path).collect(),
        Err(e) => {
            warnings.push(format!("{FILE} could not be parsed: {e}"));
            Vec::new()
        }
    }
}

/// Read the jars a descriptor lists, and theirs, up to [`MAX_NESTING`].
///
/// A bundled jar without a descriptor is a plain library and is skipped
/// without comment; one that is listed but missing or unreadable would stop
/// the loader, so that is a warning on the carrying jar.
fn collect_nested<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    paths: &[String],
    depth: usize,
    out: &mut Vec<NestedMod>,
    warnings: &mut Vec<String>,
) {
    if depth > MAX_NESTING {
        return;
    }
    for path in paths {
        let bytes = {
            let Ok(mut entry) = archive.by_name(path) else {
                warnings.push(format!("bundled jar {path} is listed but not in the jar"));
                continue;
            };
            let mut bytes = Vec::new();
            if let Err(e) = entry.read_to_end(&mut bytes) {
                warnings.push(format!("bundled jar {path} could not be read: {e}"));
                continue;
            }
            bytes
        };
        let mut inner = match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
            Ok(archive) => archive,
            Err(e) => {
                warnings.push(format!("bundled jar {path} is not a valid jar: {e}"));
                continue;
            }
        };
        let mut inner_warnings = Vec::new();
        let descriptor = read_descriptor(&mut inner, &mut inner_warnings);
        warnings.extend(inner_warnings.into_iter().map(|w| format!("{path}: {w}")));
        let Some(descriptor) = descriptor else {
            continue;
        };
        out.push(NestedMod {
            file: path.clone(),
            mod_id: descriptor.metadata.mod_id,
            version: descriptor.metadata.version,
            provides: descriptor.metadata.provides,
        });
        collect_nested(
            &mut inner,
            &descriptor.nested_paths,
            depth + 1,
            out,
            warnings,
        );
    }
}

fn unknown_metadata(file_name: &str) -> ModMetadata {
    let stem = file_name
        .trim_end_matches(DISABLED_SUFFIX)
        .trim_end_matches(".jar");
    ModMetadata {
        mod_id: String::new(),
        name: stem.to_string(),
        version: String::new(),
        loader: LoaderKind::Unknown,
        description: String::new(),
        authors: Vec::new(),
        dependencies: Vec::new(),
        provides: Vec::new(),
    }
}

// ---- Fabric ----

#[derive(Deserialize)]
struct FabricJson {
    id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    authors: Vec<FabricPerson>,
    #[serde(default)]
    depends: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    recommends: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    suggests: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    breaks: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    conflicts: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    provides: Vec<String>,
    #[serde(default)]
    jars: Vec<FabricJar>,
}

/// `jars` entries: `{ "file": "META-INF/jars/x.jar" }`.
#[derive(Deserialize)]
struct FabricJar {
    file: String,
}

/// `authors` entries are either a plain name or `{ "name": ... }`.
#[derive(Deserialize)]
#[serde(untagged)]
enum FabricPerson {
    Name(String),
    Object { name: String },
}

impl FabricPerson {
    fn into_name(self) -> String {
        match self {
            FabricPerson::Name(n) => n,
            FabricPerson::Object { name } => name,
        }
    }
}

/// A requirement value is a string or an array of strings ("any of").
fn requirement_from_json(value: &serde_json::Value) -> (VersionReq, String) {
    match value {
        serde_json::Value::String(s) => (VersionReq::parse(s), s.clone()),
        serde_json::Value::Array(items) => {
            let texts: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            let joined = texts.join(" || ");
            (VersionReq::parse(&joined), joined)
        }
        other => (VersionReq::parse("*"), other.to_string()),
    }
}

fn collect_deps(
    map: &BTreeMap<String, serde_json::Value>,
    kind: DependencyKind,
    out: &mut Vec<Dependency>,
) {
    for (mod_id, value) in map {
        let (requirement, raw) = requirement_from_json(value);
        out.push(Dependency {
            mod_id: mod_id.clone(),
            kind,
            requirement,
            raw,
        });
    }
}

/// Fabric Loader's JSON reader accepts raw line breaks and tabs inside a
/// string (a multi-line `description` is common: BetterGrassify, Entity
/// Texture Features); strict JSON does not. Escape them so such descriptors
/// parse instead of turning the mod into "no recognizable descriptor".
/// Control characters between tokens are untouched.
fn escape_control_chars(raw: &str) -> std::borrow::Cow<'_, str> {
    if !raw.chars().any(|c| (c as u32) < 0x20) {
        return std::borrow::Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len() + 16);
    let (mut in_string, mut escaped) = (false, false);
    for c in raw.chars() {
        match (in_string, escaped, c) {
            (false, _, '"') => {
                in_string = true;
                out.push(c);
            }
            (false, _, _) => out.push(c),
            (true, true, _) => {
                escaped = false;
                out.push(c);
            }
            (true, false, '\\') => {
                escaped = true;
                out.push(c);
            }
            (true, false, '"') => {
                in_string = false;
                out.push(c);
            }
            (true, false, '\n') => out.push_str("\\n"),
            (true, false, '\r') => out.push_str("\\r"),
            (true, false, '\t') => out.push_str("\\t"),
            (true, false, c) if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            (true, false, c) => out.push(c),
        }
    }
    std::borrow::Cow::Owned(out)
}

fn parse_fabric(raw: &str) -> Result<Descriptor, String> {
    let json: FabricJson =
        serde_json::from_str(&escape_control_chars(raw)).map_err(|e| e.to_string())?;
    let mut dependencies = Vec::new();
    collect_deps(&json.depends, DependencyKind::Required, &mut dependencies);
    collect_deps(
        &json.recommends,
        DependencyKind::Optional,
        &mut dependencies,
    );
    collect_deps(&json.suggests, DependencyKind::Optional, &mut dependencies);
    collect_deps(&json.breaks, DependencyKind::Breaks, &mut dependencies);
    collect_deps(&json.conflicts, DependencyKind::Breaks, &mut dependencies);

    Ok(Descriptor {
        metadata: ModMetadata {
            name: json.name.clone().unwrap_or_else(|| json.id.clone()),
            mod_id: json.id,
            version: json.version,
            loader: LoaderKind::Fabric,
            description: json.description.unwrap_or_default(),
            authors: json
                .authors
                .into_iter()
                .map(FabricPerson::into_name)
                .collect(),
            dependencies,
            provides: json.provides,
        },
        nested_paths: json.jars.into_iter().map(|j| j.file).collect(),
    })
}

// ---- Quilt ----

#[derive(Deserialize)]
struct QuiltJson {
    quilt_loader: QuiltLoader,
}

#[derive(Deserialize)]
struct QuiltLoader {
    id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    metadata: Option<QuiltMeta>,
    #[serde(default)]
    depends: Vec<QuiltDep>,
    #[serde(default)]
    breaks: Vec<QuiltDep>,
    #[serde(default)]
    provides: Vec<QuiltProvides>,
    #[serde(default)]
    jars: Vec<QuiltJar>,
}

/// Quilt lists bundled jars as plain paths; tolerate the `{ file }` form too.
#[derive(Deserialize)]
#[serde(untagged)]
enum QuiltJar {
    Path(String),
    Object { file: String },
}

#[derive(Deserialize)]
struct QuiltMeta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Quilt dependencies are `"id"` or `{ id, version?, optional? }`.
#[derive(Deserialize)]
#[serde(untagged)]
enum QuiltDep {
    Id(String),
    Object {
        id: String,
        #[serde(default)]
        version: Option<serde_json::Value>,
        #[serde(default)]
        optional: bool,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum QuiltProvides {
    Id(String),
    Object { id: String },
}

fn parse_quilt(raw: &str) -> Result<Descriptor, String> {
    let json: QuiltJson =
        serde_json::from_str(&escape_control_chars(raw)).map_err(|e| e.to_string())?;
    let loader = json.quilt_loader;
    let nested_paths = loader
        .jars
        .into_iter()
        .map(|j| match j {
            QuiltJar::Path(file) | QuiltJar::Object { file } => file,
        })
        .collect();

    let mut dependencies = Vec::new();
    let mut push = |dep: QuiltDep, breaking: bool| {
        let (mod_id, requirement, raw_text, optional) = match dep {
            QuiltDep::Id(id) => (id, VersionReq::parse("*"), "*".to_string(), false),
            QuiltDep::Object {
                id,
                version,
                optional,
            } => {
                let (req, raw_text) = match &version {
                    Some(v) => requirement_from_json(v),
                    None => (VersionReq::parse("*"), "*".to_string()),
                };
                (id, req, raw_text, optional)
            }
        };
        dependencies.push(Dependency {
            mod_id,
            kind: if breaking {
                DependencyKind::Breaks
            } else if optional {
                DependencyKind::Optional
            } else {
                DependencyKind::Required
            },
            requirement,
            raw: raw_text,
        });
    };
    for dep in loader.depends {
        push(dep, false);
    }
    for dep in loader.breaks {
        push(dep, true);
    }

    let meta = loader.metadata.unwrap_or(QuiltMeta {
        name: None,
        description: None,
    });
    Ok(Descriptor {
        metadata: ModMetadata {
            name: meta.name.unwrap_or_else(|| loader.id.clone()),
            mod_id: loader.id,
            version: loader.version,
            loader: LoaderKind::Quilt,
            description: meta.description.unwrap_or_default(),
            authors: Vec::new(),
            dependencies,
            provides: loader
                .provides
                .into_iter()
                .map(|p| match p {
                    QuiltProvides::Id(id) => id,
                    QuiltProvides::Object { id } => id,
                })
                .collect(),
        },
        nested_paths,
    })
}

// ---- Forge / NeoForge (mods.toml) ----

#[derive(Deserialize)]
struct ForgeToml {
    #[serde(default)]
    mods: Vec<ForgeMod>,
    #[serde(default)]
    dependencies: BTreeMap<String, Vec<ForgeDep>>,
}

#[derive(Deserialize)]
struct ForgeMod {
    #[serde(rename = "modId")]
    mod_id: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    authors: Option<String>,
}

#[derive(Deserialize)]
struct ForgeDep {
    #[serde(rename = "modId")]
    mod_id: String,
    /// Forge writes `mandatory`; NeoForge writes `type`
    /// (`required` / `optional` / `incompatible` / `discouraged`).
    #[serde(default)]
    mandatory: Option<bool>,
    #[serde(rename = "type", default)]
    dep_type: Option<String>,
    #[serde(rename = "versionRange", default)]
    version_range: Option<String>,
}

fn parse_mods_toml(raw: &str, loader: LoaderKind) -> Result<Descriptor, String> {
    let parsed: ForgeToml = toml::from_str(raw).map_err(|e| e.to_string())?;
    let first = parsed
        .mods
        .into_iter()
        .next()
        .ok_or_else(|| "no [[mods]] entry".to_string())?;

    // Dependencies are keyed by the mod id they belong to; a jar with one
    // mod may still key them by that id, so take every listed group.
    let mut dependencies = Vec::new();
    for dep in parsed.dependencies.into_values().flatten() {
        let kind = match dep.dep_type.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("incompatible") => DependencyKind::Breaks,
            Some(t) if t.eq_ignore_ascii_case("discouraged") => DependencyKind::Optional,
            Some(t) if t.eq_ignore_ascii_case("optional") => DependencyKind::Optional,
            Some(t) if t.eq_ignore_ascii_case("required") => DependencyKind::Required,
            // Forge's older boolean form.
            _ => match dep.mandatory {
                Some(false) => DependencyKind::Optional,
                _ => DependencyKind::Required,
            },
        };
        let raw_range = dep.version_range.unwrap_or_else(|| "*".to_string());
        dependencies.push(Dependency {
            mod_id: dep.mod_id,
            kind,
            requirement: VersionReq::parse_maven(&raw_range),
            raw: raw_range,
        });
    }

    Ok(Descriptor {
        metadata: ModMetadata {
            name: first.display_name.unwrap_or_else(|| first.mod_id.clone()),
            mod_id: first.mod_id,
            // `${file.jarVersion}` is substituted by the loader at runtime; we
            // cannot resolve it, so present it as unknown rather than literal.
            version: match first.version {
                Some(v) if v.contains("${") => String::new(),
                Some(v) => v,
                None => String::new(),
            },
            loader,
            description: first.description.unwrap_or_default().trim().to_string(),
            authors: first
                .authors
                .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            dependencies,
            provides: Vec::new(),
        },
        // Bundled jars live in META-INF/jarjar/metadata.json, read by the caller.
        nested_paths: Vec::new(),
    })
}

fn parse_forge(raw: &str) -> Result<Descriptor, String> {
    parse_mods_toml(raw, LoaderKind::Forge)
}

fn parse_neoforge(raw: &str) -> Result<Descriptor, String> {
    parse_mods_toml(raw, LoaderKind::NeoForge)
}

/// Scan every jar in a directory, in parallel. Non-jar files are ignored;
/// unreadable jars are returned as warnings rather than failing the scan.
pub async fn scan_directory(dir: &Path) -> (Vec<ScannedMod>, Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            p.is_file()
                && name
                    .map(|n| n.ends_with(".jar") || n.ends_with(".jar.disabled"))
                    .unwrap_or(false)
        })
        .collect();

    let handles: Vec<_> = paths
        .into_iter()
        .map(|path| tokio::task::spawn_blocking(move || scan_jar(&path)))
        .collect();

    let mut mods = Vec::new();
    let mut problems = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(m)) => mods.push(m),
            Ok(Err(e)) => problems.push(e.to_string()),
            Err(e) => problems.push(format!("mod scan task failed: {e}")),
        }
    }
    mods.sort_by(|a, b| {
        a.metadata
            .name
            .to_lowercase()
            .cmp(&b.metadata.name.to_lowercase())
    });
    (mods, problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn parse_fabric(raw: &str) -> Result<ModMetadata, String> {
        super::parse_fabric(raw).map(|d| d.metadata)
    }
    fn parse_quilt(raw: &str) -> Result<ModMetadata, String> {
        super::parse_quilt(raw).map(|d| d.metadata)
    }
    fn parse_forge(raw: &str) -> Result<ModMetadata, String> {
        super::parse_forge(raw).map(|d| d.metadata)
    }
    fn parse_neoforge(raw: &str) -> Result<ModMetadata, String> {
        super::parse_neoforge(raw).map(|d| d.metadata)
    }

    /// Build a jar in memory from `(entry name, bytes)` pairs.
    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn scan_bytes(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> ScannedMod {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        scan_jar(&path).unwrap()
    }

    #[test]
    fn bundled_jars_are_read_at_every_depth() {
        let deepest = jar(&[(
            "fabric.mod.json",
            br#"{ "id": "deepest", "version": "3.0.0" }"#,
        )]);
        let module = jar(&[
            (
                "fabric.mod.json",
                br#"{ "id": "fabric-api-base", "version": "2.0.4+ece0632",
                     "provides": ["fabric-api-base-alias"],
                     "jars": [{ "file": "META-INF/jars/deepest.jar" }] }"#,
            ),
            ("META-INF/jars/deepest.jar", &deepest),
        ]);
        let library = jar(&[("com/example/Lib.class", b"not a mod")]);
        let outer = jar(&[
            (
                "fabric.mod.json",
                br#"{ "id": "fabric-api", "version": "0.160.0+26.2",
                     "jars": [{ "file": "META-INF/jars/base.jar" },
                              { "file": "META-INF/jars/lib.jar" }] }"#,
            ),
            ("META-INF/jars/base.jar", &module),
            ("META-INF/jars/lib.jar", &library),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let scanned = scan_bytes(&dir, "fabric-api.jar", &outer);

        assert!(scanned.warnings.is_empty(), "{:?}", scanned.warnings);
        let ids: Vec<&str> = scanned.nested.iter().map(|n| n.mod_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["fabric-api-base", "deepest"],
            "a plain library is skipped, a nested mod's own jars are followed"
        );
        assert_eq!(scanned.nested[0].file, "META-INF/jars/base.jar");
        let provided: Vec<(&str, &str)> = scanned.provided_versions().collect();
        assert_eq!(
            provided,
            vec![
                ("fabric-api", "0.160.0+26.2"),
                ("fabric-api-base", "2.0.4+ece0632"),
                ("fabric-api-base-alias", "2.0.4+ece0632"),
                ("deepest", "3.0.0"),
            ],
            "each bundled mod carries its own version, not the carrier's"
        );
    }

    #[test]
    fn forge_bundled_jars_come_from_jarjar_metadata() {
        let inner = jar(&[(
            "META-INF/mods.toml",
            b"[[mods]]\nmodId = \"innerlib\"\nversion = \"1.2.3\"\n",
        )]);
        let outer = jar(&[
            (
                "META-INF/mods.toml",
                b"[[mods]]\nmodId = \"outer\"\nversion = \"1.0\"\n",
            ),
            (
                "META-INF/jarjar/metadata.json",
                br#"{ "jars": [ { "identifier": { "group": "x", "artifact": "innerlib" },
                                  "version": { "range": "[1.2.3,)", "artifactVersion": "1.2.3" },
                                  "path": "META-INF/jarjar/innerlib-1.2.3.jar",
                                  "isObfuscated": false } ] }"#,
            ),
            ("META-INF/jarjar/innerlib-1.2.3.jar", &inner),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let scanned = scan_bytes(&dir, "outer.jar", &outer);
        assert_eq!(scanned.metadata.loader, LoaderKind::Forge);
        assert_eq!(scanned.nested.len(), 1);
        assert_eq!(scanned.nested[0].mod_id, "innerlib");
        assert_eq!(scanned.nested[0].version, "1.2.3");
    }

    #[test]
    fn quilt_bundled_jar_paths_are_plain_strings() {
        let inner = jar(&[(
            "quilt.mod.json",
            br#"{ "quilt_loader": { "id": "qlib", "version": "0.1" } }"#,
        )]);
        let outer = jar(&[
            (
                "quilt.mod.json",
                br#"{ "quilt_loader": { "id": "qmod", "version": "1.0",
                                        "jars": ["META-INF/jars/qlib.jar"] } }"#,
            ),
            ("META-INF/jars/qlib.jar", &inner),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let scanned = scan_bytes(&dir, "qmod.jar", &outer);
        assert_eq!(scanned.nested.len(), 1);
        assert_eq!(scanned.nested[0].mod_id, "qlib");
    }

    #[test]
    fn a_listed_but_missing_bundled_jar_is_a_warning_naming_it() {
        let outer = jar(&[(
            "fabric.mod.json",
            br#"{ "id": "broken", "version": "1.0",
                 "jars": [{ "file": "META-INF/jars/gone.jar" }] }"#,
        )]);
        let dir = tempfile::tempdir().unwrap();
        let scanned = scan_bytes(&dir, "broken.jar", &outer);
        assert!(scanned.nested.is_empty());
        assert_eq!(scanned.warnings.len(), 1);
        assert!(scanned.warnings[0].contains("META-INF/jars/gone.jar"));
        assert_eq!(
            scanned.metadata.mod_id, "broken",
            "the mod itself still reads"
        );
    }

    #[test]
    fn fabric_descriptor_with_all_dependency_kinds() {
        let raw = r#"{
            "schemaVersion": 1,
            "id": "sodium",
            "version": "0.5.8",
            "name": "Sodium",
            "description": "A rendering engine replacement.",
            "authors": ["JellySquid", { "name": "Contributor" }],
            "depends": { "minecraft": ">=1.20.1", "fabricloader": ">=0.15.0" },
            "recommends": { "indium": "*" },
            "breaks": { "optifabric": "*" },
            "provides": ["sodium-extra-compat"]
        }"#;
        let meta = parse_fabric(raw).unwrap();
        assert_eq!(meta.mod_id, "sodium");
        assert_eq!(meta.name, "Sodium");
        assert_eq!(meta.loader, LoaderKind::Fabric);
        assert_eq!(meta.authors, vec!["JellySquid", "Contributor"]);
        assert_eq!(meta.provides, vec!["sodium-extra-compat"]);

        let required: Vec<&str> = meta
            .dependencies
            .iter()
            .filter(|d| d.kind == DependencyKind::Required)
            .map(|d| d.mod_id.as_str())
            .collect();
        assert_eq!(required, vec!["fabricloader", "minecraft"]);
        assert!(meta
            .dependencies
            .iter()
            .any(|d| d.mod_id == "optifabric" && d.kind == DependencyKind::Breaks));
        assert!(meta
            .dependencies
            .iter()
            .any(|d| d.mod_id == "indium" && d.kind == DependencyKind::Optional));
    }

    #[test]
    fn fabric_array_requirement_becomes_alternatives() {
        let raw = r#"{ "id": "m", "version": "1.0",
            "depends": { "minecraft": ["1.20.1", "1.20.2"] } }"#;
        let meta = parse_fabric(raw).unwrap();
        let dep = &meta.dependencies[0];
        assert!(dep.requirement.matches_str("1.20.1"));
        assert!(dep.requirement.matches_str("1.20.2"));
        assert!(!dep.requirement.matches_str("1.19.4"));
    }

    #[test]
    fn quilt_descriptor_parses_objects_and_strings() {
        let raw = r#"{
            "schema_version": 1,
            "quilt_loader": {
                "group": "com.example",
                "id": "example_mod",
                "version": "2.1.0",
                "metadata": { "name": "Example Mod", "description": "Hi" },
                "depends": [
                    "quilt_base",
                    { "id": "minecraft", "version": ">=1.20" },
                    { "id": "optional_thing", "version": "*", "optional": true }
                ],
                "breaks": [{ "id": "badmod", "version": "*" }]
            }
        }"#;
        let meta = parse_quilt(raw).unwrap();
        assert_eq!(meta.mod_id, "example_mod");
        assert_eq!(meta.name, "Example Mod");
        assert_eq!(meta.loader, LoaderKind::Quilt);
        assert_eq!(meta.dependencies.len(), 4);
        assert!(meta
            .dependencies
            .iter()
            .any(|d| d.mod_id == "optional_thing" && d.kind == DependencyKind::Optional));
        assert!(meta
            .dependencies
            .iter()
            .any(|d| d.mod_id == "badmod" && d.kind == DependencyKind::Breaks));
    }

    #[test]
    fn forge_mods_toml_with_mandatory_flag() {
        let raw = r#"
modLoader = "javafml"
loaderVersion = "[47,)"
license = "MIT"

[[mods]]
modId = "examplemod"
version = "1.0.0"
displayName = "Example Mod"
authors = "Alice, Bob"
description = '''A test mod.'''

[[dependencies.examplemod]]
    modId = "forge"
    mandatory = true
    versionRange = "[47,)"

[[dependencies.examplemod]]
    modId = "sodium"
    mandatory = false
    versionRange = "*"
"#;
        let meta = parse_forge(raw).unwrap();
        assert_eq!(meta.mod_id, "examplemod");
        assert_eq!(meta.loader, LoaderKind::Forge);
        assert_eq!(meta.authors, vec!["Alice", "Bob"]);
        let forge_dep = meta
            .dependencies
            .iter()
            .find(|d| d.mod_id == "forge")
            .unwrap();
        assert_eq!(forge_dep.kind, DependencyKind::Required);
        assert!(forge_dep.requirement.matches_str("47.2.0"));
        assert!(!forge_dep.requirement.matches_str("46.0.0"));
        let optional = meta
            .dependencies
            .iter()
            .find(|d| d.mod_id == "sodium")
            .unwrap();
        assert_eq!(optional.kind, DependencyKind::Optional);
    }

    #[test]
    fn neoforge_toml_uses_type_field() {
        let raw = r#"
modLoader = "javafml"
loaderVersion = "[21,)"

[[mods]]
modId = "neomod"
version = "${file.jarVersion}"
displayName = "Neo Mod"

[[dependencies.neomod]]
    modId = "neoforge"
    type = "required"
    versionRange = "[21.0.0,)"

[[dependencies.neomod]]
    modId = "conflicting"
    type = "incompatible"
    versionRange = "*"
"#;
        let meta = parse_neoforge(raw).unwrap();
        assert_eq!(meta.loader, LoaderKind::NeoForge);
        assert_eq!(
            meta.version, "",
            "unresolved ${{...}} placeholder should not be shown as a version"
        );
        assert!(meta
            .dependencies
            .iter()
            .any(|d| d.mod_id == "neoforge" && d.kind == DependencyKind::Required));
        assert!(meta
            .dependencies
            .iter()
            .any(|d| d.mod_id == "conflicting" && d.kind == DependencyKind::Breaks));
    }

    #[test]
    fn quilt_loads_fabric_mods_but_not_the_reverse() {
        assert!(LoaderKind::Quilt.accepts_mods_for(LoaderKind::Fabric));
        assert!(LoaderKind::Quilt.accepts_mods_for(LoaderKind::Quilt));
        assert!(!LoaderKind::Fabric.accepts_mods_for(LoaderKind::Quilt));
        assert!(!LoaderKind::Forge.accepts_mods_for(LoaderKind::Fabric));
    }

    #[test]
    fn malformed_descriptor_is_reported_not_fatal() {
        assert!(parse_fabric("{ not json").is_err());
        assert!(parse_forge("[[mods]]\nnope = true").is_err());
    }

    /// BetterGrassify 1.8.7, Entity Texture Features 7.2.4 and Entity Model
    /// Features 3.3.8 ship a `description` with a raw line break in it.
    /// Fabric Loader loads them; so must the scanner.
    #[test]
    fn raw_line_breaks_inside_strings_parse_as_fabric_loader_accepts_them() {
        let dir = tempfile::tempdir().unwrap();
        let descriptor = "{\n  \"schemaVersion\": 1,\n  \"id\": \"bettergrass\",\n  \
                          \"version\": \"1.8.7\",\n  \"name\": \"BetterGrassify\",\n  \
                          \"description\": \"Gamers can finally touch grass!?\nOptiFine's \
                          Fancy\tand Fast\",\n  \"depends\": { \"fabric-api\": \"*\" }\n}";
        let scanned = scan_bytes(
            &dir,
            "bettergrassify.jar",
            &jar(&[("fabric.mod.json", descriptor.as_bytes())]),
        );
        assert_eq!(scanned.metadata.mod_id, "bettergrass");
        assert_eq!(scanned.metadata.version, "1.8.7");
        assert!(
            scanned.metadata.description.contains("grass!?\nOptiFine"),
            "{}",
            scanned.metadata.description
        );
        assert_eq!(scanned.metadata.dependencies.len(), 1);
        assert!(scanned.warnings.is_empty(), "{:?}", scanned.warnings);
    }

    #[test]
    fn control_character_escaping_touches_only_the_inside_of_strings() {
        // Between tokens: untouched, and borrowed when nothing is inside a string.
        let clean = "{\n\t\"a\": \"b\"\n}";
        assert_eq!(escape_control_chars(clean), clean);
        // Inside: escaped, including after an escaped quote and a backslash.
        assert_eq!(
            escape_control_chars("{\"a\": \"x\ny\", \"b\": \"q\\\"\tz\\\\\nw\"}"),
            "{\"a\": \"x\\ny\", \"b\": \"q\\\"\\tz\\\\\\nw\"}"
        );
        assert_eq!(escape_control_chars("\"\u{1}\""), "\"\\u0001\"");
    }
}
