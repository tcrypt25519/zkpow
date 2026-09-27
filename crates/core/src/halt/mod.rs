//! Halting on a bad prover hint.
//!
//! Inside the zkVM this is exit code 3, which separates hint-validation failures from ordinary
//! panics (exit code 1) so a malicious prover cannot pass off wrong hints as a panicking program.
//! There is no syscall off-target, so the host build prints the same diagnostic and panics.

#[cfg(target_os = "zkvm")]
mod zkvm;
#[cfg(target_os = "zkvm")]
pub use zkvm::{__invalid_hint_fmt, halt_invalid_hint};

#[cfg(not(target_os = "zkvm"))]
mod platform;
#[cfg(not(target_os = "zkvm"))]
pub use platform::{__invalid_hint_fmt, halt_invalid_hint};

/// Halt with the invalid-hint exit code, optionally writing a formatted diagnostic first.
///
/// Wraps the SP1 syscalls `sp1_lib::halt_invalid_hint` and `sp1_lib::__invalid_hint_fmt`,
/// covering two axes:
///
///  - bare halts vs. halts with a diagnostic message
///  - on-guest (the syscall) vs. on-host (prints the same diagnostic and panics)
///
/// Format-arg syntax matches [`panic!`].
///
/// ```ignore
/// zkpow_core::invalid_hint!();
/// zkpow_core::invalid_hint!("median hint missing");
/// zkpow_core::invalid_hint!("expected {} bytes, got {}", expected, got);
/// ```
#[macro_export]
macro_rules! invalid_hint {
    () => {{
        $crate::halt::halt_invalid_hint()
    }};
    ($($arg:tt)*) => {{
        $crate::halt::__invalid_hint_fmt(::core::format_args!($($arg)*))
    }};
}

/// [`assert!`] with invalid-hint halt semantics.
///
/// ```ignore
/// zkpow_core::assert_hint!(value == expected_value);
/// ```
#[macro_export]
macro_rules! assert_hint {
    ($cond:expr $(,)?) => {
        if !$cond {
            $crate::invalid_hint!()
        }
    };
    ($cond:expr, $($arg:tt)+) => {
        if !$cond {
            $crate::invalid_hint!($($arg)+)
        }
    };
}

/// [`assert_eq!`] with invalid-hint halt semantics.
///
/// ```ignore
/// zkpow_core::assert_hint_eq!(value, expected_value);
/// ```
#[macro_export]
macro_rules! assert_hint_eq {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    $crate::invalid_hint!(
                        "assertion `left == right` failed: left: `{:?}`, right: `{:?}`",
                        left_val,
                        right_val
                    )
                }
            }
        }
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    $crate::invalid_hint!($($arg)+)
                }
            }
        }
    };
}

/// [`assert_ne!`] with invalid-hint halt semantics.
///
/// ```ignore
/// zkpow_core::assert_hint_ne!(value, expected_value);
/// ```
#[macro_export]
macro_rules! assert_hint_ne {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    $crate::invalid_hint!(
                        "assertion `left != right` failed: left: `{:?}`, right: `{:?}`",
                        left_val,
                        right_val
                    )
                }
            }
        }
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    $crate::invalid_hint!($($arg)+)
                }
            }
        }
    };
}
