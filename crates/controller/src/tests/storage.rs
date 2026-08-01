use std::path::PathBuf;

use crate::protocol::FileLocation;
use crate::storage::{ArtifactStore, LocalArtifactStore, S3ArtifactStore, S3Config};

fn temp_root(test_name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("zkpow-artifact-store-{test_name}-{}-{nonce}", std::process::id()))
}


#[test]
fn local_store_round_trips_artifacts() {
    let root = temp_root("round-trip");
    let store = LocalArtifactStore::new(&root);

    let put = store.put_target("run-a", "proof.bin").unwrap();
    let FileLocation::Path(path) = &put else {
        panic!("expected path target")
    };
    std::fs::write(path, b"proof bytes").unwrap();

    assert_eq!(store.fetch("run-a", "proof.bin").unwrap(), b"proof bytes");
    assert_eq!(store.get_target("run-a", "proof.bin").unwrap(), put);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn local_store_segments_by_run_id() {
    let root = temp_root("segments");
    let store = LocalArtifactStore::new(&root);

    let FileLocation::Path(path) = store.put_target("run-b", "proof.bin").unwrap() else {
        panic!("expected path target")
    };
    assert!(path.contains("run-b"), "path should be segmented by run id: {path}");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn s3_store_mints_url_targets_with_run_scoped_keys() {
    let store = S3ArtifactStore::new(&S3Config {
        bucket: "test-bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint: "http://127.0.0.1:9000".to_string(),
        access_key: "ak".to_string(),
        secret_key: "sk".to_string(),
        prefix: "proofs".to_string(),
    })
    .unwrap();

    let FileLocation::Url(url) = store.put_target("run-c", "proof.bin").unwrap() else {
        panic!("expected url target")
    };
    assert!(url.contains("test-bucket"), "presigned url should reference the bucket: {url}");
    assert!(
        url.contains("proofs/run-c/proof.bin"),
        "presigned url should carry the run-scoped key: {url}"
    );
    assert!(url.contains("X-Amz-Signature"), "presigned url should be signed: {url}");
}
