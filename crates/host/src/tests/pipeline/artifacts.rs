use zkpow_controller::protocol::FileLocation;

use crate::pipeline::artifacts::{fetch_artifact, put_artifact};

#[tokio::test]
async fn path_targets_round_trip_and_create_parent_dirs() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("zkpow-artifacts-{nonce}"));
    let path = root.join("run/proof.bin");
    let target = FileLocation::Path(path.to_string_lossy().into_owned());
    let client = reqwest::Client::new();

    put_artifact(&client, &target, b"proof bytes".to_vec()).await.unwrap();
    assert_eq!(fetch_artifact(&client, &target).await.unwrap(), b"proof bytes");

    std::fs::remove_dir_all(&root).ok();
}
