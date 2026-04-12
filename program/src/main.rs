//! Bitcoin Header Chain Prover — zkVM Program
//!
//! Validates a batch of Bitcoin block headers by verifying:
//! - Genesis block hash matches the expected genesis (for height 0)
//! - Chain linkage: each header's prev_blockhash matches the previous block's hash
//! - Proof-of-work: each header's double SHA-256 hash meets the difficulty target from bits
//! - Difficulty retargeting every 2016 blocks
//! - BIP113 median timestamp check
//! - Cumulative chain work using Bitcoin's canonical formula

#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};

// ============================================================================
// Error codes — committed to public values on failure.
// The program exits cleanly (HALT 0) with a non-zero status, producing a
// valid proof that "validation failed at header X for reason Y".
// ============================================================================

const STATUS_SUCCESS: u8 = 0;
const STATUS_GENESIS_HASH_MISMATCH: u8 = 1;
const STATUS_PREV_BLOCKHASH_MISMATCH: u8 = 2;
const STATUS_POW_INSUFFICIENT: u8 = 3;
const STATUS_TIMESTAMP_TOO_OLD: u8 = 4;
const STATUS_TIMESTAMP_FUTURE: u8 = 5;
const STATUS_BITS_MISMATCH: u8 = 6;
const STATUS_HEADER_COUNT_MISMATCH: u8 = 7;
const STATUS_PREV_PROOF_TOO_SHORT: u8 = 8;
const STATUS_PREV_PROOF_GENESIS_MISMATCH: u8 = 9;
const STATUS_PREV_PROOF_FAILED: u8 = 10;

/// Commit all public values for an error exit and return.
/// This produces a valid proof with a non-zero success_code, proving that
/// the program ran correctly and found an error at the given header index.
///
/// Note: We use a loop with `break` to simulate early exit, since the zkVM
/// requires reaching HALT(0) normally (via return from main) to produce a proof.
struct ValidationError {
    code: u8,
    detail: u32,
}

fn commit_error_and_exit(
    _genesis_hash: &[u8; 32],
    prev_hash: &[u8; 32],
    cumulative_chain_work: [u64; 4],
    last_epoch_start_timestamp: u32,
    median_timestamps: [u32; 11],
    start_height: u64,
    num_headers: u64,
    error_code: u8,
    detail: u32,
) -> ! {
    sp1_zkvm::io::commit_slice(prev_hash);
    let total_validated = start_height + num_headers;
    sp1_zkvm::io::commit_slice(&total_validated.to_le_bytes());
    sp1_zkvm::io::commit_slice(&[0u8; 80]); // placeholder final_header
    for &word in &cumulative_chain_work {
        sp1_zkvm::io::commit_slice(&word.to_le_bytes());
    }
    sp1_zkvm::io::commit_slice(&last_epoch_start_timestamp.to_le_bytes());
    for &ts in &median_timestamps {
        sp1_zkvm::io::commit_slice(&ts.to_le_bytes());
    }
    sp1_zkvm::io::commit_slice(&[error_code]);
    sp1_zkvm::io::commit_slice(&detail.to_le_bytes());

    unsafe {
        sp1_zkvm::syscalls::syscall_halt(0);
    }
}

fn double_sha256(data: &[u8]) -> [u8; 32] {
    let inner = Sha256::digest(data);
    let outer = Sha256::digest(inner);
    outer.into()
}

fn bits_to_target(bits: u32) -> [u8; 32] {
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

/// Extract bits from raw 80-byte header and convert to target.
fn bits_to_target_from_header_bytes(header_bytes: &[u8]) -> [u8; 32] {
    let bits = u32::from_le_bytes(header_bytes[72..76].try_into().unwrap());
    bits_to_target(bits)
}

fn hash_meets_target(hash: &[u8; 32], bits: u32) -> bool {
    let target = bits_to_target(bits);
    for i in (0..32).rev() {
        if hash[i] > target[i] {
            return false;
        }
        if hash[i] < target[i] {
            return true;
        }
    }
    true
}

fn target_to_bits(target: &[u8; 32]) -> u32 {
    let mut high_byte = 31usize;
    while high_byte > 0 && target[high_byte] == 0 {
        high_byte -= 1;
    }
    if target[high_byte] == 0 {
        return 0;
    }
    let bit_length = high_byte * 8 + (8 - target[high_byte].leading_zeros() as usize);
    let nbytes = (bit_length + 7) / 8;
    let mantissa: u32 = if high_byte >= 2 {
        (target[high_byte] as u32) << 16
            | (target[high_byte - 1] as u32) << 8
            | target[high_byte - 2] as u32
    } else if high_byte == 1 {
        (target[1] as u32) << 8 | target[0] as u32
    } else {
        target[0] as u32
    };
    let (mantissa, nbytes) = if mantissa & 0x800000 != 0 {
        (mantissa >> 8, nbytes + 1)
    } else {
        (mantissa, nbytes)
    };
    ((nbytes as u32) << 24) | (mantissa & 0x00ffffff)
}

fn u256_add(a: [u64; 4], b: [u64; 4]) -> [u64; 4] {
    let mut result = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let sum = (a[i] as u128) + (b[i] as u128) + carry;
        result[i] = sum as u64;
        carry = sum >> 64;
    }
    result
}

