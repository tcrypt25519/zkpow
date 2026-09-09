use assert_cmd::Command;
use predicates::prelude::*;

fn cargo_zkvm_test_cmd() -> Command {
    let mut cmd = Command::cargo_bin("cargo-zkvm-test").unwrap();
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(ws_root) = manifest_dir.parent().and_then(|p| p.parent()) {
        cmd.current_dir(ws_root);
    }
    // Help output is asserted verbatim, so a developer shell forcing color must not
    // inject ANSI escapes between the words.
    cmd.env_remove("CLICOLOR_FORCE").env("NO_COLOR", "1");
    cmd
}
#[test]
fn test_cargo_zkvm_test_help() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "cargo-zkvm-test: Nextest-grade test runner for zkVM binaries",
        ))
        .stdout(predicate::str::contains("-p, --package"))
        .stdout(predicate::str::contains("-E, --expr"))
        .stdout(predicate::str::contains("--junit-path"))
        .stdout(predicate::str::contains("--accept-snapshots"))
        .stdout(predicate::str::contains("--mode <MODE>"));
}

#[test]
fn test_cargo_zkvm_test_invalid_filter_syntax() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-E", "test(unclosed"]);
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("invalid filter expression"))
        .stderr(predicate::str::contains("expected close parenthesis"));
}

#[test]
fn test_cargo_zkvm_test_filter_prune_all_packages() {
    // `kind(bench)` has no matching targets in the workspace crates
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-E", "kind(bench)"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0 packages matched filter expression"));
}

#[test]
fn test_zkvm_test_runner_no_args() {
    let mut cmd = Command::cargo_bin("zkvm-test-runner").unwrap();
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("usage: zkvm-test-runner <elf-path>"));
}

#[test]
fn test_zkvm_test_runner_nonexistent_elf() {
    let mut cmd = Command::cargo_bin("zkvm-test-runner").unwrap();
    cmd.arg("/path/does/not/exist/test.elf");
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("failed to read /path/does/not/exist/test.elf"));
}

#[test]
fn test_cargo_zkvm_test_list_tests() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-p", "zkpow-sha256", "--list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("tests::test_genesis_header_hash_snapshot: test"))
        .stdout(predicate::str::contains("tests, 0 benchmarks"));
}

#[test]
fn test_cargo_zkvm_test_nextest_filter_execution() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-p", "zkpow-sha256", "-E", "test(genesis)"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "test zkpow_sha256::tests::test_genesis_header_hash_snapshot ... ok",
        ))
        .stdout(predicate::str::contains("test result: ok. 1 passed; 0 failed"));
}

#[test]
fn test_cargo_zkvm_test_junit_export() {
    let temp_dir = tempfile::tempdir().unwrap();
    let junit_file = temp_dir.path().join("junit_cli_test.xml");

    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args([
        "-p",
        "zkpow-sha256",
        "-E",
        "test(genesis)",
        "--junit-path",
        junit_file.to_str().unwrap(),
    ]);

    cmd.assert().success().stdout(predicate::str::contains(
        "test zkpow_sha256::tests::test_genesis_header_hash_snapshot ... ok",
    ));

    assert!(junit_file.exists(), "JUnit XML file should be created");
    let content = std::fs::read_to_string(&junit_file).unwrap();
    assert!(
        content.contains("<testsuites name=\"zkvm-test\""),
        "Root testsuites tag expected"
    );
    assert!(content.contains("tests=\"1\""), "One test expected in suite");
    assert!(
        content.contains("name=\"zkpow_sha256::tests::test_genesis_header_hash_snapshot\""),
        "Testcase expected"
    );
    assert!(content.contains("<property name=\"cycles\""), "Cycle property expected");
}

#[test]
fn test_zkvm_test_runner_rejects_bad_flag_value() {
    let mut cmd = Command::cargo_bin("zkvm-test-runner").unwrap();
    cmd.args(["/path/does/not/exist/test.elf", "--test-threads=zero"]);
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("--test-threads expects a positive integer"));
}

#[test]
fn test_cargo_zkvm_test_subcycles_alias_reaches_runner() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-p", "zkpow-sha256", "-E", "test(genesis)", "--subcycles"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Instruction Categories:"));
}

#[test]
fn test_cargo_zkvm_test_host_only_package() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-p", "zkvm-test-runner", "--", "test_parse_libtest_line"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("does not target zkVM; running host tests"))
        .stdout(predicate::str::contains("test plugin::tests::test_parse_libtest_line ... ok"));
}

#[test]
fn test_cargo_zkvm_test_host_flag_on_zkvm_package() {
    let mut cmd = cargo_zkvm_test_cmd();
    cmd.args(["-p", "zkpow-sha256", "--host", "--", "test_genesis_header_hash_snapshot"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("testing 1 host package: zkpow-sha256"))
        .stdout(predicate::str::contains("Running host tests for `zkpow_sha256`"))
        .stdout(predicate::str::contains("test tests::test_genesis_header_hash_snapshot ... ok"));
}
