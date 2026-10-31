# lib Module

**Path:** `crates/core/src/lib.rs`

## Description

Crate root of `zkpow-core`: the `no_std` header chain state transition and the consensus math under it. Re-exports the value types from `zkpow-types` and the median-time-past helpers from `zkpow-mtp`, so consumers import one crate.

Holds the target and work math: compact `bits` conversion both ways, proof-of-work checks on hashes and on SHA state words, block work, and `calculate_next_work_required`, which returns the next epoch's bits, target, and work as one [`TargetTriplet`](../entities/TargetTriplet.md). In the zkVM, block work takes the quotient `floor(2^256 / (target + 1))` as a prover hint and checks it with one 256-bit multiply syscall. Header hashing goes through a wrapper that tests can mock.

## Imports

| Source | Symbols |
|--------|---------|
| `errors::ApplyError` | `ApplyError` |
| `errors::ParseError` | `ParseError` |
| `errors::ReportableError` | `ReportableError` |
| `header::HeaderInput` | `HeaderInput` |
| `input::cast_slice` | `cast_slice` |
| `input::serialize_inputs` | `serialize_inputs` |
| `public_values::ErrorCode` | `ErrorCode` |
| `public_values::PublicValues` | `PublicValues` |
| `public_values::ReturnCode` | `ReturnCode` |
| `public_values::VerifierKeyDigest` | `VerifierKeyDigest` |
| `state::State` | `State` |
| `zkpow_mtp::WINDOW_LARGE_HALF_SIZE` | `WINDOW_LARGE_HALF_SIZE` |
| `zkpow_mtp::WINDOW_MAX_INDEX` | `WINDOW_MAX_INDEX` |
| `zkpow_mtp::WINDOW_SIZE` | `WINDOW_SIZE` |
| `zkpow_mtp::WINDOW_SMALL_HALF_SIZE` | `WINDOW_SMALL_HALF_SIZE` |
| `zkpow_mtp::advance_ordered_timestamp_run` | `advance_ordered_timestamp_run` |
| `zkpow_mtp::has_ordered_timestamp_window` | `has_ordered_timestamp_window` |
| `zkpow_mtp::ordered_median_timestamp_slot` | `ordered_median_timestamp_slot` |
| `zkpow_mtp::ordered_timestamp_run` | `ordered_timestamp_run` |
| `zkpow_mtp::requires_median_hint` | `requires_median_hint` |
| `zkpow_mtp::timestamp_count` | `timestamp_count` |
| `zkpow_mtp::timestamp_slot` | `timestamp_slot` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_types::BlockWork` | `BlockWork` |
| `zkpow_types::ChainWork` | `ChainWork` |
| `zkpow_types::CompactTarget` | `CompactTarget` |
| `zkpow_types::Header` | `Header` |
| `zkpow_types::NonMaxU256` | `NonMaxU256` |
| `zkpow_types::RunId` | `RunId` |
| `zkpow_types::RunIdError` | `RunIdError` |
| `zkpow_types::Target` | `Target` |
| `zkpow_types::TargetTriplet` | `TargetTriplet` |
| `zkpow_types::u256` | `u256` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 8 | 5 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `target_to_bits` | `(target: Target) -> CompactTarget` | — | Convert a full 256-bit target into compact `bits` encoding. |
| `target_from_bits` | `(compact_target: CompactTarget) -> Target` | — | Expand compact Bitcoin `bits` into a normalized full 256-bit target. |
| `check_proof_of_work` | `(hash: BlockHash, target: &Target) -> bool` | — | Check whether a header hash satisfies a target. |
| `check_proof_of_work_words` | `(words: &[u64 ; 8], target: &Target) -> bool` | — | Check whether a header hash in SHA state words satisfies a target. |
| `compute_block_work_division` | `(target: &Target) -> (u256, BlockWork)` | — | Compute one-block cumulative-work units from the expanded target using binary long division. |
| `block_work_from_target` | `(target: &Target) -> BlockWork` | — | Compute one-block cumulative-work units from the expanded target. |
| `calculate_next_work_required` | `(old_target: Target, epoch_start_timestamp: BlockTimestamp, previous_timestamp: BlockTimestamp) -> TargetTriplet` | — | Compute the work and corresponding target required at a difficulty boundary. |
