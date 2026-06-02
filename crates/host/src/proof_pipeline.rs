use std::time::Instant;

use sp1_sdk::prelude::*;
use sp1_sdk::ProverClient;

use crate::pipeline::batch::prepare_batch;
use crate::pipeline::diagnostics::{
    clear_phase_timings, collected_phase_timings, timed_async, timed_sync,
};
pub use crate::pipeline::diagnostics::{log_execution_report, PhaseTiming};
use crate::pipeline::execution::{
    execute_batch_with_prover, execute_batch_without_proof, UnprovenBatchInput,
};
pub use crate::pipeline::input::{config_from_env, parse_genesis_hash};
use crate::pipeline::proof::compressed::generate_compressed_proof_with_prover;
use crate::pipeline::proof::groth16::generate_groth16_proof;
use crate::pipeline::proof::{get_prepared_prover, PreparedProver};
pub use crate::pipeline::{BoxError, ProofArtifacts, ProofGenerationConfig, ProverBackend};

pub const ELF: Elf = include_elf!("zkpow-guest");

fn log_prover_backend_selection(config: &ProofGenerationConfig) {
    match config.prover_backend {
        ProverBackend::Mock => {
            tracing::info!("Using the Mock prover for instant execution");
        }
        ProverBackend::Cpu => {
            if cfg!(feature = "CUDA") {
                tracing::info!(
                    "CUDA support is compiled in, but CUDA=1 was not set; using the CPU prover"
                );
            } else {
                tracing::info!("CUDA support is not compiled in; using the CPU prover");
            }
        }
        ProverBackend::Cuda => {
            tracing::info!(
                "CUDA=1 requested; preparing the GPU prover{}",
                config
                    .cuda_device_id
                    .map(|id| format!(" on device {}", id))
                    .unwrap_or_else(|| " on device 0".to_owned()),
            );
        }
    }
}

pub async fn generate_and_save_proofs(
    config: &ProofGenerationConfig,
) -> Result<ProofArtifacts, BoxError> {
    log_prover_backend_selection(config);
    if config.execute_only && config.prover_backend == ProverBackend::Mock {
        return generate_execute_only_without_setup(config).await;
    }

    match get_prepared_prover(config).await? {
        PreparedProver::Mock {
            prover,
            proving_key,
        } => generate_and_save_proofs_inner(config, "mock", &prover, &proving_key).await,
        PreparedProver::Cpu {
            prover,
            proving_key,
        } => generate_and_save_proofs_inner(config, "cpu", &prover, &proving_key).await,
        #[cfg(feature = "CUDA")]
        PreparedProver::Cuda {
            prover,
            proving_key,
        } => generate_and_save_proofs_inner(config, "cuda", &prover, &proving_key).await,
    }
}

async fn generate_execute_only_without_setup(
    config: &ProofGenerationConfig,
) -> Result<ProofArtifacts, BoxError> {
    if config.prev_proof_path.is_some() {
        tracing::info!("Execute-only resume needs a previous proof witness; preparing prover");
        match get_prepared_prover(config).await? {
            PreparedProver::Mock {
                prover,
                proving_key,
            } => {
                return generate_and_save_proofs_inner(config, "mock", &prover, &proving_key).await
            }
            _ => unreachable!("mock backend should prepare a mock prover"),
        }
    }

    clear_phase_timings();
    let overall_start = Instant::now();
    let genesis_hash = timed_sync("parse_genesis_hash", parse_genesis_hash)?;
    let batch = prepare_batch(config, genesis_hash)?;
    let prover = timed_async("build_mock_executor", || async {
        Ok::<_, BoxError>(ProverClient::builder().mock().build().await)
    })
    .await?;

    let executed = execute_batch_without_proof(
        "mock",
        &prover,
        UnprovenBatchInput {
            current_state: &batch.current_state,
            headers: &batch.headers,
            median_hints: &batch.median_hints,
            expected_state: &batch.expected_state,
            expected_continuation_digest: batch.expected_continuation_digest,
        },
    )
    .await?;
    let total_duration = overall_start.elapsed();
    Ok(ProofArtifacts {
        compressed_path: None,
        groth16_path: None,
        compressed_proof: None,
        groth16_proof: None,
        before_prove_sample: executed.before_prove_sample,
        execution_report: executed.execution_report,
        first_new_height: batch.first_new_height,
        end_height: batch.end_height,
        total_duration_secs: total_duration.as_secs_f64(),
        phase_timings: collected_phase_timings(),
    })
}

