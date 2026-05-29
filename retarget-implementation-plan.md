# Retarget Boundary Rework Plan

## Summary

Move Bitcoin difficulty retargeting from "precompute the next epoch at the
previous epoch tip" to "prepare the current epoch at the first block of that
epoch." This keeps epoch-boundary rule changes local to the boundary block and
prevents future consensus rules from being applied one block too early.

`AGENTS.md` references `AGENT_RULES.md`, but `AGENT_RULES.md` is not present in
this checkout. Keep the implementation narrow, consensus-focused, and avoid new
helper layers beyond the retarget/compact-target seam.

## Pre-Implementation Code Map

- Guest entrypoint: `crates/guest/src/main.rs` parses input/state/header/MTP
  witnesses, verifies recursive proof state binding when needed, then calls
  `State::apply_headers`.
- State transition: `crates/core/src/env/mod.rs::apply_headers` builds each
  full `Header`, hashes it, checks MTP hints, then delegates validation and
  mutation to `next_inner`.
- Current retarget behavior: `next_inner` checks MTP and PoW, increments height,
  writes the timestamp, updates `epoch_start_timestamp` on
  `height.is_multiple_of(2016)`, and precomputes the next epoch difficulty on
  `(height + 1).is_multiple_of(2016)`.
- Target math: `crates/core/src/lib.rs::calculate_next_work_required` currently
  accepts a pre-clamped timespan and returns only a full target. The notes call
  this `calculate_next_target_required`; the implementation should rename and
  rework the existing helper rather than add a duplicate path.
- Host reconstruction: `crates/host/src/util.rs::state_from_db_at_height`
  currently loads `next_nbits`/target/work from height `H + 1`; with current
  difficulty semantics it must use height `H`.

## Implementation Order

1. Add `target_from_bits(CompactTarget) -> Target` to `zkpow-core`, and use it
   everywhere compact targets are expanded. Remove the duplicate host-only
   expansion helper.
2. Rename private continuation and state fields from `next_nbits`,
   `next_work`, and `next_target` to `current_nbits`, `current_work`, and
   `current_target`. Preserve all serialized byte offsets and sizes.
3. Rework `calculate_next_work_required` into
   `calculate_next_target_required(old_target, epoch_start_timestamp,
   previous_timestamp) -> (CompactTarget, Target)`. It must compute and clamp
   the timespan internally, clamp to `GENESIS_TARGET`, compact the result, then
   re-expand that compact value and return the normalized target.
4. Replace `next_timestamp_slot` with `current_timestamp_slot`, using
   `height % WINDOW_SIZE`. In `apply_headers`, read `previous_timestamp` from
   that slot before mutating state.
5. Inline the old `next_inner` responsibilities into `apply_headers` so each
   header is processed transactionally: validate median hint, derive
   `candidate_height`, retarget if `candidate_height.is_multiple_of(EPOCH_LENGTH)`,
   build/hash the header with the active compact target, check PoW, then commit
   height/header/hash/timestamp/chainwork.
6. Update host DB reconstruction and MTP-hint fallback to the new slot and
   current-difficulty semantics.
7. Update retarget tests and logs to assert that pre-boundary states keep the
   previous epoch difficulty and the boundary block itself activates the new
   difficulty.

## Validation

- Run targeted checks after the core refactor:
  `cargo test -p zkpow-core --lib`.
- Run host DB checks after reconstruction changes:
  `cargo test -p zkpow-host db_retarget_schedule_matches_height_40320`.
- Final local validation:
  `cargo test -p zkpow-core --lib`,
  `cargo test -p zkpow-host`,
  `cargo run --release -p zkpow-host --bin test_errors`, and
  `git diff --check -- crates`.

## Assumptions

- No wire-format migration: `STATE_SIZE`, `PRIVATE_CONTINUATION_STATE_SIZE`,
  continuation digest layout, and public values layout remain unchanged.
- The Rust field names should reflect current difficulty semantics; do not keep
  old `next_*` aliases.
- The active per-block work in state is the work for the current difficulty
  period and should change on the same boundary as the compact target. The
  host DB loader preserves the existing `height + 1` chainwork alignment used
  for public-claim compatibility.
