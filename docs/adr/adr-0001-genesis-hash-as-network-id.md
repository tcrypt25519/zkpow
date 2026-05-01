---
title: "ADR-0001: Include genesis hash in state as network anchor"
status: "Proposed"
date: "2026-04-14"
authors: "tcrypt (maintainer)"
tags: ["architecture","decision"]
supersedes: ""
superseded_by: ""
---

# Status

Proposed

# Context

The prover produces inductive proofs that a sequence of Bitcoin block headers is valid (PoW, linkage, retargeting, MTP). Each proof demonstrates that the latest header is the result of a sequence of valid state transitions from some starting point. However, an inductive proof that a header is "valid" and has the most cumulative work does not by itself identify which Bitcoin network the chain belongs to (mainnet, testnet, regtest, etc.).

We must ensure consumers of a proof can deterministically and unambiguously associate a proof with the intended Bitcoin network. The repository already has incidental examples of ADRs; this decision documents why the genesis block hash is included in every state produced by the prover rather than only checked at the initial bootstrapping step.

Constraints and forces:
- Consumers may accept proofs from untrusted channels; the system must guard against cross-network replay or confusion.
- Proof size and state size matter for storage and recursive-chaining efficiency.
- Simplicity and verifiability for lightweight verifiers (who may only see the final state and proof).

# Decision

Include the genesis block hash (32-byte little-endian or canonical byte order as used elsewhere in the project) as an explicit committed field in the public state that is carried forward with every validated header batch.

Rationale:
- The genesis hash acts as a stable, immutable network identifier. Verifiers can check the genesis hash in the public state against the expected genesis for their network to ensure proofs correspond to the intended chain.
- Propagating the genesis hash with every state makes each proof self-contained: a verifier need not have historical context beyond the network's expected genesis to accept or reject a proof.
- This approach simplifies recursive composition: when chaining or extending proofs, the network identity is preserved automatically by the state transition function (the genesis hash is part of the committed public values), avoiding subtle errors where a proof might be linked to a different genesis at a later step.

# Consequences

Positive
- POS-001: Verifiability — Any verifier can confirm network identity by checking the genesis hash in the public state, preventing cross-network acceptance.
- POS-002: Self-containment — Proofs are self-descriptive; no out-of-band state is required to know network identity, simplifying verification and archival workflows.
- POS-003: Safety for recursive proofs — Recursive chaining preserves the network anchor, avoiding accidental mixing of chains when aggregating or extending proofs.

Negative
- NEG-001: Extra public bytes — Each state increases by 32 bytes (small but non-zero) which slightly increases proof/public-value size.
- NEG-002: Redundancy — The genesis hash is unchanged across transitions, so it is duplicated repeatedly; this is duplication of information across states.
- NEG-003: Migration friction — Existing tooling that assumed genesis checked only once may need minor updates to read/verify the committed genesis field in public values.

# Alternatives Considered

Do nothing (check genesis only at bootstrap)
- ALT-001: Description: Verify the genesis hash once at initial proof generation or verifier bootstrap and never include it in subsequent states.
- ALT-002: Rejection Reason: This requires verifiers to maintain out-of-band trusted state; proofs are not self-contained and are vulnerable to confusion if the verifier's bootstrap step or trust anchor is wrong or missing.

Use a short network identifier (e.g., 1-byte enum)
- ALT-003: Description: Commit a compact network id (enum) instead of full genesis hash to reduce size.
- ALT-004: Rejection Reason: Network ids are convention-based and may collide or become ambiguous across forks; the genesis hash is a canonical cryptographic identifier that is widely published and unambiguous.

Commit an attestation signed by a trusted key
- ALT-005: Description: Have a trusted authority sign a network identifier and include the signature in the state.
- ALT-006: Rejection Reason: Introduces external trust, key management, and raises operational complexity; genesis-hash-as-anchor is trustless and verifiable by anyone.

# Implementation Notes

- IMP-001: Public Values Layout — Add a 32-byte genesis_hash field to the public values blob (follow existing byte layout and endianness conventions used elsewhere in the project). Update the compute_pv_digest() host code to include this field.
- IMP-002: Program/zkVM — Ensure the zkVM program commits the genesis_hash as an input (or preserved state) and that state transition code copies it unchanged between steps.
- IMP-003: Backwards compatibility — Define a migration plan for existing proofs or tooling: accept either the new public-value layout or the old one for a transition period, or provide a converter for old proofs into the new format.
- IMP-004: Verification tooling — Update inspect_proof and other verifier utilities to check the genesis_hash against an expected value (configurable via CLI env var or parameter).
- IMP-005: Documentation — Add a short note to README/docs noting that genesis_hash is the canonical network identifier and how to obtain the expected genesis for common networks.

# References
- REF-001: Example ADRs (repository): ../example-adrs/ (see adr-001.md .. adr-004.md)
- REF-002: Project public values layout (repository docs/ or program docs)
- REF-003: Bitcoin genesis hashes (external): https://en.bitcoin.it/wiki/Genesis_block

# Quality Checklist

- [x] ADR number: 0001 (starting new docs/adr directory)
- [x] File name: adr-0001-genesis-hash-as-network-id.md
- [x] Front matter complete
- [x] Status: Proposed
- [x] Date format: 2026-04-14
- [x] Context explained
- [x] Decision stated clearly
- [x] Positive and negative consequences documented
- [x] Alternatives documented with rejection reasons
- [x] Implementation notes provided
- [x] References included



