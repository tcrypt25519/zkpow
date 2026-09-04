#![no_std]

#[cfg(target_os = "zkvm")]
extern crate alloc;

pub use zkvm_test_macros::{test, zkvm_test};

pub const MAGIC: &[u8; 4] = b"ZKVT";
pub const CMD_LIST: u8 = 0;
pub const CMD_RUN: u8 = 1;

pub const FLAG_IGNORE: u8 = 1 << 0;
pub const FLAG_SHOULD_PANIC: u8 = 1 << 1;
pub const FLAG_EXPECTED_EXIT_CODE: u8 = 1 << 2;
pub const FLAG_MAX_CYCLES: u8 = 1 << 3;
pub const FLAG_FIXTURE: u8 = 1 << 4;
pub const FLAG_SNAPSHOT: u8 = 1 << 5;

#[derive(Clone, Copy)]
pub struct TestCase {
    pub name:               &'static str,
    pub test_fn:            fn(),
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_exit_code: Option<u32>,
    pub max_cycles:         Option<u64>,
    pub fixture:            Option<&'static str>,
    pub snapshot:           Option<&'static str>,
}

#[cfg(target_os = "zkvm")]
pub fn test_runner(tests: &[&TestCase]) {
    use alloc::vec::Vec;

    let input = sp1_zkvm::io::read_vec();
    if input.len() < 5 || &input[..4] != MAGIC {
        // Fallback for direct execution without custom runner harness
        for t in tests {
            if !t.ignore {
                (t.test_fn)();
            }
        }
        return;
    }

    match input[4] {
        CMD_LIST => {
            let mut buf = Vec::new();
            buf.extend_from_slice(&(tests.len() as u32).to_le_bytes());
            for t in tests {
                let mut flags = 0u8;
                if t.ignore {
                    flags |= FLAG_IGNORE;
                }
                if t.should_panic {
                    flags |= FLAG_SHOULD_PANIC;
                }
                if t.expected_exit_code.is_some() {
                    flags |= FLAG_EXPECTED_EXIT_CODE;
                }
                if t.max_cycles.is_some() {
                    flags |= FLAG_MAX_CYCLES;
                }
                if t.fixture.is_some() {
                    flags |= FLAG_FIXTURE;
                }
                if t.snapshot.is_some() {
                    flags |= FLAG_SNAPSHOT;
                }
                buf.push(flags);
                if let Some(code) = t.expected_exit_code {
                    buf.extend_from_slice(&code.to_le_bytes());
                }
                if let Some(max) = t.max_cycles {
                    buf.extend_from_slice(&max.to_le_bytes());
                }
                if let Some(fix) = t.fixture {
                    let fix_bytes = fix.as_bytes();
                    buf.extend_from_slice(&(fix_bytes.len() as u32).to_le_bytes());
                    buf.extend_from_slice(fix_bytes);
                }
                if let Some(snap) = t.snapshot {
                    let snap_bytes = snap.as_bytes();
                    buf.extend_from_slice(&(snap_bytes.len() as u32).to_le_bytes());
                    buf.extend_from_slice(snap_bytes);
                }
                let name_bytes = t.name.as_bytes();
                buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(name_bytes);
            }
            sp1_zkvm::io::commit_slice(&buf);
        },
        CMD_RUN => {
            let name_bytes = &input[5..];
            let target_name = core::str::from_utf8(name_bytes).unwrap_or("");
            for t in tests {
                if t.name == target_name {
                    (t.test_fn)();
                    return;
                }
            }
            panic!("zkvm-test: test '{}' not found in test suite", target_name);
        },
        cmd => {
            panic!("zkvm-test: unknown command {}", cmd);
        },
    }
}

/// Commit a byte slice to public values.
///
/// On zkVM, invokes `sp1_zkvm::io::commit_slice`.
/// On host (`cargo test`), buffers the committed bytes for snapshot verification.
pub fn commit_slice(slice: &[u8]) {
    #[cfg(target_os = "zkvm")]
    {
        sp1_zkvm::io::commit_slice(slice);
    }
    #[cfg(not(target_os = "zkvm"))]
    {
        __internal::record_host_commit(slice);
    }
}

/// Retrieve the raw bytes of the fixture injected for the current test.
#[cfg(target_os = "zkvm")]
pub fn fixture() -> alloc::vec::Vec<u8> {
    sp1_zkvm::io::read_vec()
}

/// Retrieve the fixture injected for the current test as a UTF-8 string.
#[cfg(target_os = "zkvm")]
pub fn fixture_str() -> alloc::string::String {
    let bytes = fixture();
    alloc::string::String::from_utf8(bytes).expect("zkvm-test: fixture is not valid UTF-8")
}

#[cfg(not(target_os = "zkvm"))]
extern crate std;

