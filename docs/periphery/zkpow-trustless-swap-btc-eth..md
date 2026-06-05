# SPV-Bridged Trustless BTC-ETH Swap Protocol Specification

Non-Custodial Cross-Chain Atomic Settlement via UTXO Inclusion Proofs

**Version:** 1.0 | **Status:** Draft

**Date:** May 5, 2026

**Author:** Tyler

**Category:** Protocol Specification

## 1. Abstract

This document specifies a trustless, non-custodial protocol for swapping native BTC for native ETH without wrapped
tokens or intermediaries. It leverages an Ethereum-resident contract that tracks Bitcoin consensus at SPV-level security
and exposes UTXO set commitments with inclusion proofs. The protocol replaces the traditional Hash Time-Locked Contract
(HTLC) pattern with a one-directional SPV proof: the BTC seller proves on-chain (Ethereum) that she created a correctly
constructed Bitcoin UTXO payable to the buyer, and the trade contract releases escrowed ETH upon successful
verification. A OP_CHECKSEQUENCEVERIFY-timelocked reclaim path on Bitcoin and an Ethereum-side deposit timeout provide
safety for both parties if the trade does not complete.

## 2. Terminology and Conventions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in RFC 2119.

| **Term**                                     | **Definition**                                                                                                                                                                                                                         |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **BCOC** (Bitcoin Consensus Oracle Contract) | The assumed Ethereum-resident contract tracking Bitcoin block headers and UTXO set state with SPV-level security. This contract is a prerequisite dependency and is not specified by this document.                                    |
| **Trade Contract**                           | The Ethereum smart contract specified by this document that handles escrow, proof verification, and settlement of cross-chain swaps.                                                                                                   |
| **UTXO Inclusion Proof**                     | A cryptographic proof (e.g., Merkle path or accumulator witness) demonstrating that a specific unspent output exists in the UTXO set committed at a given Bitcoin block height. The proof covers txid, vout, amount, and scriptPubKey. |
| **CSV** (OP_CHECKSEQUENCEVERIFY)             | A Bitcoin opcode (BIP 112) enforcing a relative timelock on spending a transaction output. Used in the reclaim branch of the swap script.                                                                                              |
| **Tdeadline** (Trade Deadline)               | The Ethereum block timestamp by which Alice MUST submit a valid proof, else Bob MAY reclaim his ETH. Default: 3 days (259,200 seconds) after deposit.                                                                                  |
| **Treclaim** (Reclaim Timelock)              | The CSV-enforced relative timelock on the Bitcoin script's reclaim branch. Default: 1,008 blocks (~7 days at 144 blocks/day).                                                                                                          |
| **Dconf** (Confirmation Depth)               | The minimum number of Bitcoin blocks that MUST build on top of the block containing Alice's UTXO before the proof is accepted. Default: 6.                                                                                             |
| **Alice**                                    | The BTC seller. Holds BTC, wants ETH. Responsible for constructing the Bitcoin payment and submitting the UTXO inclusion proof on Ethereum.                                                                                            |
| **Bob**                                      | The ETH seller / BTC buyer. Holds ETH, wants BTC. Responsible for depositing ETH escrow and spending the Bitcoin UTXO after settlement.                                                                                                |

## 3. System Model and Assumptions

### 3.1 Bitcoin Consensus Oracle Contract (BCOC)

The protocol assumes the existence of an Ethereum-resident contract that tracks the Bitcoin main chain via block header
relay (analogous to BTC Relay or ERC-8002-style constructs). The BCOC MUST satisfy the following requirements:

1. It MUST maintain or be capable of verifying a UTXO set accumulator commitment at each tracked Bitcoin block height.
1. It MUST expose the interface verifyUtxoInclusion(blockHeight, utxoData, proof) → bool, where utxoData includes (txid,
   vout, amountSats, scriptPubKey).
1. It MUST provide getBlockConfirmations(blockHeight) → uint, returning the number of blocks built on top of the
   specified block.
1. It MUST provide isMainChain(blockHeight) → bool, returning whether the block at the given height is on the canonical
   Bitcoin main chain.

The underlying UTXO set commitment structure (e.g., Utreexo-style Merkle forest, sparse Merkle tree, or RSA accumulator)
is opaque to this specification. This spec requires only the verification interface described above.

### 3.2 Order Matching System

An external system (on-chain order book, off-chain matching engine, or peer-to-peer negotiation) is assumed to have
identified two counterparties and produced agreed-upon trade parameters. The order matching system is out of scope for
this specification. The Trade Contract receives the already-matched trade parameters as inputs to its initialization
function.

