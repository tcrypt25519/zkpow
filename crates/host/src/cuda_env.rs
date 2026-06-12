use std::process::Command;

use crate::pipeline::{BoxError, ProofGenerationConfig};

const MIN_CUDA_COMPUTE_CAPABILITY: ComputeCapability = ComputeCapability { major: 8, minor: 6 };
const RECOMMENDED_MIN_VRAM_MIB: u32 = 24 * 1024;
const MIN_REPORTED_CUDA_VERSION: NumericVersion = NumericVersion {
    major: 12,
    minor: 5,
    patch: 0,
};

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
pub(crate) struct CudaPreflightReport {
    selected_device_id: u32,
    gpu_count: usize,
    selected_gpu: CudaGpuInfo,
    reported_cuda_version: Option<NumericVersion>,
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

pub(crate) fn run_preflight(
    cuda_device_id: Option<u32>,
) -> Result<CudaPreflightReport, BoxError> {
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

    let selected_device_id = cuda_device_id.unwrap_or(0);
    let selected_gpu = gpus
        .get(selected_device_id as usize)
        .ok_or_else(|| {
            format!(
                "ZKPOW_CUDA_DEVICE_ID={} is out of range; machine only reports {} GPU(s)",
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

pub(crate) fn log_preflight(report: &CudaPreflightReport) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
