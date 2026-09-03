use std::{sync::Arc, time::Instant};

use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};

pub const MAGIC: &[u8; 4] = b"ZKVT";
pub const CMD_LIST: u8 = 0;
pub const CMD_RUN: u8 = 1;

#[derive(Debug, Clone)]
pub struct TestDesc {
    pub name:               String,
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_exit_code: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RunnerArgs {
    pub elf_path:        String,
    pub filters:         Vec<String>,
    pub list:            bool,
    pub exact:           bool,
    pub ignored:         bool,
    pub include_ignored: bool,
    pub nocapture:       bool,
    pub quiet:           bool,
    pub test_threads:    usize,
    pub prover:          Option<String>,
}

pub fn parse_args_from<I, T>(iter: I) -> RunnerArgs
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let mut raw_args = iter.into_iter().map(Into::into);
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
            "--prover" => {
                if let Some(val) = iter.next() {
                    args.prover = Some(val);
                }
            },
            s if s.starts_with("--prover=") => {
                args.prover = Some(s["--prover=".len()..].to_string());
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

pub fn decode_test_list(bytes: &[u8]) -> Option<Vec<TestDesc>> {
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

pub struct TestOutcome {
    pub desc:           TestDesc,
    pub passed:         bool,
    pub cycles:         u64,
    pub failure_reason: Option<String>,
    pub stdout:         String,
    pub stderr:         String,
}

#[derive(Clone)]
pub enum AnyProver {
    Mock(sp1_sdk::MockProver),
    Cpu(sp1_sdk::CpuProver),
    #[cfg(feature = "cuda")]
    Cuda(sp1_sdk::CudaProver),
}

impl AnyProver {
    pub async fn from_choice(choice: &str) -> Result<Self, String> {
        match choice.to_lowercase().as_str() {
            "mock" => Ok(AnyProver::Mock(ProverClient::builder().mock().build().await)),
            "cpu" => Ok(AnyProver::Cpu(ProverClient::builder().cpu().build().await)),
            #[cfg(feature = "cuda")]
            "cuda" => Ok(AnyProver::Cuda(ProverClient::builder().cuda().build().await)),
            #[cfg(not(feature = "cuda"))]
            "cuda" => Err("CUDA prover requested, but zkvm-test-runner was compiled without the 'cuda' feature".to_string()),
            other => Err(format!("unknown prover '{other}'. Expected 'mock', 'cpu', or 'cuda'.")),
        }
    }

    pub async fn execute_test(
        &self,
        elf: Elf,
        stdin: SP1Stdin,
        stdout_tx: tokio::sync::watch::Sender<String>,
        stderr_tx: tokio::sync::watch::Sender<String>,
    ) -> Result<(sp1_sdk::SP1PublicValues, sp1_sdk::ExecutionReport), sp1_core_executor::ExecutionError> {
        match self {
            AnyProver::Mock(p) => p.execute(elf, stdin).stdout(stdout_tx).stderr(stderr_tx).await,
            AnyProver::Cpu(p) => p.execute(elf, stdin).stdout(stdout_tx).stderr(stderr_tx).await,
            #[cfg(feature = "cuda")]
            AnyProver::Cuda(p) => p.execute(elf, stdin).stdout(stdout_tx).stderr(stderr_tx).await,
        }
    }
}

pub async fn run(args: RunnerArgs) -> i32 {
    let elf_bytes = match std::fs::read(&args.elf_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("zkvm-test-runner: failed to read {}: {err}", args.elf_path);
            return 2;
        },
    };

    let prover_choice = args
        .prover
        .clone()
        .or_else(|| std::env::var("ZKVM_PROVER").ok())
        .unwrap_or_else(|| "mock".to_string());

    let prover = match AnyProver::from_choice(&prover_choice).await {
        Ok(p) => Arc::new(p),
        Err(err) => {
            eprintln!("zkvm-test-runner: {err}");
            return 2;
        },
    };

    // 1. Query test catalog (Tier 2 LIST command)
    let mut list_stdin = SP1Stdin::new();
    let mut list_payload = Vec::new();
    list_payload.extend_from_slice(MAGIC);
    list_payload.push(CMD_LIST);
    list_stdin.write_slice(&list_payload);

    let (list_tx, _list_rx) = tokio::sync::watch::channel(String::new());
    let (list_err_tx, _list_err_rx) = tokio::sync::watch::channel(String::new());
    let list_res = prover.execute_test(Elf::from(elf_bytes.clone()), list_stdin, list_tx, list_err_tx).await;

    let test_catalog = match list_res {
        Ok((public_values, report)) if report.exit_code == 0 => decode_test_list(public_values.as_slice()),
        _ => None,
    };

    let all_tests = match test_catalog {
        Some(tests) => tests,
        None => {
            // Tier 1 Fallback: run the ELF once directly with empty stdin and forward output.
            let (stdout_tx, stdout_rx) = tokio::sync::watch::channel(String::new());
            let (stderr_tx, stderr_rx) = tokio::sync::watch::channel(String::new());

            let execute_res = prover
                .execute_test(Elf::from(elf_bytes), SP1Stdin::new(), stdout_tx, stderr_tx)
                .await;

            let stdout = stdout_rx.borrow().clone();
            let stderr = stderr_rx.borrow().clone();
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            let (_pv, report) = match execute_res {
                Ok(res) => res,
                Err(err) => {
                    eprintln!("zkvm-test-runner: execution failed: {err}");
                    return 1;
                },
            };
            let exit_code = u8::try_from(report.exit_code).unwrap_or(255);
            return exit_code.into();
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
        return 0;
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
        println!(
            "\nrunning {total_count} test{} (prover: {prover_choice}, threads: {})",
            if total_count == 1 { "" } else { "s" },
            args.test_threads
        );
    }

    let start_time = Instant::now();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(args.test_threads));
    let mut handles = Vec::new();

    for test in tests_to_run {
        let sem = semaphore.clone();
        let prover = prover.clone();
        let elf_bytes = elf_bytes.clone();
        let quiet = args.quiet;
        let nocapture = args.nocapture;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let mut run_stdin = SP1Stdin::new();
            let mut payload = Vec::new();
            payload.extend_from_slice(MAGIC);
            payload.push(CMD_RUN);
            payload.extend_from_slice(test.name.as_bytes());
            run_stdin.write_slice(&payload);

            let (stdout_tx, stdout_rx) = tokio::sync::watch::channel(String::new());
            let (stderr_tx, stderr_rx) = tokio::sync::watch::channel(String::new());

            let res = prover
                .execute_test(Elf::from(elf_bytes), run_stdin, stdout_tx, stderr_tx)
                .await;

            let stdout = stdout_rx.borrow().clone();
            let stderr = stderr_rx.borrow().clone();

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

                if nocapture {
                    if !stdout.is_empty() {
                        print!("{stdout}");
                    }
                    if !stderr.is_empty() {
                        eprint!("{stderr}");
                    }
                }
            }

            TestOutcome {
                desc: test,
                passed,
                cycles,
                failure_reason,
                stdout,
                stderr,
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
            if !f.stdout.is_empty() {
                println!("\n---- {} stdout ----", f.desc.name);
                print!("{}", f.stdout);
                if !f.stdout.ends_with('\n') {
                    println!();
                }
            }
            if !f.stderr.is_empty() {
                println!("\n---- {} stderr ----", f.desc.name);
                print!("{}", f.stderr);
                if !f.stderr.ends_with('\n') {
                    println!();
                }
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

    if failed_count > 0 { 1 } else { 0 }
}
