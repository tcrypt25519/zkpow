use wiretype::WireType;

fn main() {
    // u32 is Pod, but it does not implement WireType.
    let values = vec![1u32, 2];
    values.to_bytes();
    values.as_slice().to_bytes();
}