### 3.3 Trust Model

This protocol requires no trusted third parties, custodians, or multisig federations. Participants trust only:

1. **Ethereum consensus** — for correct execution of the Trade Contract and BCOC.
1. **Bitcoin consensus (SPV-level)** — subject to the Dconf confirmation requirement; specifically, that a Dconf-deep
   block will not be reorganized.
1. **Smart contract correctness** — that the BCOC and Trade Contract are correctly implemented and deployed.

The order matching system is explicitly *untrusted*. It cannot steal funds; it can only fail to match orders or provide
incorrect (but harmless) parameters that will be rejected by the Trade Contract.

### 3.4 Timing Assumptions

| **Parameter**       | **Value**                | **Notes**                                                   |
| ------------------- | ------------------------ | ----------------------------------------------------------- |
| Bitcoin block time  | ~10 minutes (average)    | Actual variance is significant; protocol accounts for this. |
| Ethereum block time | ~12 seconds              | Post-merge consensus timing.                                |
| Tdeadline           | 259,200 seconds (3 days) | Ethereum-side deadline for proof submission.                |
| Treclaim            | 1,008 blocks (~7 days)   | Bitcoin-side CSV timelock for Alice's reclaim path.         |
| Dconf               | 6 blocks (~1 hour)       | Minimum confirmation depth for proof acceptance.            |

|                                                                                                                                                                                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Critical Invariant** Tdeadline MUST be strictly less than Treclaim in wall-clock time. With defaults of 3 days vs. 7 days, there is a 4-day safety margin. This invariant ensures that Bob can always reclaim his ETH before Alice can reclaim her BTC, preventing a scenario where Alice obtains both assets. |

## 4. Data Structures

### 4.1 Trade Record (Ethereum Storage)

Each trade is stored as a record in the Trade Contract's state. The following table defines the fields:

| **Field**        | **Type**          | **Description**                                                                                       |
| ---------------- | ----------------- | ----------------------------------------------------------------------------------------------------- |
| tradeId          | bytes32           | Unique identifier for the trade. Derived deterministically from matched order parameters and a nonce. |
| aliceEthAddress  | address           | Alice's Ethereum address. Receives ETH upon successful settlement.                                    |
| bobEthAddress    | address           | Bob's Ethereum address. Receives refund if trade expires.                                             |
| bobBtcPubKey     | bytes (33 bytes)  | Bob's compressed Bitcoin public key. Used to reconstruct the expected witness script.                 |
| ethAmount        | uint256 (wei)     | The amount of ETH escrowed by Bob.                                                                    |
| btcAmount        | uint64 (satoshis) | The amount of BTC Alice must send.                                                                    |
| depositTimestamp | uint256           | The block.timestamp at which Bob deposited ETH.                                                       |
| deadline         | uint256           | Computed as depositTimestamp + T_deadline. Alice MUST submit proof before this time.                  |
| status           | enum              | One of: AWAITING_DEPOSIT, AWAITING_BTC, COMPLETED, EXPIRED, CANCELLED.                                |

## 4.2 Expected Bitcoin Script Template

Alice MUST construct a P2WSH output whose underlying witness script matches the following template exactly:

OP_IF
<bobBtcPubKey> // 33-byte compressed public key
OP_CHECKSIG
OP_ELSE
\<T_reclaim> // e.g., 0xF003 for 1008 in minimal encoding
OP_CHECKSEQUENCEVERIFY
OP_DROP
<aliceBtcPubKey> // 33-byte compressed public key
OP_CHECKSIG
OP_ENDIF

**Script semantics:**

- The OP_IF branch (spend path) allows Bob to spend the output at any time by providing a valid signature against bobBtcPubKey.
- The OP_ELSE branch (reclaim path) allows Alice to reclaim the output after T_reclaim blocks (relative to the output's
  confirmation) by providing a valid signature against aliceBtcPubKey.

**P2WSH derivation:** For P2WSH, the actual scriptPubKey committed to the Bitcoin output is:

OP_0 \<SHA256(witnessScript)>

This is a 34-byte value: 0x0020 followed by the 32-byte SHA-256 hash of the serialized witness script. The Trade
Contract MUST reconstruct the expected witness script from known parameters (bobBtcPubKey, aliceBtcPubKey, T_reclaim),
compute its SHA-256 hash, and compare the resulting scriptPubKey against the one provided in the UTXO inclusion proof.

### 4.3 UTXO Inclusion Proof

The proof structure submitted by Alice to the Trade Contract:

