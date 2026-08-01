use super::*;

#[test]
fn parses_cuda_version_from_nvidia_smi_banner() {
    let banner = r#"
| NVIDIA-SMI 555.52.04             Driver Version: 555.52.04     CUDA Version: 12.6     |
"#;

    let parsed = parse_cuda_version_from_nvidia_smi_output(banner)
        .expect("banner should parse")
        .expect("banner should contain a CUDA version");
    assert_eq!(parsed, NumericVersion {
        major: 12,
        minor: 6,
        patch: 0,
    });
}

#[test]
fn parses_nvidia_smi_gpu_query_rows() {
    let query = "\
NVIDIA RTX 4090, 8.9, 24564\n\
NVIDIA RTX 3090, 8.6, 24268\n";

    let parsed = parse_nvidia_smi_gpu_query(query).expect("query output should parse");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "NVIDIA RTX 4090");
    assert_eq!(parsed[0].compute_capability, ComputeCapability { major: 8, minor: 9 });
    assert_eq!(parsed[1].memory_total_mib, 24268);
}
