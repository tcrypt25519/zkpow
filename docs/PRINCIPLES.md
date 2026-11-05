# Principles: zkpow

## 1. Strict Consensus Truth Over Subjective Rules
zkpow adheres strictly to the original Satoshi-era proof-of-work consensus mechanics: valid SHA-256d target satisfaction, 2,016-block difficulty adjustment schedule, and 11-block median-time-past windows. 
- **No Checkpoints:** The system contains zero hardcoded block hashes beyond the mainnet genesis anchor.
- **No Retroactive Soft-Fork Opinions:** Rules such as BIP34 (coinbase block height), BIP66, or version 3 negative-version rejections are intentionally omitted. If an alternate chain emerges from genesis with valid PoW and higher accumulated work under original consensus rules, zkpow follows it without subjective filtering.

## 2. Cryptographic Soundness Rests Exclusively on the Guest ZKP
The mathematical soundness and security of zkpow relies entirely on the SP1 guest circuit and the SP1 verifier:
- **Zero Soundness Dependency on External Code:** Neither the controller, host runner, SQLite database, nor P2P synchronizer needs to be bug-free or trusted for the system's security to hold. A buggy, crashed, or malicious controller cannot cause an invalid chain state to verify.
- **Controller Validation Is Strictly Operational:** The controller actively computes expected states and pre-validates proofs before committing to storage. This validation exists solely for smooth operations—failing fast, catching transient pipeline errors, and avoiding wasted GPU cycles. It contributes zero percent to cryptographic soundness.
- **Guest Self-Defense:** The guest program running inside SP1 treats all inputs and hints as adversarial. Hints for median timestamps and retarget division quotients are mathematically validated in-guest ($O(N)$ rank count check and a 256-bit multiplication syscall, respectively). Corrupted hints cause the zkVM to abort immediately without producing a valid proof.

## 3. Zero-Knowledge Cycle Economy
Every instruction executed within the zkVM incurs cryptographic proving overhead. Code design prioritizes cycle efficiency over general-purpose abstractions:
- **Avoid In-Guest Sorting:** Never execute sorting algorithms inside the zkVM circuit when verification via fixed lookup tables or precomputed hints is possible.
- **Batch State Amortization:** Difficulty transitions and per-block work calculations are amortized across difficulty epoch segments rather than recomputed per header.
- **Truncated Wire Representation:** Transmit only the minimal necessary header bytes (44 bytes per header) into the guest, reconstructing predictable fields (`prev_blockhash`, `nbits`) from verified internal state.

## 4. Unconditional Crash Resilience and Idempotence
Proving the entire Bitcoin chain requires hours across hundreds of discrete batches:
- **Zero Lost Work:** Every batch boundary is an idempotent, transactional checkpoint recorded in SQLite (`headers.db`).
- **Resilient Preemption:** Proving nodes can be killed, preempted, or evicted mid-batch with zero corruption. Restarting with the same `ZKPOW_RUN_ID` instantly resumes from the last verified height.
- **Decoupled Cold Storage:** Proof artifacts are immediately staged for offsite replication to prevent local disk failure from stalling pipeline progress.

## 5. Standard Verifier Compatibility
zkpow avoids bespoke smart contract deployments or custom on-chain verification infrastructure:
- Proof outputs adhere directly to standard SP1 interfaces (`verify_sp1_proof` and standard EVM `ISP1Verifier`).
- Verification logic is universal: any light client, bridge, or contract capable of running SP1 verification can verify the chain tip without custom protocol tooling.

## 6. Rolling Foundations
Code and documentation express current, declarative truth:
- System documentation avoids speculative roadmaps, migration notes, and historical phase commentary.
- When architecture or consensus mechanics evolve, foundation documents are rolled forward in place.
