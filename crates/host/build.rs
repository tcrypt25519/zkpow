fn env_var_is_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

fn main() {
    // Building the guest inside an already-containerized `docker build` must use SP1's local
    // path, otherwise the build script tries to invoke Docker from inside Docker.
    let build_with_docker = env_var_is_truthy("SP1_BUILD_WITH_DOCKER");

    sp1_build::build_program_with_args(
        "../guest",
        sp1_build::BuildArgs {
            docker: build_with_docker,
            ignore_rust_version: true,
            ..Default::default()
        },
    );
}
