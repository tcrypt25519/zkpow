# Cycle Profiling

Optimizing zkVM guest code requires understanding where instruction cycles are spent. `cargo-zkvm` provides two complementary profiling mechanisms:
1. **Automatic Function-Level Profiling (`--profile`)**: Disassembles and analyzes the execution trace against the binary's symbol table with zero code modifications.
2. **Hierarchical Sub-Routine Scopes (`sp1-cycle`)**: Declarative cycle brackets for custom logical pipelines.

---

## Automatic Function Profiling (`--profile`)

Pass `--profile` (or `--subcycles`) to generate an in-depth cycle consumption table for every function invoked during the test:

```bash
cargo zkvm test -p zkpow-sha256 --profile
```

### Profile Table Output

```console
        PASS zkpow_sha256::tests::subroutine_cycle_profiling_demonstration
             [    460,644 cyc │    384,816 gas │  130.06 MB │   0.13s ]
        ┌─ function cycle profile ──────────────────────────────────────────────────────────────────────────────────────┐
        │ Function                                                         │ Cycles │  % Tot │  Calls │   Math │ Memory │
        ├──────────────────────────────────────────────────────────────────┼────────┼────────┼────────┼────────┼────────┤
        │ sha2::sha256::soft::sha256_digest_round_x2                       │ 82,080 │  17.8% │ 15,408 │ 19,584 │ 58,752 │
        │ core::intrinsics::rotate_right::<u32>                            │ 60,480 │  13.1% │ 17,328 │ 38,880 │ 17,280 │
        │ sha2::sha256::soft::sha256_digest_block_u32                      │ 27,918 │   6.1% │  4,980 │ 11,259 │ 15,966 │
        │ memcpy                                                           │ 25,783 │   5.6% │  1,613 │  8,498 │ 12,674 │
        │ sha2::sha256::soft::sha256msg1::sigma0x4                         │ 23,760 │   5.2% │    972 │  8,208 │ 15,444 │
        │ sha2::sha256::soft::sha256msg2                                   │ 14,580 │   3.2% │  4,320 │  3,888 │  9,720 │
        │ sha2::sha256::soft::schedule                                     │ 14,364 │   3.1% │  2,808 │  5,400 │  8,532 │
        │ <*const ()>::is_aligned_to                                       │ 11,925 │   2.6% │  3,718 │  9,010 │  2,120 │
        │ sha2::sha256::soft::compress                                     │ 11,142 │   2.4% │  3,634 │  3,073 │  6,909 │
        └──────────────────────────────────────────────────────────────────┴────────┴────────┴────────┴────────┴────────┘
```

### Table Columns
- **Function**: Demangled Rust function symbol. Symbol hashes (`[bb7aedaf0461c108]`) are stripped automatically, and long symbols use middle truncation (`head...tail`) to preserve namespace and method context.
- **Cycles**: Total instruction cycles attributed to this function.
- **% Tot**: Percentage of overall test cycles consumed by this function.
- **Calls**: Total invocation count.
- **Math**: Cycles spent on arithmetic and logic operations (ALU, multiplication, division).
- **Memory**: Cycles spent executing load and store instructions (`lw`, `sw`, etc.).

---

## Filtering Profile Results (`--profile-filter`)

To focus on specific modules or functions in large call graphs, pass `--profile-filter <PATTERN>`:

```bash
cargo zkvm test -p zkpow-sha256 --profile --profile-filter sha256
```

This restricts the profile report to functions whose names match the filter substring.

---

## Hierarchical Sub-Routine Scopes (`cycle_scope!`)

When optimizing multi-stage algorithms (e.g. signature verification, state transition, and Merkle root calculation), you can measure logical phases using `sp1-cycle::cycle_scope!`:

```rust
use zkvm_test::test;

#[test]
fn test_complex_pipeline() {
    let _outer = sp1_cycle::cycle_scope!("outer_pipeline");

    {
        let _pass1 = sp1_cycle::cycle_scope!("first_hash_pass");
        compute_stage_one();
    }

    {
        let _pass2 = sp1_cycle::cycle_scope!("second_hash_pass");
        compute_stage_two();
    }
}
```

The runner formats nested scopes into an intuitive tree structure:

```console
        ┌─ Sub-Routine Cycles:
        │  ├── outer_pipeline            386,924 cyc  ( 84.0%)  [1 call]
        │  ├── second_hash_pass          186,267 cyc  ( 40.4%)  [1 call]
        │  └── first_hash_pass           186,264 cyc  ( 40.4%)  [1 call]
```