| **Field**    | **Type** | **Description**                                                                                |
| ------------ | -------- | ---------------------------------------------------------------------------------------------- |
| blockHeight  | uint256  | The Bitcoin block height containing Alice's transaction.                                       |
| txid         | bytes32  | The transaction ID (double-SHA256 hash of the serialized transaction, in internal byte order). |
| vout         | uint32   | The output index of the P2WSH output within the transaction.                                   |
| amountSats   | uint64   | The output amount in satoshis. MUST equal trade.btcAmount.                                     |
| scriptPubKey | bytes    | The raw scriptPubKey bytes of the output (0x0020 + SHA-256 of witness script).                 |
| proof        | bytes    | Opaque accumulator witness / Merkle path, passed directly to the BCOC for verification.        |

## 5. Protocol Flow — Detailed

### 5.1 Phase 1: Trade Initialization

1. The order matching system produces a matched trade: Alice sells btcAmount BTC for Bob's ethAmount ETH.
1. A tradeId is derived deterministically, e.g., keccak256(abi.encodePacked(aliceEthAddress, bobEthAddress, btcAmount,
   ethAmount, nonce)).
1. Alice calls initializeTrade(tradeId, btcAmount, ethAmount, aliceBtcPubKey) on the Trade Contract, providing her
   Ethereum address (implicit via msg.sender) and her 33-byte compressed Bitcoin public key.
1. The contract MUST store a partial trade record with status = AWAITING_DEPOSIT.
1. The contract MUST emit TradeInitialized(tradeId, aliceEthAddress, btcAmount, ethAmount).

### 5.2 Phase 2: ETH Deposit (Bob)

1. Bob calls deposit(tradeId, bobBtcPubKey) sending exactly ethAmount ETH as msg.value.
1. The contract MUST verify:
   - msg.value == trade.ethAmount
   - trade.status == AWAITING_DEPOSIT
   - bobBtcPubKey is exactly 33 bytes and begins with 0x02 or 0x03 (compressed public key prefix validation)
1. The contract records bobEthAddress = msg.sender, bobBtcPubKey, sets depositTimestamp = block.timestamp, computes
   deadline = block.timestamp + T_deadline, and transitions status to AWAITING_BTC.
1. The contract MUST emit TradeDeposited(tradeId, bobEthAddress, bobBtcPubKey, ethAmount, deadline).

### 5.3 Phase 3: BTC Payment (Alice — Off-Chain)

1. Alice constructs a Bitcoin transaction with an output of exactly btcAmount satoshis sent to a P2WSH address derived
   from the witness script template (Section 4.2), substituting bobBtcPubKey and her own aliceBtcPubKey.
1. Alice broadcasts the transaction to the Bitcoin network.
1. Alice waits for at least D_conf confirmations (6 blocks, ~1 hour).
1. Alice monitors the BCOC on Ethereum to confirm the relevant Bitcoin block has been relayed and has sufficient depth.

|                                                                                                                                                                                                                        |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Note** Alice SHOULD construct the Bitcoin transaction promptly after Bob's deposit is confirmed on Ethereum. The 3-day deadline provides ample time, but delays increase Bob's capital lockup and griefing exposure. |

### 5.4 Phase 4: Proof Construction and Submission (Alice)

1. Alice constructs a UtxoInclusionProof (Section 4.3) containing:
   - blockHeight: the Bitcoin block height containing her transaction.
   - txid: the transaction ID.
   - vout: the output index of the P2WSH output.
   - amountSats: the output amount (MUST equal trade.btcAmount).
   - scriptPubKey: the raw scriptPubKey bytes (0x0020 + SHA-256 of witness script).
   - proof: the accumulator witness / Merkle path obtained from the BCOC or a UTXO set proof provider.
1. Alice also provides aliceBtcPubKey (her Bitcoin public key used in the reclaim branch).
1. Alice calls claimEth(tradeId, utxoProof, aliceBtcPubKey) on the Trade Contract.

### 5.5 Phase 5: Verification and Settlement (Trade Contract)

The Trade Contract MUST perform the following checks **in order**. If any check fails, the transaction MUST revert.