#[cfg(not(target_os = "zkvm"))]
#[doc(hidden)]
pub mod __internal {
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
        thread_local,
    };

    thread_local! {
        pub static CURRENT_FIXTURE: RefCell<Option<&'static str>> = const { RefCell::new(None) };
        pub static CURRENT_SNAPSHOT: RefCell<Option<&'static str>> = const { RefCell::new(None) };
        pub static COMMITTED_BYTES: RefCell<std::vec::Vec<u8>> = const { RefCell::new(std::vec::Vec::new()) };
    }

    pub fn record_host_commit(slice: &[u8]) {
        COMMITTED_BYTES.with(|b| {
            b.borrow_mut().extend_from_slice(slice);
        });
    }

    pub fn with_host_fixture<R>(path: &'static str, f: impl FnOnce() -> R) -> R {
        CURRENT_FIXTURE.with(|c| {
            *c.borrow_mut() = Some(path);
        });
        let res = f();
        CURRENT_FIXTURE.with(|c| {
            *c.borrow_mut() = None;
        });
        res
    }

    pub fn with_host_snapshot<R>(path: &'static str, f: impl FnOnce() -> R) -> R {
        CURRENT_SNAPSHOT.with(|c| {
            *c.borrow_mut() = Some(path);
        });
        COMMITTED_BYTES.with(|b| {
            b.borrow_mut().clear();
        });
        let res = f();
        let committed = COMMITTED_BYTES.with(|b| b.borrow().clone());
        CURRENT_SNAPSHOT.with(|c| {
            *c.borrow_mut() = None;
        });
        verify_host_snapshot(path, &committed);
        res
    }

    pub fn with_host_fixture_and_snapshot<R>(
        fixture_path: &'static str,
        snapshot_path: &'static str,
        f: impl FnOnce() -> R,
    ) -> R {
        with_host_fixture(fixture_path, || {
            with_host_snapshot(snapshot_path, f)
        })
    }

    pub fn read_host_fixture(path: &str) -> std::vec::Vec<u8> {
        let resolved = resolve_host_path(path)
            .unwrap_or_else(|| panic!("zkvm-test: could not find fixture file '{}' on host", path));
        std::fs::read(&resolved)
            .unwrap_or_else(|e| panic!("zkvm-test: failed to read fixture '{}': {}", resolved.display(), e))
    }

    pub fn resolve_host_path(path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let candidate = Path::new(&manifest_dir).join(path);
            if candidate.exists() {
                return Some(candidate);
            }
            let mut curr = Path::new(&manifest_dir);
            while let Some(parent) = curr.parent() {
                let candidate = parent.join(path);
                if candidate.exists() {
                    return Some(candidate);
                }
                curr = parent;
            }
        }
        None
    }

    fn resolve_host_write_path(path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let candidate = Path::new(&manifest_dir).join(path);
            if candidate.exists() {
                return candidate;
            }
            let mut curr = Path::new(&manifest_dir);
            while let Some(parent) = curr.parent() {
                let candidate = parent.join(path);
                if candidate.exists() {
                    return candidate;
                }
                curr = parent;
            }
            Path::new(&manifest_dir).join(path)
        } else {
            p.to_path_buf()
        }
    }

    pub fn verify_host_snapshot(snapshot_path: &str, committed: &[u8]) {
        let resolved = resolve_host_write_path(snapshot_path);
        let accept_snapshots = std::env::var("ZKVM_ACCEPT_SNAPSHOTS").is_ok()
            || std::env::var("ACCEPT_SNAPSHOTS").is_ok();

        let is_hex = snapshot_path.ends_with(".hex");
        let actual_str = if is_hex {
            committed
                .iter()
                .map(|b| std::format!("{b:02x}"))
                .collect::<std::string::String>()
        } else {
            std::string::String::new()
        };

        if !resolved.exists() {
            if accept_snapshots {
                if let Some(parent) = resolved.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if is_hex {
                    let mut content = actual_str.clone();
                    content.push('\n');
                    let _ = std::fs::write(&resolved, content);
                } else {
                    let _ = std::fs::write(&resolved, committed);
                }
                return;
            } else {
                panic!(
                    "zkvm-test: snapshot file '{}' does not exist. Run with ZKVM_ACCEPT_SNAPSHOTS=1 or `cargo zkvm-test --accept-snapshots` to create it.",
                    resolved.display()
                );
            }
        }

        if is_hex {
            let content = std::fs::read_to_string(&resolved)
                .unwrap_or_else(|e| panic!("failed to read snapshot '{}': {e}", resolved.display()));
            let expected_str = content.trim().to_ascii_lowercase();
            if expected_str != actual_str {
                if accept_snapshots {
                    let mut new_content = actual_str;
                    new_content.push('\n');
                    let _ = std::fs::write(&resolved, new_content);
                } else {
                    panic!(
                        "zkvm-test: snapshot mismatch for '{}':\n  expected: {}\n    actual: {}\nRun with `cargo zkvm-test --accept-snapshots` to update.",
                        resolved.display(), expected_str, actual_str
                    );
                }
            }
        } else {
            let expected_bytes = std::fs::read(&resolved)
                .unwrap_or_else(|e| panic!("failed to read snapshot '{}': {e}", resolved.display()));
            if expected_bytes != committed {
                if accept_snapshots {
                    let _ = std::fs::write(&resolved, committed);
                } else {
                    panic!(
                        "zkvm-test: snapshot mismatch for '{}': expected {} bytes, committed {} bytes. Run with `cargo zkvm-test --accept-snapshots` to update.",
                        resolved.display(), expected_bytes.len(), committed.len()
                    );
                }
            }
        }
    }
}

/// Retrieve the raw bytes of the fixture injected for the current test (host fallback).
#[cfg(not(target_os = "zkvm"))]
pub fn fixture() -> std::vec::Vec<u8> {
    let path_str = __internal::CURRENT_FIXTURE
        .with(|c| *c.borrow())
        .expect("zkvm-test: fixture() called outside of a test declaring #[test(fixture = \"...\")]");
    __internal::read_host_fixture(path_str)
}

/// Retrieve the fixture injected for the current test as a UTF-8 string (host fallback).
#[cfg(not(target_os = "zkvm"))]
pub fn fixture_str() -> std::string::String {
    let bytes = fixture();
    std::string::String::from_utf8(bytes).expect("zkvm-test: fixture is not valid UTF-8")
}

#[cfg(not(target_os = "zkvm"))]
pub fn test_runner(_tests: &[&TestCase]) {}
