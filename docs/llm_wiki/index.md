# LLM Wiki Index

This page is an exhaustive reference inventory of the selected source. Task-oriented guides are not yet available.

## Surface Overview

| Surface | Count | Start here |
|---|---:|---|
| Entities | 63 | [Open section](#entities) |
| Modules | 122 | [Open section](#modules) |
| Workflows | 0 | No pages |
| Guides | 0 | No pages |
| Entry-point flows | 2 | [Open section](#entry-point-flows) |
| Infrastructure | 3 | [Open section](#infrastructure) |
| HTTP API contracts | 0 | No pages |
| Dependency architecture | 2 | [Open section](#dependency-architecture) |
| Log | 1 | [Open log](log.md) |

## Entities

- [ApplyError](entities/ApplyError.md)
- [ArtifactStore](entities/ArtifactStore.md)
- [Batch](entities/Batch.md)
- [Bucket](entities/Bucket.md)
- [Cold](entities/Cold.md)
- [Command](entities/Command.md)
- [ConnectedHeader](entities/ConnectedHeader.md)
- [Consensus](entities/Consensus.md)
- [ConsensusParams](entities/ConsensusParams.md)
- [ControllerConfig](entities/ControllerConfig.md)
- [ControllerError](entities/ControllerError.md)
- [ControllerRuntime](entities/ControllerRuntime.md)
- [Error](entities/Error.md)
- [ErrorCode](entities/ErrorCode.md)
- [ErrorKind](entities/ErrorKind.md)
- [HeaderBatchWitness](entities/HeaderBatchWitness.md)
- [HeaderFixture](entities/HeaderFixture.md)
- [HeaderInput](entities/HeaderInput.md)
- [HeaderRecord](entities/HeaderRecord.md)
- [HeaderStore](entities/HeaderStore.md)
- [InsufficientColdHeaders](entities/InsufficientColdHeaders.md)
- [InvalidParts](entities/InvalidParts.md)
- [LocalArtifactStore](entities/LocalArtifactStore.md)
- [Mainnet](entities/Mainnet.md)
- [Network](entities/Network.md)
- [NonMaxU256](entities/NonMaxU256.md)
- [NotMedian](entities/NotMedian.md)
- [ParseError](entities/ParseError.md)
- [PeerConnection](entities/PeerConnection.md)
- [PeerEntry](entities/PeerEntry.md)
- [PeerStatus](entities/PeerStatus.md)
- [PeerStore](entities/PeerStore.md)
- [PhaseTiming](entities/PhaseTiming.md)
- [PreparedProverVerifier](entities/PreparedProverVerifier.md)
- [ProofGenerationConfig](entities/ProofGenerationConfig.md)
- [ProofVerifier](entities/ProofVerifier.md)
- [ProverBackend](entities/ProverBackend.md)
- [PublicValues](entities/PublicValues.md)
- [ReportableError](entities/ReportableError.md)
- [Result](entities/Result.md)
- [ResultOpt](entities/ResultOpt.md)
- [ReturnCode](entities/ReturnCode.md)
- [Ring](entities/Ring.md)
- [RunId](entities/RunId.md)
- [RunIdError](entities/RunIdError.md)
- [RunRecord](entities/RunRecord.md)
- [ShortEpoch](entities/ShortEpoch.md)
- [StageSample](entities/StageSample.md)
- [State](entities/State.md)
- [SyncHeadersError](entities/SyncHeadersError.md)
- [TargetTriplet](entities/TargetTriplet.md)
- [Tip](entities/Tip.md)
- [VerifierKeyDigest](entities/VerifierKeyDigest.md)
- [VersionMessage](entities/VersionMessage.md)
- [Warm](entities/Warm.md)
- [Window](entities/Window.md)
- [WireType](entities/WireType.md)
- [WorkerConfig](entities/WorkerConfig.md)
- [controller_src_lib_BoxError](entities/controller_src_lib_BoxError.md)
- [host_src_pipeline_mod_BoxError](entities/host_src_pipeline_mod_BoxError.md)
- [src_types_Header](entities/src_types_Header.md)
- [types_src_header_Header](entities/types_src_header_Header.md)
- [u256](entities/u256.md)

## Modules

- [arguments](modules/arguments.md) - `crates/wiretype/tests/ui/arguments.rs`
- [build](modules/build.md) - `crates/host/build.rs`
- [bytes](modules/bytes.md) - `crates/types/src/tests/bytes.rs`
- [cli_src_main](modules/cli_src_main.md) - `crates/cli/src/main.rs`
- [compressed](modules/compressed.md) - `crates/host/src/pipeline/proof/compressed.rs`
- [consensus](modules/consensus.md) - `crates/params/src/consensus.rs`
- [controller_src_lib](modules/controller_src_lib.md) - `crates/controller/src/lib.rs`
- [controller_src_tests_mod](modules/controller_src_tests_mod.md) - `crates/controller/src/tests/mod.rs`
- [controller_src_validation](modules/controller_src_validation.md) - `crates/controller/src/validation.rs`
- [core_src_errors](modules/core_src_errors.md) - `crates/core/src/errors.rs`
- [core_src_halt_mod](modules/core_src_halt_mod.md) - `crates/core/src/halt/mod.rs`
- [core_src_header](modules/core_src_header.md) - `crates/core/src/header.rs`
- [core_src_lib](modules/core_src_lib.md) - `crates/core/src/lib.rs`
- [core_src_tests_mod](modules/core_src_tests_mod.md) - `crates/core/src/tests/mod.rs`
- [error](modules/error.md) - `crates/sync-headers/src/error.rs`
- [execution](modules/execution.md) - `crates/host/src/pipeline/execution.rs`
- [failed_proof](modules/failed_proof.md) - `crates/host/tests/test_errors/proofs/failed_proof.rs`
- [generate-epoch-profile-fixture](modules/generate-epoch-profile-fixture.md) - Export real mainnet HeaderInput records for the opt-in zkVM epoch profile.
- [guest_src_main](modules/guest_src_main.md) - `crates/guest/src/main.rs`
- [halt](modules/halt.md) - `crates/core/src/tests/halt.rs`
- [hash](modules/hash.md) - `crates/host/src/util/hash.rs`
- [host_src_lib](modules/host_src_lib.md) - `crates/host/src/lib.rs`
- [host_src_main](modules/host_src_main.md) - `crates/host/src/main.rs`
- [host_src_pipeline_mod](modules/host_src_pipeline_mod.md) - `crates/host/src/pipeline/mod.rs`
- [host_src_tests_mod](modules/host_src_tests_mod.md) - `crates/host/src/tests/mod.rs`
- [input_failure](modules/input_failure.md) - `crates/core/src/tests/input/failure.rs`
- [input_success](modules/input_success.md) - `crates/core/src/tests/input/success.rs`
- [inspect-proof](modules/inspect-proof.md) - `crates/host/src/bin/inspect-proof.rs`
- [jobs](modules/jobs.md) - `crates/worker/src/jobs.rs`
- [memory_monitor](modules/memory_monitor.md) - `crates/host/src/memory_monitor.rs`
- [msg](modules/msg.md) - `crates/sync-headers/src/msg.rs`
- [mtp_failure](modules/mtp_failure.md) - `crates/core/src/tests/mtp/failure.rs`
- [mtp_src_errors](modules/mtp_src_errors.md) - `crates/mtp/src/errors.rs`
- [mtp_src_lib](modules/mtp_src_lib.md) - `crates/mtp/src/lib.rs`
- [mtp_success](modules/mtp_success.md) - `crates/core/src/tests/mtp/success.rs`
- [not_pod](modules/not_pod.md) - `crates/wiretype/tests/ui/not_pod.rs`
- [observability](modules/observability.md) - `crates/host/src/observability.rs`
- [p2p_sync](modules/p2p_sync.md) - `crates/sync-headers/src/p2p_sync.rs`
- [params_src_lib](modules/params_src_lib.md) - `crates/params/src/lib.rs`
- [params_src_tests_mod](modules/params_src_tests_mod.md) - `crates/params/src/tests/mod.rs`
- [pipeline_input](modules/pipeline_input.md) - `crates/host/src/pipeline/input.rs`
- [pipeline_verify](modules/pipeline_verify.md) - `crates/host/src/pipeline/verify.rs`
- [platform](modules/platform.md) - `crates/core/src/halt/platform.rs`
- [pow_failure](modules/pow_failure.md) - `crates/core/src/tests/pow/failure.rs`
- [pow_success](modules/pow_success.md) - `crates/core/src/tests/pow/success.rs`
- [profile](modules/profile.md) - `crates/core/src/tests/state/profile.rs`
- [public_values](modules/public_values.md) - `crates/core/src/public_values.rs`
- [public_values_failure](modules/public_values_failure.md) - `crates/core/src/tests/public_values/failure.rs`
- [public_values_success](modules/public_values_success.md) - `crates/core/src/tests/public_values/success.rs`
- [recursive_chain](modules/recursive_chain.md) - `crates/host/tests/test_errors/proofs/recursive_chain.rs`
- [retarget_failure](modules/retarget_failure.md) - `crates/core/src/tests/retarget/failure.rs`
- [retarget_success](modules/retarget_success.md) - `crates/core/src/tests/retarget/success.rs`
- [ring](modules/ring.md) - `crates/mtp/src/ring.rs`
- [serialization](modules/serialization.md) - `crates/wiretype/tests/serialization.rs`
- [sha256_const_src_lib](modules/sha256_const_src_lib.md) - `crates/sha256_const/src/lib.rs`
- [sha256_const_src_tests_mod](modules/sha256_const_src_tests_mod.md) - `crates/sha256_const/src/tests/mod.rs`
- [sha256_src_lib](modules/sha256_src_lib.md) - `crates/sha256/src/lib.rs`
- [sha256_src_tests_mod](modules/sha256_src_tests_mod.md) - `crates/sha256/src/tests/mod.rs`
- [sp1-stress](modules/sp1-stress.md) - `crates/host/src/bin/sp1-stress.rs`
- [src_bigint](modules/src_bigint.md) - `crates/types/src/bigint.rs`
- [src_brand](modules/src_brand.md) - `crates/types/src/brand.rs`
- [src_config](modules/src_config.md) - `crates/worker/src/config.rs`
- [src_connection](modules/src_connection.md) - `crates/sync-headers/src/connection.rs`
- [src_db](modules/src_db.md) - `crates/store/src/db.rs`
- [src_input](modules/src_input.md) - `crates/core/src/input.rs`
- [src_peers](modules/src_peers.md) - `crates/sync-headers/src/peers.rs`
- [src_pipeline_artifacts](modules/src_pipeline_artifacts.md) - `crates/host/src/pipeline/artifacts.rs`
- [src_pipeline_diagnostics](modules/src_pipeline_diagnostics.md) - `crates/host/src/pipeline/diagnostics.rs`
- [src_pipeline_proof_mod](modules/src_pipeline_proof_mod.md) - `crates/host/src/pipeline/proof/mod.rs`
- [src_protocol](modules/src_protocol.md) - `crates/controller/src/protocol.rs`
- [src_run_id](modules/src_run_id.md) - `crates/types/src/run_id.rs`
- [src_runtime](modules/src_runtime.md) - `crates/controller/src/runtime.rs`
- [src_storage](modules/src_storage.md) - `crates/controller/src/storage.rs`
- [src_tests_header](modules/src_tests_header.md) - `crates/types/src/tests/header.rs`
- [src_tests_input_mod](modules/src_tests_input_mod.md) - `crates/core/src/tests/input/mod.rs`
- [src_tests_mtp_mod](modules/src_tests_mtp_mod.md) - `crates/core/src/tests/mtp/mod.rs`
- [src_tests_pipeline_mod](modules/src_tests_pipeline_mod.md) - `crates/host/src/tests/pipeline/mod.rs`
- [src_tests_pow_mod](modules/src_tests_pow_mod.md) - `crates/core/src/tests/pow/mod.rs`
- [src_tests_public_values_mod](modules/src_tests_public_values_mod.md) - `crates/core/src/tests/public_values/mod.rs`
- [src_tests_retarget_mod](modules/src_tests_retarget_mod.md) - `crates/core/src/tests/retarget/mod.rs`
- [src_tests_state_mod](modules/src_tests_state_mod.md) - `crates/core/src/tests/state/mod.rs`
- [src_tests_validation](modules/src_tests_validation.md) - `crates/sync-headers/src/tests/validation.rs`
- [src_types](modules/src_types.md) - `crates/sync-headers/src/types.rs`
- [src_verify](modules/src_verify.md) - `crates/controller/src/verify.rs`
- [state](modules/state.md) - `crates/core/src/state.rs`
- [state_failure](modules/state_failure.md) - `crates/core/src/tests/state/failure.rs`
- [state_success](modules/state_success.md) - `crates/core/src/tests/state/success.rs`
- [store_src_lib](modules/store_src_lib.md) - `crates/store/src/lib.rs`
- [store_src_tests_mod](modules/store_src_tests_mod.md) - `crates/store/src/tests/mod.rs`
- [summarize-prove-spans](modules/summarize-prove-spans.md) - `scripts/summarize-prove-spans.py`
- [sync-headers_src_lib](modules/sync-headers_src_lib.md) - `crates/sync-headers/src/lib.rs`
- [sync-headers_src_tests_mod](modules/sync-headers_src_tests_mod.md) - `crates/sync-headers/src/tests/mod.rs`
- [sync-headers_src_validation](modules/sync-headers_src_validation.md) - `crates/sync-headers/src/validation.rs`
- [tampered_witness](modules/tampered_witness.md) - `crates/host/tests/test_errors/proofs/tampered_witness.rs`
- [target_triplet](modules/target_triplet.md) - `crates/types/src/target_triplet.rs`
- [test_errors](modules/test_errors.md) - `crates/host/tests/test_errors.rs`
- [test_utils](modules/test_utils.md) - `crates/store/src/tests/test_utils.rs`
- [tests](modules/tests.md) - `crates/mtp/src/tests.rs`
- [tests_bigint](modules/tests_bigint.md) - `crates/types/src/tests/bigint.rs`
- [tests_brand](modules/tests_brand.md) - `crates/types/src/tests/brand.rs`
- [tests_config](modules/tests_config.md) - `crates/worker/src/tests/config.rs`
- [tests_connection](modules/tests_connection.md) - `crates/sync-headers/src/tests/connection.rs`
- [tests_db](modules/tests_db.md) - `crates/store/src/tests/db.rs`
- [tests_peers](modules/tests_peers.md) - `crates/sync-headers/src/tests/peers.rs`
- [tests_pipeline_artifacts](modules/tests_pipeline_artifacts.md) - `crates/host/src/tests/pipeline/artifacts.rs`
- [tests_pipeline_diagnostics](modules/tests_pipeline_diagnostics.md) - `crates/host/src/tests/pipeline/diagnostics.rs`
- [tests_protocol](modules/tests_protocol.md) - `crates/controller/src/tests/protocol.rs`
- [tests_run_id](modules/tests_run_id.md) - `crates/types/src/tests/run_id.rs`
- [tests_runtime](modules/tests_runtime.md) - `crates/controller/src/tests/runtime.rs`
- [tests_storage](modules/tests_storage.md) - `crates/controller/src/tests/storage.rs`
- [tests_test_errors_proofs_mod](modules/tests_test_errors_proofs_mod.md) - `crates/host/tests/test_errors/proofs/mod.rs`
- [tests_types](modules/tests_types.md) - `crates/sync-headers/src/tests/types.rs`
- [types_src_header](modules/types_src_header.md) - `crates/types/src/header.rs`
- [types_src_lib](modules/types_src_lib.md) - `crates/types/src/lib.rs`
- [types_src_tests_mod](modules/types_src_tests_mod.md) - `crates/types/src/tests/mod.rs`
- [ui](modules/ui.md) - `crates/wiretype/tests/ui.rs`
- [util](modules/util.md) - `crates/host/src/util.rs`
- [wiretype_macro_src_lib](modules/wiretype_macro_src_lib.md) - `crates/wiretype_macro/src/lib.rs`
- [wiretype_src_lib](modules/wiretype_src_lib.md) - `crates/wiretype/src/lib.rs`
- [worker_src_lib](modules/worker_src_lib.md) - `crates/worker/src/lib.rs`
- [worker_src_tests_mod](modules/worker_src_tests_mod.md) - `crates/worker/src/tests/mod.rs`
- [zkvm](modules/zkvm.md) - `crates/core/src/halt/zkvm.rs`

<a id="entry-point-flows"></a>
<a id="user-flows"></a>
## Entry-point flows <!-- llm-wiki-generated:index:entry-point-flows -->

**process**

- [process-generate-epoch-profile-fixture](flows/process-generate-epoch-profile-fixture.md) - entry: `main`
- [process-summarize-prove-spans](flows/process-summarize-prove-spans.md) - entry: `main`

## Infrastructure

- [GitHub Actions: CI](infrastructure/_github_workflows_ci_yml.md) - github_actions
- [GitHub Actions: docker](infrastructure/_github_workflows_docker_yml.md) - github_actions
- [docker_Dockerfile_vast-cuda](infrastructure/docker_Dockerfile_vast-cuda.md) - dockerfile

## Dependency Architecture

- [Dependencies](dependencies.md)
- [Load order](load-order.md)

## Log

- [Architectural log](log.md)

## Notes

zkpow proves the Bitcoin header chain inside the SP1 zkVM. `zkpow-cli sync` pulls headers from P2P peers into `headers.db` ([db](modules/src_db.md)). The controller picks the next batch, and the host runs the guest program, which applies the batch to a [`State`](entities/State.md) and commits [`PublicValues`](entities/PublicValues.md). Each batch's proof also verifies the proof before it.

Start with the consensus core: [params](modules/params_src_lib.md) defines what a network is, [core](modules/core_src_lib.md) runs the state transition over it, and [mtp](modules/mtp_src_lib.md) tracks the median-time-past window.

This page lists every generated page. The source set is fixed by `.llm-wiki/source-selection.json`: `crates/`, `scripts/`, `docker/`, `.github/`, and the root `Cargo.toml`. Shell scripts are listed as unsupported and have no pages.

## About zkpow

zkpow proves the Bitcoin header chain inside the SP1 zkVM. `zkpow-cli sync` pulls headers from P2P peers into `headers.db` ([db](modules/src_db.md)). The controller picks the next batch, and the host runs the guest program, which applies the batch to a [`State`](entities/State.md) and commits [`PublicValues`](entities/PublicValues.md). Each batch's proof also verifies the proof before it.

Start with the consensus core: [params](modules/params_src_lib.md) defines what a network is, [core](modules/core_src_lib.md) runs the state transition over it, and [mtp](modules/mtp_src_lib.md) tracks the median-time-past window.

The source set is fixed by `.llm-wiki/source-selection.json`: `crates/`, `scripts/`, `docker/`, `.github/`, and the root `Cargo.toml`. Shell scripts are listed as unsupported and have no pages.

## Bootstrap Remainder

Pages still on generated or docstring-only descriptions are listed in the [remainder backlog](bootstrap-remainder.md), ranked for the next pass.
