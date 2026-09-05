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

pub trait ByteBuf {
    fn write_u8(&mut self, val: u8);
    fn write_u32_le(&mut self, val: u32);
    fn write_u64_le(&mut self, val: u64);
    fn write_len_prefixed_slice(&mut self, slice: &[u8]);
    fn write_len_prefixed_str(&mut self, s: &str) {
        self.write_len_prefixed_slice(s.as_bytes());
    }
}

#[cfg(target_os = "zkvm")]
impl ByteBuf for alloc::vec::Vec<u8> {
    #[inline]
    fn write_u8(&mut self, val: u8) {
        self.push(val);
    }

    #[inline]
    fn write_u32_le(&mut self, val: u32) {
        self.extend_from_slice(&val.to_le_bytes());
    }

    #[inline]
    fn write_u64_le(&mut self, val: u64) {
        self.extend_from_slice(&val.to_le_bytes());
    }

    #[inline]
    fn write_len_prefixed_slice(&mut self, slice: &[u8]) {
        self.write_u32_le(slice.len() as u32);
        self.extend_from_slice(slice);
    }
}

impl TestCase {
    pub const fn flags(&self) -> u8 {
        let mut flags = 0u8;
        if self.ignore {
            flags |= FLAG_IGNORE;
        }
        if self.should_panic {
            flags |= FLAG_SHOULD_PANIC;
        }
        if self.expected_exit_code.is_some() {
            flags |= FLAG_EXPECTED_EXIT_CODE;
        }
        if self.max_cycles.is_some() {
            flags |= FLAG_MAX_CYCLES;
        }
        if self.fixture.is_some() {
            flags |= FLAG_FIXTURE;
        }
        if self.snapshot.is_some() {
            flags |= FLAG_SNAPSHOT;
        }
        flags
    }

    #[cfg(target_os = "zkvm")]
    pub fn encode<B: ByteBuf>(&self, buf: &mut B) {
        buf.write_u8(self.flags());
        if let Some(code) = self.expected_exit_code {
            buf.write_u32_le(code);
        }
        if let Some(max) = self.max_cycles {
            buf.write_u64_le(max);
        }
        if let Some(fix) = self.fixture {
            buf.write_len_prefixed_str(fix);
        }
        if let Some(snap) = self.snapshot {
            buf.write_len_prefixed_str(snap);
        }
        buf.write_len_prefixed_str(self.name);
    }
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
            buf.write_u32_le(tests.len() as u32);
            for t in tests {
                t.encode(&mut buf);
            }
            sp1_zkvm::io::commit_slice(&buf);
        },
        CMD_RUN => {
            let name_bytes = &input[5..];
            let target_name = core::str::from_utf8(name_bytes).unwrap_or("");
            if let Some(t) = tests.iter().find(|t| t.name == target_name) {
                (t.test_fn)();
                return;
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SnapshotFormat {
        Hex,
        Binary,
    }

    impl SnapshotFormat {
        pub fn from_path(path: &Path) -> Self {
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("hex")) {
                Self::Hex
            } else {
                Self::Binary
            }
        }

        pub fn format_committed(&self, committed: &[u8]) -> std::vec::Vec<u8> {
            match self {
                Self::Hex => {
                    let mut s = committed
                        .iter()
                        .map(|b| std::format!("{b:02x}"))
                        .collect::<std::string::String>();
                    s.push('\n');
                    s.into_bytes()
                },
                Self::Binary => committed.to_vec(),
            }
        }
    }

    fn find_in_manifest_ancestors(path: &Path, manifest_dir: &Path) -> Option<PathBuf> {
        let candidate = manifest_dir.join(path);
        if candidate.exists() {
            return Some(candidate);
        }
        let mut curr = manifest_dir;
        while let Some(parent) = curr.parent() {
            let candidate = parent.join(path);
            if candidate.exists() {
                return Some(candidate);
            }
            curr = parent;
        }
        None
    }

    pub fn resolve_host_path(path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
        find_in_manifest_ancestors(p, Path::new(&manifest_dir))
    }

    fn resolve_host_write_path(path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest_path = Path::new(&manifest_dir);
            find_in_manifest_ancestors(p, manifest_path)
                .unwrap_or_else(|| manifest_path.join(path))
        } else {
            p.to_path_buf()
        }
    }

    pub fn verify_host_snapshot(snapshot_path: &str, committed: &[u8]) {
        let resolved = resolve_host_write_path(snapshot_path);
        let accept_snapshots = std::env::var("ZKVM_ACCEPT_SNAPSHOTS").is_ok()
            || std::env::var("ACCEPT_SNAPSHOTS").is_ok();
        let format = SnapshotFormat::from_path(&resolved);

        if !resolved.exists() {
            if accept_snapshots {
                if let Some(parent) = resolved.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&resolved, format.format_committed(committed));
                return;
            } else {
                panic!(
                    "zkvm-test: snapshot file '{}' does not exist. Run with ZKVM_ACCEPT_SNAPSHOTS=1 or `cargo zkvm-test --accept-snapshots` to create it.",
                    resolved.display()
                );
            }
        }

        match format {
            SnapshotFormat::Hex => {
                let content = std::fs::read_to_string(&resolved)
                    .unwrap_or_else(|e| panic!("failed to read snapshot '{}': {e}", resolved.display()));
                let expected = content.trim().to_ascii_lowercase();
                let actual = committed.iter().map(|b| std::format!("{b:02x}")).collect::<std::string::String>();
                if expected != actual {
                    if accept_snapshots {
                        let _ = std::fs::write(&resolved, format.format_committed(committed));
                    } else {
                        panic!(
                            "zkvm-test: snapshot mismatch for '{}':\n  expected: {}\n    actual: {}\nRun with `cargo zkvm-test --accept-snapshots` to update.",
                            resolved.display(), expected, actual
                        );
                    }
                }
            },
            SnapshotFormat::Binary => {
                let expected = std::fs::read(&resolved)
                    .unwrap_or_else(|e| panic!("failed to read snapshot '{}': {e}", resolved.display()));
                if expected != committed {
                    if accept_snapshots {
                        let _ = std::fs::write(&resolved, committed);
                    } else {
                        panic!(
                            "zkvm-test: snapshot mismatch for '{}': expected {} bytes, committed {} bytes. Run with `cargo zkvm-test --accept-snapshots` to update.",
                            resolved.display(), expected.len(), committed.len()
                        );
                    }
                }
            },
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
