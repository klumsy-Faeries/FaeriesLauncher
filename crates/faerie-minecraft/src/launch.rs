//! Build the exact process invocation for a Minecraft version: classpath,
//! JVM arguments, main class, and game arguments with all `${...}`
//! substitutions applied. No launching happens here — see [`crate::process`].

use std::path::PathBuf;

use crate::install::InstalledVersion;
use crate::java::JavaInstallation;
use crate::version::args::{self};
use crate::version::rules::{FeatureSet, RuleContext};
use crate::version::{self};
use crate::GamePaths;

/// The account identity passed to the game. For an offline/demo launch these
/// carry placeholder values; for a real launch they come from `faerie-auth`.
#[derive(Debug, Clone)]
pub struct Session {
    pub player_name: String,
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
    /// `msa` for real accounts, `legacy` for offline.
    pub user_type: String,
}

impl Session {
    /// A deterministic offline session for testing and demo launches. The
    /// access token is not valid for online play (§35: no auth bypass).
    pub fn offline(player_name: &str) -> Self {
        Self {
            player_name: player_name.to_string(),
            uuid: offline_uuid(player_name),
            access_token: "0".to_string(),
            xuid: "0".to_string(),
            user_type: "legacy".to_string(),
        }
    }
}

/// Everything the game needs beyond the installed version.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub game_dir: PathBuf,
    pub java_path: PathBuf,
    pub session: Session,
    pub min_ram_mb: Option<u32>,
    pub max_ram_mb: Option<u32>,
    pub extra_jvm_args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub resolution: Option<(u32, u32)>,
    pub launcher_name: String,
    pub launcher_version: String,
}

/// A fully-resolved process invocation.
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

/// Pick the Java installation to launch with: an explicit override wins;
/// otherwise prefer an exact major-version match, then the lowest major that
/// still meets the requirement, then the highest available as a last resort.
pub fn select_java<'a>(
    installations: &'a [JavaInstallation],
    required_major: Option<u32>,
    override_path: Option<&PathBuf>,
) -> Option<JavaChoice<'a>> {
    if let Some(path) = override_path {
        return Some(JavaChoice::Override(path.clone()));
    }
    let required = required_major?;
    if let Some(exact) = installations.iter().find(|j| j.major == required) {
        return Some(JavaChoice::Detected(exact));
    }
    let mut sufficient: Vec<&JavaInstallation> = installations
        .iter()
        .filter(|j| j.major >= required)
        .collect();
    sufficient.sort_by_key(|j| j.major);
    sufficient
        .first()
        .copied()
        .map(JavaChoice::Detected)
        // No sufficient runtime: hand back the best available so the caller
        // can decide (launch-and-warn vs. refuse). None only if list is empty.
        .or_else(|| {
            installations
                .iter()
                .max_by_key(|j| j.major)
                .map(JavaChoice::Detected)
        })
}

#[derive(Debug)]
pub enum JavaChoice<'a> {
    Override(PathBuf),
    Detected(&'a JavaInstallation),
}

impl JavaChoice<'_> {
    pub fn path(&self) -> PathBuf {
        match self {
            JavaChoice::Override(p) => p.clone(),
            JavaChoice::Detected(j) => j.path.clone(),
        }
    }
}

