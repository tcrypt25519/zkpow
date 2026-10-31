# summarize-prove-spans

**Entry point:** `main` (`process`)
**Source:** [summarize-prove-spans](../modules/summarize-prove-spans.md)
**Modules touched:** [summarize-prove-spans](../modules/summarize-prove-spans.md)

## Call sequence

<!-- Auto-generated from static call edges. Dashed arrows are external or unresolved calls. Reviewed runtime conditions and side effects belong in Behavior. -->
```mermaid
sequenceDiagram
    participant p0 as main
    participant p1 as len
    participant p2 as print
    participant p3 as int
    participant p4 as summarize
    participant p5 as defaultdict
    participant p6 as open
    participant p7 as json.loads
    participant p8 as record.get (scripts/summarize-prove-spans.py:summarize)
    participant p9 as fields.get
    participant p10 as str(…).startswith
    participant p11 as str (scripts/summarize-prove-spans.py:summarize)
    participant p12 as parse_duration_seconds
    participant p13 as DURATION_RE.match
    participant p14 as str (scripts/summarize-prove-s….py:parse_duration_seconds)
    participant p15 as float
    participant p16 as match.group
    participant p17 as record_path
    participant p18 as span_name
    participant p19 as isinstance
    participant p20 as span.get
    participant p21 as record.get (scripts/summarize-prove-spans.py:record_path)
    participant p22 as names.append
    p0-->>p1: len
    p0-->>p2: print
    p0-->>p1: len
    p0-->>p3: int
    p0->>p4: summarize
    p4-->>p5: defaultdict
    p4-->>p6: open
    p4-->>p7: json.loads
    p4-->>p8: record.get (scripts/summarize-prove-spans.py:summarize)
    p4-->>p9: fields.get
    p4-->>p10: str(…).startswith
    p4-->>p11: str (scripts/summarize-prove-spans.py:summarize)
    p4->>p12: parse_duration_seconds
    p12-->>p13: DURATION_RE.match
    p12-->>p14: str (scripts/summarize-prove-s….py:parse_duration_seconds)
    p12-->>p15: float
    p12-->>p16: match.group
    p12-->>p16: match.group
    p4-->>p9: fields.get
    p4->>p12: parse_duration_seconds
    p4-->>p9: fields.get
    p4->>p17: record_path
    p17->>p18: span_name
    p18-->>p19: isinstance
    p18-->>p20: span.get
    p18-->>p20: span.get
    p17-->>p21: record.get (scripts/summarize-prove-spans.py:record_path)
    p17->>p18: span_name
    p17-->>p21: record.get (scripts/summarize-prove-spans.py:record_path)
    p17-->>p22: names.append
```

> Call sequence diagram shows 30 of 51 interactions; 21 omitted to keep the visualization within the 30-interaction and generated-diagram limits.

## Data flow

<!-- Auto-generated static analysis. Treat values and boundaries as best-effort hints, not runtime proof. -->
```mermaid
flowchart LR
    s1["1. main"]
    s2["2. len"]
    s3["3. print"]
    s4["4. len"]
    s5["5. int"]
    s6["6. summarize"]
    s7["7. defaultdict"]
    s8["8. open"]
    s9["9. json.loads"]
    s10["10. record.get (scripts/summarize-prove-spans.py:summarize)"]
    s11["11. fields.get"]
    s12["12. str(…).startswith"]
    s1 -. "len(sys.argv)" .-> s2
    s1 -. "print('usage: summarize-prove-spans.py #60;run.jsonl#62; #60;output.txt#62; [limit]', file=sys.stderr)" .-> s3
    s1 -. "len(sys.argv)" .-> s4
    s1 -. "int(sys.argv[...])" .-> s5
    s1 -->|"summarize(sys.argv[...], sys.argv[...], limit)"| s6
    s6 -. "defaultdict(Bucket)" .-> s7
    s6 -. "open(jsonl_path, 'r', encoding='utf-8')" .-> s8
    s6 -. "json.loads(line)" .-> s9
    s6 -. "record.get (scripts/summarize-prove-spans.py:summarize)('fields', {...})" .-> s10
    s6 -. "fields.get('message')" .-> s11
    s6 -. "str(…).startswith('prove_compressed finished')" .-> s12
    b0["output print"]
    s1 -. "output print" .-> b0
    b1["filesystem_read open"]
    s6 -. "filesystem_read open" .-> b1
    b2["filesystem_write open"]
    s6 -. "filesystem_write open" .-> b2
    click s1 "../modules/summarize-prove-spans.md"
    click s6 "../modules/summarize-prove-spans.md"
    classDef boundary stroke:#b45309,stroke-dasharray: 4 2
    class b0 boundary
    class b1 boundary
    class b2 boundary
```

