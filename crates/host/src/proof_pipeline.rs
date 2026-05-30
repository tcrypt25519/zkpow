use std::collections::{BTreeMap, HashMap};
use std::env::VarError;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::memory_monitor;
use crate::util;
use crate::util::{
    HeaderChainPublicValues, Input, PublicValuesDigest, RecursiveProof, VerifierKeyDigest,
};
use memory_usage::StageSample;
use num_bigint::BigUint;
use sp1_prover::build::{build_constraints_and_witness, try_build_groth16_artifacts_dir};
use sp1_prover::worker::{cpu_worker_builder, SP1LocalNodeBuilder};
use sp1_recursion_gnark_ffi::Groth16Bn254Prover;
use sp1_sdk::prelude::*;
use sp1_sdk::proof::SP1Proof;
use sp1_sdk::Elf;
use sp1_sdk::ExecutionReport;
use sp1_sdk::{
    CpuProver, HashableKey, ProveRequest, Prover, ProverClient, ProvingKey,
    SP1ProofWithPublicValues, SP1ProvingKey,
};
use tokio::sync::Mutex as AsyncMutex;
use tracing::Instrument;

#[cfg(feature = "CUDA")]
use sp1_sdk::CudaProver;

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

pub const ELF: Elf = include_elf!("zkpow-guest");
pub const GENESIS_HASH_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
pub const DEFAULT_DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db");

const ACTUAL_ANSI_START: &str = "\x1b[31m";
const EXPECTED_ANSI_START: &str = "\x1b[32m";
const LEADING_ZERO_ANSI_START: &str = "\x1b[97m";
const ANSI_END: &str = "\x1b[0m";
const BRACKET_START: &str = "[";
const BRACKET_END: &str = "]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HighlightMode {
    Ansi,
    Brackets,
}

fn format_claim_pretty(claim: &util::PublicChainClaim) -> String {
    let mut output = String::new();
    output.push_str("    PublicChainClaim {\n");
    push_claim_field(
        &mut output,
        "genesis_hash",
        &claim.genesis_hash_display_hex(),
    );
    push_claim_field(&mut output, "tip_hash", &claim.tip_hash_display_hex());
    push_claim_field(&mut output, "chain_work", &claim.chain_work_display_hex());
    push_claim_field(&mut output, "height", &claim.height_display_hex());
    output.push_str("    }");
    output
}

fn push_claim_field(output: &mut String, name: &str, value: &str) {
    let prefix = field_prefix(name);
    if value.len() == 64 {
        output.push_str(&prefix);
        output.push_str(&value[..32]);
        output.push_str("\n");
        output.push_str(&value_prefix(name));
        output.push_str(&value[32..]);
        output.push_str(",\n");
    } else {
        output.push_str(&prefix);
        output.push_str(value);
        output.push_str(",\n");
    }
}

fn format_claim_mismatch(
    actual: &util::PublicChainClaim,
    expected: &util::PublicChainClaim,
) -> String {
    format_claim_mismatch_with_mode(actual, expected, highlight_mode_from_env())
}

fn format_claim_mismatch_with_mode(
    actual: &util::PublicChainClaim,
    expected: &util::PublicChainClaim,
    mode: HighlightMode,
) -> String {
    let mut output = String::new();
    output.push_str("  actual claim:\n");
    output.push_str("    PublicChainClaim {\n");
    push_claim_mismatch_field(
        &mut output,
        "genesis_hash",
        &actual.genesis_hash_display_hex(),
        &expected.genesis_hash_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "tip_hash",
        &actual.tip_hash_display_hex(),
        &expected.tip_hash_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "chain_work",
        &actual.chain_work_display_hex(),
        &expected.chain_work_display_hex(),
        mode,
    );
    push_claim_mismatch_field(
        &mut output,
        "height",
        &actual.height_display_hex(),
        &expected.height_display_hex(),
        mode,
    );
    output.push_str("    }\n");
    output.push_str("  expected claim:\n");
    output.push_str(&format_claim_pretty(expected));
    output
}

fn push_claim_mismatch_field(
    output: &mut String,
    name: &str,
    actual: &str,
    expected: &str,
    mode: HighlightMode,
) {
    if actual.len() == 64 {
        push_wide_claim_mismatch_field(output, name, actual, expected, mode);
    } else {
        let has_mismatch = actual != expected;
        let (actual, expected) =
            highlight_with_expected_mode(expected, actual, mode, actual.len() == 8);
        output.push_str(&field_prefix(name));
        output.push_str(&actual);
        output.push_str(",\n");
        if has_mismatch {
            output.push_str(&value_prefix(name));
            output.push_str(&expected);
            output.push('\n');
        }
    }
}

fn push_wide_claim_mismatch_field(
    output: &mut String,
    name: &str,
    actual: &str,
    expected: &str,
    mode: HighlightMode,
) {
    let first_actual = &actual[..32];
    let first_expected = &expected[..32];
    let second_actual = &actual[32..];
    let second_expected = &expected[32..];
    let first_mismatch = first_actual != first_expected;
    let second_mismatch = second_actual != second_expected;
    let has_mismatch = first_mismatch || second_mismatch;

    let (first_actual, first_expected) =
        highlight_with_expected_mode(first_expected, first_actual, mode, false);
    let (second_actual, second_expected) =
        highlight_with_expected_mode(second_expected, second_actual, mode, false);

    output.push_str(&field_prefix(name));
    output.push_str(&first_actual);
    output.push('\n');
    if first_mismatch {
        output.push_str(&value_prefix(name));
        output.push_str(&first_expected);
        output.push('\n');
    }
    if has_mismatch {
        output.push('\n');
    }
    output.push_str(&value_prefix(name));
    output.push_str(&second_actual);
    output.push_str(",\n");
    if second_mismatch {
        output.push_str(&value_prefix(name));
        output.push_str(&second_expected);
        output.push('\n');
    }
}

