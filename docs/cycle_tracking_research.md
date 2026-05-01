# SP1 Cycle Tracking Subsystem — Implementation-Level Research

**Date**: 2026-04-26  
**Repository**: `succinctlabs/sp1` (SB1 RISC-V zkVM)  
**Scope**: Full cycle tracking subsystem — all variants, end-to-end flow, semantics, gaps

---

## Part 1: File Map

All files involved in cycle tracking, grouped by role.

### Core Execution Layer

| File | Role |
|------|------|
| `crates/core/executor/src/report.rs` | Defines `ExecutionReport` struct with `cycle_tracker: HashMap<String, u64>` and `invocation_tracker: HashMap<String, u64>`. Implements `AddAssign` merging (`or_insert(0) += count`). |
| `crates/core/executor/src/minimal/write.rs` | **Parser entry point**: `handle_output()` processes stdout (fd=1) lines, detects `cycle-tracker-start:` / `cycle-tracker-end:` / `cycle-tracker-report-start:` / `cycle-tracker-report-end:` prefixes via `parse_command()`. Calls `ctx.cycle_tracker_start()`, `ctx.cycle_tracker_end()`, `ctx.cycle_tracker_report_end()`. |
| `crates/core/executor/src/minimal/arch/portable/mod.rs` | `MinimalExecutor` owns the tracking state: `cycle_tracker_starts: HashMap<String, (u64, u32)>`, `cycle_tracker_totals: HashMap<String, u64>`, `invocation_tracker: HashMap<String, u64>`. Contains getter methods `cycle_tracker_totals()`, `invocation_tracker()`, `take_cycle_tracker_totals()`, `take_invocation_tracker()`. |
| `crates/core/executor/src/vm/gas.rs` | `ReportGenerator` — separate gas estimation system. NOT part of cycle tracking (different concern: estimates STARK trace area/complexity for cost modeling). |
| `crates/core/executor/src/profiler.rs` | Gecko profiler (`Profiler` struct) — statistical sampling profiler (instruction-level, not cycle-tracker). Separate from cycle tracking. |
| `crates/core/executor/src/lib.rs` | Module structure; re-exports `CycleResult`, `get_complexity_mapping`. Feature-gates `mod profiler` behind `#[cfg(feature = "profiling")]`. |

### VM Abstraction Layer

| File | Role |
|------|------|
| `crates/core/jit/src/context.rs` | Defines `SyscallContext` trait with `cycle_tracker_start()`, `cycle_tracker_end()`, `cycle_tracker_report_end()` methods (all `#[cfg(feature = "profiling")]`). `JitContext` implements them as no-ops (JIT mode skips profiling). |
| `crates/core/runner/src/portable.rs` | `MinimalExecutorRunner` wraps `MinimalExecutor`. Forwards `take_cycle_tracker_totals()` and `take_invocation_tracker()` (both `#[cfg(feature = "profiling")]`). |

### Derive Macros

| File | Role |
|------|------|
| `crates/derive/src/lib.rs` | Defines two proc macros: `#[cycle_tracker]` (wraps function body in `eprintln!("cycle-tracker-start: fn_name")` / `eprintln!("cycle-tracker-end: fn_name")`) and `#[cycle_tracker_recursion]` (inserts `CircuitV2Builder::cycle_tracker_v2_enter/exit` calls). |

### Prover / Orchestration Layer

| File | Role |
|------|------|
| `crates/prover/src/worker/prover/execute.rs` | **Orchestrator**: Spawns `MinimalExecutorRunner` in a blocking task, then extracts `take_cycle_tracker_totals()` / `take_invocation_tracker()` after execution. Merges into final `ExecutionReport.cycle_tracker` / `.invocation_tracker`. |
| `crates/sdk/src/prover/execute.rs` | `ExecuteRequest` — SDK's execution API. Returns `(SP1PublicValues, ExecutionReport)`. No direct cycle tracking logic; the `ExecutionReport` is populated by the prover layer. |
| `crates/sdk/src/prover/prove.rs` | `BaseProveRequest` — proof generation options (cycle_limit, gas, etc). No cycle tracking logic. |

### Recursion Circuit Tracker (Separate System)

