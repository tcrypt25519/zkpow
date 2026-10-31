# Bootstrap Remainder Backlog

Generated: 2026-10-31
Source directory: .
Wiki directory: docs/llm_wiki
Bootstrap command: `llm-wiki bootstrap --src-dir . --wiki-dir docs/llm_wiki --depth full --format json --source-selection .llm-wiki/source-selection.json`
Bootstrap summary: not kept in the repository; rerun the command above with `--format json` against an empty target to reproduce it.
Semantic budget: P0 complete plus top 30 P1 module/entity pages
Ranking policy: fan_in * 100 + cycle_bonus * 25 + fan_out * 5 + entrypoint_bonus * 20
Last validation: `llm-wiki lint --strict --profile --src-dir . --wiki-dir docs/llm_wiki --source-selection .llm-wiki/source-selection.json` exited 0

## Ranking Input

The extractor resolved one internal edge for the Rust workspace, so every module's `fan_in` and `fan_out` was 0 and the formula could not separate pages. `fan_in` here is the crate's count of workspace dependents from the `Cargo.toml` files: params 7, core 6, wiretype 5, types 3, sha256 3, store 3, sync-headers 2, and mtp, controller, worker, and wiretype_macro 1 each. Every module and entity takes its crate's score. No cycles were found, so `cycle_bonus` is 0. `entrypoint_bonus` is 1 for binary mains and the two script flows. Ties break by source path. Test code is P2 whatever its crate score.

## Coverage Notices

| ID | Status | Priority | Page | Source | Rank | Reason deferred | Acceptance |
|---|---|---|---|---|---:|---|---|
| WB-20261031-C001 | open | P2 | — | `docker/entrypoint.sh`, `scripts/*.sh`, `scripts/vast/*.sh` | — | Shell is unsupported; 7 scripts have no pages. | Pages exist once shell extraction is supported, or the scripts are described in a guide. |
| WB-20261031-C002 | open | P2 | `dependencies.md` | `crates/` | — | Cross-crate `use` paths do not resolve to modules, so the module graph is empty. | Fan-in/fan-out show real edges after an extractor fix; then rerank this backlog. |

## Completed In This Pass

| Page | Surface | Rank | Why completed |
|---|---|---:|---|
| `index.md` | overview | P0 | First-reader context. |
| `flows/process-summarize-prove-spans.md` | behavior | P0 | Entry-point flow. |
| `flows/process-generate-epoch-profile-fixture.md` | behavior | P0 | Entry-point flow. |
| `dependencies.md` | notes | P0 | Explains the empty Rust module graph. |
| `load-order.md` | notes | P0 | Explains the path-ordered load order. |
| `infrastructure/_github_workflows_ci_yml.md` | notes | P0 | CI gate. |
| `infrastructure/_github_workflows_docker_yml.md` | notes | P0 | Image publishing. |
| `infrastructure/docker_Dockerfile_vast-cuda.md` | notes | P0 | Prover node image. |
| `modules/consensus.md` | description | 700 | Rewritten. |
| `modules/params_src_lib.md` | description | 700 | Rewritten. |
| `entities/Consensus.md` | description | 700 | Docstring already accurate. |
| `entities/ConsensusParams.md` | description | 700 | Docstring already accurate. |
| `entities/Mainnet.md` | description | 700 | Docstring already accurate. |
| `entities/ShortEpoch.md` | description | 700 | Docstring already accurate. |
| `modules/core_src_errors.md` | description | 600 | Rewritten. |
| `modules/core_src_halt_mod.md` | description | 600 | Rewritten. |
| `modules/platform.md` | description | 600 | Rewritten. |
| `modules/zkvm.md` | description | 600 | Rewritten. |
| `modules/core_src_header.md` | description | 600 | Rewritten. |
| `modules/src_input.md` | description | 600 | Rewritten. |
| `modules/core_src_lib.md` | description | 600 | Rewritten. |
| `modules/public_values.md` | description | 600 | Rewritten. |
| `modules/state.md` | description | 600 | Rewritten. |
| `entities/ApplyError.md` | description | 600 | Docstring already accurate. |
| `entities/ParseError.md` | description | 600 | Docstring already accurate. |
| `entities/ReportableError.md` | description | 600 | Docstring already accurate. |
| `entities/HeaderInput.md` | description | 600 | Docstring already accurate. |
| `entities/ErrorCode.md` | description | 600 | Docstring already accurate. |
| `entities/PublicValues.md` | description | 600 | Docstring already accurate. |
| `entities/ReturnCode.md` | description | 600 | Docstring already accurate. |
| `entities/VerifierKeyDigest.md` | description | 600 | Docstring already accurate. |
| `entities/State.md` | description | 600 | Docstring already accurate. |
| `modules/wiretype_src_lib.md` | description | 500 | Rewritten. |
| `entities/WireType.md` | description | 500 | Rewritten. |
| `modules/sha256_src_lib.md` | description | 300 | Rewritten. |
| `modules/src_db.md` | description | 300 | Rewritten. |
| `modules/store_src_lib.md` | description | 300 | Rewritten. |
| `entities/Error.md` | description | 300 | Rewritten. |

## Open Remainder