| **Step** | **Check**                    | **Condition**                                                              | **Revert Reason**           |
| -------- | ---------------------------- | -------------------------------------------------------------------------- | --------------------------- |
| 1        | Status check                 | trade.status == AWAITING_BTC                                               | "InvalidTradeStatus"        |
| 2        | Deadline check               | block.timestamp \<= trade.deadline                                         | "TradeDeadlineExceeded"     |
| 3        | Caller check                 | msg.sender == trade.aliceEthAddress                                        | "UnauthorizedCaller"        |
| 4        | Confirmation depth           | BCOC.getBlockConfirmations(proof.blockHeight) >= D_conf                    | "InsufficientConfirmations" |
| 5        | Main chain check             | BCOC.isMainChain(proof.blockHeight) == true                                | "BlockNotOnMainChain"       |
| 6        | UTXO inclusion               | BCOC.verifyUtxoInclusion(proof.blockHeight, utxoData, proof.proof) == true | "InvalidUtxoProof"          |
| 7        | Amount check                 | proof.amountSats == trade.btcAmount                                        | "BtcAmountMismatch"         |
| 8        | Script template verification | See sub-steps (a)–(c) below                                                | "ScriptMismatch"            |

**Step 8 sub-steps:**

1. Reconstruct the expected witness script using trade.bobBtcPubKey, aliceBtcPubKey, and the constant T_reclaim.
1. Compute expectedScriptPubKey = 0x0020 || SHA256(expectedWitnessScript).
1. Compare expectedScriptPubKey == proof.scriptPubKey. MUST revert if mismatch.

**If all checks pass:**

1. Transfer trade.ethAmount to trade.aliceEthAddress via a low-level call. MUST revert on transfer failure.
1. Set trade.status = COMPLETED.
1. Emit TradeCompleted(tradeId, aliceEthAddress, txid).

### 5.6 Phase 6: BTC Claiming (Bob — Off-Chain)

1. Bob constructs a Bitcoin transaction spending the UTXO using the OP_IF (spend) branch of the witness script.
1. Bob's witness stack: <bobSignature> \<1> <witnessScript>.
1. Bob broadcasts the transaction to the Bitcoin network. No Ethereum interaction is required.

|                                                                                                                                                                                                                                                                                                                                                          |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Important** Bob SHOULD claim the BTC promptly. He has until Treclaim blocks (~7 days) after the UTXO was created before Alice's reclaim path becomes spendable. Given the 4-day safety margin between Tdeadline and Treclaim, Bob has at least 4 days even in the worst case (where Alice submitted her proof at the last moment before the deadline). |

## 6. Timeout and Recovery Paths

### 6.1 Bob's ETH Reclaim (Alice Did Not Fulfill)

If block.timestamp > trade.deadline and trade.status == AWAITING_BTC:

1. Bob (or any address) calls reclaimEth(tradeId).
1. The contract MUST verify that the deadline has passed and that the trade status is still AWAITING_BTC.
1. The contract transfers trade.ethAmount back to trade.bobEthAddress. MUST revert on transfer failure.
1. The contract sets trade.status = EXPIRED.
1. The contract emits TradeExpired(tradeId).

Note: reclaimEth is permissionless (any caller MAY invoke it), but the ETH is always sent to the recorded bobEthAddress.
This allows third-party relayers or keeper bots to trigger reclaims on Bob's behalf.

### 6.2 Alice's BTC Reclaim (Trade Failed on Ethereum Side)

If Alice sent BTC but the Ethereum trade expired (Bob reclaimed ETH), Alice can reclaim her BTC after T_reclaim blocks
using the OP_ELSE branch of the witness script:

- Witness stack: <aliceSignature> \<0> <witnessScript>
- The CSV timelock ensures Alice cannot reclaim before 1,008 blocks (~7 days) after the UTXO was created.
- No Ethereum interaction is required for this operation.

### 6.3 Timing Safety Proof

The following timeline demonstrates the safety margin between the two recovery paths:

t=0 Bob deposits ETH on Ethereum.
Alice can begin constructing and broadcasting BTC transaction.
t+~1h Alice's BTC transaction achieves D_conf (6 confirmations).
Alice can submit proof to Trade Contract.
t+3d T_deadline expires.
After this point, Bob MAY reclaim ETH.
Alice can no longer submit proofs.
──── 4-day safety margin ────
t+7d T_reclaim expires (1,008 Bitcoin blocks).
After this point, Alice MAY reclaim BTC via CSV branch.

**Scenario analysis:**

