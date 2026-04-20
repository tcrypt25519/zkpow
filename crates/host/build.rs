//! Build script for the host crate.
//! Sets the `docker` arg iff the env var `USE_DOCKER_BUILD` is set.

fn main() {
    let use_docker = std::env::var("USE_DOCKER_BUILD").is_ok();
    sp1_build::build_program_with_args(
        "../guest",
        sp1_build::BuildArgs {
            docker: use_docker,
            ignore_rust_version: true,
            ..Default::default()
        },
    );
}
