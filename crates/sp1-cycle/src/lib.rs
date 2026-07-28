mod r#macro;

pub use sp1_cycle_macros::tracked;

#[cfg(target_os = "zkvm")]
extern crate alloc;
#[cfg(target_os = "zkvm")]
use sp1_zkvm;

#[cfg(target_os = "zkvm")]
#[inline(always)]
#[doc(hidden)]
pub fn run_track<T, F>(label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    sp1_zkvm::io::write(
        1,
        alloc::format!("cycle-tracker-report-start: {label}\n").as_bytes(),
    );
    let output = f();
    sp1_zkvm::io::write(
        1,
        alloc::format!("cycle-tracker-report-end: {label}\n").as_bytes(),
    );
    output
}

#[cfg(not(target_os = "zkvm"))]
#[inline(always)]
#[doc(hidden)]
pub fn run_track<T, F>(_label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    f()
}