### Step data

| Step | Inputs | Reads | Writes | Returns |
|---|---|---|---|---|
| `main` | - | `sys`, `sys`, `sys`, `sys` | - | `2`, `0` |
| `len` | - | - | - | - |
| `print` | - | - | - | - |
| `len` | - | - | - | - |
| `int` | - | - | - | - |
| `summarize` | `jsonl_path`, `output_path`, `limit` | `Bucket`, `json` | `bucket.busy`, `bucket.idle`, `bucket.count` | `none`, `none` |
| `defaultdict` | - | - | - | - |
| `open` | - | - | - | - |
| `json.loads` | - | - | - | - |
| `record.get (scripts/summarize-prove-spans.py:summarize)` | - | - | - | - |
| `fields.get` | - | - | - | - |
| `str(…).startswith` | - | - | - | - |

### Call data

| From | To | Line | Call |
|---|---|---:|---|
| main | len | 144 | `len(sys.argv)` |
| main | print | 145 | `print('usage: summarize-prove-spans.py <run.jsonl> <output.txt> [limit]', file=sys.stderr)` |
| main | len | 150 | `len(sys.argv)` |
| main | int | 150 | `int(sys.argv[...])` |
| main | summarize | 151 | `summarize(sys.argv[...], sys.argv[...], limit)` |
| summarize | defaultdict | 66 | `defaultdict(Bucket)` |
| summarize | open | 71 | `open(jsonl_path, 'r', encoding='utf-8')` |
| summarize | json.loads | 74 | `json.loads(line)` |
| summarize | record.get (scripts/summarize-prove-spans.py:summarize) | 78 | `record.get('fields', {...})` |
| summarize | fields.get | 79 | `fields.get('message')` |
| summarize | str(…).startswith | 84 | `str(message).startswith('prove_compressed finished')` |

### Boundary effects

| Kind | Target | Step | Line |
|---|---|---|---:|
| output | `print` | `main` | 145 |
| filesystem_read | `open` | `summarize` | 71 |
| filesystem_write | `open` | `summarize` | 108 |

### Static analysis gaps

| Kind | Step | Target | Line |
|---|---|---|---:|
| external_call | `summarize` | `defaultdict` | 66 |
| external_call | `summarize` | `json.loads` | 74 |
| unresolved_call | `summarize` | `record.get` | 78 |
| unresolved_call | `summarize` | `fields.get` | 79 |
| unresolved_call | `summarize` | `str(message).startswith` | 84 |
| step_limit | `main` | `first 12 steps` | 0 |

## Behavior

Reads a `tracing` JSON log (the `logs/run.jsonl` written during `scripts/profile-sp1.sh`) and writes a text table of where SP1's `prove_compressed` spent wall-clock time. Only `close` events between the `prove_compressed started` and `prove_compressed finished` messages count. Each event's `time.busy` and `time.idle` (in ns, µs/us, ms, or s) are summed into a bucket keyed by span path. The path drops `runtime.spawn` and a leading `prove_compressed_detail`, and labels `prove` spans with their `mode`. Rows sort by busy time, then idle time, and the top `limit` (default 40) are written.

Spans nest and can overlap, so the totals rank spans; they do not add up to the proof's wall time. With no start marker, or no close events because the run lacked `PROVE_COMPRESSED_SPANS=1`, the output says so instead of printing a table. A log that ends before the finish marker gets a warning line. A wrong argument count prints usage and exits 2. Lines that are not JSON are skipped.
