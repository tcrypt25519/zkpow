use std::path::PathBuf;

use crate::storage::{ArtifactStore, LocalArtifactStore};

const RUN_ID_A: &str = "run0000a";
const RUN_ID_B: &str = "run0000b";
const RUN_ID_C: &str = "run0000c";
const PROOF_FILE_NAME: &str = "proof.bin";
const MISSING_PROOF_FILE_NAME: &str = "proof_2016.bin";

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

    let path = store.put_target(RUN_ID_A, PROOF_FILE_NAME).unwrap();
    std::fs::write(&path, b"proof bytes").unwrap();

    assert_eq!(store.fetch(RUN_ID_A, PROOF_FILE_NAME).unwrap(), b"proof bytes");
    assert_eq!(store.get_target(RUN_ID_A, PROOF_FILE_NAME).unwrap(), path);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn local_store_segments_by_run_id() {
    let root = temp_root("segments");
    let store = LocalArtifactStore::new(&root);

    let path = store.put_target(RUN_ID_B, PROOF_FILE_NAME).unwrap();
    assert!(path.contains(RUN_ID_B), "path should be segmented by run id: {path}");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn local_store_refuses_a_target_it_never_stored() {
    let root = temp_root("missing");
    let store = LocalArtifactStore::new(&root);

    let error = store.get_target(RUN_ID_C, MISSING_PROOF_FILE_NAME).unwrap_err().to_string();
    assert!(error.contains(RUN_ID_C), "error should name the run: {error}");
    assert!(error.contains(MISSING_PROOF_FILE_NAME), "error should name the object: {error}");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn local_store_rejects_unsafe_run_ids() {
    let root = temp_root("unsafe-run-ids");
    let store = LocalArtifactStore::new(&root);

    let outside_escape = std::env::temp_dir().join(format!("zkpow-escape-target-{}", std::process::id()));
    let outside_escape_str = outside_escape.to_string_lossy().into_owned();

    let unsafe_ids = [
        "",
        "..",
        ".",
        "../escape",
        "/absolute",
        &outside_escape_str,
        "run/nested",
        "run\\nested",
        "run-0001",
        "short",
        "toolongrunid",
        "run\0id01",
    ];

    for unsafe_id in unsafe_ids {
        assert!(
            store.put_target(unsafe_id, PROOF_FILE_NAME).is_err(),
            "put_target should reject {unsafe_id:?}"
        );
        assert!(
            store.get_target(unsafe_id, PROOF_FILE_NAME).is_err(),
            "get_target should reject {unsafe_id:?}"
        );
        assert!(
            store.fetch(unsafe_id, PROOF_FILE_NAME).is_err(),
            "fetch should reject {unsafe_id:?}"
        );
    }

    // Verify no directories or files were created outside the root
    assert!(!outside_escape.exists(), "put_target must not escape root");

    std::fs::remove_dir_all(&root).ok();
}
