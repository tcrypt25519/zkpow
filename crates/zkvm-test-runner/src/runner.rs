use std::{sync::Arc, time::Instant};

use sp1_sdk::{prelude::*, Elf, ProverClient, SP1Stdin};

pub const MAGIC: &[u8; 4] = b"ZKVT";
pub const CMD_LIST: u8 = 0;
pub const CMD_RUN: u8 = 1;

pub const FLAG_IGNORE: u8 = 1 << 0;
pub const FLAG_SHOULD_PANIC: u8 = 1 << 1;
pub const FLAG_EXPECTED_EXIT_CODE: u8 = 1 << 2;
pub const FLAG_MAX_CYCLES: u8 = 1 << 3;

pub const METRICS_DIR: &str = ".zkvm-test-metrics";
pub const DEFAULT_BASELINE_NAME: &str = "default";
pub const DEFAULT_PROVER: &str = "mock";
pub const PROVER_ENV_VAR: &str = "ZKVM_PROVER";
const DEFAULT_CONCURRENCY: usize = 4;

#[derive(Debug, Clone)]
pub struct TestDesc {
    pub name:               String,
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_exit_code: Option<u32>,
    pub max_cycles:         Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct RunnerArgs {
    pub elf_path:           String,
    pub filters:            Vec<String>,
    pub list:               bool,
    pub exact:              bool,
    pub ignored:            bool,
    pub include_ignored:    bool,
    pub nocapture:          bool,
    pub quiet:              bool,
    pub profile:            bool,
    pub test_threads:       usize,
    pub prover:             Option<String>,
    pub save_baseline:      Option<String>,
    pub compare_baseline:   Option<String>,
    pub fail_on_regression: Option<f64>,
}

pub fn parse_args_from<I, T>(iter: I) -> RunnerArgs
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let mut raw_args = iter.into_iter().map(Into::into);
    let mut args = RunnerArgs {
        test_threads: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(DEFAULT_CONCURRENCY),
        ..Default::default()
    };

    let Some(elf) = raw_args.next() else {
        eprintln!("usage: zkvm-test-runner <elf-path> [filters/flags...]");
        std::process::exit(2);
    };
    args.elf_path = elf;

    let mut iter = raw_args.peekable();
    while let Some(arg) = iter.next() {
        let (key, mut inline_val) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        let mut next_val = || inline_val.take().or_else(|| iter.next());

        match key {
            "--list" => args.list = true,
            "--exact" => args.exact = true,
            "--ignored" => args.ignored = true,
            "--include-ignored" => args.include_ignored = true,
            "--nocapture" | "--no-capture" => args.nocapture = true,
            "--profile" => args.profile = true,
            "-q" | "--quiet" => args.quiet = true,
            "--test-threads" => {
                if let Some(val) = next_val() {
                    if let Ok(n) = val.parse::<usize>() {
                        args.test_threads = n.max(1);
                    }
                }
            },
            "--prover" => {
                args.prover = next_val();
            },
            "--save-baseline" => {
                args.save_baseline = inline_val.or_else(|| {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            return iter.next();
                        }
                    }
                    Some(DEFAULT_BASELINE_NAME.to_string())
                });
            },
            "--compare-baseline" => {
                args.compare_baseline = next_val();
            },
            "--fail-on-regression" => {
                if let Some(val) = next_val() {
                    let cleaned = val.trim_end_matches('%');
                    if let Ok(pct) = cleaned.parse::<f64>() {
                        args.fail_on_regression = Some(pct);
                    }
                }
            },
            s if s.starts_with("--format") || s.starts_with("-Z") || s == "--show-output" => {
                // Ignore standard libtest formatting flags
            },
            s if s.starts_with('-') => {
                // Ignore unknown flags for forward compatibility with cargo
            },
            _ => {
                args.filters.push(arg);
            },
        }
    }

    if args.nocapture {
        args.test_threads = 1;
    }

    args
}

