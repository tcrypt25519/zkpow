# ShortEpoch

**Location:** `crates/params/src/lib.rs:60`
**Kind:** Class
**Bases:** `Consensus`
**Module:** [params_src_lib](../modules/params_src_lib.md)

## Description

A synthetic network that differs from [`Mainnet`] only in `epoch_length`, so tests can cross a
retarget boundary cheaply and prove the transition reads the boundary from the parameters
rather than assuming 2016.

Not Bitcoin's regtest: real regtest shares mainnet's epoch length, which is exactly what makes
it useless here. Feature-gated so it cannot be reached from a production build.

## Attributes

*No annotated attributes found.*

## Methods

*No public methods. Inherits from base classes.*

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
```mermaid
flowchart LR
    n0["ShortEpoch (crates/params/src/lib.rs)"]
    n1["Consensus (crates/params/src/consensus.rs)"]
    n0 --> n1
    click n0 "../modules/params_src_lib.md"
    click n1 "../modules/consensus.md"
```

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [params_src_lib](../modules/params_src_lib.md) | 0 | — |

### Structure

| Kind | Entity | Module |
|---|---|---|
| Base | `Consensus` | [consensus](../modules/consensus.md) |
