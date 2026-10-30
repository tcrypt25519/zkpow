use bytemuck::{Pod, PodCastError, Zeroable, cast_slice, try_cast_slice};
pub use wiretype_macro::wiretype;

pub trait WireType<T>
where
    T: Pod + Zeroable,
{
    fn to_bytes(&self) -> &[u8];
    fn parse(bytes: &[u8]) -> Result<&[T], PodCastError>;
}

// Implement for slices
impl<T> WireType<T> for &[T]
where
    T: Pod + Zeroable,
{
    fn to_bytes(&self) -> &[u8] {
        cast_slice(self)
    }

    fn parse(bytes: &[u8]) -> Result<&[T], PodCastError> {
        try_cast_slice::<u8, T>(bytes)
    }
}

// Implement for Vec
impl<T> WireType<T> for Vec<T>
where
    T: Pod + Zeroable,
{
    fn to_bytes(&self) -> &[u8] {
        cast_slice(self.as_slice())
    }

    fn parse(bytes: &[u8]) -> Result<&[T], PodCastError> {
        try_cast_slice::<u8, T>(bytes)
    }
}
