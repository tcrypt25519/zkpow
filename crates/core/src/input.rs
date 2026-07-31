//! Reading the proof-carrying state, header batch, and hint inputs to the guest.

use alloc::vec::Vec;

use bytemuck::{Pod, PodCastError};

pub fn cast_slice<B: Pod>(bytes: &[u8]) -> Result<&[B], PodCastError> {
    bytemuck::try_cast_slice(bytes)
}

pub fn serialize_inputs<B: Pod>(values: &[B]) -> Vec<u8> {
    bytemuck::cast_slice(values).to_vec()
}
