use sp1_sdk::HashableKey;

use super::super::*;

#[tokio::test]
#[ignore = "generates two full SP1 compressed proofs"]
async fn test_recursive_chain_success() -> Result<(), String> {
    let client = ProverClient::from_env().await;
    let pk = client.setup(ELF).await.map_err(|e| format!("setup: {}", e))?;
    let vk = pk.verifying_key();
    let db = open_db();

    // === Run 1: genesis state → block 10 ===
    let genesis_state = mainnet_genesis_state();
    let witness1 = db.load_header_batch_witness(1, 10).expect("batch witness");
    let verifier_key = VerifierKeyDigest::from_raw(vk.hash_u32());
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
        .map_err(|e| format!("Run 1 proof generation failed: {}", e))?;
    client
        .verify(&proof1, vk, None)
        .map_err(|e| format!("Run 1 proof verification failed: {}", e))?;
    let pv1_bytes = proof1.public_values.to_vec();

    let state1 = expect_success(&pv1_bytes)?;
    if state1.height != 10 {
        return Err(format!("Run 1: expected height 10, got {}", state1.height));
    }

    // === Run 2: Extend from Run 1 (blocks 11-20) ===
    let genesis_state2 = mainnet_genesis_state();
    let witness_to_10 = db.load_header_batch_witness(1, 10).expect("batch witness");
    let state1 = compute_final_state_with_sparse_hints(
        &genesis_state2,
        &witness_to_10.headers,
        &witness_to_10.median_time_past_hints,
    );

    let witness2 = db.load_header_batch_witness(11, 10).expect("batch witness");
    let mut stdin2 = build_stdin(verifier_key, &state1, &witness2.headers, &witness2.median_time_past_hints);
    let sp1_sdk::SP1Proof::Compressed(inner_proof) = &proof1.proof else {
        return Err("Run 1 proof is not compressed".to_string());
    };
    stdin2.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());

    let proof2 = client
        .prove(&pk, stdin2)
        .compressed()
        .await
        .map_err(|e| format!("Run 2 proof generation failed: {}", e))?;
    client
        .verify(&proof2, vk, None)
        .map_err(|e| format!("Run 2 proof verification failed: {}", e))?;
    let state2 = expect_success(&proof2.public_values.to_vec())?;
    if state2.height != 20 {
        return Err(format!("Run 2: expected height 20, got {}", state2.height));
    }
    Ok(())
}
