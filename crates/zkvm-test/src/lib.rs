#![no_std]

extern crate alloc;

#[cfg(not(target_os = "zkvm"))]
extern crate std;

use alloc::{string::String, vec::Vec};

pub use zkvm_test_macros::{test, zkvm_test};

pub const MAGIC: &[u8; 4] = b"ZKVT";
pub const CMD_LIST: u8 = 0;
pub const CMD_RUN: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TestFlag {
    Ignore           = 1 << 0,
    ShouldPanic      = 1 << 1,
    ExpectedExitCode = 1 << 2,
    MaxCycles        = 1 << 3,
    Fixture          = 1 << 4,
    Snapshot         = 1 << 5,
    ExpectedMessage  = 1 << 6,
}

impl TestFlag {
    pub const fn mask(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy)]
pub struct TestCase {
    pub name:               &'static str,
    pub test_fn:            fn(),
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_message:   Option<&'static str>,
    pub expected_exit_code: Option<u32>,
    pub max_cycles:         Option<u64>,
    pub fixture:            Option<&'static str>,
    pub snapshot:           Option<&'static str>,
}

impl TestCase {
    pub const fn flags(&self) -> u8 {
        let mut flags = 0u8;
        if self.ignore {
            flags |= TestFlag::Ignore.mask();
        }
        if self.should_panic {
            flags |= TestFlag::ShouldPanic.mask();
        }
        if self.expected_message.is_some() {
            flags |= TestFlag::ExpectedMessage.mask();
        }
        if self.expected_exit_code.is_some() {
            flags |= TestFlag::ExpectedExitCode.mask();
        }
        if self.max_cycles.is_some() {
            flags |= TestFlag::MaxCycles.mask();
        }
        if self.fixture.is_some() {
            flags |= TestFlag::Fixture.mask();
        }
        if self.snapshot.is_some() {
            flags |= TestFlag::Snapshot.mask();
        }
        flags
    }

    /// Append this case to a `CMD_LIST` reply. Field order is the contract the
    /// runner's `TestDesc::decode` reads back; the two must change together.
    #[cfg(target_os = "zkvm")]
    pub fn encode(&self, buf: &mut Vec<u8>) {
        buf.push(self.flags());
        if let Some(code) = self.expected_exit_code {
            buf.extend_from_slice(&code.to_le_bytes());
        }
        if let Some(max) = self.max_cycles {
            buf.extend_from_slice(&max.to_le_bytes());
        }
        if let Some(msg) = self.expected_message {
            write_len_prefixed_str(buf, msg);
        }
        if let Some(fix) = self.fixture {
            write_len_prefixed_str(buf, fix);
        }
        if let Some(snap) = self.snapshot {
            write_len_prefixed_str(buf, snap);
        }
        write_len_prefixed_str(buf, self.name);
    }
}

#[cfg(target_os = "zkvm")]
fn write_len_prefixed_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

#[cfg(target_os = "zkvm")]
pub fn test_runner(tests: &[&TestCase]) {
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
            // The reply is tagged so the runner can tell a catalog it cannot decode
            // from an ELF that ignored the command entirely.
            let mut buf = Vec::new();
            buf.extend_from_slice(MAGIC);
            buf.extend_from_slice(&(tests.len() as u32).to_le_bytes());
            for t in tests {
                t.encode(&mut buf);
            }
            sp1_zkvm::io::commit_slice(&buf);
        },
        CMD_RUN => {
            let target_name = core::str::from_utf8(&input[5..]).unwrap_or("");
            match tests.iter().find(|t| t.name == target_name) {
                Some(t) => (t.test_fn)(),
                None => panic!("zkvm-test: test '{}' not found in test suite", target_name),
            }
        },
        cmd => panic!("zkvm-test: unknown command {}", cmd),
    }
}

#[cfg(not(target_os = "zkvm"))]
pub fn test_runner(_tests: &[&TestCase]) {}

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
pub fn fixture() -> Vec<u8> {
    #[cfg(target_os = "zkvm")]
    {
        sp1_zkvm::io::read_vec()
    }
    #[cfg(not(target_os = "zkvm"))]
    {
        let path = __internal::CURRENT_FIXTURE
            .with(|c| *c.borrow())
            .expect("zkvm-test: fixture() called outside of a test declaring #[test(fixture = \"...\")]");
        __internal::read_host_fixture(path)
    }
}

/// Retrieve the fixture injected for the current test as a UTF-8 string.
pub fn fixture_str() -> String {
    String::from_utf8(fixture()).expect("zkvm-test: fixture is not valid UTF-8")
}

/// Golden-file comparison shared by the host harness and `zkvm-test-runner`.
#[cfg(not(target_os = "zkvm"))]
pub mod snapshot {
    use alloc::{string::String, vec::Vec};
    use std::path::Path;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Format {
        Hex,
        Binary,
    }

