//! Manifest service tests against the in-process HTTP server: network,
//! ETag revalidation, freshness window, and offline fallback.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use faerie_minecraft::manifest::{ManifestService, ManifestSource};
use test_support::{Response, TestServer};

const BODY: &str = r#"{
    "latest": { "release": "26.2", "snapshot": "26w35a" },
    "versions": [
        { "id": "26.2", "type": "release",
          "url": "http://example.invalid/26.2.json",
          "releaseTime": "2026-07-20T10:00:00+00:00", "sha1": "abc" }
    ]
}"#;

fn service(cache_dir: &std::path::Path, url: String) -> ManifestService {
    ManifestService::new(faerie_net::build_client(), cache_dir, Some(url))
}

#[tokio::test]
async fn fetches_caches_and_serves_fresh_without_refetching() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_seen = Arc::clone(&hits);
    let server = TestServer::start(move |_req| {
        hits_seen.fetch_add(1, Ordering::SeqCst);
        Response::ok(BODY.as_bytes().to_vec()).with_header("ETag", "\"v1\"")
    })
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let service = service(tmp.path(), server.url("/manifest.json"));

    let first = service.fetch(false).await.unwrap();
    assert_eq!(first.source, ManifestSource::Network);
    assert_eq!(first.manifest.latest.release, "26.2");
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Within the freshness window: served from cache, zero requests.
    let second = service.fetch(false).await.unwrap();
    assert_eq!(second.source, ManifestSource::CacheFresh);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn force_refresh_revalidates_with_etag() {
    let saw_etag = Arc::new(AtomicUsize::new(0));
    let etags = Arc::clone(&saw_etag);
    let server = TestServer::start(move |req| {
        if req.header("if-none-match") == Some("\"v1\"") {
            etags.fetch_add(1, Ordering::SeqCst);
            return Response::ok(Vec::new()).with_status(304);
        }
        Response::ok(BODY.as_bytes().to_vec()).with_header("ETag", "\"v1\"")
    })
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let service = service(tmp.path(), server.url("/manifest.json"));

    service.fetch(false).await.unwrap();
    let revalidated = service.fetch(true).await.unwrap();
    assert_eq!(revalidated.source, ManifestSource::Network);
    assert_eq!(revalidated.manifest.versions.len(), 1);
    assert_eq!(
        saw_etag.load(Ordering::SeqCst),
        1,
        "second fetch sent the ETag"
    );
}

#[tokio::test]
async fn offline_serves_stale_cache_instead_of_failing() {
    let tmp = tempfile::tempdir().unwrap();
    let url;
    {
        let server = TestServer::start(move |_req| Response::ok(BODY.as_bytes().to_vec())).await;
        url = server.url("/manifest.json");
        let service = service(tmp.path(), url.clone());
        service.fetch(false).await.unwrap();
    } // server dropped: network is now "down"

    let service = service(tmp.path(), url);
    let result = service.fetch(true).await.unwrap();
    assert_eq!(result.source, ManifestSource::CacheStale);
    assert_eq!(result.manifest.latest.release, "26.2");
}

#[tokio::test]
async fn no_network_and_no_cache_is_a_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    // Nothing listens on this port (bound then immediately dropped).
    let unreachable = {
        let server = TestServer::start(|_req| Response::ok(Vec::new())).await;
        server.url("/manifest.json")
    };
    let service = service(tmp.path(), unreachable);
    let error = service.fetch(false).await.unwrap_err();
    let text = error.to_string();
    assert!(text.contains("no cached copy"), "got: {text}");
}
