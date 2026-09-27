/// `size_of`/`align_of` as associated consts, so any sized type can be asked for them by name.
pub trait ByteInfo {
    const SIZE: usize;
    const ALIGN: usize;
}
impl<T> ByteInfo for T
where
    T: Sized,
{
    const SIZE: usize = size_of::<T>();
    const ALIGN: usize = align_of::<T>();
}
