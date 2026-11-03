# Architectural Log

Append-only chronological log.

## 2026-10-31

### Wiki bootstrap
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:0f68dd15e6bfd05e380d3106b17d26b50d51f78963bf2ae6d1ebad6bfa683055`
- Depth: `full`
- Entities created: 63
- Modules created: 122
- Workflows created: 0
- Entry-point flows created: 2
- Infrastructure created: 3
- Architecture pages created: 2
- HTTP API contract pages created: 0
- API operations tracked: 0
- Total classes tracked: 63
- Total files scanned: 122
- Docker/Compose files: 1
- GitHub Actions files: 2
- Kubernetes files: 0
- Runtime/config YAML files: 0
- Cross-references resolved: 248

## 2026-10-31

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:0f68dd15e6bfd05e380d3106b17d26b50d51f78963bf2ae6d1ebad6bfa683055`
- Pages created: 0
- Pages updated: 1
- Pages metadata-only: 0
- Pages skipped (unchanged): 4
- Pages deprecated: 0
- Semantic fields preserved: 0
- Moved entities: none

## 2026-10-31

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:6225994136aaaf6a25a25bdebf83a26058a03c72389c2fa80e83cc554f87770a`
- Pages created: 3
- Pages updated: 14
- Pages metadata-only: 0
- Pages skipped (unchanged): 176
- Pages deprecated: 0
- Semantic fields preserved: 0
- Moved entities: none

Connection failures now use the public `ConnectionError` enum through the P2P handshake, framing, and header-batch paths. Sync errors retain the typed source, and exhausted retries preserve the last failure and resumable tip.

## 2026-10-31

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:03678dd4efcae076e80e254a8f8b2b50c41e6016f82c3bf8948c29771f424df4`
- Pages created: 0
- Pages updated: 2
- Pages metadata-only: 4
- Pages skipped (unchanged): 189
- Pages deprecated: 0
- Semantic fields preserved: 4
- Moved entities: none

## 2026-10-31

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:43df71d1b98c312448ad154a861bf47d5f86fc23c39e9b0f9e2f1c94442b317a`
- Pages created: 0
- Pages updated: 2
- Pages metadata-only: 2
- Pages skipped (unchanged): 191
- Pages deprecated: 0
- Semantic fields preserved: 3
- Moved entities: none

## 2026-10-31

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:1a636f9845dfc74dc1e8923fc3e40ebdb302250eb70730f5d4cef80fedfebf56`
- Pages created: 0
- Pages updated: 9
- Pages metadata-only: 5
- Pages skipped (unchanged): 180
- Pages deprecated: 0
- Semantic fields preserved: 3
- Moved entities: none

Peer-sync failures now carry only their cause. Accepted progress stays in the shared mutable tip, and the public sync boundary moves that tip into terminal errors without cloning it.

## 2026-10-31

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:9299bde28c8ca7ff4155d7093ae53e6e38a3b67b398b97b435c25ba4fad33ecc`
- Pages created: 0
- Pages updated: 0
- Pages metadata-only: 0
- Pages skipped (unchanged): 193
- Pages deprecated: 0
- Semantic fields preserved: 0
- Moved entities: none

## 2026-11-03

### Wiki sync
- Source: `.`
- Generator version: `2.3.2`
- Source selection profile: `.llm-wiki/source-selection.json`
- Source selection fingerprint: `sha256:f24d3dfaea0e90b588e9fb1b5923d3a49fda035f817c90caf26e5efd1e6bbd4d`
- Source snapshot digest: `sha256:ac1e9d09f7f3232e742c2279a042fd664e66aef2bbae55ac1f5f7f138ac9afde`
- Pages created: 4
- Pages updated: 87
- Pages metadata-only: 2
- Pages skipped (unchanged): 102
- Pages deprecated: 8
- Semantic fields preserved: 14
- Moved entities: `WireType` (crates/wiretype/src/lib.rs → crates/wiretype/src/implementation.rs)