fn highlight_with_expected_mode(
    expected: &str,
    actual: &str,
    mode: HighlightMode,
    highlight_leading_zeros: bool,
) -> (String, String) {
    assert_eq!(expected.len(), actual.len());
    assert_eq!(expected.len() % 2, 0);

    let mut actual_out = String::with_capacity(actual.len() * 2);
    let mut expected_out = String::with_capacity(expected.len() * 2);
    let mut in_mismatch_run = false;
    let mut in_leading_zero_run = false;
    let mut still_leading_zeros = highlight_leading_zeros;

    for i in (0..actual.len()).step_by(2) {
        let exp = &expected[i..i + 2];
        let act = &actual[i..i + 2];

        if exp == act {
            if in_mismatch_run {
                actual_out.push_str(mode.end());
                expected_out.push_str(mode.end());
                in_mismatch_run = false;
            }
            if still_leading_zeros && exp == "00" && mode == HighlightMode::Ansi {
                if !in_leading_zero_run {
                    actual_out.push_str(LEADING_ZERO_ANSI_START);
                    in_leading_zero_run = true;
                }
            } else {
                if in_leading_zero_run {
                    actual_out.push_str(ANSI_END);
                    in_leading_zero_run = false;
                }
                still_leading_zeros = false;
            }
            actual_out.push_str(act);
            expected_out.push_str("  ");
        } else {
            if in_leading_zero_run {
                actual_out.push_str(ANSI_END);
                in_leading_zero_run = false;
            }
            still_leading_zeros = false;
            if !in_mismatch_run {
                actual_out.push_str(mode.actual_start());
                expected_out.push_str(mode.expected_start());
                in_mismatch_run = true;
            }
            actual_out.push_str(act);
            expected_out.push_str(exp);
        }
    }

    if in_leading_zero_run {
        actual_out.push_str(ANSI_END);
    }
    if in_mismatch_run {
        actual_out.push_str(mode.end());
        expected_out.push_str(mode.end());
    }

    (actual_out, expected_out)
}

impl HighlightMode {
    fn actual_start(self) -> &'static str {
        match self {
            Self::Ansi => ACTUAL_ANSI_START,
            Self::Brackets => BRACKET_START,
        }
    }

    fn expected_start(self) -> &'static str {
        match self {
            Self::Ansi => EXPECTED_ANSI_START,
            Self::Brackets => BRACKET_START,
        }
    }

    fn end(self) -> &'static str {
        match self {
            Self::Ansi => ANSI_END,
            Self::Brackets => BRACKET_END,
        }
    }
}

fn highlight_mode_from_env() -> HighlightMode {
    match std::env::var_os("CLICOLOR") {
        Some(value) if value.to_string_lossy() != "0" => HighlightMode::Ansi,
        _ => HighlightMode::Brackets,
    }
}

fn field_prefix(name: &str) -> String {
    format!("      {name}: ")
}

fn value_prefix(name: &str) -> String {
    format!("      {}  ", " ".repeat(name.len()))
}

trait ClaimHex {
    fn genesis_hash_display_hex(&self) -> String;
    fn tip_hash_display_hex(&self) -> String;
    fn chain_work_display_hex(&self) -> String;
    fn height_display_hex(&self) -> String;
}

impl ClaimHex for util::PublicChainClaim {
    fn genesis_hash_display_hex(&self) -> String {
        display_hex_32(self.genesis_hash.to_le_bytes())
    }

    fn tip_hash_display_hex(&self) -> String {
        display_hex_32(self.tip_hash.to_le_bytes())
    }

    fn chain_work_display_hex(&self) -> String {
        display_hex_32(self.chain_work.to_le_bytes())
    }

    fn height_display_hex(&self) -> String {
        hex::encode(self.height.to_be_bytes())
    }
}

