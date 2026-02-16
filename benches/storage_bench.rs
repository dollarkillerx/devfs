use std::collections::HashMap;

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::Rng;
use tokio::io::AsyncReadExt;
use tokio::runtime::Runtime;

use devfs::storage::FsStorage;
use devfs::types::ListObjectsV2Params;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_random_bytes(size: usize) -> Bytes {
    let mut rng = rand::thread_rng();
    let data: Vec<u8> = (0..size).map(|_| rng.gen()).collect();
    Bytes::from(data)
}

fn setup_storage_with_bucket() -> (tempfile::TempDir, FsStorage, &'static str) {
    let tmp = tempfile::tempdir().unwrap();
    let storage = FsStorage::new(tmp.path());
    let rt = rt();
    rt.block_on(storage.create_bucket("bench")).unwrap();
    (tmp, storage, "bench")
}

fn populate_objects(storage: &FsStorage, bucket: &str, count: usize) {
    let rt = rt();
    rt.block_on(async {
        for i in 0..count {
            let group = i % 10;
            let key = format!("group-{:02}/obj-{:04}", group, i);
            let body = Bytes::from(format!("data-{}", i));
            storage
                .put_object(bucket, &key, body, None, HashMap::new())
                .await
                .unwrap();
        }
    });
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn skip_large_in_ci() -> bool {
    std::env::var("CI").is_ok()
}

// ---------------------------------------------------------------------------
// Group 1: streaming_upload
// ---------------------------------------------------------------------------

fn bench_streaming_upload(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_upload");

    let sizes: &[(u64, &str)] = &[
        (1_024, "1KB"),
        (1_024 * 1_024, "1MB"),
        (10 * 1_024 * 1_024, "10MB"),
        (100 * 1_024 * 1_024, "100MB"),
    ];

    for &(size, label) in sizes {
        if size >= 100 * 1_024 * 1_024 && skip_large_in_ci() {
            continue;
        }

        if size >= 10 * 1_024 * 1_024 {
            group.sample_size(10);
        }

        group.throughput(Throughput::Bytes(size));

        let data = create_random_bytes(size as usize);

        group.bench_with_input(
            BenchmarkId::new("put_object_stream", label),
            &data,
            |b, data| {
                let (_tmp, storage, bucket) = setup_storage_with_bucket();
                let rt = rt();
                b.iter(|| {
                    let body = axum::body::Body::from(data.clone());
                    rt.block_on(
                        storage.put_object_stream(
                            bucket,
                            "bench-key",
                            body,
                            Some("application/octet-stream".to_string()),
                            HashMap::new(),
                            Some(data.len() as u64),
                        ),
                    )
                    .unwrap();
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Group 2: streaming_download
// ---------------------------------------------------------------------------

fn bench_streaming_download(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_download");

    let sizes: &[(u64, &str)] = &[
        (1_024, "1KB"),
        (1_024 * 1_024, "1MB"),
        (10 * 1_024 * 1_024, "10MB"),
        (100 * 1_024 * 1_024, "100MB"),
    ];

    for &(size, label) in sizes {
        if size >= 100 * 1_024 * 1_024 && skip_large_in_ci() {
            continue;
        }

        if size >= 10 * 1_024 * 1_024 {
            group.sample_size(10);
        }

        group.throughput(Throughput::Bytes(size));

        let data = create_random_bytes(size as usize);

        group.bench_with_input(
            BenchmarkId::new("get_object_stream", label),
            &data,
            |b, data| {
                let (_tmp, storage, bucket) = setup_storage_with_bucket();
                let rt = rt();
                // Pre-populate the object
                rt.block_on(
                    storage.put_object(
                        bucket,
                        "bench-key",
                        data.clone(),
                        Some("application/octet-stream".to_string()),
                        HashMap::new(),
                    ),
                )
                .unwrap();

                b.iter(|| {
                    rt.block_on(async {
                        let result = storage.get_object_stream(bucket, "bench-key").await.unwrap();
                        let mut buf = Vec::with_capacity(data.len());
                        let mut file = result.file;
                        file.read_to_end(&mut buf).await.unwrap();
                        buf
                    })
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Group 3: list_objects_v2
// ---------------------------------------------------------------------------

fn bench_list_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_objects_v2");

    // --- flat listing (no prefix, no delimiter) ---
    for &count in &[100, 1_000, 10_000] {
        if count >= 10_000 {
            group.sample_size(10);
        }

        group.bench_with_input(
            BenchmarkId::new("flat", count),
            &count,
            |b, &count| {
                let (_tmp, storage, bucket) = setup_storage_with_bucket();
                populate_objects(&storage, bucket, count);
                let rt = rt();

                let params = ListObjectsV2Params {
                    max_keys: count as u32,
                    ..Default::default()
                };

                b.iter(|| {
                    rt.block_on(storage.list_objects_v2(bucket, &params)).unwrap();
                });
            },
        );
    }

    // --- prefix pruning ---
    for &count in &[1_000, 10_000] {
        if count >= 10_000 {
            group.sample_size(10);
        }

        group.bench_with_input(
            BenchmarkId::new("prefix_pruning", count),
            &count,
            |b, &count| {
                let (_tmp, storage, bucket) = setup_storage_with_bucket();
                populate_objects(&storage, bucket, count);
                let rt = rt();

                let params = ListObjectsV2Params {
                    prefix: Some("group-05/".to_string()),
                    max_keys: count as u32,
                    ..Default::default()
                };

                b.iter(|| {
                    rt.block_on(storage.list_objects_v2(bucket, &params)).unwrap();
                });
            },
        );
    }

    // --- delimiter grouping ---
    for &count in &[1_000, 10_000] {
        if count >= 10_000 {
            group.sample_size(10);
        }

        group.bench_with_input(
            BenchmarkId::new("delimiter", count),
            &count,
            |b, &count| {
                let (_tmp, storage, bucket) = setup_storage_with_bucket();
                populate_objects(&storage, bucket, count);
                let rt = rt();

                let params = ListObjectsV2Params {
                    delimiter: Some("/".to_string()),
                    max_keys: count as u32,
                    ..Default::default()
                };

                b.iter(|| {
                    rt.block_on(storage.list_objects_v2(bucket, &params)).unwrap();
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Group 4: etag_consistency
// ---------------------------------------------------------------------------

fn bench_etag_consistency(c: &mut Criterion) {
    let mut group = c.benchmark_group("etag_consistency");

    for &(size, label) in &[(1_024u64, "1KB"), (1_024 * 1_024, "1MB")] {
        let data = create_random_bytes(size as usize);

        group.bench_with_input(
            BenchmarkId::new("stream_vs_bulk", label),
            &data,
            |b, data| {
                let (_tmp, storage, bucket) = setup_storage_with_bucket();
                let rt = rt();

                b.iter(|| {
                    rt.block_on(async {
                        // Bulk upload
                        let etag_bulk = storage
                            .put_object(
                                bucket,
                                "etag-bulk",
                                data.clone(),
                                None,
                                HashMap::new(),
                            )
                            .await
                            .unwrap();

                        // Stream upload
                        let body = axum::body::Body::from(data.clone());
                        let etag_stream = storage
                            .put_object_stream(
                                bucket,
                                "etag-stream",
                                body,
                                None,
                                HashMap::new(),
                                Some(data.len() as u64),
                            )
                            .await
                            .unwrap();

                        assert_eq!(etag_bulk, etag_stream);
                    });
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_streaming_upload,
    bench_streaming_download,
    bench_list_objects,
    bench_etag_consistency
);
criterion_main!(benches);
