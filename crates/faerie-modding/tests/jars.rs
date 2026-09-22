//! End-to-end tests over real jar files: build zip archives containing real
//! descriptor formats, scan them from disk, and check the results.

use std::io::Write;
use std::path::Path;

use faerie_modding::compat::{self, Environment, IssueKind};
use faerie_modding::scan::{self, LoaderKind};
use faerie_modding::store::{apply_profile, ModProfile, ModStore, ProfileMod};

/// Write a jar containing one descriptor entry.
fn make_jar(dir: &Path, name: &str, entry: Option<(&str, &str)>) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    if let Some((entry_name, body)) = entry {
        zip.start_file(entry_name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    // Every real jar has a manifest; include one so these look authentic.
    zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
    zip.write_all(b"Manifest-Version: 1.0\n").unwrap();
    zip.finish().unwrap();
    path
}

const SODIUM: &str = r#"{
  "schemaVersion": 1,
  "id": "sodium",
  "version": "0.5.8",
  "name": "Sodium",
  "depends": { "minecraft": ">=1.20.4", "fabricloader": ">=0.15.0" },
  "breaks": { "optifabric": "*" }
}"#;

const INDIUM: &str = r#"{
  "schemaVersion": 1,
  "id": "indium",
  "version": "1.0.30",
  "name": "Indium",
  "depends": { "minecraft": ">=1.20.4", "sodium": ">=0.5.0" }
}"#;

const FORGE_MOD: &str = r#"
modLoader = "javafml"
loaderVersion = "[47,)"
license = "MIT"

[[mods]]
modId = "jei"
version = "15.2.0"
displayName = "Just Enough Items"

[[dependencies.jei]]
    modId = "forge"
    mandatory = true
    versionRange = "[47,)"
"#;

#[tokio::test]
async fn scans_a_directory_of_real_jars() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    make_jar(&mods, "sodium.jar", Some(("fabric.mod.json", SODIUM)));
    make_jar(&mods, "indium.jar", Some(("fabric.mod.json", INDIUM)));
    make_jar(&mods, "jei.jar", Some(("META-INF/mods.toml", FORGE_MOD)));
    make_jar(&mods, "mystery.jar", None);
    std::fs::write(mods.join("README.txt"), b"not a jar").unwrap();

    let (found, problems) = scan::scan_directory(&mods).await;
    assert!(problems.is_empty(), "problems: {problems:?}");
    assert_eq!(found.len(), 4, "non-jar files are ignored");

    let sodium = found
        .iter()
        .find(|m| m.metadata.mod_id == "sodium")
        .unwrap();
    assert_eq!(sodium.metadata.name, "Sodium");
    assert_eq!(sodium.metadata.version, "0.5.8");
    assert_eq!(sodium.metadata.loader, LoaderKind::Fabric);
    assert!(sodium.size > 0);

    let jei = found.iter().find(|m| m.metadata.mod_id == "jei").unwrap();
    assert_eq!(jei.metadata.loader, LoaderKind::Forge);

    // A jar with no descriptor is surfaced, not silently dropped.
    let mystery = found.iter().find(|m| m.file_name == "mystery.jar").unwrap();
    assert_eq!(mystery.metadata.loader, LoaderKind::Unknown);
}

#[tokio::test]
async fn a_realistic_fabric_set_reports_the_expected_issues() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    make_jar(&mods, "sodium.jar", Some(("fabric.mod.json", SODIUM)));
    make_jar(&mods, "indium.jar", Some(("fabric.mod.json", INDIUM)));
    // A Forge mod dropped into a Fabric instance by mistake.
    make_jar(&mods, "jei.jar", Some(("META-INF/mods.toml", FORGE_MOD)));

    let (found, _) = scan::scan_directory(&mods).await;
    let env = Environment {
        loader: LoaderKind::Fabric,
        loader_version: "0.15.7".into(),
        minecraft_version: "1.20.4".into(),
    };
    let report = compat::check(&env, &found);

    // Sodium + Indium are fine together; only the Forge mod is wrong.
    assert!(!report.is_compatible());
    let wrong_loader: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.kind == IssueKind::WrongLoader)
        .collect();
    assert_eq!(wrong_loader.len(), 1);
    assert_eq!(wrong_loader[0].subject, "jei.jar");
    assert!(wrong_loader[0].fix.contains("Fabric build"));

    // No missing-dependency complaints: indium's dep on sodium is satisfied.
    assert!(!report
        .issues
        .iter()
        .any(|i| i.kind == IssueKind::MissingDependency));
}

#[tokio::test]
async fn wrong_minecraft_version_is_caught_against_real_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    make_jar(&mods, "sodium.jar", Some(("fabric.mod.json", SODIUM)));

    let (found, _) = scan::scan_directory(&mods).await;
    let env = Environment {
        loader: LoaderKind::Fabric,
        loader_version: "0.15.7".into(),
        // Sodium requires >=1.20.4.
        minecraft_version: "1.19.2".into(),
    };
    let report = compat::check(&env, &found);
    let issue = report
        .issues
        .iter()
        .find(|i| i.kind == IssueKind::WrongMinecraftVersion)
        .expect("mismatch detected");
    assert!(issue.detail.contains("Required: minecraft >=1.20.4"));
    assert!(issue.detail.contains("Installed: minecraft 1.19.2"));
}

#[tokio::test]
async fn store_profile_round_trip_survives_a_rescan() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("downloads");
    let store = ModStore::new(tmp.path().join("mod-store"));

    let sodium_jar = make_jar(&source, "sodium.jar", Some(("fabric.mod.json", SODIUM)));
    let indium_jar = make_jar(&source, "indium.jar", Some(("fabric.mod.json", INDIUM)));
    let sodium_hash = store.add(&sodium_jar).unwrap();
    let indium_hash = store.add(&indium_jar).unwrap();

    let profile = ModProfile {
        name: "Performance".into(),
        mods: vec![
            ProfileMod {
                sha1: sodium_hash,
                file_name: "sodium.jar".into(),
                enabled: true,
            },
            ProfileMod {
                sha1: indium_hash,
                file_name: "indium.jar".into(),
                enabled: false,
            },
        ],
    };

    let mods_dir = tmp.path().join("instance/mods");
    apply_profile(&store, &profile, &mods_dir).unwrap();

    // Scanning the materialized directory must yield working metadata: the
    // hardlinked jars are byte-identical to the originals.
    let (found, problems) = scan::scan_directory(&mods_dir).await;
    assert!(problems.is_empty(), "problems: {problems:?}");
    assert_eq!(found.len(), 2);
    let indium = found
        .iter()
        .find(|m| m.metadata.mod_id == "indium")
        .unwrap();
    assert!(indium.disabled, "disabled state survives materialization");
    let sodium = found
        .iter()
        .find(|m| m.metadata.mod_id == "sodium")
        .unwrap();
    assert!(!sodium.disabled);

    // The disabled mod must not satisfy indium's dependency... and since
    // indium itself is disabled, the set is clean.
    let env = Environment {
        loader: LoaderKind::Fabric,
        loader_version: "0.15.7".into(),
        minecraft_version: "1.20.4".into(),
    };
    let report = compat::check(&env, &found);
    assert!(report.is_compatible(), "issues: {:?}", report.issues);
    assert_eq!(report.mods_checked, 1);
}
