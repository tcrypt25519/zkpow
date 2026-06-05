pub(crate) mod batch;
pub(crate) mod diagnostics;
pub(crate) mod execution;
pub mod input;
pub(crate) mod proof;

use memory_usage::StageSample;
use sp1_sdk::{ExecutionReport, SP1ProofWithPublicValues};
use std::path::PathBuf;

pub use diagnostics::PhaseTiming;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverBackend {
    Mock,
    Cpu,
    Cuda,
}

#[derive(Debug, Clone)]
pub struct ProofGenerationConfig {
    pub prev_proof_path: Option<PathBuf>,
    pub num_headers: u32,
    pub batch_count: u32,
    pub db_path: PathBuf,
    pub output_dir: PathBuf,
    pub generate_groth16: bool,
    pub execute_only: bool,
    pub prover_backend: ProverBackend,
    pub cuda_device_id: Option<u32>,
}

#[derive(Debug)]
pub struct ProofArtifacts {
    pub compressed_path: Option<PathBuf>,
    pub groth16_path: Option<PathBuf>,
    pub compressed_proof: Option<SP1ProofWithPublicValues>,
    pub groth16_proof: Option<SP1ProofWithPublicValues>,
    pub before_prove_sample: StageSample,
    pub execution_report: ExecutionReport,
    pub first_new_height: u32,
    pub end_height: u32,
    pub total_duration_secs: f64,
    pub phase_timings: Vec<PhaseTiming>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    ExecuteOnly,
    ProveCompressed,
    ProveCompressedAndGroth16,
}

impl ProofGenerationConfig {
    pub fn mode(&self) -> PipelineMode {
        if self.execute_only {
            PipelineMode::ExecuteOnly
        } else if self.generate_groth16 {
            PipelineMode::ProveCompressedAndGroth16
        } else {
            PipelineMode::ProveCompressed
        }
    }
}

pub type PipelineRequest = ProofGenerationConfig;
pub type PipelineArtifacts = ProofArtifacts;
