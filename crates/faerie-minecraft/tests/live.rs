//! Live tests against the real world (Mojang's CDN, this machine's JVMs).
//! Ignored by default so CI and normal test runs stay hermetic; run with:
//! `cargo test -p faerie-minecraft --test live -- --ignored --nocapture`

use faerie_minecraft::manifest::{ManifestService, ManifestSource};

#[tokio::test]
#[ignore = "requires network access to piston-meta.mojang.com"]
async fn real_mojang_manifest_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let service = ManifestService::new(faerie_net::build_client(), tmp.path(), None);
    let result = service.fetch(false).await.expect("manifest fetch");

    assert_eq!(result.source, ManifestSource::Network);
    assert!(
        result.manifest.versions.len() > 500,
        "Mojang lists hundreds of versions, got {}",
        result.manifest.versions.len()
    );
    assert!(result.manifest.versions.iter().any(|v| v.id == "1.8.9"));
    println!(
        "latest release: {} · latest snapshot: {} · {} versions total",
        result.manifest.latest.release,
        result.manifest.latest.snapshot,
        result.manifest.versions.len()
    );
}

#[tokio::test]
#[ignore = "depends on the machine's installed JVMs"]
async fn real_java_detection_reports_installations() {
    let tmp = tempfile::tempdir().unwrap();
    let found = faerie_minecraft::java::detect_installations(tmp.path()).await;
    println!("detected {} java installation(s):", found.len());
    for java in &found {
        println!(
            "  Java {} ({}, {}, {}) at {}",
            java.major,
            java.version,
            java.vendor,
            java.source,
            java.path.display()
        );
    }
    // No assertion on count: a machine may legitimately have none.
}
