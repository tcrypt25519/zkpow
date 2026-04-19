# Bitcoin Header Relay Contracts

This directory contains the initial Solidity relay scaffold for the Bitcoin header chain prover.

## What It Is

The relay is a bounded on-chain anchor for SP1-backed Bitcoin header proofs. Each relay instance is
parameterized by:

- a verifier address, which may be an SP1 gateway or a direct verifier
- a program verification key
- a trusted genesis block hash / network anchor
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

The on-chain contract expects a packed binary summary with this field order:

1. `uint32 schemaVersion`
2. `uint32 headerVersion`
3. `bytes32 genesisBlockHash`
4. `bytes32 currentBlockHash`
5. `bytes32 prevBlockHash`
6. `uint32 height`
7. `uint256 cumulativeChainWork`
8. `uint32 timestamp`
9. `uint32 nBits`
10. `uint32 nonce`
11. `bytes32 merkleRoot`
12. `uint32[11] medianTimePastWindow`

This is intentionally different from the current Rust `rkyv` state serialization. The Rust side
should keep its internal proof-state format, then serialize this compact summary once at the proof
boundary.

## Build Notes

This scaffold is written for Foundry.

Typical commands once the environment is wired up:

```bash
forge build
forge test
```

If the verifier dependency is vendored later, it should continue to implement the same `ISP1Verifier`
interface used here.