/// Build the launch invocation.
pub fn build_spec(
    installed: &InstalledVersion,
    paths: &GamePaths,
    options: &LaunchOptions,
) -> LaunchSpec {
    let detail = &installed.detail;
    let features = FeatureSet {
        has_custom_resolution: options.resolution.is_some(),
        ..Default::default()
    };
    let ctx = RuleContext::current(features);

    let classpath = build_classpath(installed, paths, &ctx);
    let natives_dir = paths.natives_dir(&detail.id);
    let assets_index = detail
        .assets
        .clone()
        .or_else(|| detail.asset_index.as_ref().map(|a| a.id.clone()))
        .unwrap_or_else(|| "legacy".into());
    let version_type = detail.kind.clone().unwrap_or_else(|| "release".into());
    let sep = if cfg!(windows) { ";" } else { ":" };

    let logging_path = detail
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        .map(|c| paths.logging_config(&c.file.id).display().to_string());

    // Substitution table shared by JVM and game arguments.
    let vars = {
        let s = &options.session;
        let game_dir = options.game_dir.display().to_string();
        let assets_root = paths.assets.display().to_string();
        let library_dir = paths.libraries.display().to_string();
        let natives = natives_dir.display().to_string();
        let classpath = classpath.clone();
        let version_id = detail.id.clone();
        let (res_w, res_h) = options.resolution.unwrap_or((0, 0));
        let launcher_name = options.launcher_name.clone();
        let launcher_version = options.launcher_version.clone();
        let logging_path = logging_path.clone().unwrap_or_default();
        move |name: &str| -> Option<String> {
            Some(match name {
                "auth_player_name" => s.player_name.clone(),
                "version_name" => version_id.clone(),
                "game_directory" => game_dir.clone(),
                "assets_root" | "game_assets" => assets_root.clone(),
                "assets_index_name" => assets_index.clone(),
                "auth_uuid" => s.uuid.clone(),
                "auth_access_token" | "accessToken" => s.access_token.clone(),
                "auth_session" => format!("token:{}:{}", s.access_token, s.uuid),
                "clientid" => "faerie-launcher".to_string(),
                "auth_xuid" => s.xuid.clone(),
                "user_type" => s.user_type.clone(),
                "version_type" => version_type.clone(),
                "user_properties" => "{}".to_string(),
                "natives_directory" => natives.clone(),
                "launcher_name" => launcher_name.clone(),
                "launcher_version" => launcher_version.clone(),
                "classpath" => classpath.clone(),
                "library_directory" => library_dir.clone(),
                "classpath_separator" => sep.to_string(),
                "path" => logging_path.clone(),
                "resolution_width" => res_w.to_string(),
                "resolution_height" => res_h.to_string(),
                _ => return None,
            })
        }
    };

    let mut argv = Vec::new();

    // Heap and user JVM args first.
    if let Some(min) = options.min_ram_mb {
        argv.push(format!("-Xms{min}M"));
    }
    if let Some(max) = options.max_ram_mb {
        argv.push(format!("-Xmx{max}M"));
    }
    argv.extend(options.extra_jvm_args.iter().cloned());

    // Version-declared JVM args (modern), or legacy defaults.
    match detail.arguments.as_ref() {
        Some(set) => {
            for arg in args::resolve(&set.jvm, &ctx) {
                argv.push(args::substitute(&arg, &vars));
            }
        }
        None => {
            argv.push(args::substitute(
                "-Djava.library.path=${natives_directory}",
                &vars,
            ));
            argv.push(args::substitute(
                "-Dminecraft.launcher.brand=${launcher_name}",
                &vars,
            ));
            argv.push(args::substitute(
                "-Dminecraft.launcher.version=${launcher_version}",
                &vars,
            ));
            argv.push("-cp".to_string());
            argv.push(classpath.clone());
        }
    }

    // Logging config (its own -D argument), if declared.
    if let (Some(client), Some(_)) = (
        detail.logging.as_ref().and_then(|l| l.client.as_ref()),
        logging_path.as_ref(),
    ) {
        argv.push(args::substitute(&client.argument, &vars));
    }

    // Main class.
    if let Some(main) = &detail.main_class {
        argv.push(main.clone());
    }

    // Game args (modern list or legacy string).
    match (
        detail.arguments.as_ref(),
        detail.minecraft_arguments.as_ref(),
    ) {
        (Some(set), _) => {
            for arg in args::resolve(&set.game, &ctx) {
                argv.push(args::substitute(&arg, &vars));
            }
        }
        (None, Some(legacy)) => {
            for token in args::split_legacy(legacy) {
                argv.push(args::substitute(&token, &vars));
            }
        }
        (None, None) => {}
    }

    // Custom resolution: modern versions gate --width/--height behind the
    // feature rule (already handled); legacy needs them appended explicitly.
    if let Some((w, h)) = options.resolution {
        if detail.arguments.is_none() {
            argv.push("--width".into());
            argv.push(w.to_string());
            argv.push("--height".into());
            argv.push(h.to_string());
        }
    }

    LaunchSpec {
        program: options.java_path.clone(),
        args: argv,
        cwd: options.game_dir.clone(),
        env: options.env.clone(),
    }
}

fn build_classpath(installed: &InstalledVersion, paths: &GamePaths, ctx: &RuleContext) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut entries: Vec<String> = Vec::new();

    for lib in &installed.detail.libraries {
        if !version::rules::allowed(&lib.rules, ctx) {
            continue;
        }
        // Only libraries with a real classpath artifact go here; pure
        // extract-only native entries (a `natives` map, no main artifact)
        // are unpacked to the natives dir, not placed on the classpath.
        if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let rel = artifact
                .path
                .clone()
                .or_else(|| version::maven_path(&lib.name));
            if let Some(rel) = rel {
                entries.push(paths.library_path(&rel).display().to_string());
            }
        } else if lib.natives.is_none() {
            // No downloads block and not a native: synthesize a maven path.
            if let Some(rel) = version::maven_path(&lib.name) {
                entries.push(paths.library_path(&rel).display().to_string());
            }
        }
    }

    // The client jar is last so mods/loaders earlier on the path win.
    entries.push(
        paths
            .version_jar(&installed.client_jar_id)
            .display()
            .to_string(),
    );
    entries.join(sep)
}

