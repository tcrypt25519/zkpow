# Dependencies

Internal module dependency graph and external package reconciliation.

## Module graph

<!-- Collapsed to top-level packages; the full module list is in the Fan-in / Fan-out table below. -->
```mermaid
flowchart TD
    n0["crates"]
    n1["scripts"]
```

## Cycles

*No import cycles detected.*

## Fan-in / Fan-out

| Module | Fan-in | Fan-out |
|--------|--------|---------|
| [test_utils](modules/test_utils.md) | 1 | 0 |
| [cli_src_main](modules/cli_src_main.md) | 0 | 0 |
| [controller_src_lib](modules/controller_src_lib.md) | 0 | 0 |
| [src_protocol](modules/src_protocol.md) | 0 | 0 |
| [src_runtime](modules/src_runtime.md) | 0 | 0 |
| [src_storage](modules/src_storage.md) | 0 | 0 |
| [controller_src_tests_mod](modules/controller_src_tests_mod.md) | 0 | 0 |
| [tests_protocol](modules/tests_protocol.md) | 0 | 0 |
| [tests_runtime](modules/tests_runtime.md) | 0 | 0 |
| [tests_storage](modules/tests_storage.md) | 0 | 0 |
| [controller_src_validation](modules/controller_src_validation.md) | 0 | 0 |
| [src_verify](modules/src_verify.md) | 0 | 0 |
| [core_src_errors](modules/core_src_errors.md) | 0 | 0 |
| [core_src_halt_mod](modules/core_src_halt_mod.md) | 0 | 0 |
| [platform](modules/platform.md) | 0 | 0 |
| [zkvm](modules/zkvm.md) | 0 | 0 |
| [core_src_header](modules/core_src_header.md) | 0 | 0 |
| [src_input](modules/src_input.md) | 0 | 0 |
| [core_src_lib](modules/core_src_lib.md) | 0 | 0 |
| [public_values](modules/public_values.md) | 0 | 0 |
| [state](modules/state.md) | 0 | 0 |
| [halt](modules/halt.md) | 0 | 0 |
| [input_failure](modules/input_failure.md) | 0 | 0 |
| [src_tests_input_mod](modules/src_tests_input_mod.md) | 0 | 0 |
| [input_success](modules/input_success.md) | 0 | 0 |
| [core_src_tests_mod](modules/core_src_tests_mod.md) | 0 | 0 |
| [mtp_failure](modules/mtp_failure.md) | 0 | 0 |
| [src_tests_mtp_mod](modules/src_tests_mtp_mod.md) | 0 | 0 |
| [mtp_success](modules/mtp_success.md) | 0 | 0 |
| [pow_failure](modules/pow_failure.md) | 0 | 0 |
| [src_tests_pow_mod](modules/src_tests_pow_mod.md) | 0 | 0 |
| [pow_success](modules/pow_success.md) | 0 | 0 |
| [public_values_failure](modules/public_values_failure.md) | 0 | 0 |
| [src_tests_public_values_mod](modules/src_tests_public_values_mod.md) | 0 | 0 |
| [public_values_success](modules/public_values_success.md) | 0 | 0 |
| [retarget_failure](modules/retarget_failure.md) | 0 | 0 |
| [src_tests_retarget_mod](modules/src_tests_retarget_mod.md) | 0 | 0 |
| [retarget_success](modules/retarget_success.md) | 0 | 0 |
| [state_failure](modules/state_failure.md) | 0 | 0 |
| [src_tests_state_mod](modules/src_tests_state_mod.md) | 0 | 0 |
| [profile](modules/profile.md) | 0 | 0 |
| [state_success](modules/state_success.md) | 0 | 0 |
| [guest_src_main](modules/guest_src_main.md) | 0 | 0 |
| [build](modules/build.md) | 0 | 0 |
| [inspect-proof](modules/inspect-proof.md) | 0 | 0 |
| [sp1-stress](modules/sp1-stress.md) | 0 | 0 |
| [host_src_lib](modules/host_src_lib.md) | 0 | 0 |
| [host_src_main](modules/host_src_main.md) | 0 | 0 |
| [memory_monitor](modules/memory_monitor.md) | 0 | 0 |
| [observability](modules/observability.md) | 0 | 0 |
| [src_pipeline_artifacts](modules/src_pipeline_artifacts.md) | 0 | 0 |
| [src_pipeline_diagnostics](modules/src_pipeline_diagnostics.md) | 0 | 0 |
| [execution](modules/execution.md) | 0 | 0 |
| [pipeline_input](modules/pipeline_input.md) | 0 | 0 |
| [host_src_pipeline_mod](modules/host_src_pipeline_mod.md) | 0 | 0 |
| [compressed](modules/compressed.md) | 0 | 0 |
| [src_pipeline_proof_mod](modules/src_pipeline_proof_mod.md) | 0 | 0 |
| [pipeline_verify](modules/pipeline_verify.md) | 0 | 0 |
| [host_src_tests_mod](modules/host_src_tests_mod.md) | 0 | 0 |
| [tests_pipeline_artifacts](modules/tests_pipeline_artifacts.md) | 0 | 0 |
| [tests_pipeline_diagnostics](modules/tests_pipeline_diagnostics.md) | 0 | 0 |
| [src_tests_pipeline_mod](modules/src_tests_pipeline_mod.md) | 0 | 0 |
| [util](modules/util.md) | 0 | 0 |
| [hash](modules/hash.md) | 0 | 0 |
| [test_errors](modules/test_errors.md) | 0 | 0 |
| [failed_proof](modules/failed_proof.md) | 0 | 0 |
| [tests_test_errors_proofs_mod](modules/tests_test_errors_proofs_mod.md) | 0 | 0 |
| [recursive_chain](modules/recursive_chain.md) | 0 | 0 |
| [tampered_witness](modules/tampered_witness.md) | 0 | 0 |
| [mtp_src_errors](modules/mtp_src_errors.md) | 0 | 0 |
| [mtp_src_lib](modules/mtp_src_lib.md) | 0 | 0 |
| [ring](modules/ring.md) | 0 | 0 |
| [tests](modules/tests.md) | 0 | 0 |
| [consensus](modules/consensus.md) | 0 | 0 |
| [params_src_lib](modules/params_src_lib.md) | 0 | 0 |
| [params_src_tests_mod](modules/params_src_tests_mod.md) | 0 | 0 |
| [sha256_src_lib](modules/sha256_src_lib.md) | 0 | 0 |
| [sha256_src_tests_mod](modules/sha256_src_tests_mod.md) | 0 | 0 |
| [sha256_const_src_lib](modules/sha256_const_src_lib.md) | 0 | 0 |
| [sha256_const_src_tests_mod](modules/sha256_const_src_tests_mod.md) | 0 | 0 |
| [src_db](modules/src_db.md) | 0 | 0 |
| [store_src_lib](modules/store_src_lib.md) | 0 | 1 |
| [tests_db](modules/tests_db.md) | 0 | 0 |
| [store_src_tests_mod](modules/store_src_tests_mod.md) | 0 | 0 |
| [src_connection](modules/src_connection.md) | 0 | 0 |
| [error](modules/error.md) | 0 | 0 |
| [sync-headers_src_lib](modules/sync-headers_src_lib.md) | 0 | 0 |
| [msg](modules/msg.md) | 0 | 0 |
| [p2p_sync](modules/p2p_sync.md) | 0 | 0 |
| [src_peers](modules/src_peers.md) | 0 | 0 |
| [tests_connection](modules/tests_connection.md) | 0 | 0 |
| [sync-headers_src_tests_mod](modules/sync-headers_src_tests_mod.md) | 0 | 0 |
| [tests_peers](modules/tests_peers.md) | 0 | 0 |
| [tests_types](modules/tests_types.md) | 0 | 0 |
| [src_tests_validation](modules/src_tests_validation.md) | 0 | 0 |
| [src_types](modules/src_types.md) | 0 | 0 |
| [sync-headers_src_validation](modules/sync-headers_src_validation.md) | 0 | 0 |
| [src_bigint](modules/src_bigint.md) | 0 | 0 |
| [src_brand](modules/src_brand.md) | 0 | 0 |
| [types_src_header](modules/types_src_header.md) | 0 | 0 |
| [types_src_lib](modules/types_src_lib.md) | 0 | 0 |
| [src_run_id](modules/src_run_id.md) | 0 | 0 |
| [target_triplet](modules/target_triplet.md) | 0 | 0 |
| [tests_bigint](modules/tests_bigint.md) | 0 | 0 |
| [tests_brand](modules/tests_brand.md) | 0 | 0 |
| [bytes](modules/bytes.md) | 0 | 0 |
| [src_tests_header](modules/src_tests_header.md) | 0 | 0 |
| [types_src_tests_mod](modules/types_src_tests_mod.md) | 0 | 0 |
| [tests_run_id](modules/tests_run_id.md) | 0 | 0 |
| [wiretype_src_lib](modules/wiretype_src_lib.md) | 0 | 0 |
| [serialization](modules/serialization.md) | 0 | 0 |
| [ui](modules/ui.md) | 0 | 0 |
| [arguments](modules/arguments.md) | 0 | 0 |
| [not_pod](modules/not_pod.md) | 0 | 0 |
| [wiretype_macro_src_lib](modules/wiretype_macro_src_lib.md) | 0 | 0 |
| [src_config](modules/src_config.md) | 0 | 0 |
| [jobs](modules/jobs.md) | 0 | 0 |
| [worker_src_lib](modules/worker_src_lib.md) | 0 | 0 |
| [tests_config](modules/tests_config.md) | 0 | 0 |
| [worker_src_tests_mod](modules/worker_src_tests_mod.md) | 0 | 0 |
| [generate-epoch-profile-fixture](modules/generate-epoch-profile-fixture.md) | 0 | 0 |
| [summarize-prove-spans](modules/summarize-prove-spans.md) | 0 | 0 |