| ID | Status | Priority | Page | Source | Rank | Reason deferred | Acceptance |
|---|---|---|---|---|---:|---|---|
| WB-20261031-0001 | open | P1 | `entities/Result.md` | `crates/store/src/db.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0002 | open | P1 | `entities/ResultOpt.md` | `crates/store/src/db.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0003 | open | P1 | `modules/src_bigint.md` | `crates/types/src/bigint.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0004 | open | P1 | `modules/src_brand.md` | `crates/types/src/brand.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0005 | open | P1 | `modules/types_src_header.md` | `crates/types/src/header.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0006 | open | P1 | `modules/types_src_lib.md` | `crates/types/src/lib.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0007 | open | P1 | `modules/src_run_id.md` | `crates/types/src/run_id.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0008 | open | P1 | `entities/TargetTriplet.md` | `crates/types/src/target_triplet.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0009 | open | P1 | `modules/target_triplet.md` | `crates/types/src/target_triplet.rs` | 300 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0010 | open | P1 | `modules/src_connection.md` | `crates/sync-headers/src/connection.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0011 | open | P1 | `modules/error.md` | `crates/sync-headers/src/error.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0012 | open | P1 | `modules/sync-headers_src_lib.md` | `crates/sync-headers/src/lib.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0013 | open | P1 | `modules/msg.md` | `crates/sync-headers/src/msg.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0014 | open | P1 | `modules/p2p_sync.md` | `crates/sync-headers/src/p2p_sync.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0015 | open | P1 | `modules/src_peers.md` | `crates/sync-headers/src/peers.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0016 | open | P1 | `modules/src_types.md` | `crates/sync-headers/src/types.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0017 | open | P1 | `modules/sync-headers_src_validation.md` | `crates/sync-headers/src/validation.rs` | 200 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0018 | open | P1 | `entities/controller_src_lib_BoxError.md` | `crates/controller/src/lib.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0019 | open | P1 | `modules/controller_src_lib.md` | `crates/controller/src/lib.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0020 | open | P1 | `entities/ControllerError.md` | `crates/controller/src/protocol.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0021 | open | P1 | `entities/ErrorKind.md` | `crates/controller/src/protocol.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0022 | open | P1 | `modules/src_protocol.md` | `crates/controller/src/protocol.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0023 | open | P1 | `entities/ControllerConfig.md` | `crates/controller/src/runtime.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0024 | open | P1 | `modules/src_runtime.md` | `crates/controller/src/runtime.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0025 | open | P1 | `entities/ArtifactStore.md` | `crates/controller/src/storage.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0026 | open | P1 | `entities/LocalArtifactStore.md` | `crates/controller/src/storage.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0027 | open | P1 | `modules/src_storage.md` | `crates/controller/src/storage.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0028 | open | P1 | `modules/controller_src_validation.md` | `crates/controller/src/validation.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0029 | open | P1 | `entities/ProofVerifier.md` | `crates/controller/src/verify.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0030 | open | P1 | `modules/src_verify.md` | `crates/controller/src/verify.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0031 | open | P1 | `modules/mtp_src_errors.md` | `crates/mtp/src/errors.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0032 | open | P1 | `modules/mtp_src_lib.md` | `crates/mtp/src/lib.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0033 | open | P1 | `modules/ring.md` | `crates/mtp/src/ring.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0034 | open | P1 | `modules/wiretype_macro_src_lib.md` | `crates/wiretype_macro/src/lib.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0035 | open | P1 | `modules/src_config.md` | `crates/worker/src/config.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0036 | open | P1 | `modules/jobs.md` | `crates/worker/src/jobs.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0037 | open | P1 | `modules/worker_src_lib.md` | `crates/worker/src/lib.rs` | 100 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0038 | open | P1 | `modules/cli_src_main.md` | `crates/cli/src/main.rs` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0039 | open | P1 | `modules/guest_src_main.md` | `crates/guest/src/main.rs` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0040 | open | P1 | `modules/inspect-proof.md` | `crates/host/src/bin/inspect-proof.rs` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0041 | open | P1 | `modules/sp1-stress.md` | `crates/host/src/bin/sp1-stress.rs` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0042 | open | P1 | `modules/host_src_main.md` | `crates/host/src/main.rs` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0043 | open | P1 | `entities/Bucket.md` | `scripts/summarize-prove-spans.py` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0044 | open | P1 | `modules/summarize-prove-spans.md` | `scripts/summarize-prove-spans.py` | 20 | Placeholder on non-test code; outside the top-30 budget. | Replace `## Description` with source-backed prose, run the owning sync, then lint. |
| WB-20261031-0045 | open | P2 | `modules/params_src_tests_mod.md` | `crates/params/src/tests/mod.rs` | 700 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0046 | open | P2 | `modules/halt.md` | `crates/core/src/tests/halt.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0047 | open | P2 | `modules/input_failure.md` | `crates/core/src/tests/input/failure.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0048 | open | P2 | `modules/src_tests_input_mod.md` | `crates/core/src/tests/input/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0049 | open | P2 | `modules/input_success.md` | `crates/core/src/tests/input/success.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0050 | open | P2 | `modules/core_src_tests_mod.md` | `crates/core/src/tests/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0051 | open | P2 | `modules/mtp_failure.md` | `crates/core/src/tests/mtp/failure.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0052 | open | P2 | `modules/src_tests_mtp_mod.md` | `crates/core/src/tests/mtp/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0053 | open | P2 | `modules/mtp_success.md` | `crates/core/src/tests/mtp/success.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0054 | open | P2 | `modules/pow_failure.md` | `crates/core/src/tests/pow/failure.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0055 | open | P2 | `modules/src_tests_pow_mod.md` | `crates/core/src/tests/pow/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0056 | open | P2 | `modules/pow_success.md` | `crates/core/src/tests/pow/success.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0057 | open | P2 | `modules/public_values_failure.md` | `crates/core/src/tests/public_values/failure.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0058 | open | P2 | `modules/src_tests_public_values_mod.md` | `crates/core/src/tests/public_values/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0059 | open | P2 | `modules/public_values_success.md` | `crates/core/src/tests/public_values/success.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0060 | open | P2 | `modules/retarget_failure.md` | `crates/core/src/tests/retarget/failure.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0061 | open | P2 | `modules/src_tests_retarget_mod.md` | `crates/core/src/tests/retarget/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0062 | open | P2 | `modules/retarget_success.md` | `crates/core/src/tests/retarget/success.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0063 | open | P2 | `modules/state_failure.md` | `crates/core/src/tests/state/failure.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0064 | open | P2 | `modules/src_tests_state_mod.md` | `crates/core/src/tests/state/mod.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0065 | open | P2 | `modules/profile.md` | `crates/core/src/tests/state/profile.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0066 | open | P2 | `modules/state_success.md` | `crates/core/src/tests/state/success.rs` | 600 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0067 | open | P2 | `modules/serialization.md` | `crates/wiretype/tests/serialization.rs` | 500 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0068 | open | P2 | `modules/ui.md` | `crates/wiretype/tests/ui.rs` | 500 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0069 | open | P2 | `modules/arguments.md` | `crates/wiretype/tests/ui/arguments.rs` | 500 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0070 | open | P2 | `modules/not_pod.md` | `crates/wiretype/tests/ui/not_pod.rs` | 500 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0071 | open | P2 | `entities/HeaderFixture.md` | `crates/sha256/src/tests/mod.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0072 | open | P2 | `modules/sha256_src_tests_mod.md` | `crates/sha256/src/tests/mod.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0073 | open | P2 | `modules/tests_db.md` | `crates/store/src/tests/db.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0074 | open | P2 | `modules/store_src_tests_mod.md` | `crates/store/src/tests/mod.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0075 | open | P2 | `modules/test_utils.md` | `crates/store/src/tests/test_utils.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0076 | open | P2 | `modules/tests_bigint.md` | `crates/types/src/tests/bigint.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0077 | open | P2 | `modules/tests_brand.md` | `crates/types/src/tests/brand.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0078 | open | P2 | `modules/bytes.md` | `crates/types/src/tests/bytes.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0079 | open | P2 | `modules/src_tests_header.md` | `crates/types/src/tests/header.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0080 | open | P2 | `modules/types_src_tests_mod.md` | `crates/types/src/tests/mod.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0081 | open | P2 | `modules/tests_run_id.md` | `crates/types/src/tests/run_id.rs` | 300 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0082 | open | P2 | `modules/tests_connection.md` | `crates/sync-headers/src/tests/connection.rs` | 200 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0083 | open | P2 | `modules/sync-headers_src_tests_mod.md` | `crates/sync-headers/src/tests/mod.rs` | 200 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0084 | open | P2 | `modules/tests_peers.md` | `crates/sync-headers/src/tests/peers.rs` | 200 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0085 | open | P2 | `modules/tests_types.md` | `crates/sync-headers/src/tests/types.rs` | 200 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0086 | open | P2 | `modules/src_tests_validation.md` | `crates/sync-headers/src/tests/validation.rs` | 200 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0087 | open | P2 | `modules/controller_src_tests_mod.md` | `crates/controller/src/tests/mod.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0088 | open | P2 | `modules/tests_protocol.md` | `crates/controller/src/tests/protocol.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0089 | open | P2 | `modules/tests_runtime.md` | `crates/controller/src/tests/runtime.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0090 | open | P2 | `modules/tests_storage.md` | `crates/controller/src/tests/storage.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0091 | open | P2 | `modules/tests.md` | `crates/mtp/src/tests.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0092 | open | P2 | `modules/tests_config.md` | `crates/worker/src/tests/config.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0093 | open | P2 | `modules/worker_src_tests_mod.md` | `crates/worker/src/tests/mod.rs` | 100 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0094 | open | P2 | `modules/build.md` | `crates/host/build.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0095 | open | P2 | `modules/host_src_lib.md` | `crates/host/src/lib.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0096 | open | P2 | `entities/StageSample.md` | `crates/host/src/memory_monitor.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0097 | open | P2 | `modules/memory_monitor.md` | `crates/host/src/memory_monitor.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0098 | open | P2 | `modules/observability.md` | `crates/host/src/observability.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0099 | open | P2 | `modules/src_pipeline_artifacts.md` | `crates/host/src/pipeline/artifacts.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0100 | open | P2 | `modules/src_pipeline_diagnostics.md` | `crates/host/src/pipeline/diagnostics.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0101 | open | P2 | `modules/execution.md` | `crates/host/src/pipeline/execution.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0102 | open | P2 | `modules/pipeline_input.md` | `crates/host/src/pipeline/input.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0103 | open | P2 | `entities/host_src_pipeline_mod_BoxError.md` | `crates/host/src/pipeline/mod.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0104 | open | P2 | `modules/host_src_pipeline_mod.md` | `crates/host/src/pipeline/mod.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0105 | open | P2 | `modules/compressed.md` | `crates/host/src/pipeline/proof/compressed.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0106 | open | P2 | `modules/src_pipeline_proof_mod.md` | `crates/host/src/pipeline/proof/mod.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0107 | open | P2 | `entities/PreparedProverVerifier.md` | `crates/host/src/pipeline/verify.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0108 | open | P2 | `modules/pipeline_verify.md` | `crates/host/src/pipeline/verify.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0109 | open | P2 | `modules/host_src_tests_mod.md` | `crates/host/src/tests/mod.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0110 | open | P2 | `modules/tests_pipeline_artifacts.md` | `crates/host/src/tests/pipeline/artifacts.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0111 | open | P2 | `modules/tests_pipeline_diagnostics.md` | `crates/host/src/tests/pipeline/diagnostics.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0112 | open | P2 | `modules/src_tests_pipeline_mod.md` | `crates/host/src/tests/pipeline/mod.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0113 | open | P2 | `modules/util.md` | `crates/host/src/util.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0114 | open | P2 | `modules/hash.md` | `crates/host/src/util/hash.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0115 | open | P2 | `modules/test_errors.md` | `crates/host/tests/test_errors.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0116 | open | P2 | `modules/failed_proof.md` | `crates/host/tests/test_errors/proofs/failed_proof.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0117 | open | P2 | `modules/tests_test_errors_proofs_mod.md` | `crates/host/tests/test_errors/proofs/mod.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0118 | open | P2 | `modules/recursive_chain.md` | `crates/host/tests/test_errors/proofs/recursive_chain.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0119 | open | P2 | `modules/tampered_witness.md` | `crates/host/tests/test_errors/proofs/tampered_witness.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0120 | open | P2 | `modules/sha256_const_src_lib.md` | `crates/sha256_const/src/lib.rs` | 0 | Placeholder on leaf code with no workspace dependents. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0121 | open | P2 | `modules/sha256_const_src_tests_mod.md` | `crates/sha256_const/src/tests/mod.rs` | 0 | Test module; low reader value. | Add a one-sentence responsibility if the source makes it clear, sync, then lint. |
| WB-20261031-0122 | open | P3 | `entities/HeaderBatchWitness.md` | `crates/store/src/db.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0123 | open | P3 | `entities/HeaderRecord.md` | `crates/store/src/db.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0124 | open | P3 | `entities/HeaderStore.md` | `crates/store/src/db.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0125 | open | P3 | `entities/RunRecord.md` | `crates/store/src/db.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0126 | open | P3 | `entities/NonMaxU256.md` | `crates/types/src/bigint.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0127 | open | P3 | `entities/u256.md` | `crates/types/src/bigint.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0128 | open | P3 | `entities/types_src_header_Header.md` | `crates/types/src/header.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0129 | open | P3 | `entities/RunId.md` | `crates/types/src/run_id.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0130 | open | P3 | `entities/RunIdError.md` | `crates/types/src/run_id.rs` | 300 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0131 | open | P3 | `entities/Command.md` | `crates/sync-headers/src/connection.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0132 | open | P3 | `entities/PeerConnection.md` | `crates/sync-headers/src/connection.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0133 | open | P3 | `entities/VersionMessage.md` | `crates/sync-headers/src/connection.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0134 | open | P3 | `entities/SyncHeadersError.md` | `crates/sync-headers/src/error.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0135 | open | P3 | `entities/PeerEntry.md` | `crates/sync-headers/src/peers.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0136 | open | P3 | `entities/PeerStatus.md` | `crates/sync-headers/src/peers.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0137 | open | P3 | `entities/PeerStore.md` | `crates/sync-headers/src/peers.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0138 | open | P3 | `entities/ConnectedHeader.md` | `crates/sync-headers/src/types.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0139 | open | P3 | `entities/Network.md` | `crates/sync-headers/src/types.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0140 | open | P3 | `entities/Tip.md` | `crates/sync-headers/src/types.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0141 | open | P3 | `entities/src_types_Header.md` | `crates/sync-headers/src/types.rs` | 200 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0142 | open | P3 | `entities/Batch.md` | `crates/controller/src/protocol.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0143 | open | P3 | `entities/ControllerRuntime.md` | `crates/controller/src/runtime.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0144 | open | P3 | `entities/InsufficientColdHeaders.md` | `crates/mtp/src/errors.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0145 | open | P3 | `entities/InvalidParts.md` | `crates/mtp/src/errors.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0146 | open | P3 | `entities/NotMedian.md` | `crates/mtp/src/errors.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0147 | open | P3 | `entities/Cold.md` | `crates/mtp/src/ring.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0148 | open | P3 | `entities/Ring.md` | `crates/mtp/src/ring.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0149 | open | P3 | `entities/Warm.md` | `crates/mtp/src/ring.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0150 | open | P3 | `entities/Window.md` | `crates/mtp/src/ring.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0151 | open | P3 | `entities/WorkerConfig.md` | `crates/worker/src/config.rs` | 100 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0152 | open | P3 | `modules/generate-epoch-profile-fixture.md` | `scripts/generate-epoch-profile-fixture.py` | 20 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0153 | open | P3 | `entities/PhaseTiming.md` | `crates/host/src/pipeline/diagnostics.rs` | 0 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0154 | open | P3 | `entities/ProofGenerationConfig.md` | `crates/host/src/pipeline/mod.rs` | 0 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |
| WB-20261031-0155 | open | P3 | `entities/ProverBackend.md` | `crates/host/src/pipeline/mod.rs` | 0 | Description is the copied docstring; adequate, could add context. | Optionally add context beyond the docstring, sync, then lint. |

## Item Details

### WB-20261031-0001

- Page: `entities/Result.md`
- Source: `crates/store/src/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `Result` in `crates/store/src/db.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0002

- Page: `entities/ResultOpt.md`
- Source: `crates/store/src/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `ResultOpt` in `crates/store/src/db.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0003

- Page: `modules/src_bigint.md`
- Source: `crates/types/src/bigint.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/bigint.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/bigint.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0004

