# Program optimization

## General optimization tips

- Profile to find bottlenecks: use profiling (https://docs.succinct.xyz/docs/sp1/optimizing-programs/profiling) or cycle tracking (https://docs.succinct.xyz/docs/sp1/optimizing-programs/cycle-tracking).
- Try different compile settings: enable LTO (lto = thin|true|fat) and set codegen-units = 1.
- Avoid unnecessary copies and (de)serialization.

## Cryptographic acceleration

- Use SP1 precompiles for heavy crypto (SHA-256, Keccak, etc.); follow instructions: https://docs.succinct.xyz/docs/sp1/optimizing-programs/precompiles.

## Prover acceleration
- Enable hardware acceleration for proving: GPU (CUDA) or CPU (AVX on Intel/ARM). See: https://docs.succinct.xyz/docs/sp1/generating-proofs/hardware-acceleration.

Note from tcrypt: Plonky3 also appears to support NEON on Apple chips, but they don't have
instructions I could find. Additionally I read that Rust in general may have some issues with it.
I think we should try it but abort quickly if it takes more than a few turns. I will get an Intel
box and a GPU box to test on as well.

## I/O optimizations

- Prefer zero-copy (de)serialization over write/read (which use bincode by default).
- Use writevec and readslice in the zkVM context.
- Consider zero-copy libraries such as rkyv and derive Archive, Serialize, Deserialize for structs (e.g., #[derive(Archive, Serialize, Deserialize)]).
- Serialize in script with rkyv::tobytes and write the bytes to stdin; deserialize in program with rkyv::frombytes.
- If rkyv is hard to integrate, benchmark other serialization libraries or implement custom (de)serialization logic for better I/O performance.

## Profiling workflow

Use the zero-config wrapper script to generate a reproducible profiling run:

```bash
./scripts/profile-sp1.sh
```

What it does:
- Sets `RUST_LOG=info` by default so guest cycle-tracker markers are visible.
- Sets `NUM_HEADERS=100` by default for a representative load, while still allowing overrides.
- Writes proofs to `profiling/sp1/<timestamp>/proofs/` through `OUTPUT_DIR`.
- Saves the full run log to `profiling/sp1/<timestamp>/run.log`.
- Extracts cycle-tracker lines into `profiling/sp1/<timestamp>/cycle-tracker.log`.
- Updates `profiling/sp1/latest` to point at the newest run.

Useful overrides:

```bash
NUM_HEADERS=1 ./scripts/profile-sp1.sh
PREV_PROOF=proof_height_1_to_100.bin ./scripts/profile-sp1.sh
OUTPUT_DIR=/tmp/custom-proof-out ./scripts/profile-sp1.sh
```

The current cycle-tracker labels are hierarchical and intentionally stable:
- `program/parse_input`
- `program/verify_recursive_proof`
- `program/apply_headers`
- `program/commit_success`
- `state/apply_headers`
- `state/apply_headers/header`
- `state/next`
- `state/next/build_header`
- `state/next/hash_header`
- `state/next/timestamp_window`
- `state/next/retarget`
- `state/next/chain_work`
- `state/validate`
- `state/validate/median_time_past`
- `state/validate/pow`
- `state/median_time_past`
- `input/parse`
- `input/parse/state`
- `input/recursive_proof`
- `input/parse/genesis_recursive_proof`
- `input/parse/headers`
- `input/parse/genesis_hash`
- `parse/state`
- `parse/header`
- `pow/retarget_target`
- `pow/work_from_bits`
- `hash/sha256d`

This is enough resolution to identify whether time is spent in parsing, hashing, median-time-past, retargeting, or proof verification without changing the run procedure.