## External dependencies

### rust

- **Used:** `bigint`, `bitcoin`, `brand`, `bytemuck`, `clap`, `config`, `connection`, `consensus`, `db`, `diagnostics`, `error`, `errors`, `execution`, `hash`, `header`, `hex`, `input`, `jobs`, `peers`, `platform`, `proc_macro`, `proof`, `public_values`, `quote`, `rand`, `ring`, `run_id`, `rusqlite`, `sha2`, `sha256_const`, `sp1_lib`, `sp1_sdk`, `sp1_zkvm`, `state`, `strum`, `strum_macros`, `syn`, `sync_headers`, `target_triplet`, `test_utils`, `thiserror`, `tokio`, `tracing`, `tracing_appender`, `tracing_subscriber`, `types`, `validation`, `wiretype`, `wiretype_macro`, `zkpow_controller`, `zkpow_core`, `zkpow_host`, `zkpow_mtp`, `zkpow_params`, `zkpow_store`, `zkpow_types`, `zkvm`, `zkvm_test`
- ⚠️ **Undeclared:** `bigint`, `brand`, `config`, `connection`, `consensus`, `db`, `diagnostics`, `error`, `errors`, `execution`, `hash`, `header`, `input`, `jobs`, `peers`, `platform`, `proc_macro`, `proof`, `public_values`, `ring`, `run_id`, `state`, `sync_headers`, `target_triplet`, `test_utils`, `types`, `validation`, `zkpow_host`, `zkvm`
- **Unused (declared, not imported):** `console_subscriber`, `num_bigint`, `postcard`, `proc_macro2`, `rkyv`, `sp1_derive`, `sp1_prover`, `zkpow_sha256`, `zkpow_sync_headers`, `zkpow_worker`

## Notes

The extractor resolved one internal edge for this Rust workspace, so every fan-in and fan-out above is zero and the module graph collapses to `crates` and `scripts`. Cross-crate `use` paths are not mapped to modules. The real structure is the crate graph in each `Cargo.toml`: `zkpow-params` has seven workspace dependents, `zkpow-core` six, `wiretype` five, and `zkpow-types`, `zkpow-sha256`, and `zkpow-store` three each.

Most "undeclared" names (`bigint`, `db`, `state`, `errors`, and so on) are a crate's own modules read as packages. `sync_headers` is the library name of `zkpow-sync-headers`, which is why the package also shows as "unused". `zkpow_sha256` is listed unused but is called from `zkpow-core`. Check the code before acting on either list.
