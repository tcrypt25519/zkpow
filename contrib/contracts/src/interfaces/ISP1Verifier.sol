// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title SP1 Verifier Interface
/// @notice Minimal interface needed by the Bitcoin header relay.
interface ISP1Verifier {
    /// @notice Verifies a proof for a program with the given verification key.
    /// @dev The verifier implementation may be a gateway or a direct verifier.
    /// @param programVKey The program verification key.
    /// @param publicValues The public values committed by the proof.
    /// @param proofBytes The proof bytes, including any verifier selector prefix.
    function verifyProof(bytes32 programVKey, bytes calldata publicValues, bytes calldata proofBytes) external view;
}
