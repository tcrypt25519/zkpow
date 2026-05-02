//! Build script for the host crate.
//! Sets the `docker` arg iff the env var `USE_DOCKER_BUILD` is set.

fn main() {
    println!("cargo:rerun-if-env-changed=GUEST_PROFILING");
    let use_docker = std::env::var("USE_DOCKER_BUILD").is_ok();
    let guest_profiling = std::env::var("GUEST_PROFILING")
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    let features = if guest_profiling {
        vec!["profiling".to_string()]
    } else {
        Vec::new()
    };
    sp1_build::build_program_with_args(
        "../guest",
        sp1_build::BuildArgs {
            docker: use_docker,
            features,
            ignore_rust_version: true,
            ..Default::default()
        },
    );
}
