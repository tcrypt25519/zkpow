# Project Conventions

## Release mapping
none

## Tag taxonomy
- consensus    Satoshi-era Bitcoin proof-of-work consensus, MTP, difficulty retarget, header validation
- perf         throughput, proving time, memory, zkVM cycle economy — routes to perf-design
- refactor     behavior-preserving structural change ONLY — routes to refactor-design
- security     cryptographic soundness, zero-knowledge circuit constraints, arithmetic bounds, proof verification
- test         test integrity, in-memory fixtures, zkVM simulation, regression tests
- infra        Vast.ai, GPU orchestration, Backblaze B2, CI/CD, build toolchains
- docs         documentation, specifications, architecture records, wikis

## Slug conventions
kebab-case, prefixed with parent slug

## Terminal-tier retention
retain-bodies

## Gate config
gates_for_release: [security, tests, cruft, docs, patterns]
gate_finding_routing:
  critical: implementing
  high: implementing
  medium: drafting
  low: backlog
  info: skip
gate_refactor_scan_library_roots:
  - .agents/skills
  - .claude/skills
binding_guard: warn
epic_cohesion: phased
review_weight: standard
backlog_staleness_days: 90

## Review weight
review_weight: standard
