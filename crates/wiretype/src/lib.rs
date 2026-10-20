pub use wiretype_macro::wiretype;

// pub trait WireType: Pod + Zeroable {
//     #[must_use]
//     fn as_bytes(&self) -> &[u8] {
//         bytemuck::bytes_of(self)
//     }

//     #[must_use]
//     fn parse(bytes: &[u8]) -> Result<&Self, PodCastError> {
//         bytemuck::try_from_bytes::<Self>(bytes)
//     }
// }