async fn generate_and_save_proofs_inner<P, K>(
    config: &ProofGenerationConfig,
    prover_name: &str,
    prover: &P,
    proving_key: &K,
) -> Result<ProofArtifacts, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: std::fmt::Display + Send + Sync + 'static,
{
    clear_phase_timings();
    let overall_start = Instant::now();

    let genesis_hash = timed_sync("parse_genesis_hash", parse_genesis_hash)?;
    let batch = prepare_batch(config, genesis_hash)?;

    if config.execute_only {
        let executed = execute_batch_with_prover(
            prover_name,
            prover,
            proving_key,
            &batch.current_state,
            batch.previous_proof.as_ref(),
            &batch.headers,
            &batch.median_hints,
            (&batch.expected_state, batch.expected_continuation_digest),
        )
        .await?;
        let total_duration = overall_start.elapsed();
        return Ok(ProofArtifacts {
            compressed_path: None,
            groth16_path: None,
            compressed_proof: None,
            groth16_proof: None,
            before_prove_sample: executed.before_prove_sample,
            execution_report: executed.execution_report,
            first_new_height: batch.first_new_height,
            end_height: batch.end_height,
            total_duration_secs: total_duration.as_secs_f64(),
            phase_timings: collected_phase_timings(),
        });
    }

    let compressed_artifacts = generate_compressed_proof_with_prover(
        prover_name,
        prover,
        proving_key,
        &batch.current_state,
        batch.previous_proof.as_ref(),
        &batch.headers,
        &batch.median_hints,
        (&batch.expected_state, batch.expected_continuation_digest),
    )
    .await?;
    let vk = compressed_artifacts.vk;
    let compressed_proof = compressed_artifacts.compressed_proof;
    let before_prove_sample = compressed_artifacts.before_prove_sample;
    let execution_report = compressed_artifacts.execution_report;

    timed_sync("create_output_dir", || -> Result<(), BoxError> {
        std::fs::create_dir_all(&config.output_dir)?;
        Ok(())
    })?;
    let compressed_path = config.output_dir.join(format!(
        "proof_height_{}_to_{}.bin",
        batch.first_new_height, batch.end_height,
    ));
    timed_sync("save_compressed_proof", || -> Result<(), BoxError> {
        compressed_proof.save(&compressed_path)?;
        Ok(())
    })?;

    let (groth16_path, groth16_proof) = if config.generate_groth16 {
        let groth16_proof = generate_groth16_proof(&compressed_proof, &vk).await?;
        let groth16_path = config.output_dir.join(format!(
            "proof_height_{}_to_{}_groth16.bin",
            batch.first_new_height, batch.end_height,
        ));
        timed_sync("save_groth16_proof", || -> Result<(), BoxError> {
            groth16_proof.save(&groth16_path)?;
            Ok(())
        })?;
        (Some(groth16_path), Some(groth16_proof))
    } else {
        tracing::info!(
            "Skipping Groth16 wrapping; set ZKPOW_GENERATE_GROTH16=1 to enable it"
        );
        (None, None)
    };

    let total_duration = overall_start.elapsed();

    Ok(ProofArtifacts {
        compressed_path: Some(compressed_path),
        groth16_path,
        compressed_proof: Some(compressed_proof),
        groth16_proof,
        before_prove_sample,
        execution_report,
        first_new_height: batch.first_new_height,
        end_height: batch.end_height,
        total_duration_secs: total_duration.as_secs_f64(),
        phase_timings: collected_phase_timings(),
    })
}

