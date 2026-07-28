use sp1_sdk::SP1PublicValues;

/// Compute SHA-256 digest of public values.
pub fn compute_pv_digest(committed_bytes: &[u8]) -> [u8; 32] {
    let digest = SP1PublicValues::from(committed_bytes).hash();
    digest.try_into().expect("SP1 public values hash must be 32 bytes")
}