| File | Role |
|------|------|
| `crates/recursion/compiler/src/circuit/builder.rs` | `CircuitV2Builder` trait with `cycle_tracker_v2_enter(name)` / `cycle_tracker_v2_exit()` pushing `DslIr::CycleTrackerV2Enter` / `DslIr::CycleTrackerV2Exit` ops. |
| `crates/recursion/compiler/src/circuit/compiler.rs` | `CompileOneErr::CycleTrackerEnter` / `CycleTrackerExit` enum variants. Under `#[cfg(feature = "debug")]`, drives a `SpanBuilder` tree for circuit instruction profiling. Outputs span tree via `cycle_tracker_root_span.lines()`. |
| `crates/recursion/compiler/src/ir/instructions.rs` | References to cycle tracker in IR instructions (DslIr variants). |

### Examples & Test Artifacts

| File | Role |
|------|------|
| `examples/cycle-tracking/program/bin/normal.rs` | Non-report variant example — uses `println!("cycle-tracker-start: name")` and `#[cycle_tracker]` derive macro. |
| `examples/cycle-tracking/program/bin/report.rs` | Report variant example — uses `println!("cycle-tracker-report-start: name")`. |
| `examples/cycle-tracking/script/src/main.rs` | Host script that executes both normal and report programs, reads `report.cycle_tracker` for output. |
| `crates/test-artifacts/programs/cycle-tracker/src/main.rs` | Test program: `f()` (derive macro, non-report), `g()` (manual non-report with nesting g/g2), `h()` (report variant), `repeated()` (report variant called 3×). Used by SDK integration tests. |
| `crates/sdk/src/lib.rs` (tests) | Integration tests: `test_cycle_tracker_report_variants`, `test_cycle_tracker_macro_non_report`, `test_cycle_tracker_across_chunks` — validate semantics. |

### Feature Gate Chain

| File | Line(s) |
|------|---------|
| `crates/core/executor/Cargo.toml` | `profiling = ["sp1-jit/profiling"]` |
| `crates/core/runner/Cargo.toml` | `profiling = ["sp1-core-executor/profiling"]` |
| `crates/sdk/Cargo.toml` | `profiling = ["sp1-core-executor/profiling", "sp1-core-executor-runner/profiling", "sp1-prover/profiling"]` |

---

## Part 2: Architecture Walkthrough

