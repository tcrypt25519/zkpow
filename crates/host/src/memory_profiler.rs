//! Memory profiler helpers for RSS and optional jemalloc diagnostics.
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "memory-diagnostics")]
use std::env;

#[cfg(feature = "memory-diagnostics")]
use jemalloc_ctl::{epoch, stats};

#[derive(Clone, Copy, Debug, Default)]
pub struct MemorySnapshot {
    pub rss_kb: Option<u64>,
    #[cfg(feature = "memory-diagnostics")]
    pub jemalloc: Option<JemallocStatsSnapshot>,
}

#[cfg(feature = "memory-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub struct JemallocStatsSnapshot {
    pub allocated_bytes: u64,
    pub active_bytes: u64,
    pub metadata_bytes: u64,
    pub resident_bytes: u64,
    pub mapped_bytes: u64,
    pub retained_bytes: u64,
}

#[cfg(feature = "memory-diagnostics")]
impl JemallocStatsSnapshot {
    fn active_gap_bytes(self) -> u64 {
        self.active_bytes.saturating_sub(self.allocated_bytes)
    }

    fn resident_gap_bytes(self) -> u64 {
        self.resident_bytes.saturating_sub(self.active_bytes)
    }

    fn mapped_gap_bytes(self) -> u64 {
        self.mapped_bytes.saturating_sub(self.active_bytes)
    }
}

fn rss_line() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let status_path = format!("/proc/{}/status", std::process::id());
        let status = std::fs::read_to_string(status_path).ok()?;
        return Some(
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .unwrap_or("VmRSS: ?")
                .to_owned(),
        );
    }

    #[cfg(not(target_os = "linux"))]
    {
        let output = std::process::Command::new("ps")
            .arg("-p")
            .arg(std::process::id().to_string())
            .arg("-o")
            .arg("rss=")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let rss = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Some(format!("VmRSS: {} kB", rss));
    }
}

pub fn current_rss_kb() -> Option<u64> {
    let line = rss_line()?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(feature = "memory-diagnostics")]
fn current_jemalloc_stats() -> Option<JemallocStatsSnapshot> {
    epoch::advance().ok()?;
    Some(JemallocStatsSnapshot {
        allocated_bytes: stats::allocated::read().ok()? as u64,
        active_bytes: stats::active::read().ok()? as u64,
        metadata_bytes: stats::metadata::read().ok()? as u64,
        resident_bytes: stats::resident::read().ok()? as u64,
        mapped_bytes: stats::mapped::read().ok()? as u64,
        retained_bytes: stats::retained::read().ok()? as u64,
    })
}

pub fn capture_snapshot() -> MemorySnapshot {
    MemorySnapshot {
        rss_kb: current_rss_kb(),
        #[cfg(feature = "memory-diagnostics")]
        jemalloc: current_jemalloc_stats(),
    }
}

fn delta_i64(end: u64, start: u64) -> i64 {
    end as i64 - start as i64
}

pub fn log_snapshot(label: &'static str, snapshot: &MemorySnapshot, message: &'static str) {
    #[cfg(feature = "memory-diagnostics")]
    if let Some(jemalloc) = snapshot.jemalloc {
        if let Some(rss_kb) = snapshot.rss_kb {
            tracing::info!(
                label,
                rss_kb,
                jemalloc_allocated_bytes = jemalloc.allocated_bytes,
                jemalloc_active_bytes = jemalloc.active_bytes,
                jemalloc_metadata_bytes = jemalloc.metadata_bytes,
                jemalloc_resident_bytes = jemalloc.resident_bytes,
                jemalloc_mapped_bytes = jemalloc.mapped_bytes,
                jemalloc_retained_bytes = jemalloc.retained_bytes,
                jemalloc_active_gap_bytes = jemalloc.active_gap_bytes(),
                jemalloc_resident_gap_bytes = jemalloc.resident_gap_bytes(),
                jemalloc_mapped_gap_bytes = jemalloc.mapped_gap_bytes(),
                "{message}"
            );
        } else {
            tracing::info!(
                label,
                jemalloc_allocated_bytes = jemalloc.allocated_bytes,
                jemalloc_active_bytes = jemalloc.active_bytes,
                jemalloc_metadata_bytes = jemalloc.metadata_bytes,
                jemalloc_resident_bytes = jemalloc.resident_bytes,
                jemalloc_mapped_bytes = jemalloc.mapped_bytes,
                jemalloc_retained_bytes = jemalloc.retained_bytes,
                jemalloc_active_gap_bytes = jemalloc.active_gap_bytes(),
                jemalloc_resident_gap_bytes = jemalloc.resident_gap_bytes(),
                jemalloc_mapped_gap_bytes = jemalloc.mapped_gap_bytes(),
                "{message}"
            );
        }
        return;
    }

    if let Some(rss_kb) = snapshot.rss_kb {
        tracing::info!(label, rss_kb, "{message}");
    } else {
        tracing::info!(label, "{message}");
    }
}

