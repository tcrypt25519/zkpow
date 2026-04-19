fn main() {
    sp1_build::build_program_with_args(
        "../guest",
        sp1_build::BuildArgs {
            // The guest must be built in the official SP1 image so it uses the canonical
            // Succinct toolchain and reproducible build environment.
            docker: true,
            // The v6.1.0 SP1 image currently ships rustc 1.94.x while a transitive dependency
            // declares rust-version 1.95. Keep the official image and bypass the version gate.
            ignore_rust_version: true,
            ..Default::default()
        },
    );
}
