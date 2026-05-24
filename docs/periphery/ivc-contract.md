# IVC Contract Export Package

Technical Specification & Engineering Handoff for Incremental Verifiable Computation Bitcoin Header Relay

# 1. Solidity Contract Skeletons

This section provides the complete Solidity interface and contract skeletons for the IVC Contract system. The proof verification stack is: **SP1 ZK RISC-V VM → Compressed STARK → Groth16 Wrapper**. The Groth16 proof is verified on-chain via a Succinct Labs deployed verifier gateway. The compressed STARK is stored off-chain on IPFS; only an IPFS CID pointer and its keccak256 hash are retained on-chain.

## 1. ISuccinctVerifier Interface

The interface for Succinct Labs' deployed Groth16 verifier gateway. This contract is deployed and maintained by Succinct Labs.

/// @param verifierSelector The 4-byte selector identifying the SP1 program/circuit.
/// @param serializedPubValues The pre-image of the SHA-256 public-input commitment,

## 1. IVCContract

The full contract skeleton for the on-chain Bitcoin header relay with ring buffer storage and ZK proof verification.

/// @notice On-chain Bitcoin header relay accepting ZK proofs of Bitcoin header chain validity.
/// Uses the SP1 ZK RISC-V VM proof stack (compressed STARK wrapped in Groth16)
/// @dev Anyone may submit a new header if the Groth16 proof is valid and the cumulative
/// chain work strictly increases. There are NO contestation, dispute, or bonding
/// @notice Represents a single accepted Bitcoin block header stored in the ring buffer.
/// @notice The nBits compact difficulty target (4 bytes, stored as uint32).
/// @notice IPFS CID hash of the compressed STARK proof (raw SHA-256 multihash digest).
/// @notice keccak256 hash of the Groth16 proof bytes submitted with this header.
/// @notice The maximum number of headers stored (ring buffer capacity N).
/// @notice The immutable genesis block hash used to anchor all proof chains.
/// @notice Total number of headers accepted (monotonically increasing).
/// @notice The highest cumulative chain work accepted so far (strictly increasing).
/// @notice The admin address (multisig recommended) for governance functions.
/// @notice Emergency pause flag. When true, submitHeader is disabled.
/// @notice Emitted when a new Bitcoin header is accepted into the ring buffer.
/// @param slot The ring buffer slot index where the header was written.
/// @param cumulativeChainWork The cumulative chain work of the accepted block.
/// @notice Emitted when the Succinct Labs verifier address is updated.
/// @notice Emitted when the SP1 program verifier selector is updated.
event VerifierSelectorUpdated(bytes4 oldSelector, bytes4 newSelector);
/// @notice Initializes the IVC Contract with all required configuration.
/// @param \_succinctVerifier Address of the deployed Succinct Labs Groth16 verifier gateway.
/// @param \_verifierSelector The 4-byte SP1 program verification key selector.
/// @param \_genesisBlockHash The Bitcoin genesis block hash (big-endian / RPC byte order).
/// @param \_ringBufferSize The capacity of the ring buffer (must be > 0).
/// @param \_admin The admin/multisig address for governance operations.
/// @notice Submit a new Bitcoin block header with a valid Groth16 proof.
/// Anyone may call this function. The header is accepted if and only if:
/// (2) the genesis block hash in the proof matches the stored genesis hash,
/// (3) the cumulative chain work strictly exceeds the current latest, and
/// @param proof The Groth16 proof bytes (wrapping the compressed STARK).
/// @param serializedPubValues The SHA-256 preimage of the public input commitment,
/// encoded as 9 concatenated 32-byte big-endian words (288 bytes total).
/// @param compressedStarkCid The IPFS CID hash (raw SHA-256 multihash digest)
require(serializedPubValues.length == 288, "Invalid pubValues length");
uint256 cumulativeWork = uint256(bytes32(serializedPubValues[96:128]));
uint256 blockTimestamp = uint256(bytes32(serializedPubValues[128:160]));
uint32 nBits = uint32(uint256(bytes32(serializedPubValues[192:224])));
uint32 nonce = uint32(uint256(bytes32(serializedPubValues[224:256])));
if (cumulativeWork \<= data-id="128" latestCumulativeChainWork) revert ChainWorkNotIncreasing();
// Store keccak256 of the proof for auditability and deduplication checks.
/// @notice Returns the header entry stored at a specific ring buffer slot.
function getHeader(uint256 slot) external view returns (HeaderEntry memory) {
function getLatestHeader() external view returns (HeaderEntry memory) {
/// @param offsetFromHead The number of positions back from the head (0 = latest).
function getHeaderByOffset(uint256 offsetFromHead) external view returns (HeaderEntry memory) {
require(offsetFromHead < ringBufferSize, "Offset exceeds buffer size");
uint256 targetSlot = (headIndex - 1 - offsetFromHead) % ringBufferSize;
function getRingBufferState() external view returns (RingBufferState memory) {
/// @dev Requires coordination with Succinct Labs when they deploy a new verifier.
function setSuccinctVerifier(address \_newVerifier) external onlyAdmin {
function setVerifierSelector(bytes4 \_newSelector) external onlyAdmin {
/// @dev WARNING: This does NOT migrate old data. Existing entries remain at their
/// current slot indices. After resize, the modular arithmetic will change,
/// potentially making old slots inaccessible or causing overwrites at

# 2. Public-Input Byte Layout Specification

The serializedPubValues parameter encodes all public inputs as a flat byte array of 9 concatenated 32-byte words (288 bytes total). This is the SHA-256 preimage of the public-input commitment checked by the Succinct Labs verifier. The SP1 guest program is responsible for serializing these values in the exact canonical order defined below.

## 2. Canonical Field Ordering

| **Word Index** | **Field Name** | **Solidity Type** | **Byte Offset** | **Encoding Notes** |
| 0 | blockHash | bytes32 | 0.. | Stored as-is (already 32 bytes). Big-endian / RPC byte order. |
| 1 | prevBlockHash | bytes32 | 32.. | Stored as-is. Big-endian / RPC byte order. |
| 2 | blockHeight | uint256 | 64.. | Big-endian, left-zero-padded to 32 bytes. |
| 3 | cumulativeChainWork | uint256 | 96.. | Big-endian, left-zero-padded. Bitcoin’s cumulative chain work fits in uint256. |
| 4 | blockTimestamp | uint256 | 128.. | Big-endian, left-zero-padded. Unix epoch seconds. |
| 5 | merkleRoot | bytes32 | 160.. | Stored as-is (already 32 bytes). |
| 6 | nBits | uint256 | 192.. | Big-endian, left-zero-padded. Only lower 4 bytes meaningful (uint32 value padded with 28 zero bytes). |
| 7 | nonce | uint256 | 224.. | Big-endian, left-zero-padded. Only lower 4 bytes meaningful (uint32 value padded with 28 zero bytes). |
| 8 | genesisBlockHash | bytes32 | 256.. | Stored as-is. Must match the contract’s immutable genesisBlockHash. |

## 2. Encoding Rules

- bytes32 fields (blockHash, prevBlockHash, merkleRoot, genesisBlockHash) are stored as-is — they are already 32 bytes.
- uint32 fields (nBits, nonce) are encoded as uint256 words: the value occupies the rightmost 4 bytes, with 28 leading zero bytes.
- The SHA-256 commitment checked by the Succinct Labs verifier is computed as: SHA256(serializedPubValues) over the full 288-byte payload.
- The SP1 guest program is responsible for producing this exact byte layout. The on-chain contract consumes it as-is and forwards it to the verifier without re-encoding.

## 2. Endianness Note

| **Important: Endianness Convention** Bitcoin internally uses **little-endian** for most integer fields in the raw block header (version, timestamp, nBits, nonce). Block hashes are computed over the little-endian header but are conventionally **displayed** in big-endian (“reversed”) “RPC byte order” by block explorers and Bitcoin Core RPC. This specification uses **big-endian / RPC byte order** for all hash fields to match conventional block explorer display. The SP1 guest program **must** perform the byte-reversal from Bitcoin’s internal little-endian representation before serializing into serializedPubValues. |

# 3. Test Vectors

The following test vectors provide known Bitcoin mainnet block data serialized into the canonical serializedPubValues format defined in Section 2. These vectors should be used for unit and integration testing of both the SP1 guest serialization logic and the on-chain contract’s deserialization and validation logic.

## 3. Bitcoin Mainnet Genesis Block (Block 0)

| Block Hash | 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |
| Prev Block Hash | 0x0000000000000000000000000000000000000000000000000000000000000000 |
| Cumulative Chain Work | 0x0000000000000000000000000000000000000000000000000000000100010001 |
| Block Timestamp | 1231006505 (2009-01-03T18:15:05Z) → 0x00000000000000000000000000000000000000000000000000000000495fab29 |
| Merkle Root | 0x4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b |
| nBits | 0x1d00ffff → 0x000000000000000000000000000000000000000000000000000000001d00ffff |
| Nonce | 2083236893 → 0x000000000000000000000000000000000000000000000000000000007c2bac1d |
| Genesis Block Hash | 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |

| 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010001000100000000000000000000000000000000000000000000000000000000495fab294a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b000000000000000000000000000000000000000000000000000000001d00ffff000000000000000000000000000000000000000000000000000000007c2bac1d000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |

## 3. Block 1 (Second Block)

| Block Hash | 0x00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048 |
| Prev Block Hash | 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |
| Cumulative Chain Work | 0x0000000000000000000000000000000000000000000000000000000200020002 |
| Block Timestamp | 1231469665 (2009-01-09T02:54:25Z) → 0x000000000000000000000000000000000000000000000000000000004966bc61 |
| Merkle Root | 0x0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098 |
| nBits | 0x1d00ffff → 0x000000000000000000000000000000000000000000000000000000001d00ffff |
| Nonce | 2573394689 → 0x000000000000000000000000000000000000000000000000000000009962e301 |
| Genesis Block Hash | 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |

| 0x00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000200020002000000000000000000000000000000000000000000000000000000004966bc610e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098000000000000000000000000000000000000000000000000000000001d00ffff000000000000000000000000000000000000000000000000000000009962e301000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |

## 3. Block 100,000

| Block Hash | 0x000000000003ba27aa200b1cecaad478d2b00432346c3f1f3986da1afd33e506 |
| Prev Block Hash | 0x000000000002d01c1fccc21636b607dfd930d31d01c3a62104612a1719011250 |
| Block Height | 100000 → 0x00000000000000000000000000000000000000000000000000000000000186a0 |
| Cumulative Chain Work | 0x0000000000000000000000000000000000000000000000000644cb7f5234089e (representative) |
| Block Timestamp | 1293623863 (2010-12-29T11:57:43Z) → 0x000000000000000000000000000000000000000000000000000000004d1b2237 |
| Merkle Root | 0xf3e94742aca4b5ef85488dc37c06c3282295ffec960994b2c0d5ac2a25a95766 |
| nBits | 0x1b04864c → 0x000000000000000000000000000000000000000000000000000000001b04864c |
| Nonce | 274148111 → 0x000000000000000000000000000000000000000000000000000000001057ab0f |
| Genesis Block Hash | 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |

| 0x000000000003ba27aa200b1cecaad478d2b00432346c3f1f3986da1afd33e506000000000002d01c1fccc21636b607dfd930d31d01c3a62104612a171901125000000000000000000000000000000000000000000000000000000000000186a00000000000000000000000000000000000000000000000000644cb7f5234089e000000000000000000000000000000000000000000000000000000004d1b2237f3e94742aca4b5ef85488dc37c06c3282295ffec960994b2c0d5ac2a25a95766000000000000000000000000000000000000000000000000000000001b04864c000000000000000000000000000000000000000000000000000000001057ab0f000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f |

## 3. SHA-256 Commitment Examples

For each test vector above, the Succinct Labs verifier checks that the Groth16 proof commits to SHA256(serializedPubValues). The actual SHA-256 computation is left to the test harness implementation; below is the placeholder format for each test:

bytes memory pubValues_block0 = hex"000000000019d668..."; // 288 bytes as above bytes memory pubValues_block1 = hex"00000000839a8e68..."; // 288 bytes as above
bytes memory pubValues_block100k = hex"000000000003ba27..."; // 288 bytes as above

| **Note** The SHA-256 commitment is computed by the Succinct Labs verifier internally. The contract passes serializedPubValues to the verifier as-is. The commitment is never computed on-chain by the IVC Contract; it is the verifier’s responsibility to check SHA256(serializedPubValues) against the commitment embedded in the Groth16 proof. |

# 4. Compressed STARK IPFS CID & proofHash Examples

## 4. IPFS CID Format

Compressed STARK proofs are stored off-chain on IPFS for data availability and auditability. The compressed STARK is **not** verified on-chain — only the Groth16 wrapper proof is verified. The IPFS CID serves as a pointer to the full proof data for auditors and researchers.

**On-chain representation:** The contract stores a bytes32 value which is the raw 32-byte SHA-256 multihash digest extracted from a CIDv1 (raw codec, SHA-256 hash function). This is the content-addressed hash of the compressed STARK data.

## 4. proofHash Examples

The proofHash stored on-chain for each accepted header is keccak256(proof), where proof is the raw Groth16 proof bytes passed to submitHeader. This allows post-hoc verification that a specific proof was used for a given header entry.

| **Note** The proofHash values above are placeholders. In production, the Groth16 proof is typically ~256 bytes (8 group elements × 32 bytes). The actual keccak256 output depends on the exact proof bytes. These examples use well-known keccak256 outputs for illustration. |

# 5. Unit Test Scaffolding

## 5. Hardhat (TypeScript) Test Template

### 5.1. Mock Verifier Contract

/// @notice A mock verifier for unit testing. Returns a configurable boolean
/// @param shouldVerify\_ If true, verify() returns true; if false, it returns false.
/// @notice Mock implementation of verify(). Ignores all inputs and returns

### 5.1. Hardhat Test File

import { loadFixture } from "@nomicfoundation/hardhat-toolbox/network-helpers";
import { IVCContract, MockSuccinctVerifier } from "../typechain-types";
import { SignerWithAddress } from "@nomicfoundation/hardhat-ethers/signers";
const pad32 = (hex: string) => hex.replace("0x", "").padStart(64, "0");
const uint256Hex = (val: bigint) => val.toString(16).padStart(64, "0");
const MockVerifier = await ethers.getContractFactory("MockSuccinctVerifier");
it("should set the correct succinctVerifier address", async function () {
// TODO: expect(await ivc.verifierSelector()).to.equal(VERIFIER_SELECTOR);
// TODO: expect(await ivc.ringBufferSize()).to.equal(RING_BUFFER_SIZE);
it("should accept a valid header and emit HeaderAccepted", async function () {
// TODO: await expect(ivc.submitHeader(DUMMY_PROOF, pubValues, DUMMY_CID))
it("should increment headerCount after acceptance", async function () {
it("should store the header in the correct ring buffer slot", async function () {
it("should revert when genesis hash does not match", async function () {
it("should revert when chain work is equal (not strictly increasing)", async function () {
blockHash: "0x0000000000000000000000000000000000000000000000000000000000aabbcc" };
blockHash: "0x0000000000000000000000000000000000000000000000000000000000aabbcc" };
it("should revert when serializedPubValues length != 288", async function () {
it("should overwrite the oldest entry after N+1 submissions", async function () {
// TODO: Submit RING_BUFFER_SIZE + 1 headers with increasing chainWork.
// After N+1 submissions, verify that slot 0 contains the (N+1)th header,
it("should return correct header via getHeader(slot)", async function () {
// TODO: Submit a header, then call getHeader(0) and verify all fields.
it("should return correct header via getLatestHeader()", async function () {
it("should return correct header via getHeaderByOffset()", async function () {
it("setSuccinctVerifier: should update and emit VerifierUpdated", async function () {
// TODO: await expect(ivc.connect(owner).setSuccinctVerifier(user1.address))
// TODO: expect(await ivc.succinctVerifier()).to.equal(user1.address);
it("setSuccinctVerifier: should revert for non-admin", async function () {
it("setVerifierSelector: should update and emit event", async function () {
it("resizeRingBuffer: should update size and emit event", async function () {
it("transferAdmin: should transfer and emit AdminTransferred", async function () {
// TODO: await expect(ivc.connect(owner).transferAdmin(user1.address))
it("transferAdmin: should revert for zero address", async function () {
it("should work correctly with ring buffer size of 1", async function () {
it("should handle maximum uint256 chain work values", async function () {
it("should allow anyone (non-admin) to submit valid headers", async function () {

## 5. Foundry Test Template

function \_validPubValues(uint256 chainWork) internal pure returns (bytes memory) {
new IVCContract(address(0), SELECTOR, GENESIS_HASH, RING_SIZE, admin);
new IVCContract(address(mockVerifier), SELECTOR, GENESIS_HASH, RING_SIZE, address(0));
new IVCContract(address(mockVerifier), SELECTOR, GENESIS_HASH, 0, admin);
// TODO: Verify slot 0 now contains header #(RING_SIZE+1), not header #1.
// TODO: IVCContract.HeaderEntry memory latest = ivc.getLatestHeader();
// TODO: IVCContract.RingBufferState memory state = ivc.getRingBufferState();

# 6. Integration Test Plan & Scaffolding

## 6. Integration Test Plan

| **Test ID** | **Description** | **Prerequisites** | **Steps** | **Expected Result** |
| INT-01 | End-to-end proof generation and submission with real SP1 prover | SP1 SDK installed; SP1 guest program compiled; local or testnet Ethereum node | 1. Generate a compressed STARK for a real Bitcoin header range using SP1. 2. Wrap the STARK in a Groth16 proof. 3. Serialize public values per Section 2. 4. Submit via submitHeader(). | Header is accepted; event emitted; ring buffer updated. |
| INT-02 | Verify compressed STARK is retrievable from IPFS after submission | IPFS node or Pinata gateway; proof uploaded to IPFS | 1. Upload compressed STARK to IPFS, record CID. 2. Submit header with the CID’s SHA-256 digest. 3. Read compressedStarkCid from on-chain entry. 4. Reconstruct CID and fetch from IPFS gateway. | Downloaded STARK matches the originally uploaded data. |
| INT-03 | Submit sequential headers (genesis → block 1 → block 2) and verify ring buffer state | 3 valid proof/pubValues sets with strictly increasing chain work | 1. Submit genesis header. 2. Submit block 1 header. 3. Submit block 2 header. 4. Query getRingBufferState() and all 3 slots. | headerCount == 3; headIndex == 3; all 3 entries are correct and ordered. |
| INT-04 | Submit with Succinct Labs testnet verifier contract | Succinct Labs testnet verifier address; valid SP1 proof; Sepolia ETH | 1. Deploy IVCContract on Sepolia pointing to Succinct testnet verifier. 2. Generate a real Groth16 proof with SP1. 3. Submit via submitHeader(). | Header is accepted on testnet. |
| INT-05 | Gas profiling under varying proof sizes | Mock verifier; multiple proof blobs of sizes 128B, 256B, 512B, 1KB | 1. Submit headers with varying proof byte lengths. 2. Record gas used for each submission. 3. Plot gas vs. proof size. | Gas increases linearly with calldata size; SSTORE cost is constant. |
| INT-06 | Verify SHA-256 commitment matches between SP1 guest output and on-chain decoding | SP1 SDK; access to guest program’s serialized output | 1. Run SP1 guest program, capture serializedPubValues output. 2. Compute SHA256(serializedPubValues) off-chain. 3. Compare with the commitment embedded in the Groth16 proof. | SHA-256 commitments match exactly. |

## 6. Integration Test Scaffolding

// npx hardhat test test/integration/IVCContract.integration.ts --network sepolia
const VERIFIER_SELECTOR = process.env.SP1_VERIFIER_SELECTOR || "0x00000000";
it("INT-01: should accept a real SP1 Groth16 proof", async function () {
// TODO: Load serializedPubValues from test/fixtures/pubvalues_block1.bin
// TODO: const proof = fs.readFileSync("test/fixtures/proof_block1.bin");
// TODO: const pubValues = fs.readFileSync("test/fixtures/pubvalues_block1.bin");
it("INT-02: should retrieve compressed STARK from IPFS", async function () {
// TODO: const response = await fetch(`${IPFS\_GATEWAY}${cidString}`);
it("INT-03: should accept sequential headers and update ring buffer", async function () {
it("INT-05: should profile gas usage for varying proof sizes", async function () {
// const tx = await ivc.submitHeader(proof, validPubValues, DUMMY_CID);

# 7. Deployment & Governance Checklist

## 7. Pre-Deployment

- ☐ Compile contracts with solc 0.8.20+ with optimizer enabled (200 runs).
- ☐ Run static analysis tools: **Slither** and **Mythril**. Resolve all high/critical findings.
- ☐ Verify MockSuccinctVerifier is **NOT** included in production deployment artifacts.
- ☐ Confirm all events are emitted for every state-changing operation.

## 7. Deployment Steps

1. **Deploy IVCContract** with the following constructor arguments:

- \_succinctVerifier: Address of Succinct Labs’ deployed Groth16 verifier gateway (obtain from Succinct Labs documentation or contract registry).
- \_verifierSelector: The bytes4 SP1 program verification key selector (obtained by compiling the SP1 guest program).
- \_genesisBlockHash: 0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f

1. **Verify contract on Etherscan** using hardhat-etherscan or forge verify-contract.
1. **Submit the genesis block header** as the first entry (block 0) with a valid proof to initialize the ring buffer.

## 7. Post-Deployment Verification

- ☐ Call succinctVerifier() and confirm the correct verifier gateway address.
- ☐ Call ringBufferSize() and confirm the value equals the intended N.
- ☐ Submit a test header on a testnet fork before mainnet genesis submission.

## 7. Governance Operations

| Verifier Upgrade | setSuccinctVerifier() | High | Requires coordination with Succinct Labs. Use timelock. Incorrect address breaks all future submissions. |
| Selector Update | setVerifierSelector() | High | Required when SP1 guest program is updated/recompiled. Use timelock. |
| Ring Buffer Resize | resizeRingBuffer() | Medium | Does **not** migrate existing data. Use timelock. Plan for data access changes. |
| Emergency Pause | pause() | Critical | Single admin call, no timelock required (emergency). Disables all header submissions. |
| Unpause | unpause() | Medium | Re-enables submissions. Verify the root cause was resolved before unpausing. |
| Admin Transfer | transferAdmin() | Critical | Must transfer to a valid multisig. Consider implementing a 2-step transfer pattern (propose → accept). |

## 7. Multisig Configuration Recommendations

- Include a **TimelockController** (e.g., OpenZeppelin) for non-emergency operations:
- **Emergency pause** should **bypass the timelock** — route pause() directly through the multisig without delay.
- Distribute signer keys across geographic regions and hardware wallets.

# 8. Gas Cost Estimation & Calldata Tradeoffs

## 8. Estimated Gas Costs

| submitHeader (cold — first write to slot) | ~150,000–250,000 | Dominated by SSTORE for new HeaderEntry (10 fields × 20,000 gas for cold SSTORE) + external call to verifier. |
| submitHeader (warm — overwriting existing slot) | ~80,000–150,000 | Warm SSTORE updates cost ~5,000 gas per slot. Significantly cheaper once ring buffer wraps around. |
| External call to succinctVerifier.verify() | ~100,000–300,000 | Depends on Succinct Labs’ verifier implementation. Groth16 verification is ~200K gas on typical implementations. |
| getLatestHeader() (view) | 0 (off-chain) | View function — free when called via eth_call. Costs gas only if called from another contract. |
| Admin functions (set/pause/transfer) | ~30,000–50,000 each | Single SSTORE + event emission. |

## 8. Calldata Cost Analysis

EVM calldata pricing: **16 gas** per non-zero byte, **4 gas** per zero byte.

| serializedPubValues | 288 | ~3,000–4,600 (mix of zero and non-zero bytes) |

## 8. Pre-serialized vs. On-chain Deserialization Tradeoff

The IVC Contract uses **pre-serialized 32-byte word encoding** for serializedPubValues, rather than ABI-encoding a Solidity struct. This is a deliberate design choice with the following tradeoffs:

| Encoding overhead | No on-chain ABI decoding overhead. The prover controls the exact byte layout. | The contract must manually slice bytes to extract fields for on-chain checks. |
| SHA-256 commitment | The SHA-256 commitment is computed over the exact bytes the verifier checks — no re-encoding ambiguity. | — |
| Calldata pass-through | Calldata is forwarded to the verifier as-is — no intermediate copy or re-encoding. | — |
| Field extraction | — | Requires manual byte slicing (Solidity bytes slice or assembly { calldataload }). |

**Recommendation:** For gas-efficient extraction of individual fields, use inline assembly with calldataload at the appropriate offsets. For code clarity during initial development and auditing, use Solidity byte slicing. The gas difference is small (~200 gas per field, ~1,800 gas total for 9 fields).

## 8. Blob (EIP-4844) Future Consideration

# 9. README — Developer Instructions

## 9. Overview

The IVC Contract is an on-chain Bitcoin header relay that accepts ZK proofs of Bitcoin header chain validity. It uses the SP1 ZK RISC-V VM to generate compressed STARKs, which are wrapped in Groth16 proofs and verified on-chain by Succinct Labs’ deployed verifier gateway. The contract maintains a ring buffer of the N most recent accepted headers, enabling downstream applications to query validated Bitcoin state without running a full node. Anyone can submit a new header if the accompanying Groth16 proof verifies and the cumulative chain work strictly increases.

## 9. Architecture Diagram

## 9. Prerequisites

- Access to Succinct Labs verifier gateway address (from their documentation or contract registry)
- SP1 program verification key (bytes4 selector from guest program compilation)
- IPFS node or gateway for compressed STARK storage (e.g., Pinata, Infura IPFS)

## 9. Setup and Build

# Clone and install

# Compile (Hardhat)

# Compile (Foundry)

# Run unit tests (Hardhat)

# Run unit tests (Foundry, verbose)

# Deploy to testnet

# Verify on Etherscan

## 9. Environment Variables

# Succinct Labs verifier gateway (deployed contract address)

# SP1 program verification key selector (bytes4)

# Bitcoin mainnet genesis block hash (big-endian / RPC byte order)

GENESIS_BLOCK_HASH=0x000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f

# Ring buffer capacity

# Admin multisig address (Gnosis Safe)

# Deployer private key (DO NOT commit to version control)

# Etherscan API key for contract verification

# IPFS gateway for compressed STARK retrieval

| **Security Warning** Never commit .env files or private keys to version control. Add .env to .gitignore. Use a hardware wallet or KMS for production deployments. |

## 9. Project Structure

## 9. Integrating with Succinct Labs

Step-by-step guide for connecting the IVC Contract to the SP1 proof stack:

# 10. Auditor Checklist & Formal Verification Targets

## 10. Auditor Checklist

| **ID** | **Category** | **Check Item** | **Severity** | **Status** |
| AUD-01 | Access Control | Admin functions restricted to onlyAdmin modifier | High | ☐ |
| AUD-02 | Access Control | transferAdmin validates non-zero address | Medium | ☐ |
| AUD-03 | Input Validation | serializedPubValues.length == 288 bytes enforced | Critical | ☐ |
| AUD-04 | Input Validation | Genesis hash comparison is constant-time (no early-exit optimization by compiler) | Medium | ☐ |
| AUD-05 | Arithmetic | cumulativeChainWork uses strict increase check (> not >=) | Critical | ☐ |
| AUD-06 | Arithmetic | No overflow in ring buffer index calculation (headIndex % ringBufferSize) | High | ☐ |
| AUD-07 | External Calls | Verifier call is view/staticcall — no state changes possible | High | ☐ |
| AUD-08 | External Calls | Verifier return value is checked (no unchecked external call) | Critical | ☐ |
| AUD-09 | Storage | Ring buffer correctly overwrites oldest entry on wrap-around | High | ☐ |
| AUD-10 | Storage | All HeaderEntry fields are written (no uninitialized storage) | Medium | ☐ |
| AUD-11 | Reentrancy | submitHeader is not vulnerable to reentrancy (verifier is view; SSTOREs occur after external call but are safe since view prevents state changes by callee) | High | ☐ |
| AUD-12 | DoS | No unbounded loops or dynamic arrays in any function | Medium | ☐ |
| AUD-13 | Governance | Pause mechanism works correctly — submitHeader reverts when paused | Medium | ☐ |
| AUD-14 | Governance | No function allows bypassing the verifier check | Critical | ☐ |
| AUD-15 | Data Integrity | proofHash == keccak256(proof) stored correctly | Medium | ☐ |
| AUD-16 | Data Integrity | compressedStarkCid stored as provided (not modified by contract) | Low | ☐ |
| AUD-17 | Byte Encoding | serializedPubValues byte slicing extracts correct fields at correct offsets | Critical | ☐ |
| AUD-18 | Events | All state-changing operations emit appropriate events | Medium | ☐ |
| AUD-19 | Upgradeability | Contract is **NOT** upgradeable (no proxy pattern). If upgradeability is added later, verify proxy pattern correctness. | High | ☐ |
| AUD-20 | Gas | submitHeader gas cost is bounded and reasonable (\<500K gas total) | Low | ☐ |

## 10. Formal Verification Targets

The following properties should be formally verified using tools such as **Certora Prover**, **Halmos**, or symbolic execution engines:

1. **Monotonicity Invariant:** latestCumulativeChainWork is strictly monotonically increasing — it can never decrease or remain the same after a successful submitHeader call. Formally: for any state transition *S → S′* caused by submitHeader, S′.latestCumulativeChainWork > S.latestCumulativeChainWork.
1. **Genesis Anchor Invariant:** genesisBlockHash is immutable after construction — no function in the contract can change its value. This is enforced by the immutable keyword, but should be verified at the bytecode level.
1. **Ring Buffer Bounds:** headIndex % ringBufferSize is always in the range \[0, ringBufferSize). Additionally, ringBufferSize is always > 0 (preventing division by zero).
1. **Verification Requirement:** A HeaderAccepted event is emitted **only if** succinctVerifier.verify() returned true in the same transaction. No code path exists that emits HeaderAccepted without a successful verification.
1. **Admin Exclusivity:** State-changing admin functions (setSuccinctVerifier, setVerifierSelector, resizeRingBuffer, pause, unpause, transferAdmin) are callable **only** by the current admin address.
1. **Pause Enforcement:** When paused == true, submitHeader always reverts. No code path in submitHeader can succeed while the contract is paused.
1. **No Ether Trapping:** The contract does not accept Ether — there is no receive() or fallback() function marked payable. ETH sent directly to the contract address will revert.

## 10. Known Design Decisions (Not Bugs)

The following are intentional design decisions that auditors should note but not flag as vulnerabilities:

| No contestation, dispute, or bonding mechanisms | By design, the system trusts the ZK proof. If the Groth16 proof verifies against the correct public inputs, the header is accepted. The security assumption is that the proof system is sound. |
| Ring buffer resize does not migrate data | Accepted limitation. Data migration would be gas-prohibitive for large ring buffers. Documented in NatSpec and governance docs. |
| Permissionless submission (anyone can call submitHeader) | By design. The proof is the access control mechanism, not the caller’s identity. This maximizes liveness and censorship resistance. |
| No timelock on admin functions | Emergency pause needs to be fast (single multisig tx). Timelock should be added via an external TimelockController contract for non-emergency operations . |
| Compressed STARK is not verified on-chain | Only the Groth16 wrapper is verified on-chain. The compressed STARK stored on IPFS is for data availability and auditability — it allows independent parties to verify the full proof stack off-chain. |
| Single admin (not role-based) | Simplicity. A multisig address provides multi-party control. Role-based access (e.g., separate pauser, upgrader) can be layered via the multisig’s internal module system. |
