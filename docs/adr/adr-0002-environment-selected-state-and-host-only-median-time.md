---
title: "ADR-0002: Select state and median-time behavior by environment"
status: "Accepted"
date: "2026-05-10"
authors: "tcrypt (maintainer)"
tags: ["architecture", "decision", "rust", "consensus"]
supersedes: ""
superseded_by: ""
---

# Status

Accepted

# Context

The core crate originally exposed a generic `State<E>` and two median-time-past implementations with different names. Callers were responsible for choosing the right method. In practice, that created an insidious failure mode:

- both versions compiled cleanly
- both versions were callable from the same places
- the guest-side version looked suspicious because it accepted extra witness data
- coding agents repeatedly treated the guest path as a bug and "optimized" it by switching to the host-style median calculation
- that change was not a neutral refactor; on the guest it doubled runtime or worse before the later rewrite

That experience exposed two separate requirements:

1. The guest and host need different available methods.
2. The choice between guest and host behavior must be enforced by the type system, not by convention or caller discipline.

The host also has a different reason to use the standard implementation. The host does not need the guest-style witness-assisted median path because the medians are now precomputed in the database. Using the standard host implementation gives operators an independent, reviewable cross-check. If the host diverges from the guest format, that divergence should be immediately visible and deliberate.

# Decision Drivers

- **Guest correctness over convenience**: the guest must use the constrained witness-driven path, not a host-style helper that only becomes obviously wrong after a runtime benchmark.
- **Compile-time friction**: the wrong method should not be a plausible default for callers.
- **Closed environment set**: only the crate should define the recognized execution environments.
- **Simple public API**: callers should import `State` and `Input`, not reason about `State<E>`.
- **Host-side operator sanity check**: the host should retain a standard median implementation that is easy to review and compare against external references.

# Decision

Split the environment-specific implementation behind a private inner state and expose only a public selected alias:

- keep the concrete generic struct private inside `crates/core/src/env/`
- define the guest/host environment markers inside the crate
- seal the environment trait so downstream code cannot invent new environments
- make the public `State` alias resolve to the guest environment when `host` is off
- make the public `State` alias resolve to the host environment when `host` is on
- put host-only methods, such as the standard median-time-past sort, in `env/host.rs`
- keep the guest-facing API surface free of environment parameters and environment-conversion helpers

In effect, callers see a single `State` type whose behavior is selected by the build feature, not by ad hoc method choice.

# Rationale

The old design had a bad middle ground:

- it was compile-time legal to call the wrong method
- it was semantically ambiguous to readers
- it was easy for code reviewers and automation to misclassify the guest median implementation as a bug
- the mistake only became obvious after running the program and observing that proving time had become dominated by the wrong path

That is exactly the kind of failure we want to make expensive. The correct design adds friction in the right place:

- if the code is compiled for guest, the guest method is the only obvious option
- if the code is compiled for host, the host method is the only exposed option
- if a caller wants a different environment, it must be a different build configuration, not a runtime conversion

The sealed environment trait is part of the same boundary. Without sealing, downstream code could create new environment markers and start depending on unsupported behavior. The crate should own the full environment taxonomy because the environment is a protocol boundary, not an extension point.

# Consequences

## Positive

- The guest cannot silently drift onto the host median-time path.
- Callers no longer need to remember which median method is safe in which environment.
- The host keeps a standard implementation that operators can compare against external code.
- The public API is smaller: `State` and `Input` remain the names users import.
- Environment switching becomes a build-time concern instead of a runtime helper.

## Negative

- The implementation is less flexible internally because the environment is now a closed set.
- The crate has to maintain the private inner type and the selected alias separately.
- Some internal code becomes slightly more explicit because host and guest behavior are separated by module structure.

## Risks

- If a new environment is ever needed, it will require changes inside the crate rather than a downstream extension.
- If internal code accidentally reintroduces a generic public `State<E>`, the same class of ambiguity can return.

## Mitigations

- Keep the inner state type private.
- Keep environment markers and the sealed trait inside `crates/core/src/env/`.
- Avoid conversion helpers that imply one in-memory state can be repurposed as another.
- Prefer host-only impl blocks for host-only behavior instead of feature-gating individual lines.

# Alternatives Considered

## Option 1: Keep two named median methods and let callers choose

Rejected.

This was the original shape. It made the wrong choice look superficially valid, especially to agents and reviewers who saw the guest path accepting extra witness data and assumed that was the bug. That ambiguity is expensive in this repository because the failure mode is not compile-time breakage; it is a large runtime regression.

## Option 2: Keep `State<E>` public and let callers pick the environment explicitly

Rejected.

This preserves the ambiguity at the API boundary. Callers can start reasoning about environments directly, and the wrong environment becomes a type parameter choice instead of a crate-owned protocol decision.

## Option 3: Add a runtime conversion helper between environments

Rejected.

This suggests that the same in-memory state can meaningfully become a different environment. That is the wrong model here. A state is either host-owned or guest-owned at construction time.

## Option 4: Use a trait object or runtime flag to pick the median implementation

Rejected.

That moves the decision later and makes the hot path harder to reason about. The point is to make the wrong implementation unavailable, not merely indirect.

# Implementation Notes

- Move the environment-specific code into `crates/core/src/env/mod.rs`.
- Put host-only state behavior in `crates/core/src/env/host.rs`.
- Keep the environment marker types and the sealed trait private to the crate boundary.
- Re-export only the public selected `State` type and the plain `Input` type.
- Keep host-specific checks, such as the standard median-time-past sort, behind the `host` feature at the module level.
- Preserve the guest median-time path as the witness-driven implementation that the circuit uses.

# References

- `crates/core/src/env/mod.rs`
- `crates/core/src/env/host.rs`
- `crates/core/src/lib.rs`
- `crates/core/src/input.rs`
- `crates/host/src/util.rs`

# Quality Checklist

- [x] ADR number: 0002
- [x] Front matter complete
- [x] Status set
- [x] Context includes the runtime regression failure mode
- [x] Decision states the alias-based environment selection
- [x] Consequences include both benefits and trade-offs
- [x] Alternatives documented and rejected
- [x] Implementation notes included
