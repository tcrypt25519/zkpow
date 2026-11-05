# Vision: zkpow

## Core Purpose

zkpow enables instant, cryptographic verification of the canonical Bitcoin header chain. By producing a recursive zero-knowledge proof in the SP1 zkVM, zkpow collapses the entire sequence of Bitcoin block headers from the genesis block to the live tip into a single succinct proof. Any client, bridge, or contract verifies 17+ years of proof-of-work in milliseconds without downloading 75+ MB of block headers or trusting third-party nodes.

## The Problem

Trust-minimized Bitcoin light clients face an inherent scaling bottleneck:
- **Bandwidth and Storage Overhead:** Traditional SPV verification requires downloading and storing every 80-byte header since genesis (over 900,000 headers and growing).
- **Compute and Power Consumption:** Every validating node repeatedly computes millions of SHA-256d operations, verifies median-time-past windows, and tracks epoch retargets.
- **On-Chain Impracticality:** Relaying Bitcoin headers to foreign smart contracts (e.g., Ethereum or other L1s) via on-chain header relay contracts incurs prohibitive gas costs and complex reorg logic.
- **Centralization Traps:** Mobile clients and cross-chain applications routinely compromise by trusting centralized RPC providers (Electrum servers, Infura, Blockstream) for chain tip and block validity.

zkpow shifts the validation burden from the consumer to the prover: the header chain is validated once inside a RISC-V zero-knowledge virtual machine, yielding a constant-size cryptographic attestation that anyone can verify immediately.

## Target Audience

1. **Cross-Chain Bridges and Relayers:** Smart contracts and cross-chain protocols that require definitive knowledge of the Bitcoin canonical chain tip to verify transaction inclusion without paying to store or validate headers on-chain.
2. **Ultra-Light Clients and Wallets:** Mobile and embedded Bitcoin wallets that achieve instant, trustless synchronization from genesis upon first launch.
3. **Rollups and L2 Protocols:** Bitcoin layer-2 ecosystems and sovereign rollups anchoring state to Bitcoin L1 proof-of-work finality.

## Core Capabilities

- **Genesis-to-Tip Chain Proof:** Produces a single recursive SP1 proof certifying that every block from mainnet genesis to height $N$ strictly adheres to Bitcoin proof-of-work consensus.
- **Constant-Size Public Interface:** Exposes a 185-byte public values commitment containing cumulative chain work, tip block hash, tip height, network identity, and program verifier key.
- **Satoshi-Era Consensus Parity:** Strictly enforces original proof-of-work consensus rules—SHA-256d target comparison, 2,016-block difficulty adjustment schedule, and 11-block median-time-past—without subjective checkpoints or retroactive soft-fork restrictions.
- **Near-Real-Time Currency:** Proves batches of headers regularly to track the live Bitcoin chain tip with sublinear batch proving overhead.
- **Universal Verifier Compatibility:** Generates proofs directly verifiable by native SP1 verification tooling and compatible with off-the-shelf SP1 Groth16 on-chain verifier contracts.

## Anti-Vision (What zkpow Is Not)

- **Not a Custom Smart Contract Suite:** zkpow does not author, deploy, or govern custom on-chain bridge contracts. It produces standardized proofs consumable by existing SP1 verification contracts.
- **Not a Soft-Fork Enforcement Engine:** zkpow does not enforce non-header rules or retrofitted soft forks (such as BIP34 coinbase height commitments or version 3 negative-version bans). It adheres purely to proof-of-work consensus starting from genesis.
- **Not a Distributed Prover Cluster:** zkpow operates as a focused, deterministic pipeline, avoiding distributed orchestration complexity, p2p coordination layers, or token mechanisms.
- **Not a Monolithic Coprocessor:** zkpow focuses strictly on header chain continuity and consensus proof-of-work. Transaction inclusion verification builds upon the committed Merkle roots rather than coupling transaction indexing into the core header prover.
