# Bitcoin Header Relay Contracts

This directory contains the initial Solidity relay scaffold for the Bitcoin header chain prover.

## What It Is

The relay is a bounded on-chain anchor for SP1-backed Bitcoin header proofs. Each relay instance is
parameterized by:

- a verifier address, which may be an SP1 gateway or a direct verifier
- a program verification key
- a trusted genesis hash / network anchor
- a recent-history capacity

Multiple relay instances can share the same verifier address while using different program keys.

## What It Stores

The contract stores:

- the latest accepted tip
- a bounded ring buffer of recent accepted tips
- the latest proof hash
- the immutable deployment parameters

It does not try to store the full chain history on chain.

## Public Values Boundary

The on-chain contract expects ABI-encoded public values with this field order:

1. `uint32 schemaVersion`
2. `bytes32 genesisHash`
3. `bytes32 blockHash`
4. `bytes32 prevBlockHash`
5. `uint32 tipHeight`
6. `uint256 cumulativeChainWork`
7. `uint32 blockTimestamp`
8. `uint32 nBits`
9. `uint32 nonce`
10. `bytes32 merkleRoot`

That is intentionally different from the current Rust `rkyv` state serialization. The Rust side
should keep its internal proof-state format, but the contract-facing public values should become this
flat ABI summary so Solidity can decode and store header fields directly.

## Build Notes

This scaffold is written for Foundry.

Typical commands once the environment is wired up:

```bash
forge build
forge test
```

If the verifier dependency is vendored later, it should continue to implement the same `ISP1Verifier`
interface used here.
