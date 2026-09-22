//! Modrinth resolution and preset planning against a local server that
//! speaks the two endpoints the launcher uses.

use faerie_modding::modrinth::{Channel, ModrinthClient};
use faerie_modding::presets::{self, BundledMod, BundledPack, Preset, PresetMod};
use faerie_modding::scan::LoaderKind;
use test_support::{Request, Response, TestServer};

fn json(body: &str) -> Response {
    Response::ok(body.as_bytes().to_vec()).with_header("content-type", "application/json")
}

fn version(
    project_id: &str,
    number: &str,
    channel: &str,
    date: &str,
    file: &str,
    deps: &[&str],
) -> String {
    let deps: Vec<String> = deps
        .iter()
        .map(|d| format!(r#"{{"project_id":"{d}","dependency_type":"required"}}"#))
        .collect();
    format!(
        r#"{{"project_id":"{project_id}","version_number":"{number}","version_type":"{channel}",
            "date_published":"{date}","dependencies":[{}],
            "files":[{{"url":"https://cdn.example/{file}","filename":"{file}","primary":true,
                       "size":1234,"hashes":{{"sha1":"da39a3ee5e6b4b0d3255bfef95601890afd80709"}}}}]}}"#,
        deps.join(",")
    )
}

fn server_handler(req: &Request) -> Response {
    let path = req.path.split('?').next().unwrap_or("");
    let query = req.path.split('?').nth(1).unwrap_or("");
    let for_26 = query.contains("26.2");
    match path {
        // Sodium: only a beta exists for 26.2; a release exists for 26.1.
        "/project/sodium/version" if for_26 => json(&format!(
            "[{}]",
            version(
                "SOD",
                "0.9.2-beta.1",
                "beta",
                "2026-08-01",
                "sodium-beta.jar",
                &[]
            )
        )),
        "/project/sodium/version" => json(&format!(
            "[{},{}]",
            version(
                "SOD",
                "0.9.1",
                "release",
                "2026-06-01",
                "sodium-0.9.1.jar",
                &[]
            ),
            version(
                "SOD",
                "0.9.2-beta.1",
                "beta",
                "2026-07-01",
                "sodium-beta.jar",
                &[]
            ),
        )),
        // Lithium: release available, older beta listed first to prove sorting.
        "/project/lithium/version" => json(&format!(
            "[{},{}]",
            version(
                "LIT",
                "0.26.0-beta",
                "beta",
                "2026-09-01",
                "lithium-beta.jar",
                &["FAPI"]
            ),
            version(
                "LIT",
                "0.25.3",
                "release",
                "2026-08-15",
                "lithium-0.25.3.jar",
                &["FAPI"]
            ),
        )),
        // ModernFix: nothing for this version.
        "/project/modernfix/version" => json("[]"),
        // Fabric API, reachable both by slug and by id.
        "/project/fabric-api/version" => json(&format!(
            "[{}]",
            version(
                "FAPI",
                "0.159.0",
                "release",
                "2026-08-20",
                "fabric-api.jar",
                &[]
            )
        )),
        "/project/FAPI" => json(r#"{"slug":"fabric-api","id":"FAPI"}"#),
        _ => Response::ok(b"{}".to_vec()).with_status(404),
    }
}

#[tokio::test]
async fn release_beats_newer_prerelease() {
    let server = TestServer::start(server_handler).await;
    let client = ModrinthClient::with_base(faerie_net::build_client(), server.url(""));

    let file = client
        .resolve("lithium", "26.1", LoaderKind::Fabric, false)
        .await
        .unwrap()
        .expect("lithium has a release");
    assert_eq!(file.version, "0.25.3");
    assert_eq!(file.channel, Channel::Release);
    assert_eq!(file.file_name, "lithium-0.25.3.jar");
    assert_eq!(file.required, ["FAPI"]);
    assert_eq!(file.sha1, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

#[tokio::test]
async fn prerelease_only_when_allowed() {
    let server = TestServer::start(server_handler).await;
    let client = ModrinthClient::with_base(faerie_net::build_client(), server.url(""));

    let strict = client
        .resolve("sodium", "26.2", LoaderKind::Fabric, false)
        .await
        .unwrap();
    assert!(strict.is_none(), "a beta must not be picked silently");

    let loose = client
        .resolve("sodium", "26.2", LoaderKind::Fabric, true)
        .await
        .unwrap()
        .expect("beta accepted when allowed");
    assert_eq!(loose.channel, Channel::Beta);
}

#[tokio::test]
async fn missing_build_is_none_not_error() {
    let server = TestServer::start(server_handler).await;
    let client = ModrinthClient::with_base(faerie_net::build_client(), server.url(""));
    let none = client
        .resolve("modernfix", "26.2", LoaderKind::Fabric, true)
        .await
        .unwrap();
    assert!(none.is_none());
}

#[tokio::test]
async fn plan_resolves_dependencies_and_reports_skips() {
    let server = TestServer::start(server_handler).await;
    let client = ModrinthClient::with_base(faerie_net::build_client(), server.url(""));

    // Fabric API is deliberately *not* in this preset: Lithium requires it,
    // so the planner must pull it in by project id.
    const MODS: &[PresetMod] = &[
        PresetMod {
            slug: "sodium",
            name: "Sodium",
            reason: "Renderer.",
            prerelease_ok: true,
        },
        PresetMod {
            slug: "lithium",
            name: "Lithium",
            reason: "Logic.",
            prerelease_ok: false,
        },
        PresetMod {
            slug: "modernfix",
            name: "ModernFix",
            reason: "Load times.",
            prerelease_ok: false,
        },
    ];
    // Two bundled jars: one built for this version, one for the previous.
    const BUNDLED: &[BundledMod] = &[
        BundledMod {
            id: "ours",
            name: "Ours",
            reason: "Ships with the launcher.",
            file_name: "ours-1.0+mc26.2.jar",
            game_version: "26.2",
            bytes: b"PK\x03\x04 pretend jar",
        },
        BundledMod {
            id: "old",
            name: "Old",
            reason: "Built for last version.",
            file_name: "old-1.0+mc26.1.jar",
            game_version: "26.1",
            bytes: b"PK\x03\x04 pretend jar",
        },
    ];
    const PACKS: &[BundledPack] = &[BundledPack {
        id: "menu",
        name: "Menu",
        reason: "Looks nice.",
        file_name: "menu.zip",
        vault: None,
        bytes: b"PK\x03\x04 pretend zip",
    }];
    let preset = Preset {
        id: "test",
        name: "Test",
        description: "",
        loader: LoaderKind::Fabric,
        mods: MODS,
        bundled: BUNDLED,
        packs: PACKS,
        retired: &[],
    };

    let plan = presets::plan(&client, &preset, "26.2").await.unwrap();
    let bundled: Vec<&str> = plan.bundled.iter().map(|b| b.file_name).collect();
    assert_eq!(bundled, ["ours-1.0+mc26.2.jar"]);
    let packs: Vec<&str> = plan.packs.iter().map(|p| p.file_name).collect();
    assert_eq!(packs, ["menu.zip"], "packs are not version-gated");
    let names: Vec<&str> = plan.files.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(
        names,
        ["sodium-beta.jar", "lithium-0.25.3.jar", "fabric-api.jar"],
        "preset mods in order, then the dependency"
    );
    assert_eq!(plan.skipped.len(), 2, "{:?}", plan.skipped);
    assert_eq!(plan.skipped[0].name, "Old");
    assert!(plan.skipped[0]
        .reason
        .contains("is for Minecraft 26.1, not 26.2"));
    assert_eq!(plan.skipped[1].name, "ModernFix");
    assert!(plan.skipped[1]
        .reason
        .contains("no Fabric build for Minecraft 26.2"));
}

#[tokio::test]
async fn network_failure_is_an_error_not_a_skip() {
    let server =
        TestServer::start(|_: &Request| Response::ok(b"down".to_vec()).with_status(503)).await;
    let client = ModrinthClient::with_base(faerie_net::build_client(), server.url(""));
    let err = presets::plan(&client, &presets::OPTIMIZED, "26.2")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("503"), "{err}");
}

/// Live: resolve the real Optimized set against Modrinth for the game
/// version the launcher targets, and print what each entry would install.
/// Fails if any listed mod has no build, so it catches a slug typo or a mod
/// that has not shipped for the version yet.
///
/// `cargo test -p faerie-modding --test modrinth -- --ignored live_optimized_preset --nocapture`
#[tokio::test]
#[ignore = "requires network access to api.modrinth.com"]
async fn live_optimized_preset_resolves_for_the_current_game() {
    const GAME_VERSION: &str = "26.2";
    let client = ModrinthClient::new(faerie_net::build_client());
    let plan = presets::plan(&client, &presets::OPTIMIZED, GAME_VERSION)
        .await
        .expect("Modrinth reachable");

    let total: u64 = plan.files.iter().map(|f| f.size).sum();
    for f in &plan.files {
        println!(
            "{:<28} {:<10} {:>8} KB  {}",
            f.project,
            format!("{:?}", f.channel).to_lowercase(),
            f.size / 1024,
            f.version
        );
    }
    println!(
        "{} files, {:.1} MB; skipped: {:?}",
        plan.files.len(),
        total as f64 / 1_048_576.0,
        plan.skipped
    );

    assert!(plan.skipped.is_empty(), "skipped: {:?}", plan.skipped);
    assert_eq!(
        plan.files.len(),
        presets::OPTIMIZED.mods.len(),
        "every dependency is already in the list, so no extra file should be added"
    );
    let betas: Vec<&str> = plan
        .files
        .iter()
        .filter(|f| f.channel != Channel::Release)
        .map(|f| f.project.as_str())
        .collect();
    for slug in &betas {
        let entry = presets::OPTIMIZED
            .mods
            .iter()
            .find(|m| m.slug == *slug)
            .expect("resolved file comes from the list");
        assert!(entry.prerelease_ok, "{slug} resolved to a pre-release");
    }
}
