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
use zkpow_core::{BlockTimestamp, HeaderInput, PublicValues, ReportableError, ReturnCode, State, VerifierKeyDigest};
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

fn read_slice_of_fixed_size_len<'g, T: Pod>() -> Result<&'g [T], PodCastError> {
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

    - `State` is mutated in place by `state.apply_headers_from_genesis(headers, medians)`
      on the first IVC batch, otherwise `state.apply_headers(headers, medians)`.
    - On failure, the last valid state is read from the same `&mut State`.
    - Between iterations, the host serializes the updated `State` via
      `state.to_bytes()` and the next guest deserializes it fresh from stdin.
    */

    let verifier_key: &VerifierKeyDigest = read_fixed_size_input::<VerifierKeyDigest>()?;
    let state: &mut State = read_fixed_size_input_mut::<State>()?;
    let headers: &[HeaderInput] = read_slice_of_fixed_size_len::<HeaderInput>()?;
    let medians: &[BlockTimestamp] = read_slice_of_fixed_size_len::<BlockTimestamp>()?;

    let apply_result = if state.height == 0 {
        state.apply_headers_from_genesis::<Mainnet>(headers, medians)
    } else {
        let actual_pv_hash =
            zkpow_sha256::of_public_values(state.as_bytes(), verifier_key.as_bytes(), ReturnCode::SUCCESS.into_u8());
        sp1_zkvm::lib::verify::verify_sp1_proof(verifier_key.as_raw(), &actual_pv_hash);

        state.apply_headers::<Mainnet>(headers, medians)
    };

    let return_code = match apply_result {
        Ok(()) => ReturnCode::SUCCESS,
        Err(err) => err.return_code(),
    };
    commit_public_values(&state, verifier_key, return_code);
}

pub fn main() {
    inner_main().expect("guest input parsing failed");
}