#[cfg(test)]
mod tests {
    use crate::pipeline::diagnostics::{
        format_claim_mismatch_with_mode, highlight_with_expected_mode, value_prefix, HighlightMode,
    };
    use crate::pipeline::execution::verify_public_values;
    use std::collections::HashMap;

    use crate::pipeline::input::{
        config_from_source, ensure_cuda_requested_configuration, parse_bool_env, parse_u32_env,
        EnvSource, MapEnvSource, ENV_ZKPOW_BATCH_SIZE, ENV_ZKPOW_EXECUTE_ONLY,
        ENV_ZKPOW_GENERATE_GROTH16, ENV_ZKPOW_USE_CUDA,
    };
    use crate::pipeline::ProverBackend;
    use crate::util;

    #[test]
    fn parses_generate_groth16_env() {
        let mut values = HashMap::new();
        values.insert(
            ENV_ZKPOW_GENERATE_GROTH16.to_string(),
            "true".to_string(),
        );
        let source = MapEnvSource::new(values);
        assert!(parse_bool_env(&source, ENV_ZKPOW_GENERATE_GROTH16).unwrap());

        let mut values = HashMap::new();
        values.insert(
            ENV_ZKPOW_GENERATE_GROTH16.to_string(),
            "false".to_string(),
        );
        let source = MapEnvSource::new(values);
        assert!(!parse_bool_env(&source, ENV_ZKPOW_GENERATE_GROTH16).unwrap());

        let source = MapEnvSource::default();
        assert!(!parse_bool_env(&source, ENV_ZKPOW_GENERATE_GROTH16).unwrap());

        let mut values = HashMap::new();
        values.insert(
            ENV_ZKPOW_GENERATE_GROTH16.to_string(),
            "invalid".to_string(),
        );
        let source = MapEnvSource::new(values);
        assert!(parse_bool_env(&source, ENV_ZKPOW_GENERATE_GROTH16).is_err());
    }

    #[test]
    fn rejects_cuda_device_without_cuda_flag() {
        assert!(ensure_cuda_requested_configuration(false, Some(0)).is_err());
        assert!(ensure_cuda_requested_configuration(false, None).is_ok());
    }

    #[test]
    #[cfg(not(feature = "CUDA"))]
    fn rejects_cuda_when_feature_is_not_compiled_in() {
        let err = ensure_cuda_requested_configuration(true, None).unwrap_err();
        assert!(
            err.to_string().contains("--features CUDA"),
            "unexpected error: {err}",
        );
    }

