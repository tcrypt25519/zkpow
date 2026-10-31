//! Build script for the host crate.
//! Sets the `docker` arg iff the env var `USE_DOCKER_BUILD` is set.

const GUEST_CRATE_PATH: &str = "../guest";
const ENV_USE_DOCKER_BUILD: &str = "USE_DOCKER_BUILD";
const ENV_GUEST_PROFILING: &str = "GUEST_PROFILING";

fn main() {
    println!("cargo:rustc-check-cfg=cfg(test_proofs)");
    println!("cargo:rerun-if-env-changed={ENV_GUEST_PROFILING}");
    let use_docker = std::env::var(ENV_USE_DOCKER_BUILD).is_ok();
    let guest_profiling = std::env::var(ENV_GUEST_PROFILING)
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    let features = if guest_profiling {
        vec!["profiling".to_string()]
    } else {
        Vec::new()
    };
    sp1_build::build_program_with_args(GUEST_CRATE_PATH, sp1_build::BuildArgs {
        docker: use_docker,
        features,
        ignore_rust_version: true,
        ..Default::default()
    });
}