fn display_hex_32(mut bytes: [u8; 32]) -> String {
    bytes.reverse();
    hex::encode(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverBackend {
    Cpu,
    Cuda,
}

#[derive(Debug, Clone)]
pub struct ProofGenerationConfig {
    pub prev_proof_path: Option<PathBuf>,
    pub num_headers: u32,
    pub db_path: PathBuf,
    pub output_dir: PathBuf,
    pub generate_groth16: bool,
    pub prover_backend: ProverBackend,
    pub cuda_device_id: Option<u32>,
}

#[derive(Debug)]
pub struct ProofArtifacts {
    pub compressed_path: PathBuf,
    pub groth16_path: Option<PathBuf>,
    pub compressed_proof: SP1ProofWithPublicValues,
    pub groth16_proof: Option<SP1ProofWithPublicValues>,
    pub before_prove_sample: StageSample,
    pub execution_report: ExecutionReport,
    pub first_new_height: u32,
    pub end_height: u32,
    pub total_duration_secs: f64,
    pub phase_timings: Vec<PhaseTiming>,
}

#[derive(Debug, Clone)]
pub struct PhaseTiming {
    pub label: String,
    pub total_duration_secs: f64,
    pub invocations: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct PhaseTimingAccum {
    total: Duration,
    invocations: u32,
}

static PHASE_TIMINGS: OnceLock<Mutex<HashMap<&'static str, PhaseTimingAccum>>> = OnceLock::new();

fn phase_timings_store() -> &'static Mutex<HashMap<&'static str, PhaseTimingAccum>> {
    PHASE_TIMINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn clear_phase_timings() {
    if let Ok(mut timings) = phase_timings_store().lock() {
        timings.clear();
    }
}

fn record_phase_timing(label: &'static str, elapsed: Duration) {
    if let Ok(mut timings) = phase_timings_store().lock() {
        let entry = timings.entry(label).or_default();
        entry.total += elapsed;
        entry.invocations += 1;
    }
}

fn collected_phase_timings() -> Vec<PhaseTiming> {
    let mut out = if let Ok(timings) = phase_timings_store().lock() {
        timings
            .iter()
            .map(|(label, accum)| PhaseTiming {
                label: (*label).to_string(),
                total_duration_secs: accum.total.as_secs_f64(),
                invocations: accum.invocations,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    out.sort_unstable_by(|a, b| {
        b.total_duration_secs
            .partial_cmp(&a.total_duration_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}

struct CompressedProofArtifacts {
    vk: sp1_prover::SP1VerifyingKey,
    compressed_proof: SP1ProofWithPublicValues,
    before_prove_sample: StageSample,
    execution_report: ExecutionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedProverConfig {
    backend: ProverBackend,
    cuda_device_id: Option<u32>,
}

#[derive(Clone)]
enum PreparedProver {
    Cpu {
        prover: CpuProver,
        proving_key: SP1ProvingKey,
    },
    #[cfg(feature = "CUDA")]
    Cuda {
        prover: CudaProver,
        proving_key: <CudaProver as Prover>::ProvingKey,
    },
}

static PREPARED_PROVER: OnceLock<AsyncMutex<Option<(PreparedProverConfig, PreparedProver)>>> =
    OnceLock::new();

fn prepared_prover_store() -> &'static AsyncMutex<Option<(PreparedProverConfig, PreparedProver)>> {
    PREPARED_PROVER.get_or_init(|| AsyncMutex::new(None))
}

fn parse_genesis_hash() -> Result<util::BlockHash, BoxError> {
    let mut genesis_hash: [u8; 32] = hex::decode(GENESIS_HASH_HEX)?
        .try_into()
        .map_err(|_| "genesis hash should be 32 bytes")?;
    genesis_hash.reverse();
    Ok(util::BlockHash::new(genesis_hash))
}

fn parse_bool_env(var_name: &'static str) -> Result<bool, BoxError> {
    match std::env::var(var_name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(format!(
                "invalid {var_name} value `{value}`; expected one of 1,true,yes,on,0,false,no,off"
            )
            .into()),
        },
        Err(VarError::NotPresent) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn parse_u32_env(var_name: &'static str) -> Result<Option<u32>, BoxError> {
    match std::env::var(var_name) {
        Ok(value) => {
            Ok(Some(value.parse().map_err(|err| {
                format!("invalid {var_name} value `{value}`: {err}")
            })?))
        }
        Err(VarError::NotPresent) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn ensure_cuda_requested_configuration(
    use_cuda: bool,
    cuda_device_id: Option<u32>,
) -> Result<ProverBackend, BoxError> {
    if !use_cuda {
        if cuda_device_id.is_some() {
            return Err("CUDA_DEVICE_ID is only valid when CUDA=1".into());
        }
        return Ok(ProverBackend::Cpu);
    }

    #[cfg(feature = "CUDA")]
    {
        return Ok(ProverBackend::Cuda);
    }

    #[cfg(not(feature = "CUDA"))]
    Err("CUDA=1 requires building zkpow-host with `--features CUDA`".into())
}

pub fn config_from_env() -> Result<ProofGenerationConfig, BoxError> {
    let use_cuda = parse_bool_env("CUDA")?;
    let cuda_device_id = parse_u32_env("CUDA_DEVICE_ID")?;

    Ok(ProofGenerationConfig {
        prev_proof_path: std::env::var("PREV_PROOF").ok().map(PathBuf::from),
        num_headers: std::env::var("NUM_HEADERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100),
        db_path: std::env::var("DB_PATH")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_DB_PATH)),
        output_dir: std::env::var("OUTPUT_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        generate_groth16: parse_bool_env("GENERATE_GROTH16")?,
        prover_backend: ensure_cuda_requested_configuration(use_cuda, cuda_device_id)?,
        cuda_device_id,
    })
}

fn log_prover_backend_selection(config: &ProofGenerationConfig) {
    match config.prover_backend {
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

fn build_recursive_proof(
    vk: &sp1_prover::SP1VerifyingKey,
    previous_proof: Option<&SP1ProofWithPublicValues>,
) -> Result<RecursiveProof, BoxError> {
    let verifier_key = VerifierKeyDigest::from_raw(vk.hash_u32());
    Ok(if let Some(prev_proof_val) = previous_proof {
        let pv_bytes = prev_proof_val.public_values.to_vec();
        // Determine the return code from the previous proof's public values.
        let previous_return_code = match zkpow_core::HeaderChainPublicValues::parse(&pv_bytes) {
            Ok(zkpow_core::HeaderChainPublicValues::Success { .. }) => 0u8,
            Ok(zkpow_core::HeaderChainPublicValues::Failure { failure, .. }) => {
                failure.error_code.as_byte()
            }
            Err(e) => {
                return Err(format!("failed to parse previous proof public values: {}", e).into())
            }
        };
        RecursiveProof {
            verifier_key,
            public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(&pv_bytes)),
            previous_return_code,
            ..Default::default()
        }
    } else {
        RecursiveProof {
            verifier_key,
            ..Default::default()
        }
    })
}

fn build_stdin(
    input: &Input,
    state: &util::State,
    headers: &[util::NewHeader],
    median_hints: &util::MedianTimePastHints,
    previous_proof: Option<&SP1ProofWithPublicValues>,
    vk: &sp1_prover::SP1VerifyingKey,
) -> Result<SP1Stdin, BoxError> {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    stdin.write_vec(state.to_bytes().to_vec());
    stdin.write_vec(util::NewHeaderHintsRef { headers }.to_bytes());
    stdin.write_vec(median_hints.to_bytes());

    if let Some(prev_proof) = previous_proof {
        let SP1Proof::Compressed(inner_proof) = &prev_proof.proof else {
            return Err("previous proof is not compressed".into());
        };
        stdin.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
    }

    Ok(stdin)
}

async fn generate_compressed_proof_with_prover<P, K>(
    prover_name: &str,
    prover: &P,
    proving_key: &K,
    current_state: &util::State,
    previous_proof: Option<&SP1ProofWithPublicValues>,
    headers: &[util::NewHeader],
    median_hints: &util::MedianTimePastHints,
    expected_pv_parts: (&util::State, [u8; 32]),
) -> Result<CompressedProofArtifacts, BoxError>
where
    P: Prover<ProvingKey = K>,
    K: ProvingKey,
    P::Error: Error + Send + Sync + 'static,
{
    let (expected_state, expected_continuation_digest) = expected_pv_parts;
    let expected_pv = util::MinimalPublicValues::success(
        &expected_state.public_claim(),
        expected_continuation_digest,
        VerifierKeyDigest::from_raw(proving_key.verifying_key().hash_u32()),
    )
    .to_bytes()
    .to_vec();
    let recursive_proof = timed_sync("build_recursive_proof", || {
        build_recursive_proof(proving_key.verifying_key(), previous_proof)
    })?;
    let input = timed_sync("build_input", || -> Result<_, BoxError> {
        Ok(Input::new(current_state.public_claim(), recursive_proof))
    })?;
    let stdin = timed_sync("serialize_input", || {
        build_stdin(
            &input,
            current_state,
            headers,
            median_hints,
            previous_proof,
            proving_key.verifying_key(),
        )
    })?;

    let (public_values, report) = timed_async("execute_program", || async {
        prover.execute(ELF, stdin.clone()).await
    })
    .await?;
    tracing::info!(prover = prover_name, "Execution completed");

    let execution_public_values = public_values.to_vec();
    match HeaderChainPublicValues::parse(&execution_public_values) {
        Ok(HeaderChainPublicValues::Success { .. }) => {
            timed_sync(
                "verify_execution_public_values",
                || -> Result<(), BoxError> {
                    verify_public_values(&execution_public_values, &expected_pv, "execution")
                },
            )?;
        }
        Ok(HeaderChainPublicValues::Failure { failure, .. }) => {
            return Err(format!(
                "execution failed with {} at height {}",
                failure.error_code, failure.failure_height
            )
            .into());
        }
        Err(err) => {
            return Err(format!("execution produced malformed public values: {err}").into());
        }
    }

    let before_prove_sample = memory_monitor::log_point(
        "before_prove_compressed",
        "Memory snapshot after VM execution before compressed proof generation",
    );

    let compressed_proof = timed_async("prove_compressed", || async {
        async { prover.prove(proving_key, stdin).compressed().await }
            .instrument(tracing::info_span!("prove_compressed_detail"))
            .await
    })
    .await?;
    timed_sync(
        "verify_compressed_public_values",
        || -> Result<(), BoxError> {
            verify_public_values(
                &compressed_proof.public_values.to_vec(),
                &expected_pv,
                "compressed proof",
            )
        },
    )?;
    timed_sync("verify_compressed_proof", || -> Result<(), BoxError> {
        Ok(prover.verify(&compressed_proof, proving_key.verifying_key(), None)?)
    })?;

    Ok(CompressedProofArtifacts {
        vk: proving_key.verifying_key().clone(),
        compressed_proof,
        before_prove_sample,
        execution_report: report,
    })
}

async fn generate_groth16_proof(
    compressed_proof: &SP1ProofWithPublicValues,
    vk: &sp1_prover::SP1VerifyingKey,
) -> Result<SP1ProofWithPublicValues, BoxError> {
    let node = timed_async("build_local_node", || async {
        SP1LocalNodeBuilder::from_worker_client_builder(cpu_worker_builder())
            .build()
            .await
    })
    .await?;
    let wrap_proof = timed_async("shrink_wrap", || async {
        node.shrink_wrap(&compressed_proof.proof).await
    })
    .await?;
    let build_dir = timed_async("build_groth16_artifacts", || async {
        try_build_groth16_artifacts_dir(&wrap_proof.vk, &wrap_proof.proof).await
    })
    .await?;
    let (_, witness) = timed_sync("build_groth16_witness", || -> Result<_, BoxError> {
        Ok(build_constraints_and_witness(
            &wrap_proof.vk,
            &wrap_proof.proof,
        )?)
    })?;
    let expected_vkey_hash = BigUint::from_bytes_be(&vk.bytes32_raw()).to_string();
    let expected_public_values_digest = compressed_proof.public_values.hash_bn254().to_string();

    let groth16_inner = timed_async("prove_groth16", || async move {
        tokio::task::spawn_blocking(move || {
            let prover = Groth16Bn254Prover::new();
            let proof = prover.prove(witness, &build_dir);
            let [vkey_hash, committed_values_digest, exit_code, vk_root, proof_nonce] =
                proof.public_inputs.clone();

            assert_eq!(
                vkey_hash, expected_vkey_hash,
                "Groth16 proof verifying-key hash does not match the program VK",
            );
            assert_eq!(
                committed_values_digest, expected_public_values_digest,
                "Groth16 proof public-values digest does not match the compressed proof",
            );

            let parse_biguint = |value: &str, label: &str| {
                value
                    .parse::<BigUint>()
                    .unwrap_or_else(|_| panic!("failed to parse {label} as BigUint"))
            };

            prover
                .verify(
                    &proof,
                    &parse_biguint(&vkey_hash, "Groth16 vkey hash"),
                    &parse_biguint(&committed_values_digest, "Groth16 public-values digest"),
                    &parse_biguint(&exit_code, "Groth16 exit code"),
                    &parse_biguint(&vk_root, "Groth16 recursion VK root"),
                    &parse_biguint(&proof_nonce, "Groth16 proof nonce"),
                    &build_dir,
                )
                .expect("native Groth16 verification failed");

            proof
        })
        .await
    })
    .await?;

    Ok(SP1ProofWithPublicValues::new(
        SP1Proof::Groth16(groth16_inner),
        compressed_proof.public_values.clone(),
        compressed_proof.sp1_version.clone(),
    ))
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
    P::Error: Error + Send + Sync + 'static,
{
    clear_phase_timings();
    let overall_start = Instant::now();

    let previous_proof: Option<SP1ProofWithPublicValues> =
        timed_sync("load_previous_proof", || {
            config
                .prev_proof_path
                .as_ref()
                .map(SP1ProofWithPublicValues::load)
                .transpose()
        })?;

    let genesis_hash = timed_sync("parse_genesis_hash", parse_genesis_hash)?;

    let current_state = timed_sync("resolve_current_state", || -> Result<_, BoxError> {
        if let Some(prev_proof) = previous_proof.as_ref() {
            let prev_public_values =
                HeaderChainPublicValues::parse(prev_proof.public_values.as_ref())
                    .map_err(|err| err.to_string())?;
            let claim = match prev_public_values {
                HeaderChainPublicValues::Success { claim, .. } => claim,
                HeaderChainPublicValues::Failure { failure, .. } => {
                    return Err(format!(
                        "previous proof ended in error: {} at height {}",
                        failure.error_code, failure.failure_height,
                    )
                    .into());
                }
            };
            if claim.genesis_hash != genesis_hash {
                return Err("previous proof genesis mismatch".into());
            }
            let start_height = claim.height;
            let state = util::state_from_db_at_height(
                path_to_str(&config.db_path)?,
                start_height,
                genesis_hash,
            );
            if state.public_claim() != claim {
                return Err(format!(
                    "db state mismatch at height {}:\n{}",
                    start_height,
                    format_claim_mismatch(&claim, &state.public_claim())
                )
                .into());
            }
            Ok(state)
        } else {
            Ok(util::state_from_db_at_height(
                path_to_str(&config.db_path)?,
                0,
                genesis_hash,
            ))
        }
    })?;

    let start_height = current_state.height;
    let first_new_height = start_height + 1;
    let header_records = timed_sync("load_header_records", || -> Result<_, BoxError> {
        Ok(util::load_header_records_from_db(
            path_to_str(&config.db_path)?,
            first_new_height as u64,
            config.num_headers as u64,
        ))
    })?;
    let headers = timed_sync("decode_headers", || -> Result<_, BoxError> {
        Ok(util::records_to_new_headers(&header_records))
    })?;
    let median_hints = timed_sync("load_median_time_past_hints", || -> Result<_, BoxError> {
        Ok(util::median_time_past_hints_from_records(&header_records))
    })?;
    let loaded_count = headers.len() as u32;
    let expected_state = timed_sync("simulate_expected_state", || -> Result<_, BoxError> {
        Ok(util::compute_final_state_with_hints(
            &current_state,
            &headers,
            &median_hints,
        ))
    })?;
    let expected_continuation_digest = util::continuation_digest_from_state(&expected_state);
    let compressed_artifacts = generate_compressed_proof_with_prover(
        prover_name,
        prover,
        proving_key,
        &current_state,
        previous_proof.as_ref(),
        &headers,
        &median_hints,
        (&expected_state, expected_continuation_digest),
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
        first_new_height,
        start_height + loaded_count,
    ));
    timed_sync("save_compressed_proof", || -> Result<(), BoxError> {
        compressed_proof.save(&compressed_path)?;
        Ok(())
    })?;

    let (groth16_path, groth16_proof) = if config.generate_groth16 {
        let groth16_proof = generate_groth16_proof(&compressed_proof, &vk).await?;
        let groth16_path = config.output_dir.join(format!(
            "proof_height_{}_to_{}_groth16.bin",
            first_new_height,
            start_height + loaded_count,
        ));
        timed_sync("save_groth16_proof", || -> Result<(), BoxError> {
            groth16_proof.save(&groth16_path)?;
            Ok(())
        })?;
        (Some(groth16_path), Some(groth16_proof))
    } else {
        tracing::info!("Skipping Groth16 wrapping; set GENERATE_GROTH16=1 to enable it");
        (None, None)
    };

    let total_duration = overall_start.elapsed();

    Ok(ProofArtifacts {
        compressed_path,
        groth16_path,
        compressed_proof,
        groth16_proof,
        before_prove_sample,
        execution_report,
        first_new_height,
        end_height: start_height + loaded_count,
        total_duration_secs: total_duration.as_secs_f64(),
        phase_timings: collected_phase_timings(),
    })
}

async fn get_prepared_prover(config: &ProofGenerationConfig) -> Result<PreparedProver, BoxError> {
    let desired = PreparedProverConfig {
        backend: config.prover_backend,
        cuda_device_id: config.cuda_device_id,
    };

    {
        let guard = prepared_prover_store().lock().await;
        if let Some((cached_cfg, prepared)) = guard.as_ref() {
            if *cached_cfg == desired {
                tracing::info!(
                    backend = ?desired.backend,
                    cuda_device_id = desired.cuda_device_id,
                    "reusing prepared prover"
                );
                return Ok(prepared.clone());
            }
            tracing::info!(
                cached_backend = ?cached_cfg.backend,
                cached_cuda_device_id = cached_cfg.cuda_device_id,
                requested_backend = ?desired.backend,
                requested_cuda_device_id = desired.cuda_device_id,
                "prepared prover cache miss due to config change"
            );
        }
    }

    tracing::info!(
        backend = ?desired.backend,
        cuda_device_id = desired.cuda_device_id,
        "building prepared prover"
    );

    let prepared = match config.prover_backend {
        ProverBackend::Cpu => {
            let prover = timed_async("build_cpu_prover", || async {
                Ok::<_, BoxError>(ProverClient::builder().cpu().build().await)
            })
            .await?;
            let proving_key =
                timed_async("setup_vkey", || async { prover.setup(ELF).await }).await?;
            PreparedProver::Cpu {
                prover,
                proving_key,
            }
        }
        ProverBackend::Cuda => {
            #[cfg(feature = "CUDA")]
            {
                let report =
                    timed_sync("cuda_preflight", || crate::cuda_env::run_preflight(config))?;
                crate::cuda_env::log_preflight(&report);
                let prover = timed_async("build_cuda_prover", || async {
                    let device_id = config.cuda_device_id;
                    let handle = tokio::spawn(async move {
                        let builder = if let Some(device_id) = device_id {
                            ProverClient::builder().cuda().with_device_id(device_id)
                        } else {
                            ProverClient::builder().cuda()
                        };
                        builder.build().await
                    });
                    handle.await.map_err(|err| -> BoxError {
                        format!("failed to initialize CUDA prover task: {err}").into()
                    })
                })
                .await?;
                let proving_key =
                    timed_async("setup_vkey", || async { prover.setup(ELF).await }).await?;
                PreparedProver::Cuda {
                    prover,
                    proving_key,
                }
            }

            #[cfg(not(feature = "CUDA"))]
            unreachable!("CUDA config should already be rejected when the CUDA feature is absent")
        }
    };

    let mut guard = prepared_prover_store().lock().await;
    *guard = Some((desired, prepared.clone()));
    Ok(prepared)
}

pub async fn generate_and_save_proofs(
    config: &ProofGenerationConfig,
) -> Result<ProofArtifacts, BoxError> {
    log_prover_backend_selection(config);
    match get_prepared_prover(config).await? {
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

fn path_to_str(path: &Path) -> Result<&str, BoxError> {
    path.to_str()
        .ok_or_else(|| format!("non-utf8 path: {}", path.display()).into())
}

fn timed_sync<T, E, F>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Result<T, E>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    let start_memory = memory_monitor::log_point(label, "proof phase started");
    let output = f().map_err(Into::into);
    let elapsed = started.elapsed();
    let end_memory = memory_monitor::sample();
    record_phase_timing(label, elapsed);
    memory_monitor::log_delta(
        label,
        start_memory,
        end_memory,
        elapsed,
        "proof phase finished",
    );
    output
}

async fn timed_async<T, E, F, Fut>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    let start_memory = memory_monitor::log_point(label, "proof phase started");
    let output = f().await.map_err(Into::into);
    let elapsed = started.elapsed();
    let end_memory = memory_monitor::sample();
    record_phase_timing(label, elapsed);
    memory_monitor::log_delta(
        label,
        start_memory,
        end_memory,
        elapsed,
        "proof phase finished",
    );
    output
}

fn verify_public_values(pv: &[u8], expected_pv: &[u8], label: &str) -> Result<(), BoxError> {
    let parsed = HeaderChainPublicValues::parse(pv).map_err(|err| err.to_string())?;
    match parsed {
        HeaderChainPublicValues::Success { .. } => {
            if pv != expected_pv {
                return Err(format!(
                    "{label} public values mismatch: expected {}, got {}",
                    hex::encode(expected_pv),
                    hex::encode(pv),
                )
                .into());
            }
            Ok(())
        }
        HeaderChainPublicValues::Failure { failure, .. } => Err(format!(
            "{label} ended in error {} at height {}",
            failure.error_code, failure.failure_height,
        )
        .into()),
    }
}

#[derive(Debug, Default)]
struct CycleTreeNode {
    self_cycles: u64,
    self_invocations: u64,
    total_cycles: u64,
    children: BTreeMap<String, CycleTreeNode>,
}

impl CycleTreeNode {
    fn insert(&mut self, path: &str, cycles: u64, invocations: u64) {
        let mut current = self;
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            current = current.children.entry(segment.to_string()).or_default();
        }
        current.self_cycles = current.self_cycles.saturating_add(cycles);
        current.self_invocations = current.self_invocations.saturating_add(invocations);
    }

    fn finalize_totals(&mut self) -> u64 {
        let mut total = self.self_cycles;
        for child in self.children.values_mut() {
            total = total.saturating_add(child.finalize_totals());
        }
        self.total_cycles = total;
        total
    }
}

fn emit_cycle_tree(node: &CycleTreeNode, total_cycles: u64, depth: usize) {
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_unstable_by(|(name_a, node_a), (name_b, node_b)| {
        node_b
            .total_cycles
            .cmp(&node_a.total_cycles)
            .then_with(|| name_a.cmp(name_b))
    });

    for (name, child) in children {
        let pct = if total_cycles == 0 {
            0.0
        } else {
            (child.total_cycles as f64 * 100.0) / total_cycles as f64
        };
        let indent = "  ".repeat(depth);
        tracing::info!(
            "  {}- {}: {} cycles ({:.2}%){}",
            indent,
            name,
            child.total_cycles,
            pct,
            if child.self_invocations > 1 {
                format!(", {} invocations", child.self_invocations)
            } else {
                String::new()
            }
        );
        emit_cycle_tree(child, total_cycles, depth + 1);
    }
}

pub fn log_execution_report(report: &ExecutionReport, total_proving_time_secs: f64) {
    tracing::info!("Execution report");
    tracing::info!("  total instructions: {}", report.total_instruction_count());

    if report.cycle_tracker.is_empty() {
        tracing::info!("  cycle tracker: unavailable or empty");
        tracing::info!("  prover gas: {}", report.gas().unwrap_or(0));
        return;
    }

    let mut entries: Vec<_> = report.cycle_tracker.iter().collect();
    entries.sort_unstable_by(|(label_a, cycles_a), (label_b, cycles_b)| {
        cycles_b.cmp(cycles_a).then_with(|| label_a.cmp(label_b))
    });

    let total_tracked_cycles: u64 = entries.iter().map(|(_, cycles)| **cycles).sum();
    tracing::info!(
        "  cycle tracker: {} tracked cycles across {} spans",
        total_tracked_cycles,
        entries.len()
    );

    tracing::info!("  top hot spans:");
    for (label, cycles) in entries.iter().take(12) {
        let invocations = report
            .invocation_tracker
            .get((*label).as_str())
            .copied()
            .unwrap_or(1);
        let percent = if total_tracked_cycles == 0 {
            0.0
        } else {
            (**cycles as f64 * 100.0) / total_tracked_cycles as f64
        };
        tracing::info!(
            "    {}: {} cycles ({:.2}%){}",
            label,
            cycles,
            percent,
            if invocations > 1 {
                format!(" across {} invocations", invocations)
            } else {
                String::new()
            }
        );
    }

    let mut tree = CycleTreeNode::default();
    for (label, cycles) in &entries {
        let invocations = report
            .invocation_tracker
            .get((*label).as_str())
            .copied()
            .unwrap_or(1);
        tree.insert(label, **cycles, invocations);
    }
    tree.finalize_totals();
    tracing::info!("  cycle hierarchy:");
    emit_cycle_tree(&tree, total_tracked_cycles, 0);

    let prover_gas = report.gas().unwrap_or(0);
    tracing::info!("  prover gas:");
    tracing::info!("    total prover_gas: {}", prover_gas);
    tracing::info!(
        "    assumptions: proportional allocation from tracked cycle share (model coefficients internal to SP1)"
    );
    if prover_gas > 0 && total_tracked_cycles > 0 {
        tracing::info!("    estimated gas by hot span:");
        for (label, cycles) in entries.iter().take(10) {
            let share = **cycles as f64 / total_tracked_cycles as f64;
            let estimated_gas = (prover_gas as f64 * share).round() as u64;
            let estimated_secs = total_proving_time_secs * share;
            tracing::info!(
                "      {}: ~{} gas ({:.2}% of cycles, ~{:.2}s of total)",
                label,
                estimated_gas,
                share * 100.0,
                estimated_secs
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[cfg(feature = "slow-tests")]
    mod slow_tests {
        use super::*;
        use std::time::{SystemTime, UNIX_EPOCH};

        fn unique_test_output_dir() -> PathBuf {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos();
            std::env::temp_dir().join(format!(
                "zkpow-proof-pipeline-{}-{}",
                std::process::id(),
                nanos,
            ))
        }

        #[tokio::test]
        async fn generates_linked_compressed_and_groth16_proofs() {
            let output_dir = unique_test_output_dir();
            let config = ProofGenerationConfig {
                prev_proof_path: None,
                num_headers: 1,
                db_path: PathBuf::from(DEFAULT_DB_PATH),
                output_dir: output_dir.clone(),
                generate_groth16: true,
                prover_backend: ProverBackend::Cpu,
                cuda_device_id: None,
            };

            let artifacts = generate_and_save_proofs(&config)
                .await
                .expect("proof pipeline should succeed");

            assert_eq!(artifacts.first_new_height, 1);
            assert_eq!(artifacts.end_height, 1);
            assert!(artifacts.compressed_path.exists());
            assert!(artifacts
                .groth16_path
                .as_ref()
                .is_some_and(|path| path.exists()));

            let saved_compressed = SP1ProofWithPublicValues::load(&artifacts.compressed_path)
                .expect("saved compressed proof should load");
            let saved_groth16 = SP1ProofWithPublicValues::load(
                artifacts
                    .groth16_path
                    .as_ref()
                    .expect("Groth16 path should be present when wrapping is enabled"),
            )
            .expect("saved groth16 proof should load");

            assert_eq!(
                saved_compressed.public_values.to_vec(),
                saved_groth16.public_values.to_vec(),
                "saved Groth16 proof should commit to the same public values as the compressed proof",
            );
            assert_eq!(
                saved_compressed.public_values.hash_bn254().to_string(),
                match &saved_groth16.proof {
                    SP1Proof::Groth16(proof) => proof.public_inputs[1].clone(),
                    other => panic!("expected Groth16 proof, got {other:?}"),
                },
                "saved Groth16 proof should carry the compressed proof public-values digest",
            );
            match saved_compressed.proof {
                SP1Proof::Compressed(_) => {}
                other => panic!("expected compressed proof, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn generates_compressed_proof_without_groth16_by_default() {
            let output_dir = unique_test_output_dir();
            let config = ProofGenerationConfig {
                prev_proof_path: None,
                num_headers: 1,
                db_path: PathBuf::from(DEFAULT_DB_PATH),
                output_dir,
                generate_groth16: false,
                prover_backend: ProverBackend::Cpu,
                cuda_device_id: None,
            };

            let artifacts = generate_and_save_proofs(&config)
                .await
                .expect("proof pipeline should succeed");

            assert_eq!(artifacts.first_new_height, 1);
            assert_eq!(artifacts.end_height, 1);
            assert!(artifacts.compressed_path.exists());
            assert!(artifacts.groth16_path.is_none());
            assert!(artifacts.groth16_proof.is_none());

            let saved_compressed = SP1ProofWithPublicValues::load(&artifacts.compressed_path)
                .expect("saved compressed proof should load");
            match saved_compressed.proof {
                SP1Proof::Compressed(_) => {}
                other => panic!("expected compressed proof, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn resume_logic_uses_previous_proof_height() {
            let output_dir = unique_test_output_dir();
            // First run: generate a short proof segment.
            let config1 = ProofGenerationConfig {
                prev_proof_path: None,
                num_headers: 2,
                db_path: PathBuf::from(DEFAULT_DB_PATH),
                output_dir: output_dir.clone(),
                generate_groth16: false,
                prover_backend: ProverBackend::Cpu,
                cuda_device_id: None,
            };

            let artifacts1 = generate_and_save_proofs(&config1)
                .await
                .expect("first pipeline run should succeed");

            // Second run: resume from previous proof; should start at next height.
            let config2 = ProofGenerationConfig {
                prev_proof_path: Some(artifacts1.compressed_path.clone()),
                num_headers: 1,
                db_path: PathBuf::from(DEFAULT_DB_PATH),
                output_dir: output_dir.clone(),
                generate_groth16: false,
                prover_backend: ProverBackend::Cpu,
                cuda_device_id: None,
            };

            let artifacts2 = generate_and_save_proofs(&config2)
                .await
                .expect("second pipeline run should succeed");

            assert_eq!(artifacts2.first_new_height, artifacts1.end_height + 1);
        }
    }

    #[test]
    fn parses_generate_groth16_env() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let key = "GENERATE_GROTH16";
        let original = std::env::var_os(key);

        std::env::remove_var(key);
        assert!(!parse_bool_env(key).expect("missing env should default to false"));

        std::env::set_var(key, "true");
        assert!(parse_bool_env(key).expect("true should parse"));

        std::env::set_var(key, "0");
        assert!(!parse_bool_env(key).expect("0 should parse"));

        std::env::set_var(key, "definitely");
        assert!(parse_bool_env(key).is_err());

        match original {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn rejects_cuda_device_without_cuda_flag() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let originals = [
            ("CUDA", std::env::var_os("CUDA")),
            ("CUDA_DEVICE_ID", std::env::var_os("CUDA_DEVICE_ID")),
            ("GENERATE_GROTH16", std::env::var_os("GENERATE_GROTH16")),
            ("OUTPUT_DIR", std::env::var_os("OUTPUT_DIR")),
            ("PREV_PROOF", std::env::var_os("PREV_PROOF")),
            ("NUM_HEADERS", std::env::var_os("NUM_HEADERS")),
        ];

        std::env::remove_var("CUDA");
        std::env::set_var("CUDA_DEVICE_ID", "0");

        assert!(config_from_env().is_err());

        for (key, value) in originals {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    #[cfg(not(feature = "CUDA"))]
    fn rejects_cuda_when_feature_is_not_compiled_in() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let originals = [
            ("CUDA", std::env::var_os("CUDA")),
            ("CUDA_DEVICE_ID", std::env::var_os("CUDA_DEVICE_ID")),
            ("GENERATE_GROTH16", std::env::var_os("GENERATE_GROTH16")),
            ("OUTPUT_DIR", std::env::var_os("OUTPUT_DIR")),
            ("PREV_PROOF", std::env::var_os("PREV_PROOF")),
            ("NUM_HEADERS", std::env::var_os("NUM_HEADERS")),
        ];

        std::env::set_var("CUDA", "1");
        std::env::remove_var("CUDA_DEVICE_ID");

        let err =
            config_from_env().expect_err("CUDA should be rejected when the feature is absent");
        assert!(
            err.to_string().contains("--features CUDA"),
            "unexpected error: {err}",
        );

        for (key, value) in originals {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn config_validation_matrix_invalid_values() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");
        let originals = [
            ("CUDA", std::env::var_os("CUDA")),
            ("CUDA_DEVICE_ID", std::env::var_os("CUDA_DEVICE_ID")),
            ("GENERATE_GROTH16", std::env::var_os("GENERATE_GROTH16")),
            ("OUTPUT_DIR", std::env::var_os("OUTPUT_DIR")),
            ("PREV_PROOF", std::env::var_os("PREV_PROOF")),
            ("NUM_HEADERS", std::env::var_os("NUM_HEADERS")),
        ];

        // Invalid CUDA boolean value should error.
        std::env::set_var("CUDA", "maybe");
        std::env::remove_var("CUDA_DEVICE_ID");
        std::env::remove_var("GENERATE_GROTH16");
        assert!(
            config_from_env().is_err(),
            "invalid CUDA should be rejected"
        );

        // Invalid CUDA_DEVICE_ID should error when present (even if CUDA=0).
        std::env::remove_var("CUDA");
        std::env::set_var("CUDA_DEVICE_ID", "abc");
        assert!(
            config_from_env().is_err(),
            "non-numeric CUDA_DEVICE_ID should be rejected"
        );

        // Invalid GENERATE_GROTH16 boolean should error.
        std::env::remove_var("CUDA_DEVICE_ID");
        std::env::set_var("GENERATE_GROTH16", "yessir");
        assert!(
            config_from_env().is_err(),
            "invalid GENERATE_GROTH16 should be rejected"
        );

        for (key, value) in originals {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn pipeline_error_mapping_parses_failure_codes() {
        // Craft a failure MinimalPublicValues and ensure verify_public_values returns an error
        // summarizing the failure code and height.
        let state: util::State = util::State::default();
        let digest = [0xAAu8; 32];
        let vk = VerifierKeyDigest::from_raw([0x11u32; 8]);
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