/// Deterministic offline UUID from a name (an offline-mode style hash).
fn offline_uuid(name: &str) -> String {
    // Not cryptographic; just a stable, well-formed UUID for offline play.
    let mut h: u128 = 0xcbf29ce484222325;
    for b in format!("OfflinePlayer:{name}").bytes() {
        h = h.wrapping_mul(0x100000001b3);
        h ^= b as u128;
    }
    // Force version 3 / variant bits so it is a syntactically valid UUID.
    let bytes = h.to_be_bytes();
    let mut b = bytes;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::VersionDetail;

    fn installed(json: &str) -> InstalledVersion {
        let detail = VersionDetail::from_json(json).unwrap();
        InstalledVersion {
            id: detail.id.clone(),
            client_jar_id: detail.id.clone(),
            detail,
            required_java_major: Some(21),
            required_java_component: None,
        }
    }

    fn options(java: &str) -> LaunchOptions {
        LaunchOptions {
            game_dir: PathBuf::from("/inst"),
            java_path: PathBuf::from(java),
            session: Session::offline("Faerie"),
            min_ram_mb: Some(1024),
            max_ram_mb: Some(4096),
            extra_jvm_args: vec!["-XX:+UseG1GC".into()],
            env: vec![],
            resolution: None,
            launcher_name: "Faerie".into(),
            launcher_version: "0.3.0".into(),
        }
    }

    #[test]
    fn modern_spec_has_heap_classpath_mainclass_and_gameargs() {
        let installed = installed(
            r#"{
                "id": "26.2",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "26",
                "assetIndex": { "id": "26", "url": "http://x/26.json" },
                "downloads": { "client": { "url": "http://x/client.jar", "sha1": "abc" } },
                "arguments": {
                    "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
                    "game": ["--username", "${auth_player_name}", "--uuid", "${auth_uuid}",
                             "--accessToken", "${auth_access_token}", "--version", "${version_name}"]
                },
                "libraries": [
                    { "name": "org.lwjgl:lwjgl:3.3.3",
                      "downloads": { "artifact": { "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                                                   "url": "http://x/lwjgl.jar", "sha1": "d" } } }
                ]
            }"#,
        );
        let paths = GamePaths::new("/data");
        let spec = build_spec(&installed, &paths, &options("/jdk/java"));

        assert_eq!(spec.program, PathBuf::from("/jdk/java"));
        assert!(spec.args.contains(&"-Xmx4096M".to_string()));
        assert!(spec.args.contains(&"-Xms1024M".to_string()));
        assert!(spec.args.contains(&"-XX:+UseG1GC".to_string()));

        let cp_index = spec.args.iter().position(|a| a == "-cp").unwrap();
        let classpath = &spec.args[cp_index + 1];
        assert!(classpath.contains("lwjgl-3.3.3.jar"));
        assert!(classpath.contains("26.2.jar"), "client jar on classpath");

        let main_index = spec
            .args
            .iter()
            .position(|a| a == "net.minecraft.client.main.Main")
            .unwrap();
        // Game args come after the main class.
        let user_index = spec.args.iter().position(|a| a == "Faerie").unwrap();
        assert!(user_index > main_index);
        assert!(spec.args.contains(&"--version".to_string()));
        assert!(spec.args.contains(&"26.2".to_string()));
    }

    #[test]
    fn legacy_spec_uses_default_jvm_and_split_game_args() {
        let installed = installed(
            r#"{
                "id": "1.8.9",
                "type": "release",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "1.8",
                "assetIndex": { "id": "1.8", "url": "http://x/1.8.json" },
                "downloads": { "client": { "url": "http://x/client.jar" } },
                "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --accessToken ${auth_access_token}",
                "libraries": []
            }"#,
        );
        let paths = GamePaths::new("/data");
        let spec = build_spec(&installed, &paths, &options("/jdk8/java"));

        assert!(spec
            .args
            .iter()
            .any(|a| a.starts_with("-Djava.library.path=")));
        assert!(spec.args.contains(&"-cp".to_string()));
        assert!(spec.args.contains(&"Faerie".to_string()));
        assert!(spec.args.contains(&"1.8.9".to_string()));
    }

    #[test]
    fn java_selection_prefers_exact_major() {
        let installs = vec![
            JavaInstallation {
                path: "/j8".into(),
                version: "1.8.0".into(),
                major: 8,
                vendor: "x".into(),
                arch: "amd64".into(),
                source: "env",
            },
            JavaInstallation {
                path: "/j21".into(),
                version: "21.0".into(),
                major: 21,
                vendor: "x".into(),
                arch: "amd64".into(),
                source: "env",
            },
            JavaInstallation {
                path: "/j25".into(),
                version: "25.0".into(),
                major: 25,
                vendor: "x".into(),
                arch: "amd64".into(),
                source: "env",
            },
        ];
        let choice = select_java(&installs, Some(21), None).unwrap();
        assert_eq!(choice.path(), PathBuf::from("/j21"));

        // No exact 17: the lowest sufficient (21) is chosen.
        let choice = select_java(&installs, Some(17), None).unwrap();
        assert_eq!(choice.path(), PathBuf::from("/j21"));

        // Override always wins.
        let override_path = PathBuf::from("/custom/java");
        let choice = select_java(&installs, Some(21), Some(&override_path)).unwrap();
        assert_eq!(choice.path(), override_path);
    }

    #[test]
    fn offline_uuid_is_stable_and_wellformed() {
        let a = offline_uuid("Faerie");
        let b = offline_uuid("Faerie");
        assert_eq!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.matches('-').count(), 4);
    }
}