### Step-by-Step End-to-End Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    GUEST PROGRAM (RISC-V)                        │
│                                                                  │
│  println!("cycle-tracker-start: setup");  ← marker via stdout   │
│  // ... work ...                                                 │
│  println!("cycle-tracker-end: setup");                          │
│                                                                  │
│  OR: #[sp1_derive::cycle_tracker]                                │
│      fn my_func() { ... }                                        │
│  → expands to: eprintln!("cycle-tracker-start: my_func");       │
│                let result = (|| { ... })();                     │
│                eprintln!("cycle-tracker-end: my_func");         │
│                result                                            │
│                                                                  │
│  OR: println!("cycle-tracker-report-start: foo");               │
│      println!("cycle-tracker-report-end: foo");                 │
└────────────────────┬────────────────────────────────────────────┘
                     │ WRITE syscall (fd=1 or fd=2)
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│        minimal/write.rs :: write() → handle_output()            │
│                                                                  │
│  1. Reads bytes from write buffer                                │
│  2. Converts to UTF-8 string                                     │
│  3. If fd == 1 (stdout):                                         │
│     For each line:                                                │
│       parse_command(line) → Option<(&str, &str)>                │
│         "cycle-tracker-start: name"    → ("start", "name")      │
│         "cycle-tracker-end: name"      → ("end", "name")        │
│         "cycle-tracker-report-start:*" → ("report-start", "*")  │
│         "cycle-tracker-report-end:*"   → ("report-end", "*")    │
│                                                                  │
│     match cmd:                                                    │
│       "start" | "report-start" → ctx.cycle_tracker_start(name) │
│       "end"                    → ctx.cycle_tracker_end(name)    │
│       "report-end"             → ctx.cycle_tracker_report_end() │
│                                                                  │
│  4. If fd == 2 (stderr): just prints (for #[cycle_tracker])      │
└────────────────────┬────────────────────────────────────────────┘
                     │ SyscallContext trait methods
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│   MinimalExecutor :: cycle_tracker_start / end / report_end     │
│   (portable/mod.rs — #[cfg(feature = "profiling")])             │
│                                                                  │
│   State:                                                         │
│     cycle_tracker_starts: HashMap<String, (u64, u32)>           │
│       key = label name                                           │
│       value = (global_clk at start, nesting depth)               │
│                                                                  │
│     cycle_tracker_totals: HashMap<String, u64>                   │
│       key = label name                                           │
│       value = cumulative cycle count (report variants only)     │
│                                                                  │
│     invocation_tracker: HashMap<String, u64>                     │
│       key = label name                                           │
│       value = invocation count (report variants only)           │
│                                                                  │
│   cycle_tracker_start(name):                                     │
│     depth = starts.len() as u32                                  │
│     starts.insert(name, (global_clk, depth))                     │
│     return depth                                                 │
│                                                                  │
│   cycle_tracker_end(name):                                       │
│     remove from starts                                           │
│     return (global_clk - start, depth) or None                  │
│     (non-report — display only, no accumulation)                │
│                                                                  │
│   cycle_tracker_report_end(name):                                │
│     remove from starts                                           │
│     cycles = global_clk - start                                 │
│     cycle_tracker_totals[name] += cycles                         │
│     invocation_tracker[name] += 1                                │
│     return (cycles, depth)                                       │
│                                                                  │
│   global_clk: incremented by 1 per instruction                   │
│     (in non-unconstrained mode only)                             │
└────────────────────┬────────────────────────────────────────────┘
                     │ After execution completes
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│   prover/src/worker/prover/execute.rs                           │
│                                                                  │
│   After MinimalExecutorRunner finishes:                          │
│     #[cfg(feature = "profiling")]                                │
│     cycle_tracker = executor.take_cycle_tracker_totals()         │
│     invocation_tracker = executor.take_invocation_tracker()     │
│                                                                  │
│   Passed back via ExecutorOutput::PublicValues enum variant      │
│   Merged into final ExecutionReport:                             │
│     final_report.cycle_tracker = cycle_tracker                   │
│     final_report.invocation_tracker = invocation_tracker        │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│   ExecutionReport (report.rs)                                    │
│                                                                  │
│   pub cycle_tracker: HashMap<String, u64>                        │
│     Label → cumulative cycle count (report variants only)       │
│                                                                  │
│   pub invocation_tracker: HashMap<String, u64>                   │
│     Label → invocation count (report variants only)             │
│                                                                  │
│   AddAssign merging:                                             │
│     for (label, count) in rhs.cycle_tracker:                    │
│         self.cycle_tracker.entry(label).or_insert(0) += count   │
│     (same for invocation_tracker)                               │
│                                                                  │
│   Display impl does NOT print cycle_tracker or invocation_tracker│
│   (only prints opcode_counts and syscall_counts)                 │
└─────────────────────────────────────────────────────────────────┘
```

### Tracking Lifetime

1. **Begins**: When guest program emits `cycle-tracker-start:` or `cycle-tracker-report-start:` via stdout/stderr
2. **Ends**: When guest program emits `cycle-tracker-end:` or `cycle-tracker-report-end:`
3. **Collection**: `MinimalExecutor` accumulates during execution (in-memory HashMap)
4. **Extraction**: After execution completes, `take_cycle_tracker_totals()` consumes the HashMap
5. **Reporting**: Merged into `ExecutionReport` returned to caller

---

## Part 3: Variant Comparison Table

### Variant 1: Non-Report Cycle Tracker (`cycle-tracker-start` / `cycle-tracker-end`)

| Property | Value |
|----------|-------|
| **Purpose** | Real-time cycle counting with tree visualization to `tracing::info!` |
| **Trigger** | `println!("cycle-tracker-start: X")` or `#[cycle_tracker]` derive macro (uses `eprintln!`) |
| **Parser** | `parse_command()` returns `("start", name)` / `("end", name)` |
| **API** | `SyscallContext::cycle_tracker_start(name)` / `cycle_tracker_end(name)` |
| **Display** | `tracing::info!("{}┌╴{}", padding, name)` — tree-style with `│ ` indentation per depth level |
| **Accumulation** | **None** — NOT stored in `ExecutionReport` |
| **Depth tracking** | Yes — `starts.len()` gives depth, used for visual tree display |
| **Status** | **Active** — primary real-time profiling tool |
| **Feature gate** | `#[cfg(feature = "profiling")]` |

### Variant 2: Report Cycle Tracker (`cycle-tracker-report-start` / `cycle-tracker-report-end`)

| Property | Value |
|----------|-------|
| **Purpose** | Accumulate cycle counts into `ExecutionReport` for programmatic access |
| **Trigger** | `println!("cycle-tracker-report-start: X")` / `println!("cycle-tracker-report-end: X")` |
| **Parser** | `parse_command()` returns `("report-start", name)` / `("report-end", name)` |
| **API** | `SyscallContext::cycle_tracker_start(name)` (same as non-report start) / `cycle_tracker_report_end(name)` |
| **Display** | Same tree visualization as non-report (via `tracing::info!`) |
| **Accumulation** | **Yes** — stored in `cycle_tracker_totals: HashMap<String, u64>` and `invocation_tracker: HashMap<String, u64>` |
| **Report field** | `ExecutionReport.cycle_tracker` and `ExecutionReport.invocation_tracker` |
| **Status** | **Active** — programmatic profiling |
| **Example** | `examples/cycle-tracking/program/bin/report.rs`, `test-artifacts/programs/cycle-tracker/src/main.rs` |

### Variant 3: Recursion Circuit V2 Cycle Tracker

| Property | Value |
|----------|-------|
| **Purpose** | Profile recursion circuit compilation (instruction generation timing) |
| **Trigger** | `#[sp1_derive::cycle_tracker_recursion]` proc macro |
| **API** | `CircuitV2Builder::cycle_tracker_v2_enter(name)` / `cycle_tracker_v2_exit()` |
| **IR** | `DslIr::CycleTrackerV2Enter(Cow<'static, str>)` / `DslIr::CycleTrackerV2Exit` |
| **Display** | `SpanBuilder` tree — printed via `cycle_tracker_root_span.lines()` |
| **Accumulation** | **No** — display-only, no persistent storage |
| **Feature gate** | `#[cfg(feature = "debug")]` (on the compiler) |
| **Status** | **Active** — but debug-only, NOT part of the VM cycle tracker |
| **Code evidence** | `compiler.rs:823-830` — creates `SpanBuilder`, calls `.lines()` for output |

### Why Multiple Coexist

1. **Non-report vs Report**: Different use cases. Non-report is for real-time observation (development/debugging). Report variant provides post-execution programmatic access via `ExecutionReport`.
2. **Recursion V2**: Completely separate system in a different layer (circuit compiler, not VM). Uses its own `SpanBuilder` infrastructure. Different feature gate (`debug` vs `profiling`).
3. **Backward compatibility**: The non-report variant predates the report variant. The report variant was added later (v4→v6 transition) to provide SDK-accessible profiling data.
4. **JIT no-ops**: `JitContext` implements cycle tracker methods as no-ops because profiling is only done in the portable (non-JIT) executor. This is explicitly documented: `// JitContext is not used when profiling is enabled`.

---

## Part 4: Semantics / Edge Cases

### 4.1 What "cycles" Means

**Definition**: `global_clk` — a counter incremented by **1 per RISC-V instruction executed** in non-unconstrained mode. This is distinct from the STARK `clk` which increments by 8 or 256 per instruction.

**Evidence**:
```rust
// portable/mod.rs:556
if self.maybe_unconstrained.is_none() {
    self.global_clk = self.global_clk.wrapping_add(1);
}
```

**Cycle count = `global_clk.saturating_sub(start)`** — the number of RISC-V instructions between markers.

**What this includes**: ALL instructions in the tracked block (RISC-V opcodes, branches, jumps, loads, stores, ecalls/syscalls). Includes cycles from nested `cycle-tracker` blocks within.

### 4.2 Nesting Behavior

**Tracker A contains Tracker B:**

```
cycle-tracker-start: outer
  cycle-tracker-start: inner
  cycle-tracker-end: inner
cycle-tracker-end: outer
```

**For non-report variant:**
- Both tracked independently via `cycle_tracker_starts` HashMap
- HashMap ensures only ONE entry per label name at a time
- If label names are **different** ("outer" vs "inner"): both tracked independently. Inner cycles are INCLUDED in outer's total (outer measures wall-clock global_clk, which includes inner's instructions).
- If label names are **identical**: second `start` **overwrites** the first entry in HashMap. First block is silently lost. This is a bug/quirk uncovered by overlapping identical labels.

**Depth tracking:**
- Depth = `cycle_tracker_starts.len()` at the moment of `start` — gives number of currently open trackers
- Depth is used ONLY for visual tree display indentation
- Depth is NOT stored in the report

**Evidence from g() in test program:**
```rust
fn g(x: usize) -> usize {
    println!("cycle-tracker-start: g");
    println!("cycle-tracker-start: g2");   // nested inside g
    let y = x + 3;
    println!("cycle-tracker-end: g2");
    println!("cycle-tracker-end: g");
    y
}
```
This demonstrates nesting with different labels — g2's cycles are included in g's total.

### 4.3 Report Variant Accumulation

**Key semantics from code:**
```rust
// portable/mod.rs — cycle_tracker_report_end
*self.cycle_tracker_totals.entry(name.to_string()).or_insert(0) += cycles;
*self.invocation_tracker.entry(name.to_string()).or_insert(0) += 1;
```

- **Multiple invocations** of the same label: cycles **accumulate** (summed). Invocation count tracks number of calls.
- **Nested labels**: cycles of inner block ARE included in outer block's total (outer's `global_clk` span includes inner's instructions).
- **No deduplication**: if A contains B, A's reported cycles include B's cycles too. This is **double-counting at the instruction level** but **flat aggregation** at the label level.

**Evidence from test:**
```rust
// sdk/src/lib.rs test_cycle_tracker_report_variants
// "repeated" called 3 times
assert_eq!(*report.invocation_tracker.get("repeated").unwrap(), 3);
```

### 4.4 What `total_instruction_count()` Represents

```rust
// report.rs
pub fn total_instruction_count(&self) -> u64 {
    self.opcode_counts.values().sum()
}
```

This is the **full program instruction count** — every RISC-V opcode executed across the entire program. This is separate from `cycle_tracker` which only contains cycles within instrumented blocks.

**Relationship**: `cycle_tracker.values().sum::<u64>()` ≤ `total_instruction_count()` — instrumented blocks may only cover a subset of program execution.

### 4.5 Chunk Boundary Handling

The test `test_cycle_tracker_across_chunks` validates correctness when trace chunks split across a cycle tracker block. This works because:
- `cycle_tracker_starts` persists across chunk boundaries in `MinimalExecutor`
- `global_clk` is continuous across chunks
- The start time was recorded as a `global_clk` value, so the subtraction works regardless of chunk boundaries

### 4.6 stdout vs stderr

- The `#[cycle_tracker]` proc macro emits to **stderr** (`eprintln!`)
- Manual `println!("cycle-tracker-start: ...")` goes to **stdout** (fd=1)
- The `write()` syscall handler routes both to `handle_output()` — but note:
  - fd=1: parsed for cycle tracker commands
  - fd=2: just printed (eprintln), NOT parsed for cycle tracker commands

**Code evidence:**
```rust
// write.rs handle_output
if fd == 1 {
    // stdout - process cycle tracker commands
    for line in content.lines() {
        if let Some((cmd, name)) = parse_command(line) { ... }
    }
} else {
    // stderr - just print
    for line in content.lines() {
        eprintln!("stderr: {line}");
    }
}
```

**Important implication**: The `#[cycle_tracker]` derive macro emits to stderr, which means its markers are **NOT parsed** for cycle tracking under the current `write()` handler — they only produce log output. This is confirmed by the test:
```rust
// test_cycle_tracker_macro_non_report
assert!(!report.cycle_tracker.contains_key("f"),
    "Non-report variant 'f' should not be in cycle_tracker");
```

**However**, the cycle-tracking example `program/bin/normal.rs` shows `#[cycle_tracker]` working for display purposes — the eprintln output shows in logs but isn't accumulated to the report. This is consistent: the derive macro is for display-only, and manual `println!("cycle-tracker-start:...")` is for both display and report accumulation (when using the report prefix).

---

## Part 5: Example Outputs

### 5.1 Non-Report Variant (stdout parse + tracing output)

From `examples/cycle-tracking/program/bin/normal.rs`:

**Guest code:**
```rust
println!("cycle-tracker-start: setup");
for _ in 0..100 { /* work */ }
println!("cycle-tracker-end: setup");

println!("cycle-tracker-start: main-body");
expensive_function(x);  // also has #[cycle_tracker]
println!("cycle-tracker-end: main-body");
```

**Expected tracing output:**
```
┌╴setup
└╴setup 1234 cycles
┌╴main-body
│ ┌╴expensive_function
│ └╴expensive_function 567 cycles
│ ┌╴expensive_function
│ └╴expensive_function 567 cycles
└╴main-body 2345 cycles
```

**ExecutionReport**: `cycle_tracker` is **empty** — non-report variants don't accumulate.

### 5.2 Report Variant

From `examples/cycle-tracking/program/bin/report.rs`:

**Guest code:**
```rust
println!("cycle-tracker-report-start: setup");
for _ in 0..100 { /* work */ }
println!("cycle-tracker-report-end: setup");
```

**Script output** (from `script/src/main.rs`):
```
Using cycle-tracker-report saves the number of cycles to the cycle-tracker mapping in the report.
Here's the number of cycles used by the setup: 1234
```

**ExecutionReport**:
```
ExecutionReport {
    cycle_tracker: {"setup": 1234},
    invocation_tracker: {"setup": 1},
    opcode_counts: { ... },
    syscall_counts: { ... },
    ...
}
```

### 5.3 Test Artifact Program (Multiple Variants)

From `test-artifacts/programs/cycle-tracker/src/main.rs`:

```
f()        — #[cycle_tracker] → stderr, NOT in report
g()        — manual non-report, nesting g2 inside g → NOT in report
h()        — report variant → in cycle_tracker["h"]
repeated() — report variant called 3× → cycle_tracker["repeated"] = sum of 3 invocations
                                       → invocation_tracker["repeated"] = 3
```

**Expected ExecutionReport**:
```
cycle_tracker: {"h": N, "repeated": M}
invocation_tracker: {"repeated": 3}
// f and g NOT present
```

Where N ≈ cycles for `h()` body, M ≈ 3 × cycles for `repeated()` body.

### 5.4 Recursion Circuit V2 Tracker (Debug Feature)

When `#[cfg(feature = "debug")]` is enabled and `#[cycle_tracker_recursion]` is used:

**Expected output** (from `SpanBuilder::lines()`):
```
cycle_tracker (total: X)
├─ printing felts (Y cycles)
│  ├─ printing felt 0 (Z cycles)
│  ├─ printing felt 1 (Z cycles)
│  └─ ...
└─ printing exts (W cycles)
   ├─ printing ext 0 (V cycles)
   └─ ...
```

This is a proper tree structure (unlike the VM tracker's flat HashMap).

---

## Part 6: Gaps / Improvement Opportunities

### 6.1 Current Limitations

| Capability | Status | Detail |
|------------|--------|--------|
| **Call tree awareness** | ❌ Missing | Report is flat `HashMap<String, u64>`. No parent-child edges preserved. |
| **Nesting awareness** | ⚠️ Partial | Depth tracked during execution but discarded in report. Only used for real-time display indentation. |
| **Execution ordering** | ❌ Missing | No timeline preserved. Multiple invocations of same label are summed, order of different labels lost. |
| **Percentage attribution** | ❌ Missing | No proportions computed. Caller must divide by `total_instruction_count()` manually. |
| **Whole-program baseline** | ⚠️ Partial | `total_instruction_count()` is available separately but not integrated with cycle tracking. No "uninstrumented cycles" bucket. |
| **Parent-child accounting** | ❌ Missing | When A contains B, A's cycles INCLUDE B's (no option for exclusive counting). |
| **Multiple same-label nesting** | ⚠️ Broken | If label "X" is started while "X" is already open, the first entry is silently overwritten. |
| **stderr parsing** | ❌ Missing | `#[cycle_tracker]` derive emits to stderr which is NOT parsed for cycle counts. Only display. |
| **Inter-chunk tracking** | ✅ Works | `cycle_tracker_starts` persists across chunks; `global_clk` is continuous. |
| **Recursion circuit tracker** | ⚠️ Debug-only | Not available in production builds. Uses separate `SpanBuilder` infra. |
| **Gecko profiler integration** | ❌ None | The `profiler.rs` Gecko profiler is a sampling profiler — completely separate from cycle tracking. No unified view. |

### 6.2 Why Flat HashMap Design?

The current design is intentionally simple:
- `ExecutionReport` is serialized/deserialized (it derives `Serialize`/`Deserialize`)
- It's merged across shards/chunks via `AddAssign` (summing)
- Tree structures don't merge well under addition
- The primary use case is cost estimation (gas calculation), not detailed profiling

### 6.3 Suggested Extension Points

1. **Add exclusive cycle tracking**: Add a `cycle-tracker-report-exclusive-start/end` variant that uses a local clock counter (reset on nested entries) so inner cycles are NOT included in outer totals.

2. **Add structured tree output**: Add `cycle_tracker_tree: Vec<CycleSpan>` to `ExecutionReport` where `CycleSpan { label, cycles, children }` preserves nesting. Disable merging for this field (or merge by label matching).

3. **Parse stderr for `#[cycle_tracker]` macros**: Modify `handle_output()` to parse cycle tracker commands from fd=2 as well, or change the derive macro to emit to stdout (fd=1).

4. **Uninstrumented cycles bucket**: Add `cycle_tracker_uninstrumented: u64 = total_instruction_count() - cycle_tracker.values().sum()`.

5. **Percentage column**: Add a helper method on `ExecutionReport`:
   ```rust
   pub fn cycle_tracker_percentages(&self) -> HashMap<String, f64> {
       let total = self.total_instruction_count() as f64;
       self.cycle_tracker.iter()
           .map(|(k, v)| (k.clone(), *v as f64 / total * 100.0))
           .collect()
   }
   ```

6. **De-duplicate label overwriting**: Change `cycle_tracker_starts` from `HashMap` to a stack-based structure that supports same-label nesting. E.g., `Vec<(String, u64, u32)>` with push/pop by label name.

7. **Unify with Gecko profiler**: Feed `cycle_tracker` spans as labeled frames into the Gecko profiler output for a unified flamegraph view.

8. **Gate the recursion circuit tracker on `profiling` instead of `debug`**: Makes it available in release builds for production circuit profiling.

---

## Summary of Key Files by Concern

```
Cycle Tracking Core:
  crates/core/executor/src/minimal/write.rs          ← Parser (marker → API call)
  crates/core/executor/src/minimal/arch/portable/mod.rs ← Storage (start/end/totals)
  crates/core/jit/src/context.rs                      ← Trait definition
  crates/core/runner/src/portable.rs                  ← Wrapper/forwarder

Data Flow / Orchestration:
  crates/prover/src/worker/prover/execute.rs          ← Extraction + merge into report
  crates/core/executor/src/report.rs                  ← ExecutionReport struct

User-Facing API:
  crates/derive/src/lib.rs                            ← #[cycle_tracker] macro
  crates/sdk/src/prover/execute.rs                    ← SDK execute → returns report
  crates/sdk/src/lib.rs                               ← Re-exports + tests

Recursion Circuit Tracker (separate):
  crates/recursion/compiler/src/circuit/builder.rs    ← V2 Enter/Exit
  crates/recursion/compiler/src/circuit/compiler.rs   ← CompileOneErr + SpanBuilder

Examples & Tests:
  examples/cycle-tracking/program/bin/normal.rs
  examples/cycle-tracking/program/bin/report.rs
  examples/cycle-tracking/script/src/main.rs
  crates/test-artifacts/programs/cycle-tracker/src/main.rs
```
