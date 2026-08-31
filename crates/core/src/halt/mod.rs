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
