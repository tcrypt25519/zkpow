use std::collections::{BTreeMap, HashMap};
use std::env::VarError;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::util;
use crate::util::{
    HeaderChainPublicValues, Input, PublicValuesDigest, RecursiveProof, VerifierKeyDigest,
};
use num_bigint::BigUint;
use sp1_prover::build::{build_constraints_and_witness, try_build_groth16_artifacts_dir};
use sp1_prover::worker::{cpu_worker_builder, SP1LocalNodeBuilder};
use sp1_recursion_gnark_ffi::Groth16Bn254Prover;
use sp1_sdk::prelude::*;
use sp1_sdk::proof::SP1Proof;
use sp1_sdk::ExecutionReport;
use sp1_sdk::{
    HashableKey, ProveRequest, Prover, ProverClient, ProvingKey, SP1ProofWithPublicValues,
};

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

pub const ELF: Elf = include_elf!("zkpow-guest");
pub const GENESIS_HASH_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
pub const DEFAULT_DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db");
const MIN_CUDA_COMPUTE_CAPABILITY: ComputeCapability = ComputeCapability { major: 8, minor: 6 };
const RECOMMENDED_MIN_VRAM_MIB: u32 = 24 * 1024;
const MIN_REPORTED_CUDA_VERSION: NumericVersion = NumericVersion {
    major: 12,
    minor: 5,
    patch: 0,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct NumericVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ComputeCapability {
    major: u32,
    minor: u32,
}

#[derive(Debug, Clone)]
struct CudaGpuInfo {
    name: String,
    compute_capability: ComputeCapability,
    memory_total_mib: u32,
}

#[derive(Debug, Clone)]
struct CudaPreflightReport {
    selected_device_id: u32,
    gpu_count: usize,
    selected_gpu: CudaGpuInfo,
    reported_cuda_version: Option<NumericVersion>,
}

struct CompressedProofArtifacts {
    vk: sp1_prover::SP1VerifyingKey,
    compressed_proof: SP1ProofWithPublicValues,
    execution_report: ExecutionReport,
}

fn parse_genesis_hash() -> Result<util::BlockHash, BoxError> {
    let mut genesis_hash: [u8; 32] = hex::decode(GENESIS_HASH_HEX)?
        .try_into()
        .map_err(|_| "genesis hash should be 32 bytes")?;
    genesis_hash.reverse();
    Ok(util::BlockHash::from_raw(genesis_hash))
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

fn parse_numeric_version(input: &str) -> Result<NumericVersion, BoxError> {
    let mut parts = input.trim().split('.');
    let major = parts
        .next()
        .ok_or_else(|| format!("missing major version in `{input}`"))?
        .parse::<u32>()?;
    let minor = parts
        .next()
        .unwrap_or("0")
        .parse::<u32>()
        .map_err(|err| format!("invalid minor version in `{input}`: {err}"))?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .parse::<u32>()
        .map_err(|err| format!("invalid patch version in `{input}`: {err}"))?;
    Ok(NumericVersion {
        major,
        minor,
        patch,
    })
}

fn parse_compute_capability(input: &str) -> Result<ComputeCapability, BoxError> {
    let version = parse_numeric_version(input)?;
    Ok(ComputeCapability {
        major: version.major,
        minor: version.minor,
    })
}

fn run_command_stdout(program: &str, args: &[&str]) -> Result<String, BoxError> {
    let output = Command::new(program).args(args).output().map_err(|err| {
        format!(
            "failed to run `{}`: {}",
            std::iter::once(program)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" "),
            err
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "`{}` exited with {}{}",
            std::iter::once(program)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" "),
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn parse_cuda_version_from_nvidia_smi_output(
    output: &str,
) -> Result<Option<NumericVersion>, BoxError> {
    let marker = "CUDA Version:";
    let Some(start) = output.find(marker) else {
        return Ok(None);
    };
    let version = output[start + marker.len()..]
        .split_whitespace()
        .next()
        .ok_or_else(|| "missing CUDA version after `CUDA Version:`".to_string())?;
    Ok(Some(parse_numeric_version(version)?))
}

fn parse_nvidia_smi_gpu_query(output: &str) -> Result<Vec<CudaGpuInfo>, BoxError> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts = line.split(',').map(|part| part.trim()).collect::<Vec<_>>();
            if parts.len() != 3 {
                return Err(format!(
                    "unexpected `nvidia-smi` GPU query row `{line}`; expected 3 comma-separated fields"
                )
                .into());
            }
            let compute_capability = parse_compute_capability(parts[1])?;
            let memory_total_mib = parts[2].parse::<u32>().map_err(|err| {
                format!("invalid GPU memory value `{}` in `nvidia-smi` output: {err}", parts[2])
            })?;
            Ok(CudaGpuInfo {
                name: parts[0].to_owned(),
                compute_capability,
                memory_total_mib,
            })
        })
        .collect()
}

fn ensure_cuda_feature_available() -> Result<(), BoxError> {
    if cfg!(feature = "CUDA") {
        Ok(())
    } else {
        Err("CUDA=1 requires building zkpow-host with `--features CUDA`".into())
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

    ensure_cuda_feature_available()?;
    Ok(ProverBackend::Cuda)
}

fn run_cuda_preflight(config: &ProofGenerationConfig) -> Result<CudaPreflightReport, BoxError> {
    if config.prover_backend != ProverBackend::Cuda {
        return Err("internal error: CUDA preflight requested for non-CUDA config".into());
    }

    if std::env::consts::ARCH != "x86_64" {
        return Err(format!(
            "CUDA proving requires an x86_64 machine; detected architecture `{}`",
            std::env::consts::ARCH
        )
        .into());
    }

    let gpu_query = run_command_stdout(
        "nvidia-smi",
        &[
            "--query-gpu=name,compute_cap,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )?;
    let gpus = parse_nvidia_smi_gpu_query(&gpu_query)?;
    if gpus.is_empty() {
        return Err("`nvidia-smi` reported no NVIDIA GPUs".into());
    }

    let selected_device_id = config.cuda_device_id.unwrap_or(0);
    let selected_gpu = gpus
        .get(selected_device_id as usize)
        .ok_or_else(|| {
            format!(
                "CUDA_DEVICE_ID={} is out of range; machine only reports {} GPU(s)",
                selected_device_id,
                gpus.len()
            )
        })?
        .clone();

    if selected_gpu.compute_capability < MIN_CUDA_COMPUTE_CAPABILITY {
        return Err(format!(
            "GPU {} (`{}`) reports compute capability {}; SP1 requires >= {}",
            selected_device_id,
            selected_gpu.name,
            selected_gpu.compute_capability,
            MIN_CUDA_COMPUTE_CAPABILITY,
        )
        .into());
    }

    let nvidia_smi_output = run_command_stdout("nvidia-smi", &[])?;
    let reported_cuda_version = parse_cuda_version_from_nvidia_smi_output(&nvidia_smi_output)?;
    if let Some(version) = reported_cuda_version {
        if version < MIN_REPORTED_CUDA_VERSION {
            return Err(format!(
                "reported CUDA runtime {} is too old; SP1 requires at least 12.5.1",
                version
            )
            .into());
        }
    } else {
        tracing::warn!(
            "Unable to parse a CUDA runtime version from `nvidia-smi`; continuing because the GPU and driver are otherwise visible"
        );
    }

    Ok(CudaPreflightReport {
        selected_device_id,
        gpu_count: gpus.len(),
        selected_gpu,
        reported_cuda_version,
    })
}

impl std::fmt::Display for NumericVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::fmt::Display for ComputeCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
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
        db_path: PathBuf::from(DEFAULT_DB_PATH),
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

fn log_cuda_preflight(report: &CudaPreflightReport) {
    tracing::info!(
        "CUDA preflight passed: selected GPU {} of {}: `{}` (compute capability {}, {} MiB VRAM{})",
        report.selected_device_id,
        report.gpu_count,
        report.selected_gpu.name,
        report.selected_gpu.compute_capability,
        report.selected_gpu.memory_total_mib,
        report
            .reported_cuda_version
            .map(|version| format!(", reported CUDA runtime {}", version))
            .unwrap_or_default(),
    );
    if report.selected_gpu.memory_total_mib < RECOMMENDED_MIN_VRAM_MIB {
        tracing::warn!(
            "Selected GPU has {} MiB VRAM; SP1 recommends at least {} MiB (24 GiB)",
            report.selected_gpu.memory_total_mib,
            RECOMMENDED_MIN_VRAM_MIB,
        );
    }
    if report.reported_cuda_version == Some(MIN_REPORTED_CUDA_VERSION) {
        tracing::warn!(
            "The reported CUDA runtime is {}; SP1's docs call for at least 12.5.1, and `nvidia-smi` does not expose patch precision here",
            MIN_REPORTED_CUDA_VERSION,
        );
    }
}

fn build_recursive_proof(
    vk: &sp1_prover::SP1VerifyingKey,
    previous_proof: Option<&SP1ProofWithPublicValues>,
) -> Result<RecursiveProof, BoxError> {
    Ok(if let Some(prev_proof_val) = previous_proof {
        RecursiveProof {
            verifier_key: VerifierKeyDigest::from_raw(vk.hash_u32()),
            public_values_digest: PublicValuesDigest::from_raw(util::compute_pv_digest(
                &prev_proof_val.public_values.to_vec(),
            )),
        }
    } else {
        RecursiveProof::default()
    })
}

fn build_stdin(
    input: &Input,
    median_hints: &util::MedianTimePastHints,
    previous_proof: Option<&SP1ProofWithPublicValues>,
    vk: &sp1_prover::SP1VerifyingKey,
) -> Result<SP1Stdin, BoxError> {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(input.to_bytes());
    stdin.write_vec(median_hints.to_bytes());

    if let Some(prev_proof) = previous_proof {
        let SP1Proof::Compressed(inner_proof) = &prev_proof.proof else {
            return Err("previous proof is not compressed".into());
        };
        stdin.write_proof(inner_proof.as_ref().clone(), vk.vk.clone());
    }

    Ok(stdin)
}

async fn generate_compressed_proof_with_prover<P>(
    prover_name: &str,
    prover: &P,
    current_state: &util::State,
    previous_proof: Option<&SP1ProofWithPublicValues>,
    headers: &[util::NewHeader],
    median_hints: &util::MedianTimePastHints,
    expected_pv: &[u8],
) -> Result<CompressedProofArtifacts, BoxError>
where
    P: Prover,
    P::Error: Error + Send + Sync + 'static,
{
    let pk = timed_async("setup_vkey", || async { prover.setup(ELF).await }).await?;
    let recursive_proof = timed_sync("build_recursive_proof", || {
        build_recursive_proof(pk.verifying_key(), previous_proof)
    })?;
    let input = timed_sync("build_input", || -> Result<_, BoxError> {
        Input::new(current_state.clone(), recursive_proof, headers.to_vec())
            .map_err(|err| err.to_string().into())
    })?;
    let stdin = timed_sync("serialize_input", || {
        build_stdin(&input, median_hints, previous_proof, pk.verifying_key())
    })?;

    let (public_values, report) = timed_async("execute_program", || async {
        prover.execute(ELF, stdin.clone()).await
    })
    .await?;
    tracing::info!(prover = prover_name, "Execution succeeded");

    timed_sync(
        "verify_execution_public_values",
        || -> Result<(), BoxError> {
            verify_public_values(&public_values.to_vec(), expected_pv, "execution")
        },
    )?;

    let compressed_proof = timed_async("prove_compressed", || async {
        prover.prove(&pk, stdin.clone()).compressed().await
    })
    .await?;
    timed_sync(
        "verify_compressed_public_values",
        || -> Result<(), BoxError> {
            verify_public_values(
                &compressed_proof.public_values.to_vec(),
                expected_pv,
                "compressed proof",
            )
        },
    )?;
    timed_sync("verify_compressed_proof", || -> Result<(), BoxError> {
        Ok(prover.verify(&compressed_proof, pk.verifying_key(), None)?)
    })?;

    Ok(CompressedProofArtifacts {
        vk: pk.verifying_key().clone(),
        compressed_proof,
        execution_report: report,
    })
}

pub async fn generate_and_save_proofs(
    config: &ProofGenerationConfig,
) -> Result<ProofArtifacts, BoxError> {
    clear_phase_timings();
    let overall_start = Instant::now();
    log_prover_backend_selection(config);

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
            let state = match prev_public_values {
                HeaderChainPublicValues::Success(state) => state,
                HeaderChainPublicValues::Failure(failure) => {
                    return Err(format!(
                        "previous proof ended in error: {} at header {}",
                        failure.error_code, failure.header_index,
                    )
                    .into());
                }
            };
            if state.genesis_hash != genesis_hash {
                return Err("previous proof genesis mismatch".into());
            }
            Ok(state)
        } else {
            let genesis = util::load_header_record_from_db(path_to_str(&config.db_path)?, 0);
            Ok(util::genesis_state_from_record(genesis, genesis_hash))
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
        Ok(util::records_to_median_time_past_hints(&header_records))
    })?;
    let loaded_count = headers.len() as u32;
    let expected_state = timed_sync("simulate_expected_state", || -> Result<_, BoxError> {
        Ok(util::compute_final_state(&current_state, &headers))
    })?;
    let expected_pv = expected_state.to_bytes();
    let compressed_artifacts = match config.prover_backend {
        ProverBackend::Cpu => {
            let prover = timed_async("build_cpu_prover", || async {
                Ok::<_, BoxError>(ProverClient::builder().cpu().build().await)
            })
            .await?;
            generate_compressed_proof_with_prover(
                "cpu",
                &prover,
                &current_state,
                previous_proof.as_ref(),
                &headers,
                &median_hints,
                &expected_pv,
            )
            .await?
        }
        ProverBackend::Cuda => {
            let report = timed_sync("cuda_preflight", || run_cuda_preflight(config))?;
            log_cuda_preflight(&report);

            #[cfg(feature = "CUDA")]
            {
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

                generate_compressed_proof_with_prover(
                    "cuda",
                    &prover,
                    &current_state,
                    previous_proof.as_ref(),
                    &headers,
                    &median_hints,
                    &expected_pv,
                )
                .await?
            }

            #[cfg(not(feature = "CUDA"))]
            {
                unreachable!(
                    "CUDA config should already be rejected when the CUDA feature is absent"
                )
            }
        }
    };
    let vk = compressed_artifacts.vk;
    let compressed_proof = compressed_artifacts.compressed_proof;
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

        let groth16_proof = SP1ProofWithPublicValues::new(
            SP1Proof::Groth16(groth16_inner),
            compressed_proof.public_values.clone(),
            compressed_proof.sp1_version.clone(),
        );
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
        execution_report,
        first_new_height,
        end_height: start_height + loaded_count,
        total_duration_secs: total_duration.as_secs_f64(),
        phase_timings: collected_phase_timings(),
    })
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
    tracing::info!("{label} started");
    let output = f().map_err(Into::into);
    let elapsed = started.elapsed();
    record_phase_timing(label, elapsed);
    tracing::info!("{label} finished in {:?}", elapsed);
    output
}

