//! Live end-to-end checks against Mojang's real metadata and CDN.
//! Ignored by default (network + hundreds of MB). Run explicitly:
//! `cargo test -p faerie-minecraft --test live_install -- --ignored --nocapture`

use std::path::PathBuf;

use faerie_minecraft::install::Installer;
use faerie_minecraft::launch::{self, LaunchOptions, Session};
use faerie_minecraft::manifest::ManifestService;
use faerie_minecraft::GamePaths;
use faerie_net::DownloadConfig;
use tokio_util::sync::CancellationToken;

/// Where live-install artifacts go. Reused across runs so repeated runs are
/// fast and exercise the "already valid, skip" path.
fn live_root() -> PathBuf {
    std::env::var_os("FAERIE_LIVE_TEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("faerie-live-test"))
}

#[tokio::test]
#[ignore = "network: resolves the real 26.2 version metadata"]
async fn resolves_real_version_metadata() {
    let root = live_root();
    let paths = GamePaths::new(root.join("data"));
    paths.ensure_base_dirs().unwrap();
    let client = faerie_net::build_client();

    let manifest = ManifestService::new(client.clone(), &root, None)
        .fetch(false)
        .await
        .expect("manifest");
    let latest = manifest.manifest.latest.release.clone();
    println!("latest release from Mojang: {latest}");

    // Resolve metadata only (no asset download) by loading the version JSON.
    let summary = manifest
        .manifest
        .versions
        .iter()
        .find(|v| v.id == latest)
        .expect("latest release is listed");
    let raw = client
        .get(&summary.url)
        .send()
        .await
        .expect("fetch version json")
        .text()
        .await
        .expect("body");
    let detail = faerie_minecraft::version::VersionDetail::from_json(&raw).expect("parse");

    println!(
        "id={} mainClass={:?} javaMajor={:?} libraries={} assetIndex={:?}",
        detail.id,
        detail.main_class,
        detail.java_version.as_ref().map(|j| j.major_version),
        detail.libraries.len(),
        detail.asset_index.as_ref().map(|a| a.id.clone()),
    );
    assert_eq!(detail.id, latest);
    assert!(
        detail.main_class.is_some(),
        "a client version has a main class"
    );
    assert!(
        !detail.libraries.is_empty(),
        "a client version has libraries"
    );
    assert!(
        detail.asset_index.is_some(),
        "a client version has an asset index"
    );
    assert!(
        detail
            .downloads
            .as_ref()
            .and_then(|d| d.client.as_ref())
            .is_some(),
        "a client version has a client jar"
    );
    assert!(
        detail.arguments.is_some(),
        "26.x uses the modern argument model"
    );
}

#[tokio::test]
#[ignore = "network + disk: downloads the full latest release (hundreds of MB)"]
async fn installs_latest_release_and_builds_a_launch_command() {
    let root = live_root();
    let paths = GamePaths::new(root.join("data"));
    paths.ensure_base_dirs().unwrap();
    let client = faerie_net::build_client();

    let manifest = ManifestService::new(client.clone(), &root, None)
        .fetch(false)
        .await
        .expect("manifest");
    let version_id = manifest.manifest.latest.release.clone();

    let installer = Installer::new(
        client,
        paths.clone(),
        DownloadConfig {
            concurrency: 8,
            retries: 3,
        },
    );
    let cancel = CancellationToken::new();
    let mut last_report = std::time::Instant::now();
    let installed = installer
        .install(&version_id, &manifest.manifest, &cancel, |p| {
            if last_report.elapsed() > std::time::Duration::from_secs(2) {
                last_report = std::time::Instant::now();
                println!(
                    "  {} {}/{} files, {:.1} MB, {:.1} MB/s",
                    p.phase,
                    p.files_done,
                    p.files_total,
                    p.bytes_done as f64 / 1_048_576.0,
                    p.bytes_per_sec as f64 / 1_048_576.0,
                );
            }
        })
        .await
        .expect("install");

    println!(
        "installed {} (java {:?})",
        installed.id, installed.required_java_major
    );

    // Every classpath entry must exist on disk after a successful install.
    let java = faerie_minecraft::java::detect_installations(&paths.java).await;
    let choice = launch::select_java(&java, installed.required_java_major, None)
        .expect("some java is installed on this machine");
    let options = LaunchOptions {
        game_dir: root.join("instance"),
        java_path: choice.path(),
        session: Session::offline("FaerieTest"),
        min_ram_mb: Some(1024),
        max_ram_mb: Some(2048),
        extra_jvm_args: vec![],
        env: vec![],
        resolution: None,
        launcher_name: "FaeriesLauncher".into(),
        launcher_version: "0.3.0".into(),
    };
    let spec = launch::build_spec(&installed, &paths, &options);

    let sep = if cfg!(windows) { ';' } else { ':' };
    let cp_index = spec
        .args
        .iter()
        .position(|a| a == "-cp")
        .expect("-cp present");
    let classpath = &spec.args[cp_index + 1];
    let entries: Vec<&str> = classpath.split(sep).collect();
    println!("classpath has {} entries", entries.len());
    let mut missing = Vec::new();
    for entry in &entries {
        if !PathBuf::from(entry).is_file() {
            missing.push(*entry);
        }
    }
    assert!(missing.is_empty(), "missing classpath files: {missing:?}");

    assert!(
        paths.version_jar(&installed.client_jar_id).is_file(),
        "client jar present"
    );
    let natives = paths.natives_dir(&installed.id);
    if natives.is_dir() {
        let count = std::fs::read_dir(&natives).unwrap().count();
        println!("natives extracted: {count} file(s)");
    }
    println!(
        "launch command: {} {}",
        spec.program.display(),
        spec.args.join(" ")
    );
}

#[tokio::test]
#[ignore = "runs the real game JVM briefly to prove the command line is valid"]
async fn launches_the_game_jvm_and_reaches_minecraft_code() {
    use faerie_minecraft::process::GameProcess;

    let root = live_root();
    let paths = GamePaths::new(root.join("data"));
    let client = faerie_net::build_client();
    let manifest = ManifestService::new(client.clone(), &root, None)
        .fetch(false)
        .await
        .expect("manifest");
    let version_id = manifest.manifest.latest.release.clone();

    let installer = Installer::new(client, paths.clone(), DownloadConfig::default());
    let installed = installer
        .install(
            &version_id,
            &manifest.manifest,
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .expect("install (run the install test first for a warm cache)");

    let game_dir = root.join("instance");
    std::fs::create_dir_all(&game_dir).unwrap();

    // 26.2 requires Java 25, which this machine may not have. Provision the
    // runtime the version metadata itself names (exercises the provisioner).
    let required = installed.required_java_major;
    let detected = faerie_minecraft::java::detect_installations(&paths.java).await;
    let suitable = detected
        .iter()
        .find(|j| required.is_none_or(|r| j.major >= r))
        .map(|j| j.path.clone());
    let java_path = match suitable {
        Some(path) => {
            println!("using detected java at {}", path.display());
            path
        }
        None => {
            let component = installed
                .required_java_component
                .clone()
                .unwrap_or_else(|| {
                    faerie_minecraft::java_runtime::component_for_major(required.unwrap_or(21))
                        .to_string()
                });
            println!("no suitable java found; provisioning {component}…");
            let provisioner = faerie_minecraft::java_runtime::RuntimeProvisioner::new(
                faerie_net::build_client(),
                faerie_net::Downloader::new(faerie_net::build_client(), DownloadConfig::default()),
                paths.clone(),
            );
            let exe = provisioner
                .provision(
                    &component,
                    None,
                    &CancellationToken::new(),
                    |done, total| {
                        if done % 200 == 0 {
                            println!("  runtime {done}/{total} files");
                        }
                    },
                )
                .await
                .expect("provision java runtime");
            println!("provisioned java at {}", exe.display());
            assert!(exe.is_file(), "the provisioned java executable exists");
            exe
        }
    };

    let options = LaunchOptions {
        game_dir: game_dir.clone(),
        java_path,
        session: Session::offline("FaerieTest"),
        min_ram_mb: Some(1024),
        max_ram_mb: Some(2048),
        extra_jvm_args: vec![],
        env: vec![],
        resolution: Some((854, 480)),
        launcher_name: "FaeriesLauncher".into(),
        launcher_version: "0.3.0".into(),
    };
    let spec = launch::build_spec(&installed, &paths, &options);

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let mut process = GameProcess::spawn(&spec, tx).expect("spawn the game JVM");
    println!("spawned pid {:?}", process.id());

    // Collect output briefly: enough to prove the JVM started and Minecraft's
    // own code ran. We stop it rather than leaving a game window open.
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(line)) => {
                println!("[game] {}", line.text);
                lines.push(line.text);
                // Minecraft's own bootstrap logging means we got all the way in.
                if lines.iter().any(|l| {
                    l.contains("Setting user")
                        || l.contains("LWJGL Version")
                        || l.contains("Backend library")
                        || l.contains("Narrator library")
                }) {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    let _ = process.kill().await;
    let report = process.wait().await;
    println!("exit: {report:?}");

    assert!(!lines.is_empty(), "the game produced no output at all");
    let joined = lines.join("\n");
    assert!(
        !joined.contains("Could not create the Java Virtual Machine")
            && !joined.contains("Unrecognized option")
            && !joined.contains("Could not find or load main class"),
        "the JVM rejected our command line:\n{joined}"
    );
}