- Page: `modules/src_brand.md`
- Source: `crates/types/src/brand.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/brand.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/brand.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0005

- Page: `modules/types_src_header.md`
- Source: `crates/types/src/header.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/header.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/header.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0006

- Page: `modules/types_src_lib.md`
- Source: `crates/types/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0007

- Page: `modules/src_run_id.md`
- Source: `crates/types/src/run_id.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/run_id.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/run_id.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0008

- Page: `entities/TargetTriplet.md`
- Source: `crates/types/src/target_triplet.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `TargetTriplet` in `crates/types/src/target_triplet.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/target_triplet.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0009

- Page: `modules/target_triplet.md`
- Source: `crates/types/src/target_triplet.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/target_triplet.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/target_triplet.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0010

- Page: `modules/src_connection.md`
- Source: `crates/sync-headers/src/connection.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/connection.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/connection.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0011

- Page: `modules/error.md`
- Source: `crates/sync-headers/src/error.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/error.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/error.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0012

- Page: `modules/sync-headers_src_lib.md`
- Source: `crates/sync-headers/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0013

- Page: `modules/msg.md`
- Source: `crates/sync-headers/src/msg.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/msg.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/msg.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0014

- Page: `modules/p2p_sync.md`
- Source: `crates/sync-headers/src/p2p_sync.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/p2p_sync.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/p2p_sync.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0015

