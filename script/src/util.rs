//! Utilities for the Bitcoin header chain prover script.

use sha2::{Digest, Sha256};

/// Load raw 80-byte concatenated headers from the SQLite database.
pub fn load_headers_from_db(db_path: &str, start_height: u64, count: u64) -> Vec<u8> {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT raw_header FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .expect("failed to prepare SQL statement");

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .expect("failed to execute query");

    let mut all_headers = Vec::with_capacity((count * 80) as usize);
    let mut loaded = 0u64;

    for row_result in rows {
        let header_bytes: Vec<u8> = row_result.expect("failed to read raw_header from database");
        assert_eq!(
            header_bytes.len(),
            80,
            "Expected 80-byte header at height {}, got {} bytes",
            start_height + loaded,
            header_bytes.len()
        );
        all_headers.extend_from_slice(&header_bytes);
        loaded += 1;
    }

    assert_eq!(
        loaded, count,
        "Expected to load {} headers, but only loaded {} from database",
        count, loaded
    );

    all_headers
}

/// Compute double SHA-256 of the given data (host-side).
pub fn double_sha256_host(data: &[u8]) -> [u8; 32] {
    let inner = Sha256::digest(data);
    let outer = Sha256::digest(inner);
    outer.into()
}

/// Convert compact "bits" to a 32-byte target (little-endian).
pub fn bits_to_target(bits: u32) -> [u8; 32] {
    let exponent = (bits >> 24) as u32;
    let mantissa = bits & 0x00ffffff;
    let mut target = [0u8; 32];
    if mantissa == 0 {
        return target;
    }
    let byte_offset = (exponent - 3) as usize;
    if byte_offset < 32 {
        target[byte_offset] = (mantissa & 0xff) as u8;
    }
    if byte_offset + 1 < 32 {
        target[byte_offset + 1] = ((mantissa >> 8) & 0xff) as u8;
    }
    if byte_offset + 2 < 32 {
        target[byte_offset + 2] = ((mantissa >> 16) & 0xff) as u8;
    }
    target
}

/// Compute work = floor(2^256 / (target + 1)) as [u64; 4] LE.
/// Canonical Bitcoin formula — no simplifications.
pub fn work_from_bits(bits: u32) -> [u64; 4] {
    let exponent = (bits >> 24) as u32;
    let mantissa = bits & 0x00ffffff;
    let k = 8 * (exponent - 3);
    let n = 256 - k;

    if mantissa == 0 || n == 0 {
        return [0; 4];
    }

    // R = 2^n mod mantissa
    let r = pow_mod_2(n, mantissa);

    // Q = (2^n - R) / mantissa = floor(2^n / mantissa)
    let q = div_2n_minus_r_by_u32(n, r, mantissa);

    // work = Q if Q <= R * 2^k, else Q - 1
    let mut work = q;
    if !q_le_r_shifted(&q, r, k) {
        for i in 0..4 {
            if work[i] > 0 {
                work[i] -= 1;
                break;
            } else {
                work[i] = u64::MAX;
            }
        }
    }

    work
}

fn pow_mod_2(exp: u32, m: u32) -> u32 {
    if m <= 1 {
        return 0;
    }
    let m64 = m as u64;
    let mut result: u64 = 1;
    let mut base: u64 = 2;
    let mut e = exp;
    while e > 0 {
        if e & 1 != 0 {
            result = (result * base) % m64;
        }
        base = (base * base) % m64;
        e >>= 1;
    }
    result as u32
}