    /// What `verify` did to the snapshot file on disk.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Outcome {
        Matched,
        Created,
        Updated,
    }

    impl Outcome {
        pub const fn as_str(&self) -> &'static str {
            match self {
                Self::Matched => "matched",
                Self::Created => "created",
                Self::Updated => "updated",
            }
        }
    }

    impl Format {
        pub fn from_path(path: &Path) -> Self {
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("hex")) {
                Self::Hex
            } else {
                Self::Binary
            }
        }

        pub fn encode(&self, committed: &[u8]) -> Vec<u8> {
            match self {
                Self::Hex => {
                    let mut s = to_hex(committed);
                    s.push('\n');
                    s.into_bytes()
                },
                Self::Binary => committed.to_vec(),
            }
        }
    }

    pub fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| std::format!("{b:02x}")).collect()
    }

    fn hex_dump(bytes: &[u8]) -> String {
        let mut out = String::new();
        for chunk in bytes.chunks(16) {
            for b in chunk {
                out.push_str(&std::format!("{b:02x} "));
            }
            out.push('\n');
        }
        out
    }

    fn write(path: &Path, data: &[u8], outcome: Outcome) -> Result<Outcome, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| std::format!("failed to create '{}': {e}", parent.display()))?;
        }
        std::fs::write(path, data).map_err(|e| std::format!("failed to write snapshot '{}': {e}", path.display()))?;
        Ok(outcome)
    }

    /// Compare `committed` against the snapshot at `path`, creating or rewriting it when
    /// `accept` is set. The error is the full user-facing diff.
    pub fn verify(path: &Path, committed: &[u8], accept: bool) -> Result<Outcome, String> {
        let format = Format::from_path(path);

        if !path.exists() {
            if !accept {
                return Err(std::format!(
                    "snapshot file '{}' does not exist. Run with '--accept-snapshots' (or ZKVM_ACCEPT_SNAPSHOTS=1 under `cargo test`) to create it.",
                    path.display()
                ));
            }
            return write(path, &format.encode(committed), Outcome::Created);
        }

        let stored =
            std::fs::read(path).map_err(|e| std::format!("failed to read snapshot '{}': {e}", path.display()))?;

        let matches = match format {
            Format::Hex => {
                let expected = std::str::from_utf8(&stored)
                    .map_err(|e| std::format!("snapshot '{}' is not valid UTF-8: {e}", path.display()))?;
                expected.trim().eq_ignore_ascii_case(&to_hex(committed))
            },
            Format::Binary => stored == committed,
        };
        if matches {
            return Ok(Outcome::Matched);
        }
        if accept {
            return write(path, &format.encode(committed), Outcome::Updated);
        }

        let (expected_len, expected_body, actual_body) = match format {
            Format::Hex => {
                let expected = std::str::from_utf8(&stored).unwrap_or_default().trim().to_ascii_lowercase();
                (expected.len() / 2, expected, to_hex(committed))
            },
            Format::Binary => (stored.len(), hex_dump(&stored), hex_dump(committed)),
        };
        Err(std::format!(
            "snapshot mismatch for '{}':\n  - expected ({expected_len} bytes):\n    {expected_body}\n  + actual ({} bytes):\n    {actual_body}\nTo accept these changes, run with '--accept-snapshots'.",
            path.display(),
            committed.len(),
        ))
    }
}

#[cfg(not(target_os = "zkvm"))]
#[doc(hidden)]
pub mod __internal {
    use alloc::vec::Vec;
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
        thread_local,
    };

    thread_local! {
        pub static CURRENT_FIXTURE: RefCell<Option<&'static str>> = const { RefCell::new(None) };
        pub static COMMITTED_BYTES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    pub fn record_host_commit(slice: &[u8]) {
        COMMITTED_BYTES.with(|b| b.borrow_mut().extend_from_slice(slice));
    }

    /// Run a host test body with its declared fixture in scope, verifying its committed
    /// public values against the declared snapshot afterwards. The macro emits one call
    /// with whichever of the two the test declared.
    pub fn with_host_context<R>(
        fixture: Option<&'static str>,
        snapshot: Option<&'static str>,
        f: impl FnOnce() -> R,
    ) -> R {
        CURRENT_FIXTURE.with(|c| *c.borrow_mut() = fixture);
        if snapshot.is_some() {
            COMMITTED_BYTES.with(|b| b.borrow_mut().clear());
        }

        let res = f();

        CURRENT_FIXTURE.with(|c| *c.borrow_mut() = None);
        if let Some(path) = snapshot {
            let committed = COMMITTED_BYTES.with(|b| b.borrow().clone());
            verify_host_snapshot(path, &committed);
        }
        res
    }

    pub fn read_host_fixture(path: &str) -> Vec<u8> {
        let resolved = resolve_host_path(path)
            .unwrap_or_else(|| panic!("zkvm-test: could not find fixture file '{path}' on host"));
        std::fs::read(&resolved)
            .unwrap_or_else(|e| panic!("zkvm-test: failed to read fixture '{}': {e}", resolved.display()))
    }

    fn find_in_manifest_ancestors(path: &Path, manifest_dir: &Path) -> Option<PathBuf> {
        std::iter::successors(Some(manifest_dir), |dir| dir.parent())
            .map(|dir| dir.join(path))
            .find(|candidate| candidate.exists())
    }

    pub fn resolve_host_path(path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
        find_in_manifest_ancestors(p, Path::new(&manifest_dir))
    }

    /// Like `resolve_host_path`, but yields the path a missing snapshot should be created at.
    fn resolve_host_write_path(path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else {
            return p.to_path_buf();
        };
        let manifest_dir = Path::new(&manifest_dir);
        find_in_manifest_ancestors(p, manifest_dir).unwrap_or_else(|| manifest_dir.join(path))
    }

    pub fn verify_host_snapshot(snapshot_path: &str, committed: &[u8]) {
        let resolved = resolve_host_write_path(snapshot_path);
        let accept = std::env::var("ZKVM_ACCEPT_SNAPSHOTS").is_ok() || std::env::var("ACCEPT_SNAPSHOTS").is_ok();
        if let Err(err) = crate::snapshot::verify(&resolved, committed, accept) {
            panic!("zkvm-test: {err}");
        }
    }
}