async fn timed_async<T, E, F, Fut>(label: &'static str, f: F) -> Result<T, BoxError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Into<BoxError>,
{
    let started = Instant::now();
    tracing::info!("{label} started");
    let output = f().await.map_err(Into::into);
    let elapsed = started.elapsed();
    record_phase_timing(label, elapsed);
    tracing::info!("{label} finished in {:?}", elapsed);
    output
}

fn verify_public_values(pv: &[u8], expected_pv: &[u8], label: &str) -> Result<(), BoxError> {
    let parsed = HeaderChainPublicValues::parse(pv).map_err(|err| err.to_string())?;
    match parsed {
        HeaderChainPublicValues::Success(state) => {
            if state.to_bytes() != expected_pv {
                return Err(format!(
                    "{label} public values mismatch: expected {}, got {}",
                    hex::encode(expected_pv),
                    hex::encode(state.to_bytes()),
                )
                .into());
            }
            Ok(())
        }
        HeaderChainPublicValues::Failure(failure) => Err(format!(
            "{label} ended in error {} at header {}",
            failure.error_code, failure.header_index,
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
                "bitcoin-header-chain-proof-pipeline-{}-{}",
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
    fn parses_cuda_version_from_nvidia_smi_banner() {
        let banner = r#"
| NVIDIA-SMI 555.52.04             Driver Version: 555.52.04     CUDA Version: 12.6     |
"#;

        let parsed = parse_cuda_version_from_nvidia_smi_output(banner)
            .expect("banner should parse")
            .expect("banner should contain a CUDA version");
        assert_eq!(
            parsed,
            NumericVersion {
                major: 12,
                minor: 6,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_nvidia_smi_gpu_query_rows() {
        let query = "\
NVIDIA RTX 4090, 8.9, 24564\n\
NVIDIA RTX 3090, 8.6, 24268\n";

        let parsed = parse_nvidia_smi_gpu_query(query).expect("query output should parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "NVIDIA RTX 4090");
        assert_eq!(
            parsed[0].compute_capability,
            ComputeCapability { major: 8, minor: 9 }
        );
        assert_eq!(parsed[1].memory_total_mib, 24268);
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
}
