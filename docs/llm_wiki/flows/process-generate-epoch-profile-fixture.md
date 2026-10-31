# generate-epoch-profile-fixture

**Entry point:** `main` (`process`)
**Source:** [generate-epoch-profile-fixture](../modules/generate-epoch-profile-fixture.md)
**Modules touched:** [generate-epoch-profile-fixture](../modules/generate-epoch-profile-fixture.md)

## Call sequence

<!-- Auto-generated from static call edges. Dashed arrows are external or unresolved calls. Reviewed runtime conditions and side effects belong in Behavior. -->
```mermaid
sequenceDiagram
    participant p0 as main
    participant p1 as argparse.ArgumentParser
    participant p2 as parser.add_argument
    participant p3 as parser.parse_args
    participant p4 as parser.error
    participant p5 as sqlite3.connect
    participant p6 as db.execute
    participant p7 as bytearray
    participant p8 as struct.pack
    participant p9 as enumerate
    participant p10 as len
    participant p11 as ValueError
    participant p12 as headers.extend
    participant p13 as hints.extend
    participant p14 as hint_offsets.extend
    participant p15 as args.output.open
    participant p16 as output.write
    participant p17 as print
    p0-->>p1: argparse.ArgumentParser
    p0-->>p2: parser.add_argument
    p0-->>p2: parser.add_argument
    p0-->>p2: parser.add_argument
    p0-->>p3: parser.parse_args
    p0-->>p4: parser.error
    p0-->>p5: sqlite3.connect
    p0-->>p6: db.execute
    p0-->>p7: bytearray
    p0-->>p7: bytearray
    p0-->>p8: struct.pack
    p0-->>p7: bytearray
    p0-->>p9: enumerate
    p0-->>p10: len
    p0-->>p11: ValueError
    p0-->>p12: headers.extend
    p0-->>p8: struct.pack
    p0-->>p13: hints.extend
    p0-->>p8: struct.pack
    p0-->>p14: hint_offsets.extend
    p0-->>p8: struct.pack
    p0-->>p10: len
    p0-->>p11: ValueError
    p0-->>p15: args.output.open
    p0-->>p16: output.write
    p0-->>p8: struct.pack
    p0-->>p16: output.write
    p0-->>p16: output.write
    p0-->>p16: output.write
    p0-->>p17: print
```

## Data flow

<!-- Auto-generated static analysis. Treat values and boundaries as best-effort hints, not runtime proof. -->
```mermaid
flowchart LR
    s1["1. main"]
    s2["2. argparse.ArgumentParser"]
    s3["3. parser.add_argument"]
    s4["4. parser.add_argument"]
    s5["5. parser.add_argument"]
    s6["6. parser.parse_args"]
    s7["7. parser.error"]
    s8["8. sqlite3.connect"]
    s9["9. db.execute"]
    s10["10. bytearray"]
    s11["11. bytearray"]
    s12["12. struct.pack"]
    s1 -. "argparse.ArgumentParser(description=__doc__)" .-> s2
    s1 -. "parser.add_argument('--epochs', type=int, default=3, help='complete epochs available to the test')" .-> s3
    s1 -. "parser.add_argument('--db', type=Path, default=...)" .-> s4
    s1 -. "parser.add_argument('--output', type=Path, default=...)" .-> s5
    s1 -. "parser.parse_args(data not statically known)" .-> s6
    s1 -. "parser.error('--epochs must be positive')" .-> s7
    s1 -. "sqlite3.connect(..., uri=True)" .-> s8
    s1 -. "db.execute(…)" .-> s9
    s1 -. "bytearray(data not statically known)" .-> s10
    s1 -. "bytearray(struct.pack(...))" .-> s11
    s1 -. "struct.pack('#60;I', 0)" .-> s12
    b0["mutation headers.extend"]
    s1 -. "mutation headers.extend" .-> b0
    b1["mutation hints.extend"]
    s1 -. "mutation hints.extend" .-> b1
    b2["mutation hint_offsets.extend"]
    s1 -. "mutation hint_offsets.extend" .-> b2
    b3["output print"]
    s1 -. "output print" .-> b3
    click s1 "../modules/generate-epoch-profile-fixture.md"
    classDef boundary stroke:#b45309,stroke-dasharray: 4 2
    class b0 boundary
    class b1 boundary
    class b2 boundary
    class b3 boundary
```

