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
//! Output: MinimalPublicValues (153 bytes) on success or failure.

#![no_main]
sp1_zkvm::entrypoint!(main);

use sp1_zkvm::lib::read_vec_raw;
use zkpow_core::{
    BlockTimestamp, Claim, HeaderInput, MinimalPublicValues, State, VerifierKeyDigest, CONTINUATION_DATA_SIZE,
    MINIMAL_PV_SIZE,
};
use zkpow_sha256;

use bytemuck;
use bytemuck::{Pod, PodCastError};

/// Compute the continuation digest: SHA-256 of the serialized continuation data.
#[sp1_cycle::tracked]
fn compute_continuation_digest(state: &State) -> [u8; 32] {
    let cd_bytes: [u8; CONTINUATION_DATA_SIZE] = state.continuation_bytes();
    zkpow_sha256::of_52bytes(&cd_bytes)
}

/// Commit the minimal public values and halt.
fn commit_minimal_pv(pv: &MinimalPublicValues) -> ! {
    let bytes: [u8; MINIMAL_PV_SIZE] = pv.to_bytes();
    sp1_zkvm::io::commit_slice(&bytes);
    sp1_zkvm::syscalls::syscall_halt(0)
}

#[sp1_cycle::tracked]
fn verify_prior_proof(
    verifier_key: &[u32; 8],
    prior_claim: &Claim,
    prior_continuation_bytes: &[u8; CONTINUATION_DATA_SIZE],
) {
    // 1. Hash the prior continuation data.
    let continuation_digest = sp1_cycle::track!({ zkpow_sha256::of_52bytes(prior_continuation_bytes) });

    // 2. Reconstruct the prior proof's minimal public values and hash them.
    let prior_pv =
        MinimalPublicValues::success(prior_claim, continuation_digest, VerifierKeyDigest::from_raw(*verifier_key));
    let prior_pv_bytes: [u8; MINIMAL_PV_SIZE] = prior_pv.to_bytes();
    let actual_pv_hash = sp1_cycle::track!({ zkpow_sha256::of_153bytes(&prior_pv_bytes) });

    // 3. Verify the SP1 proof.
    sp1_cycle::track!({
        sp1_zkvm::lib::verify::verify_sp1_proof(verifier_key, &actual_pv_hash);
    });
}

//fn read_array_input<'g, T, const N: usize>() -> &'g [T; N] {
//    let read_vec_results = unsafe { read_vec_raw() };
//    let input_slice = std::ptr::slice_from_raw_parts(read_vec_results.ptr, read_vec_results.len);
//    let input_array = input_slice.cast::<[T; N]>();
//    let input_array_ref = unsafe { input_array.as_ref() }.expect("N byte array");
//    input_array_ref
//}
//
//fn read_array_input_mut<'g, T, const N: usize>() -> &'g mut [T; N] {
//    let read_vec_results = unsafe { read_vec_raw() };
//    let input_slice = std::ptr::slice_from_raw_parts_mut(read_vec_results.ptr, read_vec_results.len);
//    let input_array = input_slice.cast::<[T; N]>();
//    let input_array_ref = unsafe { input_array.as_mut() }.expect("N byte array");
//    input_array_ref
//}

fn read_fixed_size_input<'g, T>() -> Result<&'g T, PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts(read_vec_results.ptr, read_vec_results.len);
    let input_sized = input_slice.cast::<T>();
    unsafe { input_sized.as_ref() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)
}

fn read_fixed_size_input_mut<'g, T>() -> Result<&'g mut T, PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts_mut(read_vec_results.ptr, read_vec_results.len);
    let input_sized = input_slice.cast::<T>();
    unsafe { input_sized.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)
}

fn read_byte_slice_input<'g>() -> Result<&'g [u8], PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts(read_vec_results.ptr, read_vec_results.len);
    unsafe { input_slice.as_ref() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)
}

//fn read_typed_scalar_input_mut<'g, T>() -> Result<&'g mut T, PodCastError> {
//    let read_vec_results = unsafe { read_vec_raw() };
//    let input_slice = std::ptr::slice_from_raw_parts_mut(read_vec_results.ptr, read_vec_results.len);
//    let input_slice_ref = unsafe { input_slice.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)?; //.expect("dynamic size input");
//    let input_type = input_slice.cast::<T>();
//    unsafe { input_type.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)
//}

fn read_slice_of_fixed_size_elements_input<'g, T: Pod>() -> Result<&'g [T], PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts(read_vec_results.ptr, read_vec_results.len);
    let input_slice_ref = unsafe { input_slice.as_ref() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)?;
    let input_type = bytemuck::cast_slice::<u8, T>(input_slice_ref);
    Ok(input_type)
}

fn read_byte_slice_mut<'g>() -> Result<&'g mut [u8], PodCastError> {
    let read_vec_results = unsafe { read_vec_raw() };
    let input_slice = std::ptr::slice_from_raw_parts_mut(read_vec_results.ptr, read_vec_results.len);
    unsafe { input_slice.as_mut() }.ok_or(PodCastError::OutputSliceWouldHaveSlop)
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
    // Claim:
    // block_hash
    // genesis_hash
    // current_nbits <- remove
    // height
    // ----
    // Continuation:
    // chain_work
    // epoch_start_timestamp
    // timestamps
    let state_bytes = read_byte_slice_mut()?;

    let vk_bytes = read_fixed_size_input::<[u32; 8]>()?;
    let state: &mut State = read_fixed_size_input_mut::<State>()?;
    let headers: &[HeaderInput] = read_slice_of_fixed_size_elements_input::<HeaderInput>()?;
    let medians: &[BlockTimestamp] = read_slice_of_fixed_size_elements_input::<BlockTimestamp>()?;

    if state.height > 0 {
        let prior_claim = state.public_claim();
        let prior_continuation_bytes: [u8; CONTINUATION_DATA_SIZE] = state.continuation_bytes();
        verify_prior_proof(vk_bytes, &prior_claim, &prior_continuation_bytes);
    }

    let vk_digest = VerifierKeyDigest::from_raw(*vk_bytes);

    // Apply headers; on failure commit minimal PV and halt.
    if let Err(failure) = state.apply_headers(headers, medians) {
        let digest = compute_continuation_digest(&failure.last_valid_state);
        let pv = MinimalPublicValues::failure(
            &failure.last_valid_state.public_claim(),
            failure.error_code,
            failure.failure_height,
            digest,
            vk_digest,
        );
        commit_minimal_pv(&pv);
    }

    let digest = compute_continuation_digest(&state);
    let pv = MinimalPublicValues::success(&state.public_claim(), digest, vk_digest);
    commit_minimal_pv(&pv);
}

pub fn main() {
    inner_main().unwrap();
}
