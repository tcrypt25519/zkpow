//! zkpow — Header-Construction Architecture
//!
//! Validates a batch of Bitcoin block headers incrementally.
//! The prover supplies only non-deterministic fields (version, merkle_root,
//! timestamp, nonce). The circuit hashes those fields with authenticated state
//! directly, then validates PoW and consensus state transitions.
//!
//! Input protocol:
//!   1. verifier_key: Vec<u8>
//!   2. state_witness: Vec<u8>        (State bytes)
//!   3. header_hints: Vec<u8>
//!   4. median_time_past_hints: Vec<u8>
//!   5. If `state.height > 0`, compressed SP1 proof witness (via write_proof)
//!
//! Output: MinimalPublicValues (165 bytes) on success or failure.

#![no_main]
sp1_zkvm::entrypoint!(main);

use bytemuck::{self, Pod, PodCastError};
use sp1_zkvm::lib::read_vec_raw;
use zkpow_core::{
    serialize_minimal_public_values, ApplyFailure, BlockTimestamp, HeaderInput, MinimalPublicValues, ReturnCode, State,
};
use zkpow_params::Mainnet;
use zkpow_sha256;

/// Compute the continuation digest: SHA-256 of the serialized continuation data.
#[sp1_cycle::tracked]
fn compute_continuation_digest(state: &State) -> [u8; 32] {
    zkpow_sha256::of_52bytes(state.continuation_bytes())
}

/// Commit the minimal public values and halt.
fn commit_minimal_pv(
    state: &State,
    verifier_key: &[u8],
    continuation_digest: &[u8],
    error_code: u8,
) -> ! {
    let bytes: [u8; MinimalPublicValues::SIZE] = serialize_minimal_public_values(
        &state.chain_work,
        verifier_key,
        continuation_digest,
        &state.genesis_hash,
        &state.block_hash,
        state.height,
        error_code,
    );
    sp1_zkvm::io::commit_slice(&bytes);
    sp1_zkvm::syscalls::syscall_halt(0)
}

fn read_fixed_size_input<'g, T: Pod>() -> Result<&'g T, PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts(read_vec_results.ptr, read_vec_results.len);
    let input_slice_ref = unsafe { input_slice.as_ref() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)?;
    bytemuck::try_from_bytes::<T>(input_slice_ref)
}

fn read_fixed_size_input_mut<'g, T: Pod>() -> Result<&'g mut T, PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts_mut(read_vec_results.ptr, read_vec_results.len);
    let input_slice_ref = unsafe { input_slice.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)?;
    bytemuck::try_from_bytes_mut::<T>(input_slice_ref)
}

//fn read_typed_scalar_input_mut<'g, T>() -> Result<&'g mut T, PodCastError> {
//    let read_vec_results = unsafe { read_vec_raw() };
//    let input_slice = std::ptr::slice_from_raw_parts_mut(read_vec_results.ptr, read_vec_results.len);
//    let input_slice_ref = unsafe { input_slice.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)?;
// //.expect("dynamic size input");    let input_type = input_slice.cast::<T>();
//    unsafe { input_type.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)
//}

fn read_slice_of_fixed_size_elements_input<'g, T: Pod>() -> Result<&'g [T], PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts(read_vec_results.ptr, read_vec_results.len);
    let input_slice_ref = unsafe { input_slice.as_ref() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)?;
    bytemuck::try_cast_slice::<u8, T>(input_slice_ref)
}

#[derive(Debug, thiserror::Error)]
enum ProgramError {
    #[error("PodCastError: {0}")]
    PodCastError(PodCastError),
}

impl From<PodCastError> for ProgramError {
    fn from(value: PodCastError) -> Self {
        Self::PodCastError(value)
    }
}