// ============================================================================
// Canonical chain work: floor(2^256 / (target + 1))
// ============================================================================

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

fn work_from_bits(bits: u32) -> [u64; 4] {
    let exponent = (bits >> 24) as u32;
    let mantissa = bits & 0x00ffffff;
    let k = 8 * (exponent - 3);
    let n = 256 - k;

    if mantissa == 0 || n == 0 {
        return [0; 4];
    }

    let r = pow_mod_2(n, mantissa);
    let q = div_2n_minus_r_by_u32(n, r, mantissa);

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

pub fn main() {
    // Read genesis hash and commit
    let expected_genesis_hash = sp1_zkvm::io::read::<[u8; 32]>();
    sp1_zkvm::io::commit_slice(&expected_genesis_hash);

    // Check if we're extending a previous proof or starting from genesis
    let has_prev_proof = sp1_zkvm::io::read::<bool>();

    let (
        mut prev_hash,
        mut cumulative_chain_work,
        mut last_epoch_start_timestamp,
        mut prev_target,
        mut median_timestamps,
        mut median_count,
        start_height,
        _prev_num_headers,
    ) = if has_prev_proof {
        let _prev_vk_digest = sp1_zkvm::io::read::<[u32; 8]>();
        let _prev_pv_digest = sp1_zkvm::io::read::<[u8; 32]>();
        let prev_public_values = sp1_zkvm::io::read_vec();

        sp1_zkvm::lib::verify::verify_sp1_proof(&_prev_vk_digest, &_prev_pv_digest);

        if prev_public_values.len() < 237 {
            panic!("Previous proof public values too short");
        }

        let prev_genesis: [u8; 32] = prev_public_values[0..32].try_into().unwrap();
        let prev_final_hash: [u8; 32] = prev_public_values[32..64].try_into().unwrap();
        let prev_num_headers = u64::from_le_bytes(prev_public_values[64..72].try_into().unwrap());

        let prev_chain_work: [u64; 4] = [
            u64::from_le_bytes(prev_public_values[152..160].try_into().unwrap()),
            u64::from_le_bytes(prev_public_values[160..168].try_into().unwrap()),
            u64::from_le_bytes(prev_public_values[168..176].try_into().unwrap()),
            u64::from_le_bytes(prev_public_values[176..184].try_into().unwrap()),
        ];
        let prev_epoch_ts = u32::from_le_bytes(prev_public_values[184..188].try_into().unwrap());
        let prev_median: [u32; 11] = core::array::from_fn(|i| {
            u32::from_le_bytes(prev_public_values[(188 + i * 4)..(192 + i * 4)].try_into().unwrap())
        });
        // Derive median_count from total headers: min(11, total - 1) for total > 0
        let prev_median_count = if prev_num_headers == 0 {
            0u32
        } else {
            (prev_num_headers.min(11)) as u32
        };
        let prev_success = prev_public_values[232];

        if prev_genesis != expected_genesis_hash {
            panic!("Genesis hash mismatch with previous proof");
        }
        if prev_success != STATUS_SUCCESS {
            panic!("Previous proof did not succeed");
        }

        (
            prev_final_hash,
            prev_chain_work,
            prev_epoch_ts,
            bits_to_target_from_header_bytes(&prev_public_values[72..152]),
            prev_median,
            prev_median_count,
            prev_num_headers,
            prev_num_headers,
        )
    } else {
        (
            expected_genesis_hash,
            [0u64; 4],
            1231006505u32,
            bits_to_target(0x1d00ffff),
            [0u32; 11],
            0u32,
            0u64,
            0u64,
        )
    };

    let start_height = sp1_zkvm::io::read::<u64>();
    let num_headers = sp1_zkvm::io::read::<u64>();
    let headers_bytes = sp1_zkvm::io::read_vec();

    let expected_len = (num_headers * 80) as usize;
    if headers_bytes.len() != expected_len {
        commit_error_and_exit(
            &expected_genesis_hash, &prev_hash, cumulative_chain_work,
            last_epoch_start_timestamp, median_timestamps,
            start_height, num_headers,
            STATUS_HEADER_COUNT_MISMATCH, 0,
        );
    }

    let mut final_header: [u8; 80] = [0; 80];

    for i in 0..num_headers {
        let offset = (i * 80) as usize;
        let header = &headers_bytes[offset..offset + 80];
        let current_height = start_height + i;

        let prev_blockhash: [u8; 32] = header[4..36].try_into().unwrap();
        let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
        let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());

        if current_height == 0 {
            let computed_hash = double_sha256(header);
            if computed_hash != expected_genesis_hash {
                commit_error_and_exit(
                    &expected_genesis_hash, &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_GENESIS_HASH_MISMATCH, i as u32,
                );
            }
            prev_hash = computed_hash;
            last_epoch_start_timestamp = timestamp;
            prev_target = bits_to_target(bits);
            cumulative_chain_work = u256_add(cumulative_chain_work, work_from_bits(bits));
            median_timestamps[0] = timestamp;
            median_count = 1;
        } else {
            if prev_blockhash != prev_hash {
                commit_error_and_exit(
                    &expected_genesis_hash, &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_PREV_BLOCKHASH_MISMATCH, i as u32,
                );
            }

            // === BIP113: Median timestamp check ===
            if median_count >= 11 {
                let mut sorted = median_timestamps;
                sorted.sort_unstable();
                let median = sorted[5];
                if timestamp <= median {
                    commit_error_and_exit(
                        &expected_genesis_hash, &prev_hash, cumulative_chain_work,
                        last_epoch_start_timestamp, median_timestamps,
                        start_height, num_headers,
                        STATUS_TIMESTAMP_TOO_OLD, i as u32,
                    );
                }
            }

            // Difficulty retargeting every 2016 blocks
            if current_height > 0 && current_height % 2016 == 0 {
                let actual_timespan = timestamp.wrapping_sub(last_epoch_start_timestamp);
                let expected_timespan: u32 = 2016 * 600;
                let clamped = actual_timespan
                    .max(expected_timespan / 4)
                    .min(expected_timespan * 4);
                prev_target = retarget_target(&prev_target, clamped, expected_timespan);
                last_epoch_start_timestamp = timestamp;
            }

            // Verify bits match expected target
            let expected_bits = target_to_bits(&prev_target);
            if bits != expected_bits {
                commit_error_and_exit(
                    &expected_genesis_hash, &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_BITS_MISMATCH, i as u32,
                );
            }

            let block_hash = double_sha256(header);
            if !hash_meets_target(&block_hash, bits) {
                commit_error_and_exit(
                    &expected_genesis_hash, &prev_hash, cumulative_chain_work,
                    last_epoch_start_timestamp, median_timestamps,
                    start_height, num_headers,
                    STATUS_POW_INSUFFICIENT, i as u32,
                );
            }

            prev_hash = block_hash;
            prev_target = bits_to_target(bits);
            cumulative_chain_work = u256_add(cumulative_chain_work, work_from_bits(bits));

            // === Update median timestamp buffer ===
            if median_count < 11 {
                median_timestamps[median_count as usize] = timestamp;
                median_count += 1;
            } else {
                for j in 0..10 {
                    median_timestamps[j as usize] = median_timestamps[(j + 1) as usize];
                }
                median_timestamps[10] = timestamp;
            }
        }

        final_header.copy_from_slice(header);
    }

    // Commit outputs — success path
    sp1_zkvm::io::commit_slice(&prev_hash);
    let total_validated = start_height + num_headers;
    sp1_zkvm::io::commit_slice(&total_validated.to_le_bytes());
    sp1_zkvm::io::commit_slice(&final_header);
    for &word in &cumulative_chain_work {
        sp1_zkvm::io::commit_slice(&word.to_le_bytes());
    }
    sp1_zkvm::io::commit_slice(&last_epoch_start_timestamp.to_le_bytes());
    for &ts in &median_timestamps {
        sp1_zkvm::io::commit_slice(&ts.to_le_bytes());
    }
    // Success code + detail (0 = no error)
    sp1_zkvm::io::commit_slice(&[STATUS_SUCCESS]);
    sp1_zkvm::io::commit_slice(&0u32.to_le_bytes());

    // median_count is NOT committed — it's derivable from total_validated:
    //   count = min(11, total_validated - 1) for total_validated > 0, else 0
    // This saves 4 bytes per proof.

    println!(
        "Successfully validated {} headers from height {} to height {}",
        num_headers,
        start_height,
        start_height + num_headers - 1
    );
}

fn retarget_target(
    old_target: &[u8; 32],
    actual_timespan: u32,
    expected_timespan: u32,
) -> [u8; 32] {
    let old_u64 = [
        u64::from_le_bytes(old_target[0..8].try_into().unwrap()),
        u64::from_le_bytes(old_target[8..16].try_into().unwrap()),
        u64::from_le_bytes(old_target[16..24].try_into().unwrap()),
        u64::from_le_bytes(old_target[24..32].try_into().unwrap()),
    ];

    let mut product = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let prod = (old_u64[i] as u128) * (actual_timespan as u128) + carry;
        product[i] = prod as u64;
        carry = prod >> 64;
    }

    let mut result = [0u64; 4];
    let mut remainder = 0u128;
    for i in (0..4).rev() {
        let val = (remainder << 64) | (product[i] as u128);
        result[i] = (val / (expected_timespan as u128)) as u64;
        remainder = val % (expected_timespan as u128);
    }

    let mut target = [0u8; 32];
    for i in 0..4 {
        target[i * 8..(i + 1) * 8].copy_from_slice(&result[i].to_le_bytes());
    }
    target
}
