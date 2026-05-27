//! SHA-256 via SP1 precompile syscalls.
//!
//! Four specialized functions for the exact sizes we need:
//! - `sha256_80bytes`: hashes exactly 80 bytes (Bitcoin block header)
//! - `sha256_32bytes`: hashes exactly 32 bytes (intermediate hash output)
//! - `sha256_169bytes`: hashes exactly 169 bytes (minimal public values)
//! - `sha256_264bytes`: hashes exactly 264 bytes (serialized recursive state)
//! - `sha256d_80bytes`: hashes exactly 80 bytes with SHA256d
//!
//! No loops, no branching on block count — the padding and block layout
//! are hardcoded for each size.

use sp1_zkvm::syscalls::{syscall_sha256_compress, syscall_sha256_extend};

/// SHA-256 IV constants.
const SHA256_IV: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Extract a 32-byte hash from the SHA-256 state.
/// The state stores 8 × u64; each contributes its low 32 bits in big-endian order.
fn state_to_hash(state: &[u64; 8]) -> [u8; 32] {
    let b0 = state[0].to_be_bytes();
    let b1 = state[1].to_be_bytes();
    let b2 = state[2].to_be_bytes();
    let b3 = state[3].to_be_bytes();
    let b4 = state[4].to_be_bytes();
    let b5 = state[5].to_be_bytes();
    let b6 = state[6].to_be_bytes();
    let b7 = state[7].to_be_bytes();
    [
        b0[4], b0[5], b0[6], b0[7], //
        b1[4], b1[5], b1[6], b1[7], //
        b2[4], b2[5], b2[6], b2[7], //
        b3[4], b3[5], b3[6], b3[7], //
        b4[4], b4[5], b4[6], b4[7], //
        b5[4], b5[5], b5[6], b5[7], //
        b6[4], b6[5], b6[6], b6[7], //
        b7[4], b7[5], b7[6], b7[7], //
    ]
}

/// Convert 4 bytes to a big-endian u64.
#[inline]
const fn be_u64(a: u8, b: u8, c: u8, d: u8) -> u64 {
    ((a as u64) << 24) | ((b as u64) << 16) | ((c as u64) << 8) | (d as u64)
}

