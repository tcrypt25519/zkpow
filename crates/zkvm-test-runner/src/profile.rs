use std::collections::BTreeMap;
use std::sync::Arc;

use object::{Object, ObjectSymbol, SymbolKind};
use rustc_demangle::demangle;
use sp1_core_executor::{ExecutionRecord, Program, SP1CoreOpts, TracingVMEnum};
use sp1_core_executor_runner::MinimalExecutorRunner;
use sp1_sdk::SP1Stdin;

use crate::style::Palette;

/// Represents a function symbol parsed from an ELF binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSymbol {
    pub name: String,
    pub start: u64,
    pub end: u64,
    pub size: u64,
}

/// A lookup table mapping RISC-V PC addresses to enclosing function symbols.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    pub functions: Vec<FunctionSymbol>,
}

impl SymbolTable {
    /// Parse function symbols from an ELF binary using the standard `object` crate.
    pub fn parse(elf_bytes: &[u8]) -> Result<Self, String> {
        let obj = object::File::parse(elf_bytes)
            .map_err(|e| format!("failed to parse ELF binary: {e}"))?;

        let mut functions = Vec::new();
        for sym in obj.symbols() {
            // Include symbols that are either explicitly marked as functions or in the text section
            if sym.kind() == SymbolKind::Text && sym.size() > 0 {
                let raw_name = sym.name().unwrap_or("");
                if raw_name.is_empty() {
                    continue;
                }
                let demangled = demangle(raw_name).to_string();
                let start = sym.address();
                let size = sym.size();
                let end = start.saturating_add(size);
                functions.push(FunctionSymbol {
                    name: demangled,
                    start,
                    end,
                    size,
                });
            }
        }

        // Sort by start address for binary search
        functions.sort_by_key(|f| f.start);
        Ok(Self { functions })
    }

    /// Fast $O(\log N)$ binary search lookup from PC address to enclosing function.
    pub fn lookup(&self, pc: u64) -> Option<&FunctionSymbol> {
        match self.functions.binary_search_by_key(&pc, |f| f.start) {
            Ok(idx) => Some(&self.functions[idx]),
            Err(0) => None,
            Err(idx) => {
                let candidate = &self.functions[idx - 1];
                if pc < candidate.end {
                    Some(candidate)
                } else {
                    None
                }
            }
        }
    }
}

/// Instruction category classification for high-resolution profiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionCategory {
    Alu,
    MulDiv,
    Memory,
    ControlFlow,
    System,
}

/// Detailed cycle metrics for a single function.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FunctionProfile {
    pub name: String,
    pub cycles: u64,
    pub invocations: u64,
    pub alu: u64,
    pub mul_div: u64,
    pub memory: u64,
    pub control_flow: u64,
    pub system: u64,
}

impl FunctionProfile {
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    /// Total arithmetic / mathematical cycles (ALU + MulDiv).
    pub fn math_cycles(&self) -> u64 {
        self.alu.saturating_add(self.mul_div)
    }

    /// Average cycles per invocation.
    pub fn avg_cycles_per_call(&self) -> f64 {
        if self.invocations == 0 {
            self.cycles as f64
        } else {
            self.cycles as f64 / self.invocations as f64
        }
    }
}

/// Aggregates and correlates trace events with the symbol table.
#[derive(Debug, Default, Clone)]
pub struct ProfileCollector {
    pub symbols: SymbolTable,
    pub profiles: BTreeMap<String, FunctionProfile>,
    pub unmapped_cycles: u64,
    last_symbol: Option<String>,
}

impl ProfileCollector {
    pub fn new(symbols: SymbolTable) -> Self {
        Self {
            symbols,
            profiles: BTreeMap::new(),
            unmapped_cycles: 0,
            last_symbol: None,
        }
    }

