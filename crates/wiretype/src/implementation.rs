use bytemuck::{Pod, PodCastError};

/// Conversion between wire values and their native memory representation.
pub trait WireType {
    /// A fixed-size byte array for a value, or a flat byte vector for a collection.
    type Bytes;
    /// The borrowed value returned by parsing; `[T]` for collections.
    type Inner: ?Sized;

    fn to_bytes(&self) -> Self::Bytes;
    fn as_bytes(&self) -> &[u8];
    fn parse(bytes: &[u8]) -> Result<&Self::Inner, PodCastError>;
}

impl<T> WireType for [T]
where
    T: WireType<Inner = T> + Pod,
{
    type Bytes = Vec<u8>;
    type Inner = [T];

    fn to_bytes(&self) -> Self::Bytes {
        self.as_bytes().to_vec()
    }

    fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(self)
    }

    fn parse(bytes: &[u8]) -> Result<&[T], PodCastError> {
        bytemuck::try_cast_slice(bytes)
    }
}

impl<T> WireType for &[T]
where
    T: WireType<Inner = T> + Pod,
{
    type Bytes = <[T] as WireType>::Bytes;
    type Inner = [T];

    fn to_bytes(&self) -> Self::Bytes {
        <[T] as WireType>::to_bytes(self)
    }

    fn as_bytes(&self) -> &[u8] {
        <[T] as WireType>::as_bytes(self)
    }

    fn parse(bytes: &[u8]) -> Result<&[T], PodCastError> {
        <[T] as WireType>::parse(bytes)
    }
}

impl<T> WireType for Vec<T>
where
    T: WireType<Inner = T> + Pod,
{
    type Bytes = <[T] as WireType>::Bytes;
    type Inner = [T];

    fn to_bytes(&self) -> Self::Bytes {
        self.as_slice().to_bytes()
    }

    fn as_bytes(&self) -> &[u8] {
        self.as_slice().as_bytes()
    }

    fn parse(bytes: &[u8]) -> Result<&[T], PodCastError> {
        <[T] as WireType>::parse(bytes)
    }
}
