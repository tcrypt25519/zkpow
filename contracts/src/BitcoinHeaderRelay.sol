// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ISP1Verifier} from "./interfaces/ISP1Verifier.sol";

/// @title Bitcoin Header Relay
/// @notice Bounded on-chain relay for SP1-backed Bitcoin header-chain tips.
/// @dev The verifier can be an SP1 gateway or a direct verifier. Each relay instance is tied to a
/// single program verification key, but multiple relay instances may share the same verifier.
contract BitcoinHeaderRelay {
    /// @notice Contract-facing public values for one accepted proof submission.
    /// @dev This is intentionally ABI-friendly; it should be easy for Solidity to decode and for
    /// off-chain consumers to index without knowing anything about the guest's internal rkyv state.
    struct PublicValuesV1 {
        uint32 schemaVersion;
        bytes32 genesisHash;
        bytes32 blockHash;
        bytes32 prevBlockHash;
        uint32 tipHeight;
        uint256 cumulativeChainWork;
        uint32 blockTimestamp;
        uint32 nBits;
        uint32 nonce;
        bytes32 merkleRoot;
    }

    /// @notice Summary stored for each accepted tip.
    struct HeaderRecord {
        uint32 schemaVersion;
        bytes32 genesisHash;
        bytes32 blockHash;
        bytes32 prevBlockHash;
        bytes32 merkleRoot;
        uint256 cumulativeChainWork;
        uint32 tipHeight;
        uint32 blockTimestamp;
        uint32 nBits;
        uint32 nonce;
    }

    error InvalidPublicValuesLength(uint256 actual);
    error UnsupportedSchemaVersion(uint32 version);
    error GenesisHashMismatch(bytes32 expected, bytes32 actual);
    error NonIncreasingHeight(uint32 currentHeight, uint32 newHeight);
    error NonIncreasingChainWork(uint256 currentChainWork, uint256 newChainWork);
    error PrevHashMismatch(bytes32 expected, bytes32 actual);
    error CapacityZero();
    error ZeroAddress();
    error OffsetOutOfRange(uint256 offset, uint256 acceptedCount);
    error SlotOutOfRange(uint256 slot, uint256 capacity);

    event HeaderAccepted(
        uint256 indexed slot,
        uint32 indexed tipHeight,
        bytes32 indexed blockHash,
        bytes32 prevBlockHash,
        bytes32 genesisHash,
        uint256 cumulativeChainWork,
        bytes32 proofHash
    );

    ISP1Verifier public immutable VERIFIER;
    bytes32 public immutable PROGRAM_V_KEY;
    bytes32 public immutable GENESIS_HASH;
    uint256 public immutable RECENT_HEADER_CAPACITY;

    uint256 public acceptedCount;
    bool public hasTip;
    bytes32 public latestProofHash;

    HeaderRecord private _latestTip;
    mapping(uint256 => HeaderRecord) private _recentHeaders;

    constructor(address verifier_, bytes32 programVKey_, bytes32 genesisHash_, uint256 recentHeaderCapacity_) {
        if (verifier_ == address(0)) revert ZeroAddress();
        if (recentHeaderCapacity_ == 0) revert CapacityZero();

        VERIFIER = ISP1Verifier(verifier_);
        PROGRAM_V_KEY = programVKey_;
        GENESIS_HASH = genesisHash_;
        RECENT_HEADER_CAPACITY = recentHeaderCapacity_;
    }

    /// @notice Submit a proof-backed header-chain tip.
    /// @dev The proof is forwarded to the configured verifier unchanged.
    function submit(bytes calldata publicValues, bytes calldata proofBytes) external {
        VERIFIER.verifyProof(PROGRAM_V_KEY, publicValues, proofBytes);

        PublicValuesV1 memory summary = _decodePublicValues(publicValues);
        _validateSummary(summary);

        uint256 slot = acceptedCount % RECENT_HEADER_CAPACITY;
        HeaderRecord memory record = HeaderRecord({
            schemaVersion: summary.schemaVersion,
            genesisHash: summary.genesisHash,
            blockHash: summary.blockHash,
            prevBlockHash: summary.prevBlockHash,
            merkleRoot: summary.merkleRoot,
            cumulativeChainWork: summary.cumulativeChainWork,
            tipHeight: summary.tipHeight,
            blockTimestamp: summary.blockTimestamp,
            nBits: summary.nBits,
            nonce: summary.nonce
        });

        _recentHeaders[slot] = record;
        _latestTip = record;
        latestProofHash = keccak256(proofBytes);
        hasTip = true;
        acceptedCount += 1;

        emit HeaderAccepted(
            slot,
            summary.tipHeight,
            summary.blockHash,
            summary.prevBlockHash,
            summary.genesisHash,
            summary.cumulativeChainWork,
            latestProofHash
        );
    }

    /// @notice Return the latest accepted tip.
    function latestTip() external view returns (HeaderRecord memory) {
        return _latestTip;
    }

    /// @notice Read a header record by ring-buffer slot.
    function getHeader(uint256 slot) external view returns (HeaderRecord memory) {
        if (slot >= RECENT_HEADER_CAPACITY) revert SlotOutOfRange(slot, RECENT_HEADER_CAPACITY);
        return _recentHeaders[slot];
    }

    /// @notice Read the tip history relative to the latest accepted tip.
    /// @dev Offset zero is the latest accepted tip, one is the previous accepted tip, and so on.
    function getHeaderByOffset(uint256 offset) external view returns (HeaderRecord memory) {
        uint256 available = acceptedCount < RECENT_HEADER_CAPACITY ? acceptedCount : RECENT_HEADER_CAPACITY;
        if (offset >= available) revert OffsetOutOfRange(offset, available);
        uint256 slot = (acceptedCount - 1 - offset) % RECENT_HEADER_CAPACITY;
        return _recentHeaders[slot];
    }

    /// @notice Return up to `count` accepted headers, newest first.
    function getRecentHeaders(uint256 count) external view returns (HeaderRecord[] memory headers) {
        uint256 available = acceptedCount < RECENT_HEADER_CAPACITY ? acceptedCount : RECENT_HEADER_CAPACITY;
        if (count > available) revert OffsetOutOfRange(count, available);
        headers = new HeaderRecord[](count);
        for (uint256 i = 0; i < count; ++i) {
            headers[i] = _recentHeaders[(acceptedCount - 1 - i) % RECENT_HEADER_CAPACITY];
        }
    }

    function _decodePublicValues(bytes calldata publicValues) internal pure returns (PublicValuesV1 memory summary) {
        if (publicValues.length != 320) {
            revert InvalidPublicValuesLength(publicValues.length);
        }

        (
            summary.schemaVersion,
            summary.genesisHash,
            summary.blockHash,
            summary.prevBlockHash,
            summary.tipHeight,
            summary.cumulativeChainWork,
            summary.blockTimestamp,
            summary.nBits,
            summary.nonce,
            summary.merkleRoot
        ) =
            abi.decode(
                publicValues, (uint32, bytes32, bytes32, bytes32, uint32, uint256, uint32, uint32, uint32, bytes32)
            );
    }

    function _validateSummary(PublicValuesV1 memory summary) internal view {
        if (summary.schemaVersion != 1) {
            revert UnsupportedSchemaVersion(summary.schemaVersion);
        }
        if (summary.genesisHash != GENESIS_HASH) {
            revert GenesisHashMismatch(GENESIS_HASH, summary.genesisHash);
        }
        if (hasTip) {
            if (summary.tipHeight <= _latestTip.tipHeight) {
                revert NonIncreasingHeight(_latestTip.tipHeight, summary.tipHeight);
            }
            if (summary.cumulativeChainWork <= _latestTip.cumulativeChainWork) {
                revert NonIncreasingChainWork(_latestTip.cumulativeChainWork, summary.cumulativeChainWork);
            }
            if (summary.prevBlockHash != _latestTip.blockHash) {
                revert PrevHashMismatch(_latestTip.blockHash, summary.prevBlockHash);
            }
        }
    }
}