### Step data

| Step | Inputs | Reads | Writes | Returns |
|---|---|---|---|---|
| `main` | - | `Path`, `ROOT`, `Path`, `ROOT`, `EPOCH_LENGTH` | - | - |
| `argparse.ArgumentParser` | - | - | - | - |
| `parser.add_argument` | - | - | - | - |
| `parser.add_argument` | - | - | - | - |
| `parser.add_argument` | - | - | - | - |
| `parser.parse_args` | - | - | - | - |
| `parser.error` | - | - | - | - |
| `sqlite3.connect` | - | - | - | - |
| `db.execute` | - | - | - | - |
| `bytearray` | - | - | - | - |
| `bytearray` | - | - | - | - |
| `struct.pack` | - | - | - | - |

### Call data

| From | To | Line | Call |
|---|---|---:|---|
| main | argparse.ArgumentParser | 14 | `argparse.ArgumentParser(description=__doc__)` |
| main | parser.add_argument | 15 | `parser.add_argument('--epochs', type=int, default=3, help='complete epochs available to the test')` |
| main | parser.add_argument | 16 | `parser.add_argument('--db', type=Path, default=...)` |
| main | parser.add_argument | 17 | `parser.add_argument('--output', type=Path, default=...)` |
| main | parser.parse_args | 18 | `parser.parse_args(data not statically known)` |
| main | parser.error | 20 | `parser.error('--epochs must be positive')` |
| main | sqlite3.connect | 23 | `sqlite3.connect(..., uri=True)` |
| main | db.execute | 24 | `db.execute('SELECT height, version, merkle_root, timestamp, nonce, median_time_past, ordered_timestamp_run FROM headers WHERE height BETWEEN 1 AND ? ORDER BY height', (...))` |
| main | bytearray | 28 | `bytearray(data not statically known)` |
| main | bytearray | 29 | `bytearray(struct.pack(...))` |
| main | struct.pack | 29 | `struct.pack('<I', 0)` |

### Boundary effects

| Kind | Target | Step | Line |
|---|---|---|---:|
| mutation | `headers.extend` | `main` | 36 |
| mutation | `hints.extend` | `main` | 38 |
| mutation | `hint_offsets.extend` | `main` | 39 |
| output | `print` | `main` | 49 |

### Static analysis gaps

| Kind | Step | Target | Line |
|---|---|---|---:|
| external_call | `main` | `argparse.ArgumentParser` | 14 |
| unresolved_call | `main` | `parser.add_argument` | 15 |
| unresolved_call | `main` | `parser.add_argument` | 16 |
| unresolved_call | `main` | `parser.add_argument` | 17 |
| unresolved_call | `main` | `parser.parse_args` | 18 |
| unresolved_call | `main` | `parser.error` | 20 |
| external_call | `main` | `sqlite3.connect` | 23 |
| unresolved_call | `main` | `db.execute` | 24 |
| external_call | `main` | `bytearray` | 28 |
| external_call | `main` | `bytearray` | 29 |
| external_call | `main` | `struct.pack` | 29 |
| step_limit | `main` | `first 12 steps` | 0 |

## Behavior

Exports mainnet headers 1 through `epochs * 2016 + 1` (default 3 epochs) from a read-only `headers.db` into `fixtures/epoch_profile_headers.bin`. That file is the input of the opt-in zkVM epoch profile tests in `crates/core/src/tests/state/profile.rs`.

The file is little-endian: a `u32` header count; each header's `version`, `merkle_root`, `timestamp`, and `nonce` (the [`HeaderInput`](../entities/HeaderInput.md) fields, since the guest derives `prev_blockhash` and the target); a `u32` hint-offset table with one entry before the first header and one after each; then the median-time-past hints. A header gets a hint from height 11 on, and only when the previous row's `ordered_timestamp_run` is below 10. This is the rule in `zkpow_mtp::requires_median_hint`.

A gap in heights, a `merkle_root` that is not 32 bytes, or too few rows raises `ValueError` before the output file is opened. `--epochs` below 1 is a usage error.
