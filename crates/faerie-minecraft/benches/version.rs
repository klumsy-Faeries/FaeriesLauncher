//! Benchmarks for the launch-critical metadata path (§43).
//!
//! Every launch parses a version JSON, walks its `inheritsFrom` chain,
//! evaluates rules over every library, and builds the argument vector. Real
//! 26.2 metadata carries ~130 libraries, which is what these sizes model.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use faerie_minecraft::version::rules::{self, FeatureSet, RuleContext};
use faerie_minecraft::version::{args, VersionDetail};

/// Build a version JSON with `libraries` entries, half of them rule-gated
/// the way Mojang gates natives and platform-specific artifacts.
fn version_json(id: &str, libraries: usize, inherits: Option<&str>) -> String {
    let mut libs = Vec::with_capacity(libraries);
    for i in 0..libraries {
        let gated = i % 2 == 0;
        let rules = if gated {
            r#", "rules": [{ "action": "allow", "os": { "name": "windows" } }]"#
        } else {
            ""
        };
        libs.push(format!(
            r#"{{ "name": "org.example:lib{i}:1.0.{i}",
                 "downloads": {{ "artifact": {{
                   "path": "org/example/lib{i}/1.0.{i}/lib{i}-1.0.{i}.jar",
                   "url": "https://libraries.minecraft.net/org/example/lib{i}.jar",
                   "sha1": "0123456789abcdef0123456789abcdef01234567",
                   "size": {} }} }}{rules} }}"#,
            10_000 + i
        ));
    }
    let inherits = inherits
        .map(|p| format!(r#""inheritsFrom": "{p}","#))
        .unwrap_or_default();

    format!(
        r#"{{
  "id": "{id}",
  {inherits}
  "type": "release",
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "32",
  "javaVersion": {{ "component": "java-runtime-epsilon", "majorVersion": 25 }},
  "assetIndex": {{ "id": "32", "url": "https://example.invalid/32.json",
                   "sha1": "abc", "size": 100, "totalSize": 200 }},
  "downloads": {{ "client": {{ "url": "https://example.invalid/client.jar",
                               "sha1": "def", "size": 25000000 }} }},
  "libraries": [{}],
  "arguments": {{
    "game": ["--username", "${{auth_player_name}}", "--version", "${{version_name}}",
             "--gameDir", "${{game_directory}}", "--assetsDir", "${{assets_root}}",
             "--assetIndex", "${{assets_index_name}}", "--uuid", "${{auth_uuid}}",
             "--accessToken", "${{auth_access_token}}",
             {{ "rules": [{{ "action": "allow", "features": {{ "is_demo_user": true }} }}],
                "value": "--demo" }},
             {{ "rules": [{{ "action": "allow", "features": {{ "has_custom_resolution": true }} }}],
                "value": ["--width", "${{resolution_width}}", "--height", "${{resolution_height}}"] }}],
    "jvm": [{{ "rules": [{{ "action": "allow", "os": {{ "name": "windows" }} }}],
               "value": "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance" }},
            "-Djava.library.path=${{natives_directory}}",
            "-cp", "${{classpath}}"]
  }}
}}"#,
        libs.join(",")
    )
}

fn parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("version_parse");
    for count in [30usize, 130, 400] {
        let raw = version_json("26.2", count, None);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &raw, |b, raw| {
            b.iter(|| std::hint::black_box(VersionDetail::from_json(raw).unwrap()));
        });
    }
    group.finish();
}

fn inheritance(c: &mut Criterion) {
    // What a Fabric instance does: a small loader profile layered onto full
    // vanilla metadata.
    let parent_raw = version_json("26.2", 130, None);
    let child_raw = version_json("fabric-loader-0.19.5-26.2", 12, Some("26.2"));

    c.bench_function("version_inherit", |b| {
        b.iter(|| {
            let parent = VersionDetail::from_json(&parent_raw).unwrap();
            let child = VersionDetail::from_json(&child_raw).unwrap();
            std::hint::black_box(child.resolve_onto(parent))
        });
    });
}

fn rule_evaluation(c: &mut Criterion) {
    let raw = version_json("26.2", 130, None);
    let detail = VersionDetail::from_json(&raw).unwrap();
    let ctx = RuleContext::current(FeatureSet::default());

    c.bench_function("library_rule_filter_130", |b| {
        b.iter(|| {
            let kept = detail
                .libraries
                .iter()
                .filter(|lib| rules::allowed(&lib.rules, &ctx))
                .count();
            std::hint::black_box(kept)
        });
    });
}

fn argument_building(c: &mut Criterion) {
    let raw = version_json("26.2", 130, None);
    let detail = VersionDetail::from_json(&raw).unwrap();
    let arguments = detail.arguments.clone().unwrap();
    let ctx = RuleContext::current(FeatureSet::default());

    let lookup = |name: &str| -> Option<String> {
        Some(match name {
            "auth_player_name" => "Faerie".into(),
            "version_name" => "26.2".into(),
            "game_directory" => r"C:\instances\demo".into(),
            "assets_root" => r"C:\data\assets".into(),
            "assets_index_name" => "32".into(),
            "auth_uuid" => "0000-0000".into(),
            "auth_access_token" => "token".into(),
            "natives_directory" => r"C:\data\natives".into(),
            "classpath" => "a.jar;b.jar;c.jar".into(),
            _ => return None,
        })
    };

    c.bench_function("build_game_arguments", |b| {
        b.iter(|| {
            let resolved = args::resolve(&arguments.game, &ctx);
            let substituted: Vec<String> = resolved
                .iter()
                .map(|arg| args::substitute(arg, &lookup))
                .collect();
            std::hint::black_box(substituted)
        });
    });
}

criterion_group!(
    benches,
    parsing,
    inheritance,
    rule_evaluation,
    argument_building
);
criterion_main!(benches);
