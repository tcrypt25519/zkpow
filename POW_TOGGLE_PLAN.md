# Proof-of-Work Verification Toggle Plan

## Goal

Enable compile-time toggling between real consensus PoW verification and a dummy/easy verification that always passes. This allows testing all validation logic **after** PoW (chain linkage, bits match, median timestamp, retargeting) without needing to grind real block headers or find preimage collisions.

## Design

### Trait-based static dispatch

```rust
pub trait PoWVerifier {
    /// Verify that the given 80-byte header satisfies the proof-of-work target.
    /// Returns true if the double-SHA256 hash of `header` meets the target
    /// encoded in the header's `bits` field.
    fn verify(header: &[u8; 80], bits: u32) -> bool;
}
```

Two implementations:

1. **`RealPoWVerifier`** — computes `double_sha256_80(header)` and compares against `bits_to_target(bits)`. This is the current behavior.

2. **`DummyPoWVerifier`** — always returns `true`. No hash computation, no target comparison. Accepts any header regardless of PoW.

### Toggle mechanism: Cargo feature flag

In `program/Cargo.toml`:

```toml
[features]
default = []
dummy-pow = []
```

Entry point dispatch via feature-gated function call:

```rust
fn main() {
    validate_headers::<ActiveVerifier>(
        expected_genesis_hash, start_height, num_headers, headers_bytes,
    );
}
```

### Why trait + generic function (not function pointers, not `#[cfg]` on every call site)

- **Static dispatch**: `validate_headers::<RealPoWVerifier>(...)` monomorphizes at compile time. The compiler inlines `RealPoWVerifier::verify` into the validation loop. Zero runtime overhead — identical to a direct function call.
- **No branching**: The generic type parameter is resolved at compile time. There is no `if cfg!(feature = "dummy-pow")` branch in the binary.
- **No `#[cfg]` sprawl**: The validation loop is written once. Only the entry-point type instantiation and the `ActiveVerifier` alias change between builds.
- **Extensible**: Adding a third verifier (e.g., `WeakPoWVerifier` with a relaxed target) requires only a new `impl PoWVerifier for WeakPoWVerifier` — no changes to the validation loop.

### Why feature flag (not const generic, not build script, not env var)

- **Feature flag** is the standard Rust idiom for compile-time feature toggling. `cargo build --features dummy-pow` is self-documenting.
- **Const generic** would require a const parameter value that still needs cfg-gating somewhere, adding complexity for no benefit.
- **Build script** is overkill for a boolean toggle.
- **Env var** is runtime, not compile-time — defeats the "static dispatch" requirement.

### Default behavior: real verification

`default = []` means no features are enabled by default. `ActiveVerifier` resolves to `RealPoWVerifier`. Nobody accidentally builds a vulnerable verifier — it must be explicitly opted into via `--features dummy-pow`.

## File changes

### `program/Cargo.toml`

Add `[features]` section:

```toml
[features]
default = []
dummy-pow = []
```

### `program/src/main.rs`

1. Define `ActiveVerifier` type alias via `#[cfg]`
2. Change `main()` to call `validate_headers::<ActiveVerifier>(...)`
3. Extract the validation loop into a generic function:

```rust
fn validate_headers<V: PoWVerifier>(
    expected_genesis_hash: [u8; 32],
    start_height: u64,
    num_headers: u64,
    headers_bytes: &[u8],
) {
    // ... (current loop body)
    
    // Replace direct hash_meets_target call with trait method:
    if !V::verify(header, bits) {
        commit_error_and_exit(..., STATUS_POW_INSUFFICIENT, i as u32);
    }
}
```

### `program/src/pow.rs` (new file)

Contains the trait and both implementations:

```rust
use crate::sha256::double_sha256_80;
use crate::bits_to_target;

pub trait PoWVerifier {
    fn verify(header: &[u8; 80], bits: u32) -> bool;
}

/// Real consensus PoW verification.
pub struct RealPoWVerifier;

impl PoWVerifier for RealPoWVerifier {
    fn verify(header: &[u8; 80], bits: u32) -> bool {
        let block_hash = double_sha256_80(header);
        crate::hash_meets_target(&block_hash, bits)
    }
}

/// Dummy PoW verifier — always returns true.
/// Enabled via the `dummy-pow` feature for testing.
pub struct DummyPoWVerifier;

impl PoWVerifier for DummyPoWVerifier {
    fn verify(_header: &[u8; 80], _bits: u32) -> bool {
        true
    }
}

// Resolve at compile time — zero overhead, fully monomorphized.
#[cfg(not(feature = "dummy-pow"))]
pub type ActiveVerifier = RealPoWVerifier;

#[cfg(feature = "dummy-pow")]
pub type ActiveVerifier = DummyPoWVerifier;
```

