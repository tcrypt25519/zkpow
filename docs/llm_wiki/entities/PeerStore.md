# PeerStore

**Location:** `crates/sync-headers/src/peers.rs:79`
**Kind:** Class
**Bases:** —
**Module:** [src_peers](../modules/src_peers.md)

**Decorators:** `@Default`

## Description

A collection of discovered peers with status tracking.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `entries` | `Vec<PeerEntry >` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `from_dns_seeds` | `() -> std::io::Result<Self >` | — | Resolve all DNS seeds and populate the peer store. |
| `pick_candidate` | `(rng: &mut impl rand::Rng) -> Option<SocketAddr >` | — | Pick a random peer with `Unknown` or `Disconnected` status. |
| `add_peer` | `(addr: SocketAddr)` | — | — |
| `mark_connected` | `(addr: &SocketAddr)` | — | — |
| `mark_unreachable` | `(addr: &SocketAddr)` | — | — |
| `mark_disconnected` | `(addr: &SocketAddr)` | — | — |
| `mark_too_slow` | `(addr: &SocketAddr)` | — | — |
| `len` | `() -> usize` | — | — |
| `is_empty` | `() -> bool` | — | — |
| `status_summary` | `() -> HashMap<PeerStatus, usize >` | — | — |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_peers](../modules/src_peers.md) | 10 | `entries` |