#[sp1_cycle::tracked]
fn inner_main() -> Result<(), ProgramError> {
    /*

     ## In-progress
        - Mapping serializaton/deserialization in this program
        - Mapping how we use the pieces of data and in what form(s)
        - Evaluating if we can simplify the data flow
        - Evaluating if we need such coupled structures **in memory**
            - Having them be in solid logical groups is easy to work with
            but I highly doubt we need the data in these groups to use
            it correctly. An example is hashing the 6 field/80 byte header.
            We don't need all 80 byts sitting together or in our memory in
            order to feed sha256 individual words. We had the necessary
            bytes in memory already so why waste copies moving them into
            a single struct? Now we feed the hashing function references
            to the individual fields.


    ## What happens to data in this program
        - First data used is prior_claim which is extracted, parsed, from state, and then used (with continuation digest and vk) to build a MinimalPublicValues which is serialized back to bytes.
        - Then vk_bytes, prior_claim, and prior_continuation_bytes are used; passed into verify_prior_proof logic.
        - vk_bytes are used again; in a conversion to VerifierKeyDigest.

        - Then state is used; the apply_headers method is called on it
        - headers and medians are used in the call to apply_headers
        - After apply_headers returns, the Verification Key is added to a new MinimalPublicValues which is then IMMEDIATE SERIALIZED BACK TO BYTES, AGAIN.

    ## So

        - Read `verifier_key` and keep it as bytes
        - Read `state` and parse it into a State struct
        - Read `headers` and parse them into HeaderInput structs
        - Read `medians` and parse them into BlockTimestamp structs

        - Get `prior_claim` (parsed Claim) from `state` (parsed State)
        - Get `prior_continuation_bytes` (un-parsed) from `state` (parsed State)
        - Hash `prior_continuation_bytes`

        - Hash the prior public-value fields (chain_work, verifier_key, continuation_digest, genesis_hash, tip_hash, height, success error_code) directly without instantiating a MinimalPublicValues
        - Call verify_sp1_proof with un-parsed verifier_key and hashed MinimalPublicValues
        - Parse the verifier_key bytes into a VerifierKeyDigest (stored as vk_digest)
        - Call apply_headers on parsed State, passing in parsed headers and parsed medians
        - There is one more use of input data after apply_headers: the Verification Key (as vk_digest) is added to a new MinimalPublicValues which is then IMMEDIATE SERIALIZED BACK TO BYTES, AGAIN.


    ## Logic Pieces

    - VK
        - [u32; 8]                                             ; Used for proof verification, committed
    - State
        - Claim
            pub genesis_hash: BlockHash,
            pub tip_hash: BlockHash,
            pub chain_work: ChainWork,
            pub height: u32,
        - Continuation
            pub current_nbits: CompactTarget,
            pub epoch_start_timestamp: BlockTimestamp,
            pub timestamps: [BlockTimestamp; Mainnet::PARAMS.window_size],
    - Headers
        pub version: u32,
        pub merkle_root: [u8; 32],
        pub timestamp: BlockTimestamp,
        pub nonce: u32,
    - Median
        pub timestamp: BlockTimestamp,

    - *MinimalPublicValues
        - A reformulation of elements from
        - State (the Claim)
            pub chain_work: ChainWork,
            pub genesis_hash: BlockHash,
            pub tip_hash: BlockHash,
            pub height: u32,
        - Continuation Hash
            pub continuation_digest: [u8; 32],
        - VerifierKey
            pub verifier_key: VerifierKeyDigest,
        - Plus an error_code
        pub error_code: Option<NonZeroU8>,
        - The failure height is `last_valid_claim.height + 1`, derived from the claim.


    ## Where state gets mutated

        - State is mutated IN PLACE by `state.apply_headers(headers, medians)` in this file, on the `&mut State` read from stdin.
        - Inside `State::apply_headers` (`crates/core/src/state.rs`):
            - `height` is incremented in `apply_header_after_median_check` to `self.height + 1`.
            - `timestamps[slot]` is overwritten with the new header timestamp.
            - `current_nbits` is updated once per chunk in `EpochManager::flush`.
            - `epoch_start_timestamp` is overwritten only at epoch boundaries.
            - `block_hash` is overwritten with the double-SHA256 of the new header.
            - `chain_work` is updated in `EpochManager::flush` by adding `active_block_work * processed`.
        - `EpochManager::flush` runs on Drop at the end of each chunk and on the failure path.
        - On failure, the caller reads the last valid state directly from the same `&mut State` that `apply_headers` mutated; no copy is needed.
        - Between iterations, the host serializes the updated State via `state.to_bytes()` and the next guest deserializes it fresh from stdin. The guest never re-serializes the canonical State internally.
        - Continuation data (`current_nbits`, `epoch_start_timestamp`, `timestamps`) is never modified on its own. It is copied out of State into ContinuationData and serialized only for hashing.

    */

    let verifier_key = read_fixed_size_input::<[u32; 8]>()?;
    let state: &mut State = read_fixed_size_input_mut::<State>()?;
    let headers: &[HeaderInput] = read_slice_of_fixed_size_elements_input::<HeaderInput>()?;
    let medians: &[BlockTimestamp] = read_slice_of_fixed_size_elements_input::<BlockTimestamp>()?;

    if state.height > 0 {
        // 1. Hash the prior continuation data directly from the State bytes.
        let continuation_digest = sp1_cycle::track!(compute_continuation_digest(&state));

        // 2. Hash the prior proof's minimal public values directly from State fields.
        let actual_pv_hash = sp1_cycle::track!({
            zkpow_sha256::of_minimal_public_values(
                state.chain_work.to_le_bytes_slice(),
                bytemuck::bytes_of(verifier_key),
                &continuation_digest,
                state.genesis_hash.to_le_bytes_slice(),
                state.block_hash.to_le_bytes_slice(),
                state.height,
                0,
            )
        });

        // 3. Verify the SP1 proof.
        sp1_cycle::track!({
            sp1_zkvm::lib::verify::verify_sp1_proof(verifier_key, &actual_pv_hash);
        });
    }

    // Apply headers; on failure commit minimal PV and halt.
    let return_code = match state.apply_headers::<Mainnet>(headers, medians) {
        Ok(()) => ReturnCode::SUCCESS,
        Err(ApplyFailure { error_code }) => ReturnCode::from(error_code),
    };
    let digest = compute_continuation_digest(&state);
    commit_minimal_pv(&state, bytemuck::bytes_of(verifier_key), &digest, return_code.into());
}

pub fn main() {
    inner_main().expect("guest input parsing failed");
}