- Page: `modules/src_peers.md`
- Source: `crates/sync-headers/src/peers.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/peers.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/peers.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0016

- Page: `modules/src_types.md`
- Source: `crates/sync-headers/src/types.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/types.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/types.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0017

- Page: `modules/sync-headers_src_validation.md`
- Source: `crates/sync-headers/src/validation.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/validation.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/validation.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0018

- Page: `entities/controller_src_lib_BoxError.md`
- Source: `crates/controller/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `BoxError` in `crates/controller/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0019

- Page: `modules/controller_src_lib.md`
- Source: `crates/controller/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0020

- Page: `entities/ControllerError.md`
- Source: `crates/controller/src/protocol.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `ControllerError` in `crates/controller/src/protocol.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/protocol.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0021

- Page: `entities/ErrorKind.md`
- Source: `crates/controller/src/protocol.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `ErrorKind` in `crates/controller/src/protocol.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/protocol.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0022

- Page: `modules/src_protocol.md`
- Source: `crates/controller/src/protocol.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/protocol.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/protocol.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0023

- Page: `entities/ControllerConfig.md`
- Source: `crates/controller/src/runtime.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `ControllerConfig` in `crates/controller/src/runtime.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/runtime.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0024

- Page: `modules/src_runtime.md`
- Source: `crates/controller/src/runtime.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/runtime.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/runtime.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0025

