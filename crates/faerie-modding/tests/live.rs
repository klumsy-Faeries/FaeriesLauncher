//! Live checks against the real loader meta services. Ignored by default so
//! normal runs and CI stay hermetic. Run with:
//! `cargo test -p faerie-modding --test live -- --ignored --nocapture`

use faerie_modding::loader::{adapter_for, LoaderAdapter};
use faerie_modding::scan::LoaderKind;

fn adapter(kind: LoaderKind) -> Box<dyn LoaderAdapter> {
    adapter_for(kind, faerie_net::build_client()).expect("adapter exists")
}

#[tokio::test]
#[ignore = "requires network access to meta.fabricmc.net"]
async fn real_fabric_versions_and_profile_install() {
    let tmp = tempfile::tempdir().unwrap();
    let fabric = adapter(LoaderKind::Fabric);

    let versions = fabric.versions_for("1.20.4").await.expect("loader list");
    assert!(!versions.is_empty(), "Fabric publishes loaders for 1.20.4");
    println!("fabric loaders for 1.20.4: {}", versions.len());
    let newest = &versions[0];
    println!("  newest: {} (stable: {})", newest.version, newest.stable);

    let installed = fabric
        .install("1.20.4", &newest.version, tmp.path())
        .await
        .expect("profile install");
    println!("  wrote version id: {}", installed.version_id);

    // The written profile must be something the Phase 3 pipeline can resolve:
    // it inherits from vanilla and names a main class.
    let path = tmp
        .path()
        .join(&installed.version_id)
        .join(format!("{}.json", installed.version_id));
    let raw = std::fs::read_to_string(&path).expect("profile written");
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["inheritsFrom"], "1.20.4");
    assert!(json["mainClass"].as_str().unwrap().contains("knot"));
    assert!(json["libraries"].as_array().unwrap().len() > 2);
    println!("  mainClass: {}", json["mainClass"]);
}

#[tokio::test]
#[ignore = "requires network access to meta.quiltmc.org"]
async fn real_quilt_versions() {
    let quilt = adapter(LoaderKind::Quilt);
    let versions = quilt.versions_for("1.20.4").await.expect("loader list");
    assert!(!versions.is_empty());
    println!("quilt loaders for 1.20.4: {}", versions.len());
    println!("  newest: {}", versions[0].version);
}

#[tokio::test]
#[ignore = "requires network access to maven.neoforged.net"]
async fn real_neoforge_version_discovery() {
    let neoforge = adapter(LoaderKind::NeoForge);
    let versions = neoforge.versions_for("1.20.4").await.expect("version list");
    println!("neoforge versions for 1.20.4: {}", versions.len());
    if let Some(first) = versions.first() {
        println!("  newest: {}", first.version);
        assert!(first.version.starts_with("20.4."));
    }
}
