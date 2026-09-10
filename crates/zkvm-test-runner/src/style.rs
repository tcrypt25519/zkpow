//! Terminal styling, color management, and formatting utilities for `zkvm-test-runner`.

use std::io::IsTerminal;

/// Determine whether color escape codes should be emitted.
///
/// Follows standard CLI color conventions:
/// - `NO_COLOR` set (non-empty) -> false
/// - `CLICOLOR=0` -> false
/// - `CLICOLOR_FORCE!=0` -> true
/// - otherwise -> `stdout().is_terminal()`
pub fn colors_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Ok(clicolor) = std::env::var("CLICOLOR") {
        if clicolor == "0" {
            return false;
        }
    }
    if let Ok(force) = std::env::var("CLICOLOR_FORCE") {
        if force != "0" {
            return true;
        }
    }
    std::io::stdout().is_terminal()
}

/// Format an unsigned integer with commas as thousands separators (e.g. `1,234,567`).
pub fn format_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let offset = s.len() % 3;
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 3 == offset {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Format byte count into human-readable units (B, KB, MB, GB).
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

/// Palette holding color configuration and ANSI styling helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub enabled: bool,
}

impl Default for Palette {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Palette {
    pub fn from_env() -> Self {
        Self {
            enabled: colors_enabled(),
        }
    }

    pub const fn no_color() -> Self {
        Self { enabled: false }
    }

    pub const fn with_color() -> Self {
        Self { enabled: true }
    }

    pub fn bold(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[1m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn dim(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[2m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn green(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[32m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold_green(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[1;32m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn red(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold_red(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[1;31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn yellow(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[33m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold_yellow(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[1;33m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn cyan(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[36m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold_cyan(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[1;36m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn dim_cyan(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[2;36m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn magenta(&self, s: &str) -> String {
        if self.enabled {
            format!("\x1b[35m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    /// Horizontal divider line (`────────────`).
    pub fn divider(&self) -> String {
        self.dim("────────────")
    }

    /// Wide horizontal divider line for tables.
    pub fn wide_divider(&self, width: usize) -> String {
        self.dim(&"─".repeat(width))
    }

    /// Nextest-grade status tag with 8-space indentation:
    /// `        PASS ` (bold green)
    pub fn status_pass(&self) -> String {
        format!("        {} ", self.bold_green("PASS"))
    }

    /// `        FAIL ` (bold red)
    pub fn status_fail(&self) -> String {
        format!("        {} ", self.bold_red("FAIL"))
    }

    /// `        SKIP ` (bold yellow)
    pub fn status_skip(&self) -> String {
        format!("        {} ", self.bold_yellow("SKIP"))
    }

    /// Summary prefix: `     Summary ` (bold white)
    pub fn summary_prefix(&self, success: bool) -> String {
        if success {
            format!("     {} ", self.bold("Summary"))
        } else {
            format!("     {} ", self.bold_red("Summary"))
        }
    }

    /// Format an aligned metric pill:
    /// `[ cycles │ gas │ trace │ duration ]`
    pub fn metric_pill(
        &self,
        cycles: u64,
        gas: Option<u64>,
        record_bytes: u64,
        duration_secs: f64,
    ) -> String {
        let open = self.dim("[");
        let close = self.dim("]");
        let sep = self.dim("│");

        let cyc_val = format_commas(cycles);
        let cyc_unit = self.dim("cyc");
        let cyc_str = format!("{:>10} {}", self.cyan(&cyc_val), cyc_unit);

        let gas_part = if let Some(g) = gas {
            let gas_val = format_commas(g);
            let gas_unit = self.dim("gas");
            format!(" {sep} {:>10} {}", self.cyan(&gas_val), gas_unit)
        } else {
            String::new()
        };

        let trace_val = format_bytes(record_bytes);
        let trace_str = format!("{:>10}", self.magenta(&trace_val));

        let dur_str = format!("{:>6.2}s", duration_secs);

        format!("{open} {cyc_str}{gas_part} {sep} {trace_str} {sep} {dur_str} {close}")
    }

    /// Format a cycle budget indicator:
    /// `[budget: 1,200,000, 69.6%]` (green if <= 100%, red if > 100%)
    pub fn budget_badge(&self, actual: u64, limit: u64) -> String {
        let pct = (actual as f64 / limit as f64) * 100.0;
        let limit_str = format_commas(limit);
        let badge_text = format!("[budget: {limit_str}, {pct:.1}%]");
        if pct <= 100.0 {
            self.green(&badge_text)
        } else {
            self.bold_red(&badge_text)
        }
    }
}