### `program/src/main.rs` usage

```rust
mod pow;
use pow::{PoWVerifier, ActiveVerifier};

fn main() {
    // ... read inputs, validate byte count ...
    
    validate_headers::<ActiveVerifier>(
        expected_genesis_hash,
        start_height,
        num_headers,
        &headers_bytes,
    );
}

fn validate_headers<V: PoWVerifier>(
    expected_genesis_hash: [u8; 32],
    start_height: u64,
    num_headers: u64,
    headers_bytes: &[u8],
) {
    // ... validation loop ...
    
    if !V::verify(header, bits) {
        commit_error_and_exit(..., STATUS_POW_INSUFFICIENT, i as u32);
    }
}
```

## Usage

### Normal build (real PoW verification — default)

```bash
cargo build --release
# or explicitly:
cargo build --release --no-default-features
```

### Test build (dummy PoW verification)

```bash
cargo build --release --features dummy-pow
```

### Testing workflow

1. **Build with `dummy-pow` feature:**
   ```bash
   cargo build --release --features dummy-pow
   ```

2. **Construct test headers** with arbitrary fields:
   - Set `prev_blockhash` to any 32 bytes
   - Set `timestamp` to any u32
   - Set `bits` to any valid compact encoding (exponent 3..=29)
   - Set `nonce` to anything — PoW check is bypassed
   - Only constraints: header must be 80 bytes, bits must be in valid range

3. **Test post-PoW validation:**
   - Chain linkage: corrupt `prev_blockhash` → expect `STATUS_PREV_BLOCKHASH_MISMATCH`
   - Bits match: change `bits` mid-chain → expect `STATUS_BITS_MISMATCH`
   - Median timestamp: set `timestamp` below median → expect `STATUS_TIMESTAMP_TOO_OLD`
   - Retargeting: verify correct target computation at 2016-block boundaries
   - Height continuity: test `STATUS_HEIGHT_MISMATCH` with wrong start_height

## Security considerations

- **Default is safe**: `dummy-pow` is not in `default = []`. No one builds a vulnerable verifier accidentally.
- **Feature name is explicit**: `dummy-pow` clearly signals "this is not real PoW".
- **Release builds can still use it**: `--release --features dummy-pow` is valid. This is intentional — we want to be able to build optimized test binaries. The feature name serves as a warning in the build log.
- **No runtime branching**: The `cfg` resolves at compile time. The binary contains exactly one implementation. There's no way to "switch" at runtime.

## Testing

Add a compile-time assertion to verify the correct verifier is active:

```rust
// In main.rs, after imports:
#[cfg(feature = "dummy-pow")]
const _: () = assert!(
    std::any::type_name::<ActiveVerifier>().contains("DummyPoWVerifier"),
    "dummy-pow feature should select DummyPoWVerifier"
);

#[cfg(not(feature = "dummy-pow"))]
const _: () = assert!(
    std::any::type_name::<ActiveVerifier>().contains("RealPoWVerifier"),
    "default build should use RealPoWVerifier"
);
```

## Future extensions

If we later want a **third** verifier (e.g., a "weak PoW" that requires hash < 2^200 instead of the real target), we'd:

1. Add `WeakPoWVerifier` impl to `pow.rs`
2. Add a `weak-pow` feature
3. Add a third `cfg` arm to the `ActiveVerifier` alias

The trait-based design makes this a 5-line change regardless of how many implementations we add.

## Files to create/modify

| File | Action |
|------|--------|
| `program/Cargo.toml` | Add `[features]` section |
| `program/src/pow.rs` | New file — trait, implementations, type alias |
| `program/src/main.rs` | Add `mod pow; use pow::ActiveVerifier;` replace direct `hash_meets_target` call |
| `program/src/sha256.rs` | No changes — `double_sha256_80` stays as-is (used by `RealPoWVerifier`) |