    /// Record a single instruction execution event at the given PC.
    pub fn record_pc(&mut self, pc: u64, category: InstructionCategory) {
        if let Some(func) = self.symbols.lookup(pc) {
            let func_name = func.name.clone();
            let is_new_entry = match &self.last_symbol {
                Some(prev) => prev != &func_name,
                None => true,
            };

            let entry = self
                .profiles
                .entry(func_name.clone())
                .or_insert_with(|| FunctionProfile::new(func_name.clone()));

            entry.cycles += 1;
            if is_new_entry {
                entry.invocations += 1;
            }

            match category {
                InstructionCategory::Alu => entry.alu += 1,
                InstructionCategory::MulDiv => entry.mul_div += 1,
                InstructionCategory::Memory => entry.memory += 1,
                InstructionCategory::ControlFlow => entry.control_flow += 1,
                InstructionCategory::System => entry.system += 1,
            }

            self.last_symbol = Some(func_name);
        } else {
            self.unmapped_cycles += 1;
            self.last_symbol = None;
        }
    }

    /// Ingest an entire `ExecutionRecord` produced by `TracingVMEnum`.
    pub fn process_execution_record(&mut self, record: &ExecutionRecord) {
        // ALU events
        for (event, _) in &record.alu_x0_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.add_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.addi_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.addw_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.sub_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.subw_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.bitwise_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.shift_left_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.shift_right_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.lt_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }
        for (event, _) in &record.utype_events {
            self.record_pc(event.pc, InstructionCategory::Alu);
        }

        // Mul / Div events
        for (event, _) in &record.mul_events {
            self.record_pc(event.pc, InstructionCategory::MulDiv);
        }
        for (event, _) in &record.divrem_events {
            self.record_pc(event.pc, InstructionCategory::MulDiv);
        }

        // Memory load events
        for (event, _) in &record.memory_load_byte_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_load_half_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_load_word_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_load_double_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_load_x0_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }

        // Memory store events
        for (event, _) in &record.memory_store_byte_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_store_half_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_store_word_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }
        for (event, _) in &record.memory_store_double_events {
            self.record_pc(event.pc, InstructionCategory::Memory);
        }

        // Control flow events
        for (event, _) in &record.branch_events {
            self.record_pc(event.pc, InstructionCategory::ControlFlow);
        }
        for (event, _) in &record.jal_events {
            self.record_pc(event.pc, InstructionCategory::ControlFlow);
        }
        for (event, _) in &record.jalr_events {
            self.record_pc(event.pc, InstructionCategory::ControlFlow);
        }
    }

    /// Total mapped cycles across all functions.
    pub fn total_mapped_cycles(&self) -> u64 {
        self.profiles.values().map(|p| p.cycles).sum()
    }

    /// Total cycles (mapped + unmapped).
    pub fn total_cycles(&self) -> u64 {
        self.total_mapped_cycles() + self.unmapped_cycles
    }

    /// Get sorted functions ordered descending by cycle count.
    pub fn top_by_cycles(&self, limit: usize) -> Vec<&FunctionProfile> {
        let mut list: Vec<&FunctionProfile> = self.profiles.values().collect();
        list.sort_by(|a, b| b.cycles.cmp(&a.cycles));
        if limit > 0 && list.len() > limit {
            list.truncate(limit);
        }
        list
    }

    /// Filter functions by substring (case-insensitive) ordered descending by cycles.
    pub fn filter_by_pattern(&self, pattern: &str, limit: usize) -> Vec<&FunctionProfile> {
        let pat_lower = pattern.to_lowercase();
        let mut list: Vec<&FunctionProfile> = self
            .profiles
            .values()
            .filter(|p| p.name.to_lowercase().contains(&pat_lower))
            .collect();
        list.sort_by(|a, b| b.cycles.cmp(&a.cycles));
        if limit > 0 && list.len() > limit {
            list.truncate(limit);
        }
        list
    }
}