/// Compute SHA-256 of exactly 80 bytes.
///
/// Produces two blocks:
/// - Block 1: bytes 0–63 of the input (all data)
/// - Block 2: bytes 64–79 (16 data bytes) + 0x80 + 47 zeros + 8-byte length (640 bits)
pub fn sha256_80bytes(data: &[u8; 80]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    w[0] = be_u64(data[0], data[1], data[2], data[3]);
    w[1] = be_u64(data[4], data[5], data[6], data[7]);
    w[2] = be_u64(data[8], data[9], data[10], data[11]);
    w[3] = be_u64(data[12], data[13], data[14], data[15]);
    w[4] = be_u64(data[16], data[17], data[18], data[19]);
    w[5] = be_u64(data[20], data[21], data[22], data[23]);
    w[6] = be_u64(data[24], data[25], data[26], data[27]);
    w[7] = be_u64(data[28], data[29], data[30], data[31]);
    w[8] = be_u64(data[32], data[33], data[34], data[35]);
    w[9] = be_u64(data[36], data[37], data[38], data[39]);
    w[10] = be_u64(data[40], data[41], data[42], data[43]);
    w[11] = be_u64(data[44], data[45], data[46], data[47]);
    w[12] = be_u64(data[48], data[49], data[50], data[51]);
    w[13] = be_u64(data[52], data[53], data[54], data[55]);
    w[14] = be_u64(data[56], data[57], data[58], data[59]);
    w[15] = be_u64(data[60], data[61], data[62], data[63]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–79 + padding + length ──
    // Layout: [data 16 bytes | 0x80 | 47× 0x00 | 8-byte big-endian length 640 = 0x280]
    let mut w = [0u64; 64];
    w[0] = be_u64(data[64], data[65], data[66], data[67]);
    w[1] = be_u64(data[68], data[69], data[70], data[71]);
    w[2] = be_u64(data[72], data[73], data[74], data[75]);
    w[3] = be_u64(data[76], data[77], data[78], data[79]);
    // 0x80 byte at word index 4, byte 0 → 0x80000000
    w[4] = 0x80000000;
    // w[15] = lower 32 bits of length = 640 = 0x280
    w[15] = 0x280;

    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 32 bytes.
///
/// Produces one block:
/// bytes 0–31 (all data) + 0x80 + 23 zeros + 8-byte length (256 bits)
pub fn sha256_32bytes(data: &[u8; 32]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Single block: 32 bytes + padding + length ──
    let mut w = [0u64; 64];
    w[0] = be_u64(data[0], data[1], data[2], data[3]);
    w[1] = be_u64(data[4], data[5], data[6], data[7]);
    w[2] = be_u64(data[8], data[9], data[10], data[11]);
    w[3] = be_u64(data[12], data[13], data[14], data[15]);
    w[4] = be_u64(data[16], data[17], data[18], data[19]);
    w[5] = be_u64(data[20], data[21], data[22], data[23]);
    w[6] = be_u64(data[24], data[25], data[26], data[27]);
    w[7] = be_u64(data[28], data[29], data[30], data[31]);
    // 0x80 at word index 8, byte 0 → 0x80000000
    w[8] = 0x80000000;
    // w[15] = length in bits = 256 = 0x100
    w[15] = 0x100;

    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 264 bytes.
///
/// Produces five blocks:
/// - Block 1: bytes 0–63
/// - Block 2: bytes 64–127
/// - Block 3: bytes 128–191
/// - Block 4: bytes 192–255
/// - Block 5: bytes 256–263 + 0x80 + zeros + 8-byte length (2112 bits)
#[allow(dead_code)]
pub fn sha256_264bytes(data: &[u8; 264]) -> [u8; 32] {
    let mut state = SHA256_IV;

    let mut w = [0u64; 64];
    w[0] = be_u64(data[0], data[1], data[2], data[3]);
    w[1] = be_u64(data[4], data[5], data[6], data[7]);
    w[2] = be_u64(data[8], data[9], data[10], data[11]);
    w[3] = be_u64(data[12], data[13], data[14], data[15]);
    w[4] = be_u64(data[16], data[17], data[18], data[19]);
    w[5] = be_u64(data[20], data[21], data[22], data[23]);
    w[6] = be_u64(data[24], data[25], data[26], data[27]);
    w[7] = be_u64(data[28], data[29], data[30], data[31]);
    w[8] = be_u64(data[32], data[33], data[34], data[35]);
    w[9] = be_u64(data[36], data[37], data[38], data[39]);
    w[10] = be_u64(data[40], data[41], data[42], data[43]);
    w[11] = be_u64(data[44], data[45], data[46], data[47]);
    w[12] = be_u64(data[48], data[49], data[50], data[51]);
    w[13] = be_u64(data[52], data[53], data[54], data[55]);
    w[14] = be_u64(data[56], data[57], data[58], data[59]);
    w[15] = be_u64(data[60], data[61], data[62], data[63]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    let mut w = [0u64; 64];
    w[0] = be_u64(data[64], data[65], data[66], data[67]);
    w[1] = be_u64(data[68], data[69], data[70], data[71]);
    w[2] = be_u64(data[72], data[73], data[74], data[75]);
    w[3] = be_u64(data[76], data[77], data[78], data[79]);
    w[4] = be_u64(data[80], data[81], data[82], data[83]);
    w[5] = be_u64(data[84], data[85], data[86], data[87]);
    w[6] = be_u64(data[88], data[89], data[90], data[91]);
    w[7] = be_u64(data[92], data[93], data[94], data[95]);
    w[8] = be_u64(data[96], data[97], data[98], data[99]);
    w[9] = be_u64(data[100], data[101], data[102], data[103]);
    w[10] = be_u64(data[104], data[105], data[106], data[107]);
    w[11] = be_u64(data[108], data[109], data[110], data[111]);
    w[12] = be_u64(data[112], data[113], data[114], data[115]);
    w[13] = be_u64(data[116], data[117], data[118], data[119]);
    w[14] = be_u64(data[120], data[121], data[122], data[123]);
    w[15] = be_u64(data[124], data[125], data[126], data[127]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    let mut w = [0u64; 64];
    w[0] = be_u64(data[128], data[129], data[130], data[131]);
    w[1] = be_u64(data[132], data[133], data[134], data[135]);
    w[2] = be_u64(data[136], data[137], data[138], data[139]);
    w[3] = be_u64(data[140], data[141], data[142], data[143]);
    w[4] = be_u64(data[144], data[145], data[146], data[147]);
    w[5] = be_u64(data[148], data[149], data[150], data[151]);
    w[6] = be_u64(data[152], data[153], data[154], data[155]);
    w[7] = be_u64(data[156], data[157], data[158], data[159]);
    w[8] = be_u64(data[160], data[161], data[162], data[163]);
    w[9] = be_u64(data[164], data[165], data[166], data[167]);
    w[10] = be_u64(data[168], data[169], data[170], data[171]);
    w[11] = be_u64(data[172], data[173], data[174], data[175]);
    w[12] = be_u64(data[176], data[177], data[178], data[179]);
    w[13] = be_u64(data[180], data[181], data[182], data[183]);
    w[14] = be_u64(data[184], data[185], data[186], data[187]);
    w[15] = be_u64(data[188], data[189], data[190], data[191]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    let mut w = [0u64; 64];
    w[0] = be_u64(data[192], data[193], data[194], data[195]);
    w[1] = be_u64(data[196], data[197], data[198], data[199]);
    w[2] = be_u64(data[200], data[201], data[202], data[203]);
    w[3] = be_u64(data[204], data[205], data[206], data[207]);
    w[4] = be_u64(data[208], data[209], data[210], data[211]);
    w[5] = be_u64(data[212], data[213], data[214], data[215]);
    w[6] = be_u64(data[216], data[217], data[218], data[219]);
    w[7] = be_u64(data[220], data[221], data[222], data[223]);
    w[8] = be_u64(data[224], data[225], data[226], data[227]);
    w[9] = be_u64(data[228], data[229], data[230], data[231]);
    w[10] = be_u64(data[232], data[233], data[234], data[235]);
    w[11] = be_u64(data[236], data[237], data[238], data[239]);
    w[12] = be_u64(data[240], data[241], data[242], data[243]);
    w[13] = be_u64(data[244], data[245], data[246], data[247]);
    w[14] = be_u64(data[248], data[249], data[250], data[251]);
    w[15] = be_u64(data[252], data[253], data[254], data[255]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    let mut w = [0u64; 64];
    w[0] = be_u64(data[256], data[257], data[258], data[259]);
    w[1] = be_u64(data[260], data[261], data[262], data[263]);
    w[2] = 0x80000000;
    w[15] = 0x840;
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA-256 of exactly 169 bytes.
///
/// Produces three blocks:
/// - Block 1: bytes 0–63
/// - Block 2: bytes 64–127
/// - Block 3: bytes 128–168 (41 data bytes) + 0x80 + zeros + 8-byte length (1352 bits = 0x548)
pub fn sha256_169bytes(data: &[u8; 169]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    w[0] = be_u64(data[0], data[1], data[2], data[3]);
    w[1] = be_u64(data[4], data[5], data[6], data[7]);
    w[2] = be_u64(data[8], data[9], data[10], data[11]);
    w[3] = be_u64(data[12], data[13], data[14], data[15]);
    w[4] = be_u64(data[16], data[17], data[18], data[19]);
    w[5] = be_u64(data[20], data[21], data[22], data[23]);
    w[6] = be_u64(data[24], data[25], data[26], data[27]);
    w[7] = be_u64(data[28], data[29], data[30], data[31]);
    w[8] = be_u64(data[32], data[33], data[34], data[35]);
    w[9] = be_u64(data[36], data[37], data[38], data[39]);
    w[10] = be_u64(data[40], data[41], data[42], data[43]);
    w[11] = be_u64(data[44], data[45], data[46], data[47]);
    w[12] = be_u64(data[48], data[49], data[50], data[51]);
    w[13] = be_u64(data[52], data[53], data[54], data[55]);
    w[14] = be_u64(data[56], data[57], data[58], data[59]);
    w[15] = be_u64(data[60], data[61], data[62], data[63]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–127 ──
    let mut w = [0u64; 64];
    w[0] = be_u64(data[64], data[65], data[66], data[67]);
    w[1] = be_u64(data[68], data[69], data[70], data[71]);
    w[2] = be_u64(data[72], data[73], data[74], data[75]);
    w[3] = be_u64(data[76], data[77], data[78], data[79]);
    w[4] = be_u64(data[80], data[81], data[82], data[83]);
    w[5] = be_u64(data[84], data[85], data[86], data[87]);
    w[6] = be_u64(data[88], data[89], data[90], data[91]);
    w[7] = be_u64(data[92], data[93], data[94], data[95]);
    w[8] = be_u64(data[96], data[97], data[98], data[99]);
    w[9] = be_u64(data[100], data[101], data[102], data[103]);
    w[10] = be_u64(data[104], data[105], data[106], data[107]);
    w[11] = be_u64(data[108], data[109], data[110], data[111]);
    w[12] = be_u64(data[112], data[113], data[114], data[115]);
    w[13] = be_u64(data[116], data[117], data[118], data[119]);
    w[14] = be_u64(data[120], data[121], data[122], data[123]);
    w[15] = be_u64(data[124], data[125], data[126], data[127]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 3: bytes 128–168 (41 bytes) + padding + length ──
    // 41 data bytes, then 0x80, then zeros, then 8-byte length (1352 bits = 0x548)
    let mut w = [0u64; 64];
    w[0] = be_u64(data[128], data[129], data[130], data[131]);
    w[1] = be_u64(data[132], data[133], data[134], data[135]);
    w[2] = be_u64(data[136], data[137], data[138], data[139]);
    w[3] = be_u64(data[140], data[141], data[142], data[143]);
    w[4] = be_u64(data[144], data[145], data[146], data[147]);
    w[5] = be_u64(data[148], data[149], data[150], data[151]);
    w[6] = be_u64(data[152], data[153], data[154], data[155]);
    w[7] = be_u64(data[156], data[157], data[158], data[159]);
    w[8] = be_u64(data[160], data[161], data[162], data[163]);
    w[9] = be_u64(data[164], data[165], data[166], data[167]);
    // data[168] in high byte, then 0x80 in next byte
    w[10] = ((data[168] as u64) << 24) | 0x0080_0000;
    // w[15] = length in bits = 169 * 8 = 1352 = 0x548
    w[15] = 0x548;
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}

/// Compute SHA256d of exactly 80 bytes: SHA-256(SHA-256(data)).
pub fn sha256d_80bytes(data: &[u8; 80]) -> [u8; 32] {
    let inner = sha256_80bytes(data);
    sha256_32bytes(&inner)
}

/// Compute SHA-256 of exactly 116 bytes.
///
/// Produces two blocks:
/// - Block 1: bytes 0–63
/// - Block 2: bytes 64–115 (52 data bytes) + 0x80 + zeros + 8-byte length (928 bits = 0x3A0)
pub fn sha256_116bytes(data: &[u8; 116]) -> [u8; 32] {
    let mut state = SHA256_IV;

    // ── Block 1: bytes 0–63 ──
    let mut w = [0u64; 64];
    w[0] = be_u64(data[0], data[1], data[2], data[3]);
    w[1] = be_u64(data[4], data[5], data[6], data[7]);
    w[2] = be_u64(data[8], data[9], data[10], data[11]);
    w[3] = be_u64(data[12], data[13], data[14], data[15]);
    w[4] = be_u64(data[16], data[17], data[18], data[19]);
    w[5] = be_u64(data[20], data[21], data[22], data[23]);
    w[6] = be_u64(data[24], data[25], data[26], data[27]);
    w[7] = be_u64(data[28], data[29], data[30], data[31]);
    w[8] = be_u64(data[32], data[33], data[34], data[35]);
    w[9] = be_u64(data[36], data[37], data[38], data[39]);
    w[10] = be_u64(data[40], data[41], data[42], data[43]);
    w[11] = be_u64(data[44], data[45], data[46], data[47]);
    w[12] = be_u64(data[48], data[49], data[50], data[51]);
    w[13] = be_u64(data[52], data[53], data[54], data[55]);
    w[14] = be_u64(data[56], data[57], data[58], data[59]);
    w[15] = be_u64(data[60], data[61], data[62], data[63]);
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    // ── Block 2: bytes 64–115 (52 bytes) + padding + length ──
    let mut w = [0u64; 64];
    w[0] = be_u64(data[64], data[65], data[66], data[67]);
    w[1] = be_u64(data[68], data[69], data[70], data[71]);
    w[2] = be_u64(data[72], data[73], data[74], data[75]);
    w[3] = be_u64(data[76], data[77], data[78], data[79]);
    w[4] = be_u64(data[80], data[81], data[82], data[83]);
    w[5] = be_u64(data[84], data[85], data[86], data[87]);
    w[6] = be_u64(data[88], data[89], data[90], data[91]);
    w[7] = be_u64(data[92], data[93], data[94], data[95]);
    w[8] = be_u64(data[96], data[97], data[98], data[99]);
    w[9] = be_u64(data[100], data[101], data[102], data[103]);
    w[10] = be_u64(data[104], data[105], data[106], data[107]);
    w[11] = be_u64(data[108], data[109], data[110], data[111]);
    w[12] = be_u64(data[112], data[113], data[114], data[115]);
    // 0x80 at word index 13, byte 0 → 0x80000000
    w[13] = 0x80000000;
    // w[15] = length in bits = 116 * 8 = 928 = 0x3A0
    w[15] = 0x3A0;
    syscall_sha256_extend(&mut w);
    syscall_sha256_compress(&mut w, &mut state);

    state_to_hash(&state)
}