| **Scenario**                                                                      | **Outcome**                                                                                                                                                                                 | **Fund Safety**                                                         |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| **Happy path:** Alice submits valid proof before t+3d.                            | Alice receives ETH. Bob spends BTC at his convenience (he has at least 4 days before Alice's reclaim becomes available, and realistically the full 7 days since the UTXO was just created). | Both parties receive their desired asset.                               |
| **Failure path:** Alice does not submit proof by t+3d.                            | Bob reclaims ETH after t+3d. Alice reclaims BTC after t+7d.                                                                                                                                 | Neither party loses funds. Both recover original assets (time-delayed). |
| **Alice sends BTC but proof fails:** Invalid proof or insufficient confirmations. | Alice retries proof submission before deadline, or recovers BTC after t+7d. Bob reclaims ETH after t+3d if Alice cannot produce a valid proof.                                              | Neither party loses funds.                                              |

### Protocol Phases & Counterparty Actions

| Phase                   | Alice’s Action                                                                                                                                         | Bob’s Action                                                                           |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| 1️⃣ Initialize            | Calls `initializeTrade` with BTC amount, ETH amount, and her Bitcoin public key.                                                                       | —                                                                                      |
| 2️⃣ Deposit               | —                                                                                                                                                      | Calls `deposit` sending exact ETH amount and providing his Bitcoin public key.         |
| 3️⃣ Pay BTC (off‑chain)   | Constructs a P2WSH output using the witness script template (Bob’s key in spend branch, Alice’s key in reclaim branch). Broadcasts to Bitcoin network. | —                                                                                      |
| 4️⃣ Submit Proof          | After ≥ Dconf confirmations, builds `UtxoInclusionProof` and calls `claimEth`.                                                                         | —                                                                                      |
| 5️⃣ Settlement            | If proof passes, Trade Contract transfers ETH to Alice, marks trade COMPLETED.                                                                         | —                                                                                      |
| 6️⃣ Claim BTC (off‑chain) | —                                                                                                                                                      | Constructs Bitcoin transaction spending the P2WSH output via OP_IF branch; broadcasts. |
| 7️⃣ Timeout / Recovery    | —                                                                                                                                                      | Calls `reclaimEth` after Tdeadline if no proof submitted; receives ETH back.           |
| 8️⃣ Reclaim BTC           | After Treclaim blocks, uses CSV timelock to spend UTXO via OP_ELSE branch.                                                                             | —                                                                                      |

## 7. Security Analysis

### 7.1 Theft Resistance

| **Threat Actor** | **Attack Vector**                                                  | **Defense**                                                                                                                                                                                                            |
| ---------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Bob**          | Attempts to withdraw escrowed ETH without Alice receiving payment. | Bob's ETH is locked until a valid UTXO proof is presented. Bob cannot fabricate a UTXO inclusion proof accepted by the BCOC. Bob can only reclaim ETH after Tdeadline, at which point Alice's BTC is still timelocked. |
| **Alice**        | Attempts to obtain ETH without providing spendable BTC to Bob.     | Alice receives ETH only after proving she created a UTXO that Bob can spend (script template verification). She cannot reclaim the BTC before the CSV timelock, which expires after the trade deadline.                |
| **Third party**  | Attempts to steal ETH from the contract.                           | Only Alice can call claimEth (caller check: msg.sender == aliceEthAddress). reclaimEth is permissionless but always sends to the recorded bobEthAddress.                                                               |

### 7.2 Reorg Attacks

**Scenario:** Alice submits a valid UTXO inclusion proof and receives ETH. Subsequently, the Bitcoin chain undergoes a
reorganization that removes Alice's transaction, effectively erasing her UTXO.

**Mitigations:**

1. **Confirmation depth:** Dconf = 6 confirmations (~1 hour of proof-of-work). A 6-block reorganization would require
   substantial hashpower, making it economically infeasible for most trade sizes.
1. **UTXO set semantics:** A UTXO inclusion proof is strictly stronger than a transaction inclusion proof. If the UTXO
   is removed by a reorg, it disappears from the UTXO set, so the proof becomes invalid for any future block height.
   This provides additional safety compared to transaction-level Merkle proofs.

|                                                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **SPV Security Trade-off** The UTXO inclusion proof is verified at submission time. Once ETH is released, a later reorganization does NOT claw back ETH. This is the inherent SPV security trade-off — the Dconf parameter controls the risk/latency balance. For high-value trades, Dconf SHOULD be increased (e.g., 20–100 blocks). |

### 7.3 Griefing Attacks

| **Vector**                               | **Impact**                                                                                                     | **Mitigation**                                                                                                                                                                        |
| ---------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bob deposits ETH, Alice never sends BTC. | Bob's ETH is locked for up to Tdeadline (3 days). Capital is idle but recoverable.                             | Shorter deadlines reduce lockup. Reputation or staking mechanisms in the order matching system can deter griefing. MAY also require Alice to post a small ETH bond at initialization. |
| Alice sends BTC but never submits proof. | Bob's ETH is locked until deadline, then reclaimable. Alice recovers BTC after Treclaim. Mild griefing on Bob. | Alice loses nothing but gains nothing; economically irrational unless deliberately griefing.                                                                                          |

### 7.4 Script Manipulation

**Attack:** Alice attempts to send BTC to a script that passes verification but is actually unspendable by Bob (e.g., by
substituting a different key in the spend branch).

**Defense:** The Trade Contract reconstructs the expected witness script byte-for-byte from known parameters
(trade.bobBtcPubKey, the submitted aliceBtcPubKey, and the constant T_reclaim) and compares the SHA-256 hash against the
scriptPubKey in the proof. Any deviation — even a single bit — results in a hash mismatch and rejection.

The aliceBtcPubKey is provided by Alice at proof submission time. The contract does not need to trust it — it only needs
to ensure that **Bob's key is in the IF branch** (verified via the hash comparison against the reconstructed script
containing trade.bobBtcPubKey).