pub fn log_snapshot_delta(
    label: &'static str,
    started: &MemorySnapshot,
    finished: &MemorySnapshot,
    elapsed: Duration,
    message: &'static str,
) {
    let elapsed_us = elapsed.as_micros() as u64;

    #[cfg(feature = "memory-diagnostics")]
    if let (Some(start_jemalloc), Some(end_jemalloc)) = (started.jemalloc, finished.jemalloc) {
        if let (Some(start_rss_kb), Some(end_rss_kb)) = (started.rss_kb, finished.rss_kb) {
            tracing::info!(
                label,
                elapsed_us,
                start_rss_kb,
                end_rss_kb,
                rss_delta_kb = delta_i64(end_rss_kb, start_rss_kb),
                start_jemalloc_allocated_bytes = start_jemalloc.allocated_bytes,
                end_jemalloc_allocated_bytes = end_jemalloc.allocated_bytes,
                jemalloc_allocated_delta_bytes =
                    delta_i64(end_jemalloc.allocated_bytes, start_jemalloc.allocated_bytes),
                start_jemalloc_active_bytes = start_jemalloc.active_bytes,
                end_jemalloc_active_bytes = end_jemalloc.active_bytes,
                jemalloc_active_delta_bytes =
                    delta_i64(end_jemalloc.active_bytes, start_jemalloc.active_bytes),
                start_jemalloc_metadata_bytes = start_jemalloc.metadata_bytes,
                end_jemalloc_metadata_bytes = end_jemalloc.metadata_bytes,
                jemalloc_metadata_delta_bytes =
                    delta_i64(end_jemalloc.metadata_bytes, start_jemalloc.metadata_bytes),
                start_jemalloc_resident_bytes = start_jemalloc.resident_bytes,
                end_jemalloc_resident_bytes = end_jemalloc.resident_bytes,
                jemalloc_resident_delta_bytes =
                    delta_i64(end_jemalloc.resident_bytes, start_jemalloc.resident_bytes),
                start_jemalloc_mapped_bytes = start_jemalloc.mapped_bytes,
                end_jemalloc_mapped_bytes = end_jemalloc.mapped_bytes,
                jemalloc_mapped_delta_bytes =
                    delta_i64(end_jemalloc.mapped_bytes, start_jemalloc.mapped_bytes),
                start_jemalloc_retained_bytes = start_jemalloc.retained_bytes,
                end_jemalloc_retained_bytes = end_jemalloc.retained_bytes,
                jemalloc_retained_delta_bytes =
                    delta_i64(end_jemalloc.retained_bytes, start_jemalloc.retained_bytes),
                end_jemalloc_active_gap_bytes = end_jemalloc.active_gap_bytes(),
                end_jemalloc_resident_gap_bytes = end_jemalloc.resident_gap_bytes(),
                end_jemalloc_mapped_gap_bytes = end_jemalloc.mapped_gap_bytes(),
                "{message}"
            );
        } else {
            tracing::info!(
                label,
                elapsed_us,
                start_jemalloc_allocated_bytes = start_jemalloc.allocated_bytes,
                end_jemalloc_allocated_bytes = end_jemalloc.allocated_bytes,
                jemalloc_allocated_delta_bytes =
                    delta_i64(end_jemalloc.allocated_bytes, start_jemalloc.allocated_bytes),
                start_jemalloc_active_bytes = start_jemalloc.active_bytes,
                end_jemalloc_active_bytes = end_jemalloc.active_bytes,
                jemalloc_active_delta_bytes =
                    delta_i64(end_jemalloc.active_bytes, start_jemalloc.active_bytes),
                start_jemalloc_metadata_bytes = start_jemalloc.metadata_bytes,
                end_jemalloc_metadata_bytes = end_jemalloc.metadata_bytes,
                jemalloc_metadata_delta_bytes =
                    delta_i64(end_jemalloc.metadata_bytes, start_jemalloc.metadata_bytes),
                start_jemalloc_resident_bytes = start_jemalloc.resident_bytes,
                end_jemalloc_resident_bytes = end_jemalloc.resident_bytes,
                jemalloc_resident_delta_bytes =
                    delta_i64(end_jemalloc.resident_bytes, start_jemalloc.resident_bytes),
                start_jemalloc_mapped_bytes = start_jemalloc.mapped_bytes,
                end_jemalloc_mapped_bytes = end_jemalloc.mapped_bytes,
                jemalloc_mapped_delta_bytes =
                    delta_i64(end_jemalloc.mapped_bytes, start_jemalloc.mapped_bytes),
                start_jemalloc_retained_bytes = start_jemalloc.retained_bytes,
                end_jemalloc_retained_bytes = end_jemalloc.retained_bytes,
                jemalloc_retained_delta_bytes =
                    delta_i64(end_jemalloc.retained_bytes, start_jemalloc.retained_bytes),
                end_jemalloc_active_gap_bytes = end_jemalloc.active_gap_bytes(),
                end_jemalloc_resident_gap_bytes = end_jemalloc.resident_gap_bytes(),
                end_jemalloc_mapped_gap_bytes = end_jemalloc.mapped_gap_bytes(),
                "{message}"
            );
        }
        return;
    }

    if let (Some(start_rss_kb), Some(end_rss_kb)) = (started.rss_kb, finished.rss_kb) {
        tracing::info!(
            label,
            elapsed_us,
            start_rss_kb,
            end_rss_kb,
            rss_delta_kb = delta_i64(end_rss_kb, start_rss_kb),
            "{message}"
        );
    } else {
        tracing::info!(label, elapsed_us, "{message}");
    }
}

