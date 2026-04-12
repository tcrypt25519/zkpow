//! Inspect a saved Bitcoin header chain proof and display its public inputs.

use sp1_sdk::SP1ProofWithPublicValues;

const MAINNET_GENESIS_HEX: &str =
    "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";

fn reverse_hash_display(hash: &[u8; 32]) -> String {
    let mut reversed = *hash;
    reversed.reverse();
    hex::encode(reversed)
}

fn main() {
    let proof_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bitcoin-header-chain-proof.bin".to_string());

    println!("Loading proof from: {}", proof_path);
    let proof = SP1ProofWithPublicValues::load(&proof_path)
        .expect("failed to load proof file");

    let pv = proof.public_values.as_ref();
    println!("\n=== Bitcoin Header Chain Proof ===\n");

    if pv.len() < 237 {
        eprintln!("ERROR: public values too short (expected 237 bytes, got {})", pv.len());
        eprintln!("Raw hex: {}", hex::encode(pv));
        std::process::exit(1);
    }

    // Layout (237 bytes) — matches program's commit order:
    //   0..32:    genesis_hash
    //  32..64:    final_header_hash
    //  64..72:    num_headers (u64 LE)
    //  72..152:   final_header (80 raw bytes)
    // 152..184:   cumulative_chain_work (4 × u64 LE)
    // 184..188:   last_epoch_start_timestamp (u32 LE)
    // 188..232:   median_timestamp_buffer ([u32; 11] LE)
    // 232..233:   success_code (u8)
    // 233..237:   error_detail (u32 LE)
    let genesis_hash: [u8; 32] = pv[0..32].try_into().unwrap();
    let num_headers = u64::from_le_bytes(pv[64..72].try_into().unwrap());
    let final_hash: [u8; 32] = pv[32..64].try_into().unwrap();
    let final_header: [u8; 80] = pv[72..152].try_into().unwrap();
    let chain_work: [u64; 4] = [
        u64::from_le_bytes(pv[152..160].try_into().unwrap()),
        u64::from_le_bytes(pv[160..168].try_into().unwrap()),
        u64::from_le_bytes(pv[168..176].try_into().unwrap()),
        u64::from_le_bytes(pv[176..184].try_into().unwrap()),
    ];
    let epoch_start_ts = u32::from_le_bytes(pv[184..188].try_into().unwrap());
    let median_timestamps: [u32; 11] = core::array::from_fn(|i| {
        u32::from_le_bytes(pv[(188 + i * 4)..(192 + i * 4)].try_into().unwrap())
    });
    let success_code = pv[232];
    let error_detail = u32::from_le_bytes(pv[233..237].try_into().unwrap());

    // median_count is derivable from num_headers
    let median_count = if num_headers == 0 { 0u32 } else { num_headers.min(11) as u32 };

    // Genesis hash
    println!("Genesis Hash:      {}", reverse_hash_display(&genesis_hash));
    let mainnet_genesis_raw: [u8; 32] = {
        let mut g: [u8; 32] = hex::decode(MAINNET_GENESIS_HEX).unwrap().try_into().unwrap();
        g.reverse();
        g
    };
    if genesis_hash == mainnet_genesis_raw {
        println!("                     ↳ mainnet ✓");
    } else {
        println!("                     ↳ NOT mainnet (different chain)");
    }

    // Chain tip
    println!("\nChain Tip:         {}", reverse_hash_display(&final_hash));

    // Headers validated
    println!("\nHeaders Validated: {}", num_headers);

    // Cumulative chain work
    let work_hex: String = chain_work
        .iter()
        .rev()
        .map(|w| format!("{:016x}", w))
        .collect();
    println!("Cumulative Work:   0x{}", work_hex);

    // Epoch start timestamp
    use std::time::UNIX_EPOCH;
    let epoch_dt = UNIX_EPOCH + std::time::Duration::from_secs(epoch_start_ts as u64);
    println!("Epoch Start:       {} (timestamp: {})",
        humantime::format_rfc3339_seconds(epoch_dt),
        epoch_start_ts);

    // Median timestamp buffer
    if median_count > 0 {
        println!("\nMedian Buffer ({}/11):", median_count);
        let display_count = median_count.min(11) as usize;
        for i in 0..display_count {
            let ts = median_timestamps[i];
            let dt = UNIX_EPOCH + std::time::Duration::from_secs(ts as u64);
            println!("  [{}] {} ({})", i, humantime::format_rfc3339_seconds(dt), ts);
        }
    }

    // Final header details
    let fh_bits = u32::from_le_bytes(final_header[72..76].try_into().unwrap());
    let fh_timestamp = u32::from_le_bytes(final_header[68..72].try_into().unwrap());
    let fh_dt = UNIX_EPOCH + std::time::Duration::from_secs(fh_timestamp as u64);
    println!("\n--- Final Header ---");
    println!("Bits:              0x{:08x}", fh_bits);
    println!("Timestamp:         {} ({})", humantime::format_rfc3339_seconds(fh_dt), fh_timestamp);

    // Status
    match success_code {
        0 => println!("\nStatus:              ✓ All headers validated"),
        code => println!("\nStatus:              ✗ Error code: {} (header #{})", code, error_detail),
    }

    // Proof metadata
    println!("\n--- Proof Details ---");
    println!("SP1 Version:       {}", proof.sp1_version);
    println!("Proof Type:        {:?}", proof.proof);
    println!("Public Values Size: {} bytes", pv.len());
}
