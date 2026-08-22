//! `const fn` SHA-256, for values that must be derived at compile time.

#![no_std]

#[cfg(test)]
mod tests;

const IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

const fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];

    let mut t = 0;
    while t < 16 {
        let i = t * 4;
        w[t] = ((block[i] as u32) << 24)
            | ((block[i + 1] as u32) << 16)
            | ((block[i + 2] as u32) << 8)
            | (block[i + 3] as u32);
        t += 1;
    }
    while t < 64 {
        let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
        let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
        w[t] = w[t - 16].wrapping_add(s0).wrapping_add(w[t - 7]).wrapping_add(s1);
        t += 1;
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    let mut t = 0;
    while t < 64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[t]).wrapping_add(w[t]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
        t += 1;
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

#[must_use]
pub const fn hash(message: &[u8]) -> [u8; 32] {
    let mut state = IV;
    let len = message.len();

    let mut i = 0;
    while i + 64 <= len {
        let mut block = [0u8; 64];
        let mut j = 0;
        while j < 64 {
            block[j] = message[i + j];
            j += 1;
        }
        compress(&mut state, &block);
        i += 64;
    }

    let remaining = len - i;
    let mut block = [0u8; 64];
    let mut j = 0;
    while j < remaining {
        block[j] = message[i + j];
        j += 1;
    }
    block[remaining] = 0x80;

    // The 8-byte length needs to fit after the 0x80 terminator, otherwise it spills into one more block.
    if remaining + 9 > 64 {
        compress(&mut state, &block);
        block = [0u8; 64];
    }

    let bits = (len as u64) * 8;
    let bit_bytes = bits.to_be_bytes();
    let mut k = 0;
    while k < 8 {
        block[56 + k] = bit_bytes[k];
        k += 1;
    }
    compress(&mut state, &block);

    let mut digest = [0u8; 32];
    let mut word = 0;
    while word < 8 {
        let bytes = state[word].to_be_bytes();
        digest[word * 4] = bytes[0];
        digest[word * 4 + 1] = bytes[1];
        digest[word * 4 + 2] = bytes[2];
        digest[word * 4 + 3] = bytes[3];
        word += 1;
    }
    digest
}

/// SHA-256d: `sha256(sha256(message))`, the Bitcoin block and merkle hash.
#[must_use]
pub const fn double(message: &[u8]) -> [u8; 32] {
    let first = hash(message);
    hash(&first)
}
