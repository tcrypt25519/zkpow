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

#[derive(Clone, Copy)]
pub struct TestCase {
    pub name:               &'static str,
    pub test_fn:            fn(),
    pub ignore:             bool,
    pub should_panic:       bool,
    pub expected_exit_code: Option<u32>,
    pub max_cycles:         Option<u64>,
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
                buf.push(flags);
                if let Some(code) = t.expected_exit_code {
                    buf.extend_from_slice(&code.to_le_bytes());
                }
                if let Some(max) = t.max_cycles {
                    buf.extend_from_slice(&max.to_le_bytes());
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

#[cfg(not(target_os = "zkvm"))]
pub fn test_runner(_tests: &[&TestCase]) {}