- Page: `entities/ArtifactStore.md`
- Source: `crates/controller/src/storage.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `ArtifactStore` in `crates/controller/src/storage.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/storage.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0026

- Page: `entities/LocalArtifactStore.md`
- Source: `crates/controller/src/storage.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `LocalArtifactStore` in `crates/controller/src/storage.rs`.
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/storage.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0027

- Page: `modules/src_storage.md`
- Source: `crates/controller/src/storage.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/storage.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/storage.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0028

- Page: `modules/controller_src_validation.md`
- Source: `crates/controller/src/validation.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/validation.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/validation.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0029

- Page: `entities/ProofVerifier.md`
- Source: `crates/controller/src/verify.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `ProofVerifier` in `crates/controller/src/verify.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/verify.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0030

- Page: `modules/src_verify.md`
- Source: `crates/controller/src/verify.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/verify.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/verify.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0031

- Page: `modules/mtp_src_errors.md`
- Source: `crates/mtp/src/errors.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/mtp/src/errors.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/errors.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0032

- Page: `modules/mtp_src_lib.md`
- Source: `crates/mtp/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/mtp/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0033

- Page: `modules/ring.md`
- Source: `crates/mtp/src/ring.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/mtp/src/ring.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/ring.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0034

- Page: `modules/wiretype_macro_src_lib.md`
- Source: `crates/wiretype_macro/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/wiretype_macro/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/wiretype_macro/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0035

- Page: `modules/src_config.md`
- Source: `crates/worker/src/config.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/worker/src/config.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/worker/src/config.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0036

- Page: `modules/jobs.md`
- Source: `crates/worker/src/jobs.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/worker/src/jobs.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/worker/src/jobs.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0037

- Page: `modules/worker_src_lib.md`
- Source: `crates/worker/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/worker/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/worker/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0038

- Page: `modules/cli_src_main.md`
- Source: `crates/cli/src/main.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `crates/cli/src/main.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/cli/src/main.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0039

- Page: `modules/guest_src_main.md`
- Source: `crates/guest/src/main.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `crates/guest/src/main.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/guest/src/main.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0040

- Page: `modules/inspect-proof.md`
- Source: `crates/host/src/bin/inspect-proof.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `crates/host/src/bin/inspect-proof.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/bin/inspect-proof.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0041

- Page: `modules/sp1-stress.md`
- Source: `crates/host/src/bin/sp1-stress.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `crates/host/src/bin/sp1-stress.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/bin/sp1-stress.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0042

- Page: `modules/host_src_main.md`
- Source: `crates/host/src/main.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `crates/host/src/main.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/main.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0043

- Page: `entities/Bucket.md`
- Source: `scripts/summarize-prove-spans.py`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `Bucket` in `scripts/summarize-prove-spans.py`._
- Suggested context: the source file and `llm-wiki query` for `scripts/summarize-prove-spans.py` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0044

- Page: `modules/summarize-prove-spans.md`
- Source: `scripts/summarize-prove-spans.py`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: _Auto-generated from `scripts/summarize-prove-spans.py`._
- Suggested context: the source file and `llm-wiki query` for `scripts/summarize-prove-spans.py` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0045

- Page: `modules/params_src_tests_mod.md`
- Source: `crates/params/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=7, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/params/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/params/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0046

- Page: `modules/halt.md`
- Source: `crates/core/src/tests/halt.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/halt.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/halt.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0047

- Page: `modules/input_failure.md`
- Source: `crates/core/src/tests/input/failure.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/input/failure.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/input/failure.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0048

- Page: `modules/src_tests_input_mod.md`
- Source: `crates/core/src/tests/input/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/input/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/input/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0049

- Page: `modules/input_success.md`
- Source: `crates/core/src/tests/input/success.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/input/success.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/input/success.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0050

