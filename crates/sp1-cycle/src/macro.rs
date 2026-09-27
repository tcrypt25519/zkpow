/// Cycle-track an expression without providing a string label.
///
/// The generated label is `module_path:line`, which is unique and maps directly
/// back to the call site. This removes the verbosity of manually naming every
/// small span while still feeding the `cycle-tracker-report-*` data used by the
/// host execution report.
#[cfg(all(target_os = "zkvm", feature = "cycle-tracking"))]
#[macro_export]
macro_rules! track {
    ($body:expr) => {
        $crate::run_track(concat!(module_path!(), ":", line!()), || $body)
    };
    ($label:expr, $body:expr) => {
        $crate::run_track($label, || $body)
    };
}

#[cfg(not(all(target_os = "zkvm", feature = "cycle-tracking")))]
#[macro_export]
macro_rules! track {
    ($body:expr) => {
        $body
    };
    ($label:expr, $body:expr) => {
        $body
    };
}