/// Execute trace mining on a test binary using unmodified stock SP1 public runner APIs.
pub fn mine_trace_profile(
    elf_bytes: &[u8],
    stdin: &SP1Stdin,
    filter_pattern: Option<&str>,
) -> Result<Vec<FunctionProfile>, String> {
    let symbols = SymbolTable::parse(elf_bytes)?;
    let mut collector = ProfileCollector::new(symbols);

    let program = Arc::new(
        Program::from(elf_bytes)
            .map_err(|e| format!("failed to parse program from ELF: {e}"))?,
    );

    let opts = SP1CoreOpts::default();
    let mut runner = MinimalExecutorRunner::new(
        program.clone(),
        false,
        Some(opts.minimal_trace_chunk_threshold),
        opts.memory_limit,
        opts.trace_chunk_slots,
    );

    for buf in &stdin.buffer {
        runner.with_input(buf);
    }

    let proof_nonce = [0u32; 4];

    while let Some(chunk) = runner
        .try_execute_chunk()
        .map_err(|e| format!("executor failure: {e:?}"))?
    {
        let mut record = ExecutionRecord::default();
        let mut vm = TracingVMEnum::new(&chunk, program.clone(), opts.clone(), proof_nonce, &mut record);
        vm.execute()
            .map_err(|e| format!("trace replay failure: {e:?}"))?;

        collector.process_execution_record(&record);
    }

    let selected = match filter_pattern {
        Some(pat) => collector.filter_by_pattern(pat, 50),
        None => collector.top_by_cycles(30),
    };

    Ok(selected.into_iter().cloned().collect())
}