- Page: `modules/core_src_tests_mod.md`
- Source: `crates/core/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0051

- Page: `modules/mtp_failure.md`
- Source: `crates/core/src/tests/mtp/failure.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/mtp/failure.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/mtp/failure.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0052

- Page: `modules/src_tests_mtp_mod.md`
- Source: `crates/core/src/tests/mtp/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/mtp/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/mtp/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0053

- Page: `modules/mtp_success.md`
- Source: `crates/core/src/tests/mtp/success.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/mtp/success.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/mtp/success.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0054

- Page: `modules/pow_failure.md`
- Source: `crates/core/src/tests/pow/failure.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/pow/failure.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/pow/failure.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0055

- Page: `modules/src_tests_pow_mod.md`
- Source: `crates/core/src/tests/pow/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/pow/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/pow/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0056

- Page: `modules/pow_success.md`
- Source: `crates/core/src/tests/pow/success.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/pow/success.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/pow/success.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0057

- Page: `modules/public_values_failure.md`
- Source: `crates/core/src/tests/public_values/failure.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/public_values/failure.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/public_values/failure.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0058

- Page: `modules/src_tests_public_values_mod.md`
- Source: `crates/core/src/tests/public_values/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/public_values/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/public_values/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0059

- Page: `modules/public_values_success.md`
- Source: `crates/core/src/tests/public_values/success.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/public_values/success.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/public_values/success.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0060

- Page: `modules/retarget_failure.md`
- Source: `crates/core/src/tests/retarget/failure.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/retarget/failure.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/retarget/failure.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0061

- Page: `modules/src_tests_retarget_mod.md`
- Source: `crates/core/src/tests/retarget/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/retarget/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/retarget/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0062

- Page: `modules/retarget_success.md`
- Source: `crates/core/src/tests/retarget/success.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/retarget/success.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/retarget/success.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0063

- Page: `modules/state_failure.md`
- Source: `crates/core/src/tests/state/failure.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/state/failure.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/state/failure.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0064

- Page: `modules/src_tests_state_mod.md`
- Source: `crates/core/src/tests/state/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/state/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/state/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0065

- Page: `modules/profile.md`
- Source: `crates/core/src/tests/state/profile.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/state/profile.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/state/profile.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0066

- Page: `modules/state_success.md`
- Source: `crates/core/src/tests/state/success.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=6, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/core/src/tests/state/success.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/core/src/tests/state/success.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0067

- Page: `modules/serialization.md`
- Source: `crates/wiretype/tests/serialization.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=5, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/wiretype/tests/serialization.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/wiretype/tests/serialization.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0068

- Page: `modules/ui.md`
- Source: `crates/wiretype/tests/ui.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=5, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/wiretype/tests/ui.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/wiretype/tests/ui.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0069

- Page: `modules/arguments.md`
- Source: `crates/wiretype/tests/ui/arguments.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=5, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/wiretype/tests/ui/arguments.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/wiretype/tests/ui/arguments.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0070

- Page: `modules/not_pod.md`
- Source: `crates/wiretype/tests/ui/not_pod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=5, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/wiretype/tests/ui/not_pod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/wiretype/tests/ui/not_pod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0071

- Page: `entities/HeaderFixture.md`
- Source: `crates/sha256/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `HeaderFixture` in `crates/sha256/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sha256/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0072

- Page: `modules/sha256_src_tests_mod.md`
- Source: `crates/sha256/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sha256/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sha256/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0073

- Page: `modules/tests_db.md`
- Source: `crates/store/src/tests/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/store/src/tests/db.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/tests/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0074

- Page: `modules/store_src_tests_mod.md`
- Source: `crates/store/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/store/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0075

- Page: `modules/test_utils.md`
- Source: `crates/store/src/tests/test_utils.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/store/src/tests/test_utils.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/tests/test_utils.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0076

- Page: `modules/tests_bigint.md`
- Source: `crates/types/src/tests/bigint.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/tests/bigint.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/tests/bigint.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0077

- Page: `modules/tests_brand.md`
- Source: `crates/types/src/tests/brand.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/tests/brand.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/tests/brand.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0078

- Page: `modules/bytes.md`
- Source: `crates/types/src/tests/bytes.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/tests/bytes.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/tests/bytes.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0079

- Page: `modules/src_tests_header.md`
- Source: `crates/types/src/tests/header.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/tests/header.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/tests/header.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0080

- Page: `modules/types_src_tests_mod.md`
- Source: `crates/types/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0081

- Page: `modules/tests_run_id.md`
- Source: `crates/types/src/tests/run_id.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/types/src/tests/run_id.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/tests/run_id.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0082

- Page: `modules/tests_connection.md`
- Source: `crates/sync-headers/src/tests/connection.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/tests/connection.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/tests/connection.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0083

- Page: `modules/sync-headers_src_tests_mod.md`
- Source: `crates/sync-headers/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0084

- Page: `modules/tests_peers.md`
- Source: `crates/sync-headers/src/tests/peers.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/tests/peers.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/tests/peers.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0085

- Page: `modules/tests_types.md`
- Source: `crates/sync-headers/src/tests/types.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/tests/types.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/tests/types.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0086

- Page: `modules/src_tests_validation.md`
- Source: `crates/sync-headers/src/tests/validation.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sync-headers/src/tests/validation.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/tests/validation.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0087

- Page: `modules/controller_src_tests_mod.md`
- Source: `crates/controller/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0088

- Page: `modules/tests_protocol.md`
- Source: `crates/controller/src/tests/protocol.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/tests/protocol.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/tests/protocol.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0089

- Page: `modules/tests_runtime.md`
- Source: `crates/controller/src/tests/runtime.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/tests/runtime.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/tests/runtime.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0090

- Page: `modules/tests_storage.md`
- Source: `crates/controller/src/tests/storage.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/controller/src/tests/storage.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/tests/storage.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0091

