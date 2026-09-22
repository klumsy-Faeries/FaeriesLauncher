//! Instance-store benchmarks (§43: "responsive with dozens/hundreds of
//! instances").
//!
//! `list` is on the path of every page that shows instances — Home, the
//! instance picker, the command palette — so it runs constantly and must
//! stay cheap even for a large collection.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use faerie_instances::InstanceStore;

fn populated(count: usize) -> (tempfile::TempDir, InstanceStore) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("instances");
    std::fs::create_dir_all(&root).unwrap();
    let store = InstanceStore::new(root);
    for i in 0..count {
        store
            .create(&format!("Instance {i}"), "26.2")
            .expect("instance created");
    }
    (tmp, store)
}

fn listing(c: &mut Criterion) {
    let mut group = c.benchmark_group("instance_list");
    for count in [10usize, 50, 200] {
        let (_tmp, store) = populated(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &store, |b, store| {
            b.iter(|| {
                let (instances, problems) = store.list();
                assert!(problems.is_empty());
                std::hint::black_box(instances)
            });
        });
    }
    group.finish();
}

fn creating(c: &mut Criterion) {
    // Creation makes the standard subdirectory tree and writes instance.json,
    // so it is dominated by filesystem calls rather than our own work.
    c.bench_function("instance_create", |b| {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("instances");
        std::fs::create_dir_all(&root).unwrap();
        let store = InstanceStore::new(root);
        let mut n = 0usize;
        b.iter(|| {
            n += 1;
            std::hint::black_box(store.create(&format!("Bench {n}"), "26.2").unwrap())
        });
    });
}

criterion_group!(benches, listing, creating);
criterion_main!(benches);
