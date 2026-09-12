# Wire Protocol

`zkvm-test` operates via a binary IPC protocol over standard I/O streams between the host supervisor (`cargo-zkvm` / `zkvm-test-runner`) and the guest test harness (`zkvm-test`).

---

## Protocol Overview

When an ELF is executed by the runner, the runner writes command bytes to the guest's input stream (`SP1Stdin`). The guest test harness reads the command byte at entry point and branches:

```
Host (Runner)                     Guest (zkvm-test harness)
     │                                      │
     ├─── CMD_LIST (0x01) ─────────────────>│
     │<── Public Values: Test Catalog ──────┤ (halts)
     │                                      │
     ├─── CMD_RUN (0x02) + Test ID ────────>│
     ├─── [Optional: CMD_FIXTURE (0x03)] ──>│
     │<── Execution Trace & Output ─────────┤ (halts or exits)
```

---

## Command Magic Bytes

| Byte | Command | Description |
| :--- | :--- | :--- |
| `0x01` | `CMD_LIST` | Instructs the guest to enumerate all registered tests and halt. |
| `0x02` | `CMD_RUN` | Instructs the guest to execute a specific test by identifier. |
| `0x03` | `CMD_FIXTURE` | Streams raw fixture payload bytes into guest memory. |

---

## The Test Catalog Payload

When `CMD_LIST` is received, the guest writes a serialized catalog into public values (`sp1_zkvm::io::commit_slice`).

### Wire Layout
1. **Magic Header**: 4-byte identifier `[0x5A, 0x4B, 0x54, 0x43]` (`"ZKTC"` - zkVM Test Catalog).
2. **Version**: 2-byte major/minor protocol version.
3. **Count**: 4-byte little-endian integer `N` (number of registered tests).
4. **Test Descriptors** (repeated `N` times):
   - `id`: 4-byte little-endian test identifier.
   - `name_len`: 2-byte string length.
   - `name`: UTF-8 encoded test path string.
   - `flags`: Bitfield denoting `ignored`, `should_panic`, `has_budget`, etc.
   - `budget`: 8-byte little-endian cycle limit (if `has_budget` flag is set).
   - `expected_exit_code`: 4-byte integer.

---

## Execution and Fallback (Tier 1 Fallback)

If an ELF was compiled without `zkvm-test` (e.g. a plain guest binary or stock libtest binary):
1. The runner issues `CMD_LIST`.
2. The binary does not recognize the command or halts with an error, failing catalog decode (`Catalog::Absent`).
3. **Tier 1 Fallback**: The runner detects that no catalog exists, re-executes the binary once with clean input, and forwards stdout/stderr directly.