- Page: `modules/tests.md`
- Source: `crates/mtp/src/tests.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/mtp/src/tests.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/tests.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0092

- Page: `modules/tests_config.md`
- Source: `crates/worker/src/tests/config.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/worker/src/tests/config.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/worker/src/tests/config.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0093

- Page: `modules/worker_src_tests_mod.md`
- Source: `crates/worker/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/worker/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/worker/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0094

- Page: `modules/build.md`
- Source: `crates/host/build.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/build.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/build.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0095

- Page: `modules/host_src_lib.md`
- Source: `crates/host/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0096

- Page: `entities/StageSample.md`
- Source: `crates/host/src/memory_monitor.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `StageSample` in `crates/host/src/memory_monitor.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/memory_monitor.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0097

- Page: `modules/memory_monitor.md`
- Source: `crates/host/src/memory_monitor.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/memory_monitor.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/memory_monitor.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0098

- Page: `modules/observability.md`
- Source: `crates/host/src/observability.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/observability.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/observability.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0099

- Page: `modules/src_pipeline_artifacts.md`
- Source: `crates/host/src/pipeline/artifacts.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/artifacts.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/artifacts.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0100

- Page: `modules/src_pipeline_diagnostics.md`
- Source: `crates/host/src/pipeline/diagnostics.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/diagnostics.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/diagnostics.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0101

- Page: `modules/execution.md`
- Source: `crates/host/src/pipeline/execution.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/execution.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/execution.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0102

- Page: `modules/pipeline_input.md`
- Source: `crates/host/src/pipeline/input.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/input.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/input.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0103

- Page: `entities/host_src_pipeline_mod_BoxError.md`
- Source: `crates/host/src/pipeline/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `BoxError` in `crates/host/src/pipeline/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0104

- Page: `modules/host_src_pipeline_mod.md`
- Source: `crates/host/src/pipeline/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0105

- Page: `modules/compressed.md`
- Source: `crates/host/src/pipeline/proof/compressed.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/proof/compressed.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/proof/compressed.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0106

- Page: `modules/src_pipeline_proof_mod.md`
- Source: `crates/host/src/pipeline/proof/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/proof/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/proof/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0107

- Page: `entities/PreparedProverVerifier.md`
- Source: `crates/host/src/pipeline/verify.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `PreparedProverVerifier` in `crates/host/src/pipeline/verif
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/verify.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0108

- Page: `modules/pipeline_verify.md`
- Source: `crates/host/src/pipeline/verify.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/pipeline/verify.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/verify.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0109

- Page: `modules/host_src_tests_mod.md`
- Source: `crates/host/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0110

- Page: `modules/tests_pipeline_artifacts.md`
- Source: `crates/host/src/tests/pipeline/artifacts.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/tests/pipeline/artifacts.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/tests/pipeline/artifacts.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0111

- Page: `modules/tests_pipeline_diagnostics.md`
- Source: `crates/host/src/tests/pipeline/diagnostics.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/tests/pipeline/diagnostics.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/tests/pipeline/diagnostics.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0112

- Page: `modules/src_tests_pipeline_mod.md`
- Source: `crates/host/src/tests/pipeline/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/tests/pipeline/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/tests/pipeline/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0113

- Page: `modules/util.md`
- Source: `crates/host/src/util.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/util.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/util.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0114

- Page: `modules/hash.md`
- Source: `crates/host/src/util/hash.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/src/util/hash.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/util/hash.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0115

- Page: `modules/test_errors.md`
- Source: `crates/host/tests/test_errors.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/tests/test_errors.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/tests/test_errors.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0116

- Page: `modules/failed_proof.md`
- Source: `crates/host/tests/test_errors/proofs/failed_proof.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/tests/test_errors/proofs/failed_proof.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/tests/test_errors/proofs/failed_proof.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0117

- Page: `modules/tests_test_errors_proofs_mod.md`
- Source: `crates/host/tests/test_errors/proofs/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/tests/test_errors/proofs/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/tests/test_errors/proofs/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0118

- Page: `modules/recursive_chain.md`
- Source: `crates/host/tests/test_errors/proofs/recursive_chain.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/tests/test_errors/proofs/recursive_chain.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/host/tests/test_errors/proofs/recursive_chain.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0119

- Page: `modules/tampered_witness.md`
- Source: `crates/host/tests/test_errors/proofs/tampered_witness.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/host/tests/test_errors/proofs/tampered_witness.rs`.
- Suggested context: the source file and `llm-wiki query` for `crates/host/tests/test_errors/proofs/tampered_witness.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0120

- Page: `modules/sha256_const_src_lib.md`
- Source: `crates/sha256_const/src/lib.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sha256_const/src/lib.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sha256_const/src/lib.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0121

- Page: `modules/sha256_const_src_tests_mod.md`
- Source: `crates/sha256_const/src/tests/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: _Auto-generated from `crates/sha256_const/src/tests/mod.rs`._
- Suggested context: the source file and `llm-wiki query` for `crates/sha256_const/src/tests/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0122

- Page: `entities/HeaderBatchWitness.md`
- Source: `crates/store/src/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The SP1 stdin witness for a batch: the new headers plus their median-time-past h
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0123

- Page: `entities/HeaderRecord.md`
- Source: `crates/store/src/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A single `headers` row, decoded into typed fields.
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0124

- Page: `entities/HeaderStore.md`
- Source: `crates/store/src/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The SQLite header store.
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0125

- Page: `entities/RunRecord.md`
- Source: `crates/store/src/db.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: One IVC chain's verified progress, keyed by `run_id` in the `runs` table.
- Suggested context: the source file and `llm-wiki query` for `crates/store/src/db.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0126

