use sp1_sdk::HashableKey;

use super::super::*;

#[tokio::test]
async fn test_error_recursive_on_tampered_state_witness() -> Result<(), String> {
    let client = ProverClient::builder().mock().build().await;
    let pk = client.setup(ELF).await.map_err(|e| format!("setup: {e}"))?;
    let vk = pk.verifying_key();
    let verifier_key = VerifierKeyDigest::from_raw(vk.hash_u32());

    let genesis_state = mainnet_genesis_state();
    let witness1 = open_db().load_header_batch_witness(1, 10).expect("batch witness");
    let stdin1 = build_stdin(
        verifier_key,
        &genesis_state,
        &witness1.headers,
        &witness1.median_time_past_hints,
    );

    let proof1 = client
        .prove(&pk, stdin1)
        .compressed()
        .await
        .map_err(|e| format!("Run 1 proof generation failed: {e}"))?;

    let state_at_10 =
        compute_final_state_with_sparse_hints(&genesis_state, &witness1.headers, &witness1.median_time_past_hints);

    let mut tampered_state = state_at_10;
    tampered_state.epoch_timestamp = BlockTimestamp::default();

    let witness2 = open_db().load_header_batch_witness(11, 10).expect("batch witness");
    let mut stdin2 = build_stdin(
        verifier_key,
        &tampered_state,
        &witness2.headers,
        &witness2.median_time_past_hints,
    );

    let sp1_sdk::SP1Proof::Compressed(inner_proof) = &proof1.proof else {
        return Err("Run 1 proof is not compressed".to_string());
    };
    stdin2.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());

    let result = client.execute(ELF, stdin2).await;

    match result {
        Err(_) => Ok(()),
        Ok((pv, _)) => {
            let pv_bytes = pv.to_vec();
            match PublicValues::parse(&pv_bytes) {
                Ok(pv) if pv.is_success() => {
                    Err("expected guest to reject tampered continuation state, but got success".to_string())
                },
                _ => Ok(()),
            }
        },
    }
}
