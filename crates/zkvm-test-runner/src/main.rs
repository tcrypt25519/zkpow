//! Cargo target runner for `riscv64im-succinct-zkvm-elf` test binaries.
//!
//! Features (Tier 2):
//! - Per-test isolation: each test executes in an isolated VM instance; a panic never halts other tests.
//! - Test filtering: test name substring matching, `--exact`, `--list`, `--ignored`.
//! - `#[should_panic]` and `#[should_panic, exit_code = N]` support.
//! - Cycle counts: reports exact VM instruction/cycle counts per test and total.
//! - Parallel execution: runs test instances concurrently across threads.
//! - Tier 1 fallback: seamlessly runs legacy/stock libtest binaries if present.

use std::{sync::Arc, time::Instant};

use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};

const MAGIC: &[u8; 4] = b"ZKVT";
const CMD_LIST: u8 = 0;
const CMD_RUN: u8 = 1;

#[derive(Debug, Clone)]
struct TestDesc {
    name:               String,
    ignore:             bool,
    should_panic:       bool,
    expected_exit_code: Option<u32>,
}

#[derive(Debug, Default)]
struct RunnerArgs {
    elf_path:        String,
    filters:         Vec<String>,
    list:            bool,
    exact:           bool,
    ignored:         bool,
    include_ignored: bool,
    nocapture:       bool,
    quiet:           bool,
    test_threads:    usize,
}

fn parse_args() -> RunnerArgs {
    let mut raw_args = std::env::args().skip(1);
    let mut args = RunnerArgs {
        test_threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        ..Default::default()
    };

    let Some(elf) = raw_args.next() else {
        eprintln!("usage: zkvm-test-runner <elf-path> [filters/flags...]");
        std::process::exit(2);
    };
    args.elf_path = elf;

    let mut iter = raw_args.peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--list" => args.list = true,
            "--exact" => args.exact = true,
            "--ignored" => args.ignored = true,
            "--include-ignored" => args.include_ignored = true,
            "--nocapture" | "--no-capture" => args.nocapture = true,
            "-q" | "--quiet" => args.quiet = true,
            "--test-threads" => {
                if let Some(val) = iter.next() {
                    if let Ok(n) = val.parse::<usize>() {
                        args.test_threads = n.max(1);
                    }
                }
            },
            s if s.starts_with("--test-threads=") => {
                if let Ok(n) = s["--test-threads=".len()..].parse::<usize>() {
                    args.test_threads = n.max(1);
                }
            },
            s if s.starts_with("--format") || s.starts_with("-Z") || s == "--show-output" => {
                // Ignore standard libtest formatting flags
            },
            s if s.starts_with('-') => {
                // Ignore unknown flags for forward compatibility with cargo
            },
            filter => {
                args.filters.push(filter.to_string());
            },
        }
    }

    if args.nocapture {
        args.test_threads = 1;
    }

    args
}

fn decode_test_list(bytes: &[u8]) -> Option<Vec<TestDesc>> {
    if bytes.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    let mut offset = 4;
    let mut tests = Vec::with_capacity(count);

    for _ in 0..count {
        if offset + 1 > bytes.len() {
            return None;
        }
        let flags = bytes[offset];
        offset += 1;

        let has_exit_code = (flags & 4) != 0;
        let expected_exit_code = if has_exit_code {
            if offset + 4 > bytes.len() {
                return None;
            }
            let code = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
            offset += 4;
            Some(code)
        } else {
            None
        };

        if offset + 4 > bytes.len() {
            return None;
        }
        let name_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;
        if offset + name_len > bytes.len() {
            return None;
        }
        let name = std::str::from_utf8(&bytes[offset..offset + name_len]).ok()?.to_string();
        offset += name_len;

        tests.push(TestDesc {
            name,
            ignore: (flags & 1) != 0,
            should_panic: (flags & 2) != 0,
            expected_exit_code,
        });
    }

    Some(tests)
}