    #[test]
    fn config_validation_matrix_invalid_values() {
        struct SingleValueEnv {
            key: &'static str,
            value: &'static str,
        }

        impl EnvSource for SingleValueEnv {
            fn get(&self, var_name: &str) -> Result<String, std::env::VarError> {
                if var_name == self.key {
                    Ok(self.value.to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            }
        }

        // Invalid CUDA boolean value should error.
        let source = SingleValueEnv {
            key: ENV_ZKPOW_USE_CUDA,
            value: "maybe",
        };
        assert!(parse_bool_env(&source, ENV_ZKPOW_USE_CUDA).is_err());

        // Invalid CUDA_DEVICE_ID should error when non-numeric.
        let source = SingleValueEnv {
            key: "ZKPOW_CUDA_DEVICE_ID",
            value: "abc",
        };
        assert!(parse_u32_env(&source, "ZKPOW_CUDA_DEVICE_ID").is_err());

        // Invalid GENERATE_GROTH16 boolean should error.
        let source = SingleValueEnv {
            key: ENV_ZKPOW_GENERATE_GROTH16,
            value: "yessir",
        };
        assert!(parse_bool_env(&source, ENV_ZKPOW_GENERATE_GROTH16).is_err());
    }

    #[test]
    fn execute_only_defaults_to_mock_backend() {
        let mut values = HashMap::new();
        values.insert(ENV_ZKPOW_EXECUTE_ONLY.to_string(), "1".to_string());
        values.insert(ENV_ZKPOW_BATCH_SIZE.to_string(), "1".to_string());

        let source = MapEnvSource::new(values);
        let config = config_from_source(&source).unwrap();

        assert_eq!(config.prover_backend, ProverBackend::Mock);
        assert!(config.execute_only);
    }

    #[test]
    fn pipeline_error_mapping_parses_failure_codes() {
        let state: util::State = util::State::default();
        let digest = [0xAAu8; 32];
        let vk = crate::util::VerifierKeyDigest::from_raw([0x11u32; 8]);
        let claim = state.public_claim();
        let pv_bytes = util::MinimalPublicValues::failure(
            &claim,
            zkpow_core::ValidationErrorCode::PowInsufficient,
            1234,
            digest,
            vk,
        )
        .to_bytes();

        let expected = util::MinimalPublicValues::success(&claim, digest, vk).to_bytes();
        let err = verify_public_values(&pv_bytes, &expected, "unit-test").unwrap_err();
        assert!(err.to_string().contains("error"));
        assert!(err.to_string().contains("height 1234"));
    }

    #[test]
    fn highlight_marks_mismatched_bytes_with_brackets() {
        let (actual, expected) =
            highlight_with_expected_mode("00010203", "0001ff04", HighlightMode::Brackets, false);

        assert_eq!(actual, "0001[ff04]");
        assert_eq!(expected, "    [0203]");
    }

    #[test]
    fn highlight_marks_mismatched_bytes_with_color() {
        let (actual, expected) =
            highlight_with_expected_mode("00010203", "0001ff04", HighlightMode::Ansi, false);

        assert_eq!(actual, "0001\x1b[31mff04\x1b[0m");
        assert_eq!(expected, "    \x1b[32m0203\x1b[0m");
    }

    #[test]
    fn highlight_marks_matching_leading_zero_bytes_with_bright_white() {
        let (actual, expected) =
            highlight_with_expected_mode("00000102", "00000002", HighlightMode::Ansi, true);

        assert_eq!(actual, "\x1b[97m0000\x1b[0m\x1b[31m00\x1b[0m02");
        assert_eq!(expected, "    \x1b[32m01\x1b[0m  ");
    }

    #[test]
    fn claim_mismatch_splits_32_byte_fields_and_pairs_corrections() {
        let actual = util::PublicChainClaim {
            genesis_hash: util::BlockHash::from_le_bytes([0x11; 32]),
            tip_hash: util::BlockHash::from_le_bytes([0x22; 32]),
            chain_work: util::ChainWork::from_le_bytes([0x33; 32]),
            height: 1,
        };
        let expected = util::PublicChainClaim {
            tip_hash: util::BlockHash::from_le_bytes([0x44; 32]),
            height: 2,
            ..actual
        };

        let output = format_claim_mismatch_with_mode(&actual, &expected, HighlightMode::Brackets);

        let tip_hash_value_prefix = value_prefix("tip_hash");
        assert!(output.contains(
            &format!("tip_hash: [22222222222222222222222222222222]\n{tip_hash_value_prefix}[44444444444444444444444444444444]\n\n{tip_hash_value_prefix}[22222222222222222222222222222222]")
        ));
        assert!(output.contains(&format!(
            "{tip_hash_value_prefix}[44444444444444444444444444444444]"
        )));
        assert!(output.contains("height: 000000[01]"));
        assert!(output.contains(&format!("{}[02]", value_prefix("height"))));
        assert!(output.contains("expected claim:"));
        assert!(output.contains(
            "tip_hash: 44444444444444444444444444444444\n                44444444444444444444444444444444"
        ));
    }

    #[test]
    fn claim_mismatch_keeps_matching_32_byte_fields_to_two_lines() {
        let claim = util::PublicChainClaim {
            genesis_hash: util::BlockHash::from_le_bytes([0x11; 32]),
            tip_hash: util::BlockHash::from_le_bytes([0x22; 32]),
            chain_work: util::ChainWork::from_le_bytes([0x33; 32]),
            height: 1,
        };

        let output = format_claim_mismatch_with_mode(&claim, &claim, HighlightMode::Brackets);

        assert!(output.contains(
            "genesis_hash: 11111111111111111111111111111111\n                    11111111111111111111111111111111,"
        ));
    }
}