fn read_u8(bytes: &[u8], offset: &mut usize) -> Option<u8> {
    let val = *bytes.get(*offset)?;
    *offset += 1;
    Some(val)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let slice = bytes.get(*offset..*offset + 4)?;
    *offset += 4;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let slice = bytes.get(*offset..*offset + 8)?;
    *offset += 8;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

fn read_str(bytes: &[u8], offset: &mut usize, len: usize) -> Option<String> {
    let slice = bytes.get(*offset..*offset + len)?;
    *offset += len;
    std::str::from_utf8(slice).ok().map(String::from)
}

pub fn decode_test_list(bytes: &[u8]) -> Option<Vec<TestDesc>> {
    let mut offset = 0;
    let count = read_u32(bytes, &mut offset)? as usize;
    let mut tests = Vec::with_capacity(count);

    for _ in 0..count {
        let flags = read_u8(bytes, &mut offset)?;
        let expected_exit_code = if (flags & FLAG_EXPECTED_EXIT_CODE) != 0 {
            Some(read_u32(bytes, &mut offset)?)
        } else {
            None
        };

        let max_cycles = if (flags & FLAG_MAX_CYCLES) != 0 {
            Some(read_u64(bytes, &mut offset)?)
        } else {
            None
        };

        let name_len = read_u32(bytes, &mut offset)? as usize;
        let name = read_str(bytes, &mut offset, name_len)?;

        tests.push(TestDesc {
            name,
            ignore: (flags & FLAG_IGNORE) != 0,
            should_panic: (flags & FLAG_SHOULD_PANIC) != 0,
            expected_exit_code,
            max_cycles,
        });
    }

    Some(tests)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestMetric {
    pub cycles:        u64,
    pub record_bytes:  u64,
    pub cycle_tracker: std::collections::BTreeMap<String, u64>,
    pub invocations:   std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaselineSuite {
    pub created_at:   String,
    pub total_cycles: u64,
    pub tests:        std::collections::BTreeMap<String, TestMetric>,
}

pub struct TestOutcome {
    pub desc:           TestDesc,
    pub passed:         bool,
    pub cycles:         u64,
    pub failure_reason: Option<String>,
    pub stdout:         String,
    pub stderr:         String,
    pub record_bytes:   u64,
    pub cycle_tracker:  std::collections::BTreeMap<String, u64>,
    pub invocations:    std::collections::BTreeMap<String, u64>,
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

pub fn format_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let offset = s.len() % 3;
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (i % 3 == offset || (offset == 0 && i % 3 == 0)) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn print_profile_breakdown(
    cycle_tracker: &std::collections::BTreeMap<String, u64>,
    invocations: &std::collections::BTreeMap<String, u64>,
    total_cycles: u64,
) {
    if cycle_tracker.is_empty() {
        return;
    }
    let mut entries: Vec<_> = cycle_tracker.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (i, (label, count)) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;
        let branch = if is_last { "└── " } else { "├── " };
        let pct = if total_cycles > 0 {
            (**count as f64 / total_cycles as f64) * 100.0
        } else {
            0.0
        };
        let inv_str = if let Some(inv) = invocations.get(*label) {
            format!(" [{} call{}]", inv, if *inv == 1 { "" } else { "s" })
        } else {
            String::new()
        };
        println!(
            "    {branch}{}: {} cycles ({:.1}%){inv_str}",
            label,
            format_commas(**count),
            pct
        );
    }
}

fn print_failures(failures: &[&TestOutcome]) {
    if failures.is_empty() {
        return;
    }
    println!("\nfailures:\n");
    for f in failures {
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
    for f in failures {
        println!("    {}", f.desc.name);
    }
}

fn compare_against_baseline(
    baseline_name: &str,
    outcomes: &[TestOutcome],
    threshold: Option<f64>,
) -> bool {
    let path = format!("{METRICS_DIR}/{baseline_name}.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        eprintln!("zkvm-test-runner: failed to read baseline from {path}");
        return false;
    };
    let Ok(baseline) = serde_json::from_str::<BaselineSuite>(&content) else {
        eprintln!("zkvm-test-runner: failed to parse baseline from {path}");
        return false;
    };

    println!("\nBaseline comparison against '{baseline_name}':");
    println!("{:<60} {:>14} {:>14} {:>14}", "Test Name", "Baseline", "Current", "Change");
    println!("{}", "-".repeat(106));

    let mut regression_detected = false;
    for o in outcomes {
        let Some(base_metric) = baseline.tests.get(&o.desc.name) else {
            continue;
        };
        let diff = (o.cycles as i64) - (base_metric.cycles as i64);
        let pct = if base_metric.cycles > 0 {
            (diff as f64 / base_metric.cycles as f64) * 100.0
        } else {
            0.0
        };
        let sign = if diff > 0 { "+" } else { "" };
        let is_regression = threshold.is_some_and(|t| pct > t);
        let flag = if is_regression {
            regression_detected = true;
            " [REGRESSION]"
        } else {
            ""
        };
        let change_str = format!("{sign}{pct:.1}%");
        println!(
            "{:<60} {:>14} {:>14} {:>14}{}",
            o.desc.name,
            format_commas(base_metric.cycles),
            format_commas(o.cycles),
            change_str,
            flag
        );
    }
    println!("{}", "-".repeat(106));
    regression_detected
}

fn save_metrics_baseline(save_name: &str, outcomes: &[TestOutcome], total_cycles: u64) {
    let _ = std::fs::create_dir_all(METRICS_DIR);
    let path = format!("{METRICS_DIR}/{save_name}.json");

    let mut tests_map = std::collections::BTreeMap::new();
    for o in outcomes {
        tests_map.insert(
            o.desc.name.clone(),
            TestMetric {
                cycles:        o.cycles,
                record_bytes:  o.record_bytes,
                cycle_tracker: o.cycle_tracker.clone(),
                invocations:   o.invocations.clone(),
            },
        );
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let suite = BaselineSuite {
        created_at: format!("{now}"),
        total_cycles,
        tests: tests_map,
    };

    let Ok(json) = serde_json::to_string_pretty(&suite) else {
        eprintln!("zkvm-test-runner: failed to serialize baseline metrics");
        return;
    };
    if std::fs::write(&path, json).is_ok() {
        println!(
            "Saved metrics baseline to {path} ({} tests, {} total cycles)",
            outcomes.len(),
            format_commas(total_cycles)
        );
    } else {
        eprintln!("zkvm-test-runner: failed to write baseline to {path}");
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
        .or_else(|| std::env::var(PROVER_ENV_VAR).ok())
        .unwrap_or_else(|| DEFAULT_PROVER.to_string());

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

    let peak_rss_kb = Arc::new(std::sync::atomic::AtomicU64::new(
        memuse::rss_kb().unwrap_or(0),
    ));

    for test in tests_to_run {
        let sem = semaphore.clone();
        let prover = prover.clone();
        let elf_bytes = elf_bytes.clone();
        let quiet = args.quiet;
        let profile = args.profile;
        let nocapture = args.nocapture;
        let peak_rss = peak_rss_kb.clone();

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

            if let Some(rss) = memuse::rss_kb() {
                peak_rss.fetch_max(rss, std::sync::atomic::Ordering::Relaxed);
            }

            let stdout = stdout_rx.borrow().clone();
            let stderr = stderr_rx.borrow().clone();

            let (mut passed, cycles, mut failure_reason, record_bytes, cycle_tracker, invocations) = match res {
                Ok((_pv, report)) => {
                    let exit_code = report.exit_code;
                    let cycles = report.total_instruction_count();
                    let record_bytes = report.total_record_size();
                    let cycle_tracker: std::collections::BTreeMap<String, u64> =
                        report.cycle_tracker.into_iter().collect();
                    let invocations: std::collections::BTreeMap<String, u64> =
                        report.invocation_tracker.into_iter().collect();

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

                    (passed, cycles, failure_reason, record_bytes, cycle_tracker, invocations)
                },
                Err(err) => (false, 0, Some(format!("executor error: {err}")), 0, Default::default(), Default::default()),
            };

            // Check declarative cycle budget
            if passed {
                if let Some(max) = test.max_cycles {
                    if cycles > max {
                        passed = false;
                        failure_reason = Some(format!(
                            "cycle budget exceeded: {} cycles > budget of {}",
                            format_commas(cycles),
                            format_commas(max)
                        ));
                    }
                }
            }

            if !quiet {
                if passed {
                    let mut meta_items = Vec::new();
                    if let Some(expected) = test.expected_exit_code {
                        meta_items.push(format!("exit code: {expected}"));
                    } else if test.should_panic {
                        meta_items.push("should panic".to_string());
                    }
                    if let Some(max) = test.max_cycles {
                        meta_items.push(format!("budget: {}", format_commas(max)));
                    }
                    meta_items.push(format!("{} cycles", format_commas(cycles)));
                    if record_bytes > 0 {
                        meta_items.push(format!("trace: {}", format_bytes(record_bytes)));
                    }

                    println!("test {} ... ok ({})", test.name, meta_items.join(", "));

                    if profile {
                        print_profile_breakdown(&cycle_tracker, &invocations, cycles);
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
                record_bytes,
                cycle_tracker,
                invocations,
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

    for o in &outcomes {
        total_cycles += o.cycles;
        if o.passed {
            passed_count += 1;
        } else {
            failed_count += 1;
            failures.push(o);
        }
    }

    print_failures(&failures);

    let mut regression_failed = false;
    if let Some(baseline_name) = &args.compare_baseline {
        regression_failed = compare_against_baseline(baseline_name, &outcomes, args.fail_on_regression);
    }

    if let Some(save_name) = &args.save_baseline {
        save_metrics_baseline(save_name, &outcomes, total_cycles);
    }

    if regression_failed {
        println!("\nerror: performance regression detected exceeding configured threshold!");
    }

    let peak_mb = peak_rss_kb.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1024.0;
    let peak_str = if peak_mb >= 1024.0 {
        format!(", peak host rss: {:.2} GB", peak_mb / 1024.0)
    } else if peak_mb > 0.0 {
        format!(", peak host rss: {:.1} MB", peak_mb)
    } else {
        String::new()
    };

    let status_str = if failed_count == 0 && !regression_failed { "ok" } else { "FAILED" };
    println!(
        "\ntest result: {status_str}. {passed_count} passed; {failed_count} failed; {ignored_count} ignored; \
         0 measured; {filtered_out} filtered out; finished in {:.2}s (total cycles: {}{})\n",
        elapsed.as_secs_f64(),
        format_commas(total_cycles),
        peak_str
    );

    if failed_count > 0 || regression_failed { 1 } else { 0 }
}

