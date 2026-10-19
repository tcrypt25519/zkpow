use bytemuck::Pod;

/// `size_of`/`align_of` as associated consts for types whose memory layout is their wire format.
pub trait WireBytes: Pod {
    const SIZE: usize = size_of::<Self>();
    const ALIGN: usize = align_of::<Self>();
}

/// The value's wire bytes. Zero-copy; this is what everything but const evaluation should use.
#[must_use]
pub fn as_wire_bytes<T: WireBytes>(value: &T) -> &[u8] {
    bytemuck::bytes_of(value)
}
