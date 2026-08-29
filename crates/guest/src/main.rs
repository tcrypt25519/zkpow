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
//! Output: PublicValues (185 bytes) on success or failure.

#![no_main]
sp1_zkvm::entrypoint!(main);

use bytemuck::{self, Pod, PodCastError};
use sp1_zkvm::lib::read_vec_raw;
use zkpow_core::{ApplyFailure, BlockTimestamp, HeaderInput, PublicValues, ReturnCode, State, VerifierKeyDigest};
use zkpow_params::Mainnet;
use zkpow_sha256;

/// Commit the public values and halt.
fn commit_public_values(state: &State, verifier_key: &VerifierKeyDigest, return_code: ReturnCode) -> ! {
    let bytes = PublicValues::serialize(state, verifier_key, return_code);
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
    ## Data flow

    - Read `verifier_key` as `[u32; 8]` and convert to `VerifierKeyDigest`.
    - Read `state` as a mutable `State`, `headers` as `&[HeaderInput]`, and
      `medians` as `&[BlockTimestamp]`.
    - If `state.height > 0`, serialize the prior public values as
      `state || verifier_key || success return_code`, hash them with the
      optimized SHA-256 helper, and call `verify_sp1_proof`.
    - Apply headers; on success or failure commit the updated state,
      verifier key, and actual return code as 185 byte public values and halt.

    ## State mutation

    - `State` is mutated in place by `state.apply_headers(headers, medians)`.
    - On failure, the last valid state is read from the same `&mut State`.
    - Between iterations, the host serializes the updated `State` via
      `state.to_bytes()` and the next guest deserializes it fresh from stdin.
    */

    let verifier_key: &VerifierKeyDigest = read_fixed_size_input::<VerifierKeyDigest>()?;
    let state: &mut State = read_fixed_size_input_mut::<State>()?;
    let headers: &[HeaderInput] = read_slice_of_fixed_size_elements_input::<HeaderInput>()?;
    let medians: &[BlockTimestamp] = read_slice_of_fixed_size_elements_input::<BlockTimestamp>()?;

    if state.height > 0 {
        let actual_pv_hash = sp1_cycle::track!(zkpow_sha256::of_public_values(
            bytemuck::bytes_of(state),
            verifier_key.to_le_bytes_slice(),
            ReturnCode::SUCCESS.into_u8(),
        ));
        sp1_cycle::track!(sp1_zkvm::lib::verify::verify_sp1_proof(verifier_key.as_raw(), &actual_pv_hash));
    }

    // Apply headers; on failure commit public values and halt.
    let return_code = match state.apply_headers::<Mainnet>(headers, medians) {
        Ok(()) => ReturnCode::SUCCESS,
        Err(ApplyFailure { error_code }) => ReturnCode::from(error_code),
    };
    commit_public_values(&state, verifier_key, return_code);
}

pub fn main() {
    inner_main().expect("guest input parsing failed");
}