#[cfg(feature = "memory-diagnostics")]
fn memory_diagnostics_dump_enabled() -> bool {
    env::var("MEMORY_DIAGNOSTICS_DUMP").as_deref() == Ok("1")
}

#[cfg(feature = "memory-diagnostics")]
pub fn maybe_dump_allocator_stats(path: &Path) {
    if !memory_diagnostics_dump_enabled() {
        return;
    }

    if let Err(err) = dump_allocator_stats(path) {
        tracing::warn!(
            path = %path.display(),
            error = %err,
            "failed to write jemalloc stats dump"
        );
    } else {
        tracing::info!(
            path = %path.display(),
            "wrote jemalloc stats dump"
        );
    }
}

#[cfg(not(feature = "memory-diagnostics"))]
pub fn maybe_dump_allocator_stats(_path: &Path) {}

#[cfg(feature = "memory-diagnostics")]
fn dump_allocator_stats(path: &Path) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let mut options = jemalloc_ctl::stats_print::Options::default();
    options.json_format = true;
    options.skip_constants = true;
    options.skip_bin_size_classes = true;
    options.skip_large_size_classes = true;
    options.skip_mutex_statistics = true;
    jemalloc_ctl::stats_print::stats_print(&mut file, options)?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}

fn snapshot_line(snapshot: &MemorySnapshot) -> String {
    let mut fields = Vec::new();

    if let Some(rss_kb) = snapshot.rss_kb {
        fields.push(format!("rss_kb={rss_kb}"));
    }

    #[cfg(feature = "memory-diagnostics")]
    if let Some(jemalloc) = snapshot.jemalloc {
        fields.push(format!("jemalloc_allocated_bytes={}", jemalloc.allocated_bytes));
        fields.push(format!("jemalloc_active_bytes={}", jemalloc.active_bytes));
        fields.push(format!("jemalloc_metadata_bytes={}", jemalloc.metadata_bytes));
        fields.push(format!("jemalloc_resident_bytes={}", jemalloc.resident_bytes));
        fields.push(format!("jemalloc_mapped_bytes={}", jemalloc.mapped_bytes));
        fields.push(format!("jemalloc_retained_bytes={}", jemalloc.retained_bytes));
        fields.push(format!(
            "jemalloc_active_gap_bytes={}",
            jemalloc.active_gap_bytes()
        ));
        fields.push(format!(
            "jemalloc_resident_gap_bytes={}",
            jemalloc.resident_gap_bytes()
        ));
        fields.push(format!(
            "jemalloc_mapped_gap_bytes={}",
            jemalloc.mapped_gap_bytes()
        ));
    }

    if fields.is_empty() {
        "memory_snapshot=unavailable".to_string()
    } else {
        fields.join(" ")
    }
}

/// Best-effort attempt to force the allocator to return unused pages to the OS.
///
/// - With jemalloc (memory-diagnostics feature): flushes thread caches and purges
///   all arenas. This only works on targets where jemalloc is the active allocator.
/// - Without jemalloc: currently a no-op.
pub fn maybe_purge_allocator() {
    #[cfg(feature = "memory-diagnostics")]
    {
        use jemalloc_sys::mallctl;
        use libc::{c_char, c_int, c_void, size_t};
        use std::ptr;

        unsafe {
            // Flush thread-local caches first.
            let _ = mallctl(
                b"thread.tcache.flush\0" as *const _ as *const c_char,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            );

            // Read number of arenas.
            let mut narenas: c_int = 0;
            let mut sz = std::mem::size_of::<c_int>() as size_t;
            let rc = mallctl(
                b"arenas.narenas\0" as *const _ as *const c_char,
                &mut narenas as *mut _ as *mut c_void,
                &mut sz,
                ptr::null_mut(),
                0,
            );
            if rc == 0 {
                for i in 0..narenas {
                    let name = format!("arena.{}.purge\0", i);
                    let _ = mallctl(
                        name.as_ptr() as *const c_char,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        0,
                    );
                }
            }
        }

        // Epoch advance so subsequent stat reads reflect the purge.
        let _ = jemalloc_ctl::epoch::advance();
    }
}

/// Spawn a background thread that writes the process's memory usage (VmRSS) to `path`
/// every `interval` seconds. The thread runs until the process exits.
pub fn spawn_mem_logger(path: PathBuf, interval: Duration) {
    let path_str = path.clone();
    thread::spawn(move || {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path_str)
            .expect("Unable to open memory log file");
        loop {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let snapshot = capture_snapshot();
            let line = format!("{} {}", ts, snapshot_line(&snapshot));
            writeln!(file, "{}", line).ok();
            file.flush().ok();
            thread::sleep(interval);
        }
    });
}