### 7.5 Frontrunning

**Attack:** A validator or MEV searcher observes Alice's claimEth transaction in the mempool and attempts to front-run it.

**Defense:** The caller check (msg.sender == trade.aliceEthAddress) prevents any address other than Alice from executing
the claim. A front-runner cannot redirect the ETH to their own address.

### 7.6 BCOC Liveness Failure

**Scenario:** The BCOC stops relaying Bitcoin block headers, preventing Alice from submitting proofs with sufficient
confirmation depth.

**Impact:** This is a *liveness* failure, not a *safety* failure:

- Bob reclaims ETH after Tdeadline.
- Alice reclaims BTC after Treclaim.
- Neither party loses funds. Trades in progress fail gracefully with time-delayed recovery.

## 8. Comparison with Alternative Approaches

| **Property**                          | **This Protocol**                    | **HTLC Atomic Swaps**                           | **Wrapped BTC (e.g., WBTC)**        | **Custodial Bridges**       |
| ------------------------------------- | ------------------------------------ | ----------------------------------------------- | ----------------------------------- | --------------------------- |
| **Custodian required**                | No                                   | No                                              | Yes (merchant + custodian)          | Yes                         |
| **Wrapped token**                     | No                                   | No                                              | Yes                                 | Yes                         |
| **BTC buyer interaction complexity**  | Low (deposit ETH + spend BTC)        | High (monitor both chains, claim with preimage) | N/A                                 | Low                         |
| **BTC seller interaction complexity** | Medium (send BTC + submit proof)     | High (initiate HTLC, monitor, claim)            | N/A                                 | Low                         |
| **Online requirement for buyer**      | Only to spend BTC (no hard deadline) | Must claim before timelock expires              | N/A                                 | Minimal                     |
| **Infrastructure dependency**         | BCOC + Trade Contract                | Both chains must support HTLCs natively         | Custodian federation                | Bridge operator             |
| **Failure mode**                      | Both reclaim (time-delayed)          | Both reclaim (time-delayed)                     | Custodian failure / insolvency risk | Bridge hack / rug pull risk |

## 9. Gas Cost Considerations

The following table provides approximate gas cost estimates for the primary Trade Contract operations. These are
illustrative and depend on the BCOC's accumulator implementation.

| **Operation**                           | **Estimated Gas**      | **Notes**                                                                                                                                                  |
| --------------------------------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| UTXO inclusion proof verification       | 100,000 – 300,000      | Depends on accumulator type. A Merkle proof with depth ~30 requires ~30 hash verifications. SHA-256 precompile on Ethereum costs 60 + 12 per 32-byte word. |
| Script reconstruction + SHA-256 hashing | ~50,000                | Serialization of witness script (~80 bytes) and one SHA-256 invocation.                                                                                    |
| ETH transfer                            | ~21,000                | Base cost for a value transfer via low-level call.                                                                                                         |
| Storage writes + event emission         | ~30,000 – 50,000       | Status update (SSTORE) + event log.                                                                                                                        |
| **Total claimEth call**                 | **~200,000 – 400,000** | Sum of above components.                                                                                                                                   |

**Indicative cost at representative network conditions:**

- At 30 gwei gas price and ETH at $3,000: approximately **$18 – $36** per trade settlement.
- At 10 gwei gas price: approximately **$6 – $12** per trade settlement.
- These estimates are illustrative only. Actual costs depend on the BCOC implementation, Ethereum gas market conditions,
  and calldata size.

## 10. Specification Interfaces (Solidity-Style)

### 10.1 Trade Contract Interface