struct TestOutcome {
    desc:           TestDesc,
    passed:         bool,
    cycles:         u64,
    failure_reason: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    let elf_bytes = match std::fs::read(&args.elf_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("zkvm-test-runner: failed to read {}: {err}", args.elf_path);
            std::process::exit(2);
        },
    };

    let client = Arc::new(ProverClient::builder().mock().build().await);

    // 1. Query test catalog (Tier 2 LIST command)
    let mut list_stdin = SP1Stdin::new();
    let mut list_payload = Vec::new();
    list_payload.extend_from_slice(MAGIC);
    list_payload.push(CMD_LIST);
    list_stdin.write_slice(&list_payload);

    let list_res = client.execute(Elf::from(elf_bytes.clone()), list_stdin).await;

    let test_catalog = match list_res {
        Ok((public_values, report)) if report.exit_code == 0 => decode_test_list(public_values.as_slice()),
        _ => None,
    };

    let all_tests = match test_catalog {
        Some(tests) => tests,
        None => {
            // Tier 1 Fallback: run the ELF once directly with empty stdin.
            let (_pv, report) = client
                .execute(Elf::from(elf_bytes), SP1Stdin::new())
                .await
                .unwrap_or_else(|err| {
                    eprintln!("zkvm-test-runner: execution failed: {err}");
                    std::process::exit(1);
                });
            let exit_code = u8::try_from(report.exit_code).unwrap_or(255);
            std::process::exit(exit_code.into());
        },
    };

    // 2. Filter tests
    let matches_filter = |name: &str| -> bool {
        if args.filters.is_empty() {
            return true;
        }
        if args.exact {
            args.filters.iter().any(|f| f == name)
        } else {
            args.filters.iter().any(|f| name.contains(f))
        }
    };

    if args.list {
        let mut count = 0;
        for t in &all_tests {
            if matches_filter(&t.name) {
                println!("{}: test", t.name);
                count += 1;
            }
        }
        println!("\n{count} tests, 0 benchmarks");
        std::process::exit(0);
    }

    let mut tests_to_run = Vec::new();
    let mut ignored_count = 0;
    let mut filtered_out = 0;

    for t in all_tests {
        if !matches_filter(&t.name) {
            filtered_out += 1;
            continue;
        }

        if t.ignore && !args.ignored && !args.include_ignored {
            ignored_count += 1;
            if !args.quiet {
                println!("test {} ... ignored", t.name);
            }
            continue;
        }

        if !t.ignore && args.ignored {
            filtered_out += 1;
            continue;
        }

        tests_to_run.push(t);
    }

    let total_count = tests_to_run.len();
    if !args.quiet {
        println!("\nrunning {total_count} test{}", if total_count == 1 { "" } else { "s" });
    }

    let start_time = Instant::now();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(args.test_threads));
    let mut handles = Vec::new();

    for test in tests_to_run {
        let sem = semaphore.clone();
        let client = client.clone();
        let elf_bytes = elf_bytes.clone();
        let quiet = args.quiet;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let mut run_stdin = SP1Stdin::new();
            let mut payload = Vec::new();
            payload.extend_from_slice(MAGIC);
            payload.push(CMD_RUN);
            payload.extend_from_slice(test.name.as_bytes());
            run_stdin.write_slice(&payload);

            let res = client.execute(Elf::from(elf_bytes), run_stdin).await;

            let (passed, cycles, failure_reason) = match res {
                Ok((_pv, report)) => {
                    let exit_code = report.exit_code;
                    let cycles = report.total_instruction_count();

                    let (passed, failure_reason) = if let Some(expected) = test.expected_exit_code {
                        if exit_code == u64::from(expected) {
                            (true, None)
                        } else {
                            (false, Some(format!("guest exited with code {exit_code}, expected {expected}")))
                        }
                    } else if test.should_panic {
                        if exit_code != 0 {
                            (true, None)
                        } else {
                            (
                                false,
                                Some("test did not panic as expected (guest exited with code 0)".to_string()),
                            )
                        }
                    } else if exit_code == 0 {
                        (true, None)
                    } else {
                        (false, Some(format!("guest exited with code {exit_code}")))
                    };

                    (passed, cycles, failure_reason)
                },
                Err(err) => (false, 0, Some(format!("executor error: {err}"))),
            };

            if !quiet {
                if passed {
                    if let Some(expected) = test.expected_exit_code {
                        println!("test {} ... ok (exit code: {}) ({} cycles)", test.name, expected, cycles);
                    } else if test.should_panic {
                        println!("test {} ... ok (should panic) ({} cycles)", test.name, cycles);
                    } else {
                        println!("test {} ... ok ({} cycles)", test.name, cycles);
                    }
                } else {
                    println!("test {} ... FAILED", test.name);
                }
            }

            TestOutcome {
                desc: test,
                passed,
                cycles,
                failure_reason,
            }
        });

        handles.push(handle);
    }

    let mut outcomes = Vec::new();
    for h in handles {
        if let Ok(outcome) = h.await {
            outcomes.push(outcome);
        }
    }

    let elapsed = start_time.elapsed();
    let mut passed_count = 0;
    let mut failed_count = 0;
    let mut total_cycles = 0;
    let mut failures = Vec::new();

    for o in outcomes {
        total_cycles += o.cycles;
        if o.passed {
            passed_count += 1;
        } else {
            failed_count += 1;
            failures.push(o);
        }
    }

    if !failures.is_empty() {
        println!("\nfailures:\n");
        for f in &failures {
            println!("---- {} ----", f.desc.name);
            if let Some(reason) = &f.failure_reason {
                println!("{reason}");
            }
        }
        println!("\nfailures:");
        for f in &failures {
            println!("    {}", f.desc.name);
        }
    }

    let status_str = if failed_count == 0 { "ok" } else { "FAILED" };
    println!(
        "\ntest result: {status_str}. {passed_count} passed; {failed_count} failed; {ignored_count} ignored; \
         0 measured; {filtered_out} filtered out; finished in {:.2}s (total cycles: {total_cycles})\n",
        elapsed.as_secs_f64()
    );

    if failed_count > 0 {
        std::process::exit(1);
    }
}
