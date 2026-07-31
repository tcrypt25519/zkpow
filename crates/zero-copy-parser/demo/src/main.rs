use zcp::ZcParse;

// Fields must implement Pod and Zeroable for bytemuck to safely cast them
#[derive(ZcParse, bytemuck::AnyBitPattern, Clone, Copy)]
pub struct Packet {
    pub header: u16,
    pub payload: u32,
}

fn main() {
    // 6 bytes total (u16 is 2 bytes, u32 is 4 bytes).
    // Note: To avoid bytemuck alignment panics on u32, ensure the initial
    // slice has the required memory alignment, or use unaligned reads in the macro.
    let mut raw_data: Vec<u8> = vec![0x01, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];

    // 1. Create the wrapper (takes ownership of mutable borrow)
    let mut parsed = ParsedPacket::from_bytes(&mut raw_data);

    // 2. Read fields
    assert_eq!(*parsed.header(), 0x0001); // Assuming little endian

    println!("header: {}", parsed.header());

    // 3. Write fields in place
    parsed.set_header(0xFFFF);
    assert_eq!(*parsed.header(), 0xFFFF);
    println!("header: {}", parsed.header());

    // 4. Extract an owned copy of the modified bytes
    let owned_bytes = parsed.copied_bytes();
    println!("payload: {}", parsed.payload());

    // The macro dynamically clipped the slice to the required size (6 bytes)
    assert_eq!(owned_bytes.len(), 6);
    assert_eq!(owned_bytes, vec![0xFF, 0xFF, 0xAA, 0xBB, 0xCC, 0xDD]);
}