```soldity
interface ITradeContract {
// Phase 1: Alice initializes a trade with agreed parameters.
function initializeTrade(
bytes32 tradeId,
uint64 btcAmount, // in satoshis
uint256 ethAmount, // in wei
bytes calldata aliceBtcPubKey // 33-byte compressed
) external;
// Phase 2: Bob deposits ETH escrow.
function deposit(
bytes32 tradeId,
bytes calldata bobBtcPubKey // 33-byte compressed
) external payable;
// Phase 4-5: Alice submits proof and claims ETH.
function claimEth(
bytes32 tradeId,
UtxoInclusionProof calldata proof,
bytes calldata aliceBtcPubKey
) external;
// Phase 6.1: Anyone triggers ETH reclaim for Bob after deadline.
function reclaimEth(bytes32 tradeId) external;
// Events
event TradeInitialized(
bytes32 indexed tradeId,
address indexed aliceEthAddress,
uint64 btcAmount,
uint256 ethAmount
);
event TradeDeposited(
bytes32 indexed tradeId,
address indexed bobEthAddress,
bytes bobBtcPubKey,
uint256 ethAmount,
uint256 deadline
);
event TradeCompleted(
bytes32 indexed tradeId,
address indexed aliceEthAddress,
bytes32 txid
);
event TradeExpired(bytes32 indexed tradeId);
}
```

### 10.2 BCOC Interface (Dependency)

The Trade Contract depends on the following interface, which MUST be implemented by the Bitcoin Consensus Oracle Contract:

```solidity
interface IBitcoinConsensusOracle {
   /// @notice Verify that a UTXO exists in the committed UTXO set
   /// at the given block height.
   /// @param blockHeight Bitcoin block height
   /// @param txid Transaction ID (internal byte order)
   /// @param vout Output index
   /// @param amountSats Output value in satoshis
   /// @param scriptPubKey Raw scriptPubKey bytes
   /// @param proof Opaque accumulator witness
   /// @return true if the UTXO is proven to exist
   function verifyUtxoInclusion(
         uint256 blockHeight,
         bytes32 txid,
         uint32 vout,
         uint64 amountSats,
         bytes calldata scriptPubKey,
         bytes calldata proof
      ) external view returns (bool);

   /// @notice Returns how many blocks have been built on top of
   /// the block at the given height.
   /// @param blockHeight Bitcoin block height
   /// @return Number of confirmations (0 if block is the chain tip)
   function getBlockConfirmations(uint256 blockHeight) external view returns (uint256);

   /// @notice Returns whether the block at the given height is on
   /// the canonical main chain (not a stale/orphan block).
   /// @param blockHeight Bitcoin block height
   /// @return true if on the main chain
   function isMainChain(uint256 blockHeight) external view returns (bool);
}
```

### 10.3 Structs

```solidity
struct UtxoInclusionProof {
   uint256 blockHeight;
   bytes32 txid;
   uint32 vout;
   uint64 amountSats;
   bytes scriptPubKey;
   bytes proof; // opaque accumulator witness
}
```

## 11. Open Questions and Future Work

1. **Confirmation depth tuning:** Dconf could be made dynamic based on trade value. Higher-value trades SHOULD require
   more confirmations. A logarithmic or tiered mapping (e.g., \<0.1 BTC → 3 conf, \<1 BTC → 6 conf, \<10 BTC → 20 conf,
   ≥10 BTC → 100 conf) is RECOMMENDED for future versions.
1. **Batch trades:** Multiple trades could share a single Bitcoin transaction with multiple outputs, amortizing the
   Bitcoin transaction fee across several swaps. Each output would correspond to a separate tradeId and proof
   submission.
1. **Taproot optimization:** Using P2TR with Bob's key as the internal key (key path spend) and Alice's CSV reclaim as a
   script path leaf (within a Merkle tree of TapLeaves) would reduce Bob's on-chain footprint to a single signature,
   improve privacy (the reclaim path is never revealed on-chain in the happy path), and reduce Bitcoin transaction fees.
1. **BCOC implementation:** This specification is intentionally agnostic to how the BCOC maintains UTXO commitments.
   Possible implementations include:
   - Utreexo-style Merkle forest accumulators
   - ZK-proven Bitcoin state transitions (e.g., using SNARKs to prove UTXO set updates)
   - Optimistic approaches with fraud proofs and a challenge period
   - RSA accumulator-based designs
1. **MEV protection:** Alice's claimEth call could be routed through a private mempool (e.g., Flashbots Protect) or use
   a commit-reveal scheme to prevent sandwich attacks on the ETH transfer. While the caller check prevents direct
   frontrunning of the claim, sophisticated MEV strategies could still extract value in adjacent transactions.
