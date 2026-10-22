use std::path::PathBuf;

use crate::storage::{ArtifactStore, LocalArtifactStore};

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

    let path = store.put_target("run-a", "proof.bin").unwrap();
    std::fs::write(&path, b"proof bytes").unwrap();

    assert_eq!(store.fetch("run-a", "proof.bin").unwrap(), b"proof bytes");
    assert_eq!(store.get_target("run-a", "proof.bin").unwrap(), path);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn local_store_segments_by_run_id() {
    let root = temp_root("segments");
    let store = LocalArtifactStore::new(&root);

    let path = store.put_target("run-b", "proof.bin").unwrap();
    assert!(path.contains("run-b"), "path should be segmented by run id: {path}");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn local_store_refuses_a_target_it_never_stored() {
    let root = temp_root("missing");
    let store = LocalArtifactStore::new(&root);

    let error = store.get_target("run-c", "proof_2016.bin").unwrap_err().to_string();
    assert!(error.contains("run-c"), "error should name the run: {error}");
    assert!(error.contains("proof_2016.bin"), "error should name the object: {error}");

    std::fs::remove_dir_all(&root).ok();
}
