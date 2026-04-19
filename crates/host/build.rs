fn main() {
    sp1_build::build_program_with_args(
        "../guest",
        sp1_build::BuildArgs {
            docker: true,
            ..Default::default()
        },
    );
}