- Page: `entities/NonMaxU256.md`
- Source: `crates/types/src/bigint.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A [`u256`] excluding the all-ones maximum.
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/bigint.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0127

- Page: `entities/u256.md`
- Source: `crates/types/src/bigint.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A 256-bit unsigned integer stored as four little-endian u64 limbs.
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/bigint.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0128

- Page: `entities/types_src_header_Header.md`
- Source: `crates/types/src/header.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A Bitcoin block header
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/header.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0129

- Page: `entities/RunId.md`
- Source: `crates/types/src/run_id.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A strongly-typed 8-byte ASCII alphanumeric identifier for proving runs.
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/run_id.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0130

- Page: `entities/RunIdError.md`
- Source: `crates/types/src/run_id.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=3, fan_out=0, cycle=false, entrypoint_related=false
- Current text: An error representing a failed attempt to construct a [`RunId`].
- Suggested context: the source file and `llm-wiki query` for `crates/types/src/run_id.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0131

- Page: `entities/Command.md`
- Source: `crates/sync-headers/src/connection.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A Bitcoin P2P network message command.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/connection.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0132

- Page: `entities/PeerConnection.md`
- Source: `crates/sync-headers/src/connection.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A P2P connection to a Bitcoin peer.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/connection.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0133

- Page: `entities/VersionMessage.md`
- Source: `crates/sync-headers/src/connection.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Simplified version message structure.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/connection.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0134

- Page: `entities/SyncHeadersError.md`
- Source: `crates/sync-headers/src/error.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The outcome of a sync-headers run: either we reached the tip or we hit
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/error.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0135

- Page: `entities/PeerEntry.md`
- Source: `crates/sync-headers/src/peers.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A known peer address with its current status and a timestamp.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/peers.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0136

- Page: `entities/PeerStatus.md`
- Source: `crates/sync-headers/src/peers.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Lifecycle status of a known peer address.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/peers.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0137

- Page: `entities/PeerStore.md`
- Source: `crates/sync-headers/src/peers.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A collection of discovered peers with status tracking.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/peers.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0138

- Page: `entities/ConnectedHeader.md`
- Source: `crates/sync-headers/src/types.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A header linked onto the chain, with the chain values derived at its height.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/types.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0139

- Page: `entities/Network.md`
- Source: `crates/sync-headers/src/types.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Network selection for Bitcoin consensus.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/types.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0140

- Page: `entities/Tip.md`
- Source: `crates/sync-headers/src/types.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The tip of the best known chain.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/types.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0141

- Page: `entities/src_types_Header.md`
- Source: `crates/sync-headers/src/types.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=2, fan_out=0, cycle=false, entrypoint_related=false
- Current text: A raw block header received from the network.
- Suggested context: the source file and `llm-wiki query` for `crates/sync-headers/src/types.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0142

- Page: `entities/Batch.md`
- Source: `crates/controller/src/protocol.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Everything required to prove the next controller-selected batch.
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/protocol.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0143

- Page: `entities/ControllerRuntime.md`
- Source: `crates/controller/src/runtime.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The controller handle. Clones share one core.
- Suggested context: the source file and `llm-wiki query` for `crates/controller/src/runtime.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0144

- Page: `entities/InsufficientColdHeaders.md`
- Source: `crates/mtp/src/errors.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Why a header slice cannot be split into the cold prefix and warm remainder.
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/errors.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0145

- Page: `entities/InvalidParts.md`
- Source: `crates/mtp/src/errors.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Why persisted ring parts could not be rebuilt in the requested fill state.
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/errors.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0146

- Page: `entities/NotMedian.md`
- Source: `crates/mtp/src/errors.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Why a claimed median is not the window's median.
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/errors.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0147

- Page: `entities/Cold.md`
- Source: `crates/mtp/src/ring.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The ring holds fewer timestamps than its capacity.
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/ring.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0148

- Page: `entities/Ring.md`
- Source: `crates/mtp/src/ring.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The last `N` timestamps, oldest overwritten first. `ordered_run` is the length o
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/ring.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0149

- Page: `entities/Warm.md`
- Source: `crates/mtp/src/ring.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Every slot in the ring holds a timestamp.
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/ring.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0150

- Page: `entities/Window.md`
- Source: `crates/mtp/src/ring.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: The consensus timestamp window.
- Suggested context: the source file and `llm-wiki query` for `crates/mtp/src/ring.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0151

- Page: `entities/WorkerConfig.md`
- Source: `crates/worker/src/config.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=1, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Resolved configuration for the CPU proving worker.
- Suggested context: the source file and `llm-wiki query` for `crates/worker/src/config.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0152

- Page: `modules/generate-epoch-profile-fixture.md`
- Source: `scripts/generate-epoch-profile-fixture.py`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=true
- Current text: Export real mainnet HeaderInput records for the opt-in zkVM epoch profile.
- Suggested context: the source file and `llm-wiki query` for `scripts/generate-epoch-profile-fixture.py` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0153

- Page: `entities/PhaseTiming.md`
- Source: `crates/host/src/pipeline/diagnostics.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: One labelled phase's summed wall-clock time within a batch.
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/diagnostics.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0154

- Page: `entities/ProofGenerationConfig.md`
- Source: `crates/host/src/pipeline/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: One proving session's settings, read from `ZKPOW_*` variables by [`input::config
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.

### WB-20261031-0155

- Page: `entities/ProverBackend.md`
- Source: `crates/host/src/pipeline/mod.rs`
- Surface: `## Description`
- Centrality: crate_fan_in=0, fan_out=0, cycle=false, entrypoint_related=false
- Current text: Which SP1 prover the host uses. `Mock` executes without real proving.
- Suggested context: the source file and `llm-wiki query` for `crates/host/src/pipeline/mod.rs` with `--source-selection .llm-wiki/source-selection.json`.