/// Strip crate and module disambiguator hashes like `[bb7aedaf0461c108]` while preserving
/// array type syntax like `[u8; 4]`.
pub fn strip_symbol_hashes(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut buf = String::new();
            let mut is_hex_disambiguator = false;
            while let Some(&next) = chars.peek() {
                if next == ']' {
                    chars.next(); // consume ']'
                    if buf.len() >= 8 && buf.chars().all(|ch| ch.is_ascii_hexdigit()) {
                        is_hex_disambiguator = true;
                    }
                    break;
                } else if next.is_ascii_hexdigit() {
                    buf.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if !is_hex_disambiguator {
                result.push('[');
                result.push_str(&buf);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Truncate long symbol names with middle ellipsis (`head...tail`), preserving both
/// the top-level namespace and the target function name.
pub fn truncate_symbol_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        return name.to_string();
    }
    if max_len < 8 {
        return name[..max_len].to_string();
    }
    let head_len = (max_len - 3) / 2;
    let tail_len = max_len - 3 - head_len;
    format!("{}...{}", &name[..head_len], &name[name.len() - tail_len..])
}

/// Render an aligned table of function profiles.
pub fn format_function_profile_table(
    functions: &[FunctionProfile],
    total_test_cycles: u64,
    palette: &Palette,
) -> String {
    if functions.is_empty() {
        return String::new();
    }

    let cleaned_names: Vec<String> = functions.iter().map(|f| strip_symbol_hashes(&f.name)).collect();
    let max_name_len = cleaned_names.iter().map(|n| n.len()).max().unwrap_or(40);
    let fn_col_width = max_name_len.clamp(40, 64);

    let cycles_col_width = functions
        .iter()
        .map(|f| crate::style::format_commas(f.cycles).len())
        .max()
        .unwrap_or(6)
        .max(6); // at least "Cycles".len() == 6

    let calls_col_width = functions
        .iter()
        .map(|f| crate::style::format_commas(f.invocations).len())
        .max()
        .unwrap_or(5)
        .max(5); // at least "Calls".len() == 5

    let math_col_width = functions
        .iter()
        .map(|f| crate::style::format_commas(f.math_cycles()).len())
        .max()
        .unwrap_or(4)
        .max(4); // at least "Math".len() == 4

    let mem_col_width = functions
        .iter()
        .map(|f| crate::style::format_commas(f.memory).len())
        .max()
        .unwrap_or(6)
        .max(6); // at least "Memory".len() == 6

    let pct_col_width = 6; // e.g. " 100.0%" or " % Tot"

    let mut out = String::new();
    let prefix = "        ";

    let total_width = fn_col_width
        + cycles_col_width
        + pct_col_width
        + calls_col_width
        + math_col_width
        + mem_col_width
        + 19;

    let sep_top = format!(
        "┌─ function cycle profile {bar}┐",
        bar = "─".repeat(total_width.saturating_sub(27))
    );

    let sep_mid = format!(
        "├{f_bar}┼{c_bar}┼{p_bar}┼{cl_bar}┼{m_bar}┼{mem_bar}┤",
        f_bar = "─".repeat(fn_col_width + 2),
        c_bar = "─".repeat(cycles_col_width + 2),
        p_bar = "─".repeat(pct_col_width + 2),
        cl_bar = "─".repeat(calls_col_width + 2),
        m_bar = "─".repeat(math_col_width + 2),
        mem_bar = "─".repeat(mem_col_width + 2),
    );

    let sep_bot = format!(
        "└{f_bar}┴{c_bar}┴{p_bar}┴{cl_bar}┴{m_bar}┴{mem_bar}┘",
        f_bar = "─".repeat(fn_col_width + 2),
        c_bar = "─".repeat(cycles_col_width + 2),
        p_bar = "─".repeat(pct_col_width + 2),
        cl_bar = "─".repeat(calls_col_width + 2),
        m_bar = "─".repeat(math_col_width + 2),
        mem_bar = "─".repeat(mem_col_width + 2),
    );

    out.push_str(&format!("{prefix}{}\n", palette.dim(&sep_top)));
    out.push_str(&format!(
        "{prefix}│ {:<fn_w$} │ {:>cyc_w$} │ {:>pct_w$} │ {:>call_w$} │ {:>math_w$} │ {:>mem_w$} │\n",
        palette.bold("Function"),
        palette.bold("Cycles"),
        palette.bold("% Tot"),
        palette.bold("Calls"),
        palette.bold("Math"),
        palette.bold("Memory"),
        fn_w = fn_col_width,
        cyc_w = cycles_col_width,
        pct_w = pct_col_width,
        call_w = calls_col_width,
        math_w = math_col_width,
        mem_w = mem_col_width,
    ));
    out.push_str(&format!("{prefix}{}\n", palette.dim(&sep_mid)));

    for (f, cleaned_name) in functions.iter().zip(cleaned_names.iter()) {
        let pct = if total_test_cycles > 0 {
            (f.cycles as f64 / total_test_cycles as f64) * 100.0
        } else {
            0.0
        };

        let display_name = truncate_symbol_name(cleaned_name, fn_col_width);

        let cycles_str = format!("{:>width$}", crate::style::format_commas(f.cycles), width = cycles_col_width);
        let pct_str = format!("{:>5.1}%", pct);
        let calls_str = format!("{:>width$}", crate::style::format_commas(f.invocations), width = calls_col_width);
        let math_str = format!("{:>width$}", crate::style::format_commas(f.math_cycles()), width = math_col_width);
        let mem_str = format!("{:>width$}", crate::style::format_commas(f.memory), width = mem_col_width);

        out.push_str(&format!(
            "{prefix}│ {:<fn_w$} │ {} │ {} │ {} │ {} │ {} │\n",
            display_name,
            palette.cyan(&cycles_str),
            palette.dim_cyan(&pct_str),
            palette.magenta(&calls_str),
            palette.yellow(&math_str),
            palette.bright_blue(&mem_str),
            fn_w = fn_col_width,
        ));
    }

    out.push_str(&format!("{prefix}{}\n", palette.dim(&sep_bot)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_table_lookup() {
        let mut table = SymbolTable::default();
        table.functions.push(FunctionSymbol {
            name: "foo::bar".to_string(),
            start: 0x1000,
            end: 0x1100,
            size: 0x100,
        });
        table.functions.push(FunctionSymbol {
            name: "baz::qux".to_string(),
            start: 0x1100,
            end: 0x1250,
            size: 0x150,
        });

        assert_eq!(table.lookup(0x1000).map(|s| s.name.as_str()), Some("foo::bar"));
        assert_eq!(table.lookup(0x10ff).map(|s| s.name.as_str()), Some("foo::bar"));
        assert_eq!(table.lookup(0x1100).map(|s| s.name.as_str()), Some("baz::qux"));
        assert_eq!(table.lookup(0x124f).map(|s| s.name.as_str()), Some("baz::qux"));
        assert_eq!(table.lookup(0x1250), None);
        assert_eq!(table.lookup(0x0fff), None);
    }

    #[test]
    fn test_profile_collector_counting() {
        let mut table = SymbolTable::default();
        table.functions.push(FunctionSymbol {
            name: "sha256::compress".to_string(),
            start: 0x2000,
            end: 0x2100,
            size: 0x100,
        });
        table.functions.push(FunctionSymbol {
            name: "state::advance".to_string(),
            start: 0x3000,
            end: 0x3100,
            size: 0x100,
        });

        let mut collector = ProfileCollector::new(table);
        // Call sha256::compress: 2 ALU, 1 Memory
        collector.record_pc(0x2000, InstructionCategory::Alu);
        collector.record_pc(0x2004, InstructionCategory::Alu);
        collector.record_pc(0x2008, InstructionCategory::Memory);

        // Call state::advance: 1 ControlFlow
        collector.record_pc(0x3000, InstructionCategory::ControlFlow);

        // Back to sha256::compress: 1 ALU
        collector.record_pc(0x200c, InstructionCategory::Alu);

        let sha = collector.profiles.get("sha256::compress").unwrap();
        assert_eq!(sha.cycles, 4);
        assert_eq!(sha.invocations, 2); // entered twice
        assert_eq!(sha.alu, 3);
        assert_eq!(sha.memory, 1);
        assert_eq!(sha.math_cycles(), 3);

        let state = collector.profiles.get("state::advance").unwrap();
        assert_eq!(state.cycles, 1);
        assert_eq!(state.invocations, 1);
        assert_eq!(state.control_flow, 1);

        assert_eq!(collector.total_mapped_cycles(), 5);
        assert_eq!(collector.top_by_cycles(1)[0].name, "sha256::compress");
    }

    #[test]
    fn test_format_function_profile_table() {
        let palette = Palette::no_color();
        let mut p1 = FunctionProfile::new("crate::hash::sha256".to_string());
        p1.cycles = 50000;
        p1.invocations = 1000;
        p1.alu = 40000;
        p1.memory = 10000;

        let mut p2 = FunctionProfile::new("crate::state::verify".to_string());
        p2.cycles = 124;
        p2.invocations = 2;
        p2.mul_div = 100;
        p2.memory = 24;

        let table = format_function_profile_table(&[p1, p2], 100000, &palette);
        assert!(table.contains("crate::hash::sha256"));
        assert!(table.contains("crate::state::verify"));
        assert!(table.contains("50.0%"));
        assert!(table.contains(" 0.1%"));
        assert!(table.contains("1,000")); // comma in calls!

        // All table rows (headers, separators, data rows) must have the EXACT same character length!
        let lines: Vec<&str> = table.lines().collect();
        assert!(lines.len() >= 5);
        let first_len = lines[0].chars().count();
        for line in &lines {
            assert_eq!(line.chars().count(), first_len, "table line misaligned: '{line}'");
        }
    }

    #[test]
    fn test_strip_symbol_hashes() {
        let raw = "sha2[bb7aedaf0461c108]::sha256::soft::sha256_digest_round_x2";
        assert_eq!(strip_symbol_hashes(raw), "sha2::sha256::soft::sha256_digest_round_x2");

        let array_type = "<[u8; 4] as core::convert::TryFrom<&[u8]>>::try_from";
        assert_eq!(strip_symbol_hashes(array_type), "<[u8; 4] as core::convert::TryFrom<&[u8]>>::try_from");
    }

    #[test]
    fn test_truncate_symbol_name() {
        let short = "sha2::compress";
        assert_eq!(truncate_symbol_name(short, 40), "sha2::compress");

        let long = "my_crate::very_long_module_name::nested_submodule::important_function";
        let truncated = truncate_symbol_name(long, 30);
        assert_eq!(truncated.len(), 30);
        assert!(truncated.starts_with("my_crate::ver"));
        assert!(truncated.ends_with("rtant_function"));
        assert!(truncated.contains("..."));
    }
}