1. **Multi-asset support:** The Trade Contract could be generalized to support ERC-20 tokens on the Ethereum side,
   allowing BTC-to-USDC or BTC-to-DAI swaps using the same proof mechanism.

## 12. Appendix: Worked Example

This section walks through the complete protocol with concrete values.

### 12.1 Trade Parameters

| **Parameter**          | **Value**                                                                                       |
| ---------------------- | ----------------------------------------------------------------------------------------------- |
| Trade                  | Alice sells 1 BTC for 0.5 ETH                                                                   |
| btcAmount              | 100000000 (100,000,000 satoshis = 1 BTC)                                                        |
| ethAmount              | 500000000000000000 (5 × 1017 wei = 0.5 ETH)                                                     |
| Alice's ETH address    | 0xA11cE...aaa                                                                                   |
| Bob's ETH address      | 0xB0b...bbb                                                                                     |
| Bob's BTC public key   | 0x02abc123def456789012345678901234567890123456789012345678901234abcd (33 bytes, compressed)     |
| Alice's BTC public key | 0x03def456abc789012345678901234567890123456789012345678901234567efab (33 bytes, compressed)     |
| Treclaim               | 1,008 blocks (minimal encoding: 0x03f003 — push 2 bytes 0xf003, which is 1008 in little-endian) |

### 12.2 Witness Script Construction

The witness script is assembled byte-by-byte as follows:

// Hex-encoded witness script (line breaks for readability)
63 // OP_IF
21 // OP_PUSHBYTES_33
02abc123def456789012345678901234567890 // bobBtcPubKey (33 bytes)
123456789012345678901234abcd
ac // OP_CHECKSIG
67 // OP_ELSE
02 // OP_PUSHBYTES_2
f003 // T_reclaim = 1008 (little-endian)
b2 // OP_CHECKSEQUENCEVERIFY
75 // OP_DROP
21 // OP_PUSHBYTES_33
03def456abc789012345678901234567890123 // aliceBtcPubKey (33 bytes)
45678901234567efab
ac // OP_CHECKSIG
68 // OP_ENDIF

### 12.3 P2WSH scriptPubKey Derivation

witnessScript = 0x6321...68 // full serialized witness script
scriptHash = SHA256(witnessScript) // 32-byte hash
scriptPubKey = 0x0020 || scriptHash // 34 bytes: OP_0 PUSH32 <hash>

Alice creates a Bitcoin transaction output paying exactly 100000000 satoshis to this scriptPubKey.

### 12.4 Ethereum-Side Verification Steps

When Alice calls claimEth, the Trade Contract executes:

1. **Status:** Confirms trade.status == AWAITING_BTC. ✓
1. **Deadline:** Confirms block.timestamp \<= trade.deadline. ✓ (Alice submitted within 3 days.)
1. **Caller:** Confirms msg.sender == 0xA11cE...aaa. ✓
1. **Confirmations:** Calls BCOC.getBlockConfirmations(850123), returns 8. 8 ≥ 6. ✓
1. **Main chain:** Calls BCOC.isMainChain(850123), returns true. ✓
1. **UTXO inclusion:** Calls BCOC.verifyUtxoInclusion(850123, txid, 0, 100000000, scriptPubKey, merkleProof), returns
   true. ✓
1. **Amount:** Confirms 100000000 == trade.btcAmount. ✓
1. **Script verification:**
   - Reconstructs witness script from trade.bobBtcPubKey (0x02abc1...), submitted aliceBtcPubKey (0x03def4...), and
     T_reclaim (0xf003).
   - Computes SHA256(reconstructedWitnessScript).
   - Verifies 0x0020 || hash == proof.scriptPubKey. ✓

All checks pass. The contract transfers 0.5 ETH to 0xA11cE...aaa and sets trade.status = COMPLETED.

### 12.5 Bob's Bitcoin Spending Transaction

Bob constructs a transaction spending the P2WSH UTXO:

| **Field**            | **Value**                                                  |
| -------------------- | ---------------------------------------------------------- |
| Input                | txid:0 (Alice's transaction, output index 0)               |
| Witness stack item 0 | <bobSignature> (DER-encoded ECDSA signature + SIGHASH_ALL) |
| Witness stack item 1 | 0x01 (selects the OP_IF / spend branch)                    |
| Witness stack item 2 | <witnessScript> (the full serialized witness script)       |
| Output               | Bob's desired Bitcoin address, minus miner fee             |

Bob broadcasts this transaction. It is valid immediately (no timelock on the OP_IF branch). Bob now holds the BTC. The
swap is complete.

— End of Specification —
