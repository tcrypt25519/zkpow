//! Reading the proof-carrying state, header batch, and hint inputs to the guest.

use alloc::vec::Vec;

use bytemuck::{Pod, PodCastError};

/// Reinterpret raw input bytes as `Pod` values without copying.
pub fn cast_slice<B: Pod>(bytes: &[u8]) -> Result<&[B], PodCastError> {
    bytemuck::try_cast_slice(bytes)
}

/// The inverse of [`cast_slice`]: the values' bytes, copied into a `Vec`.
pub fn serialize_inputs<B: Pod>(values: &[B]) -> Vec<u8> {
    bytemuck::cast_slice(values).to_vec()
}