fn div_2n_minus_r_by_u32(n: u32, r: u32, m: u32) -> [u64; 4] {
    if n <= 63 {
        let val = ((1u64 << n) - r as u64) / m as u64;
        return [val, 0, 0, 0];
    }
    if n <= 127 {
        let val = ((1u128 << n) - r as u128) / m as u128;
        return [val as u64, (val >> 64) as u64, 0, 0];
    }

    let bit_limb = (n / 64) as usize;
    let bit_offset = (n % 64) as u32;

    let mut limbs = [0u64; 5];
    limbs[0] = u64::MAX.wrapping_sub(r as u64 - 1);
    for i in 1..bit_limb.min(4) {
        limbs[i] = u64::MAX;
    }
    if bit_limb < 4 {
        limbs[bit_limb] = (1u64 << bit_offset).wrapping_sub(1);
    } else if bit_limb == 4 {
        limbs[4] = (1u64 << bit_offset).wrapping_sub(1);
    }

    let m64 = m as u64;
    let mut q = [0u64; 4];
    let mut rem = 0u64;
    for i in (0..5).rev() {
        let val = ((rem as u128) << 64) | (limbs[i] as u128);
        let quot = (val / m64 as u128) as u64;
        rem = (val % m64 as u128) as u64;
        if i < 4 {
            q[i] = quot;
        }
    }

    q
}

fn q_le_r_shifted(q: &[u64; 4], r: u32, k: u32) -> bool {
    if r == 0 {
        return q.iter().all(|&x| x == 0);
    }
    if k >= 256 {
        return true;
    }

    let r64 = r as u64;
    let lo = (k % 64) as u32;
    let hi_limb = (k / 64) as usize;

    let mut rv = [0u64; 4];
    if hi_limb < 4 {
        rv[hi_limb] = r64 << lo;
    }
    if hi_limb + 1 < 4 && lo > 0 {
        rv[hi_limb + 1] = r64 >> (64 - lo);
    }

    for i in (0..4).rev() {
        if q[i] < rv[i] {
            return true;
        }
        if q[i] > rv[i] {
            return false;
        }
    }
    true
}

/// Add two u256 values represented as [u64; 4] little-endian arrays.
pub fn u256_add(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut result = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let sum = (a[i] as u128) + (b[i] as u128) + carry;
        result[i] = sum as u64;
        carry = sum >> 64;
    }
    result
}

/// Build the expected public values buffer for verification.
///
/// Layout (237 bytes total) — MUST match program's commit order:
///   0..32:    genesis_hash
///  32..64:    final_header_hash
///  64..72:    num_headers (u64 LE)
///  72..152:   final_header (80 raw bytes)
/// 152..184:   cumulative_chain_work (4 × u64 LE)
/// 184..188:   last_epoch_start_timestamp (u32 LE)
/// 188..232:   median_timestamp_buffer ([u32; 11] LE)
/// 232..233:   success_code (u8)
/// 233..237:   error_detail (u32 LE)
///
/// Note: median_count is NOT included — it's derivable from num_headers:
///   count = min(11, num_headers - 1) for num_headers > 0, else 0
pub fn build_expected_public_values(
    genesis_hash: &[u8; 32],
    final_hash: &[u8; 32],
    num_headers: u64,
    final_header: &[u8; 80],
    cumulative_chain_work: [u64; 4],
    last_epoch_start_timestamp: u32,
    median_timestamps: [u32; 11],
) -> Vec<u8> {
    let mut pv = Vec::with_capacity(237);
    pv.extend_from_slice(&genesis_hash[..]);
    pv.extend_from_slice(&final_hash[..]);
    pv.extend_from_slice(&num_headers.to_le_bytes());
    pv.extend_from_slice(&final_header[..]);
    for &word in &cumulative_chain_work {
        pv.extend_from_slice(&word.to_le_bytes());
    }
    pv.extend_from_slice(&last_epoch_start_timestamp.to_le_bytes());
    for &ts in &median_timestamps {
        pv.extend_from_slice(&ts.to_le_bytes());
    }
    pv.push(0); // success_code = 0
    pv.extend_from_slice(&0u32.to_le_bytes()); // error_detail = 0
    pv
}

/// Compute the SHA-256 hash of all committed public values bytes.
pub fn compute_pv_digest(committed_bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(committed_bytes).into()
}
