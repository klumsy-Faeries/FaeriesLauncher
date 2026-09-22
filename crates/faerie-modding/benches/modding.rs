//! Benchmarks for the modding hot paths (§43).
//!
//! These are the operations that run against *user-sized* data: a big pack
//! is several hundred jars, and every one is opened, parsed, and checked
//! each time the Mods page opens. The sizes below bracket realistic packs.

use std::io::Write;
use std::path::{Path, PathBuf};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use faerie_modding::compat::{self, Environment};
use faerie_modding::scan::{self, LoaderKind};
use faerie_modding::version_range::{Version, VersionReq};

/// A realistic Fabric descriptor: a handful of dependencies, one conflict.
fn descriptor(index: usize) -> String {
    format!(
        r#"{{
  "schemaVersion": 1,
  "id": "mod{index}",
  "version": "1.{index}.0",
  "name": "Benchmark Mod {index}",
  "description": "A mod used to measure scanning throughput.",
  "authors": ["Someone", "Someone Else"],
  "depends": {{ "minecraft": ">=1.20.4", "fabricloader": ">=0.15.0", "fabric-api": "*" }},
  "recommends": {{ "sodium": ">=0.5.0" }},
  "breaks": {{ "conflicting{index}": "*" }}
}}"#
    )
}

/// Write `count` jars that look like real mods: a descriptor, a manifest,
/// and enough class-ish padding that the zip is not trivially small.
fn build_pack(dir: &Path, count: usize) -> PathBuf {
    let mods = dir.join(format!("mods{count}"));
    std::fs::create_dir_all(&mods).unwrap();
    let padding = vec![0u8; 24 * 1024];

    for index in 0..count {
        let path = mods.join(format!("mod{index}.jar"));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
        zip.write_all(b"Manifest-Version: 1.0\n").unwrap();
        // Class files come before the descriptor in most real jars, so the
        // reader cannot rely on it being the first entry.
        zip.start_file("net/example/Mod.class", options).unwrap();
        zip.write_all(&padding).unwrap();
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(descriptor(index).as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    mods
}

fn scanning(c: &mut Criterion) {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("mod_scan");
    // 50 = a modest pack, 200 = a large one, 500 = the spec's stress case.
    for count in [50usize, 200, 500] {
        let dir = build_pack(tmp.path(), count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &dir, |b, dir| {
            b.iter(|| {
                let (mods, problems) = runtime.block_on(scan::scan_directory(dir));
                assert!(problems.is_empty());
                std::hint::black_box(mods)
            });
        });
    }
    group.finish();
}

fn single_jar(c: &mut Criterion) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = build_pack(tmp.path(), 1);
    let jar = dir.join("mod0.jar");
    c.bench_function("scan_one_jar", |b| {
        b.iter(|| std::hint::black_box(scan::scan_jar(&jar).unwrap()));
    });
}

fn compatibility(c: &mut Criterion) {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("compat_check");
    for count in [50usize, 200, 500] {
        let dir = build_pack(tmp.path(), count);
        let (mods, _) = runtime.block_on(scan::scan_directory(&dir));
        let env = Environment {
            loader: LoaderKind::Fabric,
            loader_version: "0.15.7".into(),
            minecraft_version: "1.20.4".into(),
        };
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &mods, |b, mods| {
            b.iter(|| std::hint::black_box(compat::check(&env, mods)));
        });
    }
    group.finish();
}

fn version_ranges(c: &mut Criterion) {
    // Every dependency check runs one of these, so the constant matters at
    // pack scale even though each call is tiny.
    let installed = Version::parse("1.20.4").unwrap();
    let simple = VersionReq::parse(">=1.20");
    let compound = VersionReq::parse(">=1.20.1 <1.21 || 1.19.4");
    let maven = VersionReq::parse_maven("[1.20,1.21)");

    c.bench_function("range_parse_compound", |b| {
        b.iter(|| std::hint::black_box(VersionReq::parse(">=1.20.1 <1.21 || 1.19.4")));
    });
    c.bench_function("range_match_simple", |b| {
        b.iter(|| std::hint::black_box(simple.matches(&installed)));
    });
    c.bench_function("range_match_compound", |b| {
        b.iter(|| std::hint::black_box(compound.matches(&installed)));
    });
    c.bench_function("range_match_maven", |b| {
        b.iter(|| std::hint::black_box(maven.matches(&installed)));
    });
}

criterion_group!(benches, scanning, single_jar, compatibility, version_ranges);
criterion_main!(benches);
