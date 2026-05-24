# Changelog Entry Template

Copy this template for each turn. Save as `docs/changelog/YYYY-MM-DD-HHMMSS.md`.

---

## Task

Brief description of what was requested.

---

## Complete Diff

```
<paste full diff here>
```

### Diff Annotations

For every chunk in the diff above:

#### Chunk N: `<file>:<line range>`

- **Why strictly necessary:**
- **Why existing code was insufficient:**
- **Justification for additions/changes:** (removing code is free — no justification needed)

---

## Alternatives Considered and Rejected

1. **Alternative A:** <description>
   - Why considered:
   - Why rejected:
   - Tradeoffs:

2. **Alternative B:** <description>
   - Why considered:
   - Why rejected:
   - Tradeoffs:

---

## Potential Issues and Defenses

1. **Risk:** <description of what could go wrong>
   - **Defense:** <how you guarded against it>
   - **If defense fails:** <what happens, how to detect, how to recover>

2. **Risk:** <description>
   - **Defense:** <how you guarded against it>
   - **If defense fails:** <what happens>

---

## Validation

- [ ] `cargo build --release` passes
- [ ] `cargo clippy --all-targets` passes
- [ ] `cargo run --release --bin test_errors` passes
- [ ] All existing tests pass
