// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {IMyChainProofVerifier} from "../interfaces/IMyChainProofVerifier.sol";
import {NitroEnclaveKeyRegistry} from "./NitroEnclaveKeyRegistry.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

/// @title NitroProofVerifier
/// @author World Contributors
/// @custom:security-contact security@toolsforhumanity.com
contract NitroProofVerifier is IMyChainProofVerifier {
    /// @dev `TransitionPublicValues` contains six ABI words.
    uint256 internal constant PUBLIC_VALUES_LENGTH = 6 * 32;

    /// @notice Registry of attested enclave keys.
    NitroEnclaveKeyRegistry public immutable registry;

    /// @param registry_ The Nitro enclave key registry to consult.
    constructor(NitroEnclaveKeyRegistry registry_) {
        registry = registry_;
    }

    /// @inheritdoc IMyChainProofVerifier
    function verify(bytes calldata proof, bytes32 verifierId, bytes calldata publicValues)
        external
        view
        returns (bool)
    {
        if (verifierId == bytes32(0) || publicValues.length != PUBLIC_VALUES_LENGTH) return false;
        (address recovered, ECDSA.RecoverError err,) = ECDSA.tryRecover(keccak256(publicValues), proof);
        return err == ECDSA.RecoverError.NoError && registry.isSignerRegisteredForImage(recovered, verifierId);
    }
}
