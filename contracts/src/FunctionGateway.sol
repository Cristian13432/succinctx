// SPDX-License-Identifier: MIT

pragma solidity ^0.8.16;

import {IFunctionGateway, FunctionRequest} from "./interfaces/IFunctionGateway.sol";
import {IFunctionVerifier} from "./interfaces/IFunctionVerifier.sol";
import {FunctionRegistry} from "./FunctionRegistry.sol";
import {TimelockedUpgradeable} from "./upgrades/TimelockedUpgradeable.sol";
import {IFeeVault} from "src/payments/interfaces/IFeeVault.sol";

contract FunctionGateway is
    IFunctionGateway,
    FunctionRegistry,
    TimelockedUpgradeable
{
    /// @dev The default gas limit for requests.
    uint256 public constant DEFAULT_GAS_LIMIT = 1000000;

    /// @dev Keeps track of the nonce for generating request ids.
    uint256 public nonce;

    /// @dev Maps request ids to their corresponding requests.
    mapping(bytes32 => FunctionRequest) public requests;

    /// @notice The dynamic scalar for requests.
    uint256 public scalar;

    /// @notice A reference to the contract where fees are sent.
    /// @dev During the request functions, this is used to add msg.value to the sender's balance.
    address public feeVault;

    function initialize(
        uint256 _scalar,
        address _feeVault,
        address _timelock,
        address _guardian
    ) external initializer {
        scalar = _scalar;
        feeVault = _feeVault;
        __TimelockedUpgradeable_init(_timelock, _guardian);
    }

    function zkCallback(
        bytes32 _functionId,
        bytes memory _input,
        bytes4 _callbackSelector,
        bytes memory _context
    ) external payable returns (bytes32) {
        return
            zkCallback(
                _functionId,
                _input,
                _callbackSelector,
                _context,
                DEFAULT_GAS_LIMIT,
                tx.origin // TODO: bearish using tx.origin here, causes some problems in Forge scripts
            );
    }

    /// @dev Requests for a proof to be generated by the marketplace.
    /// @param _functionId The id of the proof to be generated.
    /// @param _input The input to the proof.
    /// @param _context The context of the runtime.
    /// @param _callbackSelector The selector of the callback function.
    /// @param _gasLimit The gas limit of the callback function.
    /// @param _refundAccount The account to refund the excess amount of gas to.
    function zkCallback(
        bytes32 _functionId,
        bytes memory _input,
        bytes memory _context,
        bytes4 _callbackSelector,
        uint256 _gasLimit,
        address _refundAccount
    ) public payable returns (uint256) {
        FunctionRequest memory r = FunctionRequest({
            functionId: _functionId,
            input: _input,
            context: _context,
            callbackAddress: msg.sender,
            callbackSelector: _callbackSelector
        });

        uint256 feeAmount = _handlePayment(
            _gasLimit,
            _refundAccount,
            msg.sender,
            msg.value
        );

        bytes32 requestId = keccak256(abi.encode(r));
        requests[nonce] = requestId;

        emit ProofRequested(
            nonce,
            _functionId,
            requestId,
            _input,
            _context,
            _gasLimit,
            feeAmount
        );
        nonce++;
        return nonce - 1;
    }

    /// @dev The entrypoint for fulfilling zkCallback requests
    /// @param _requestId The id of the request to be fulfilled.
    /// @param _outputHash The output hash of the proof.
    /// @param _proof The proof.
    function zkCallbackFulfill(
        uint256 _nonce,
        bytes32 _functionId,
        bytes memory _input,
        bytes memory _output,
        bytes memory _proof,
        bytes memory _context,
        address _callbackAddress,
        bytes32 _callbackSelector
    ) external {
        FunctionRequest memory r = FunctionRequest({
            functionId: _functionId,
            input: _input,
            context: _context,
            callbackAddress: _callbackAddress,
            callbackSelector: _callbackSelector
        });
        bytes32 requestId = keccak256(abi.encode(r));
        if (requests[_nonce] != requestId) {
            revert RequestNotFound(_nonce);
        }

        // Verify the proof.
        address verifier = verifiers[_functionId];
        bytes32 inputHash = sha256(_input);
        bytes32 outputHash = sha256(_output);
        if (
            !IFunctionVerifier(verifier).verify(inputHash, outputHash, _proof)
        ) {
            revert InvalidProof(
                address(verifier),
                inputHash,
                outputHash,
                proof
            );
        }

        emit ProofFulfilled(_nonce, _input, _output, _proof);

        // Call the callback.
        (bool status, ) = _callbackAddress.call(
            abi.encodeWithSelector(_callbackSelector, _output, _context)
        );
        if (!status) {
            revert CallbackFailed(_callbackAddress, _callbackSelector);
        }

        emit CallbackFulfilled(_nonce, _input, _output);
    }

    /// @dev zkCall is like `call`
    /// @param _requestId The id of the request to be fulfilled.
    /// @param _output The output of the proof.
    /// @param _context The context of the runtime.
    function zkCall(
        bytes32 _functionId,
        bytes memory _input
    ) external returns (bool, bytes memory) {
        if (
            currentVerifiedCall.functionId == _functionId &&
            currentVerifiedCall.inputHash == sha256(_input)
        ) {
            return (true, currentVerifiedCall.output);
        } else {
            // TODO: process payment
            emit ProofRequested(_functionId, _input);
            return (false, "");
        }
    }

    function zkCallFulfill(
        bytes32 _functionId,
        bytes memory _input,
        bytes memory _output,
        bytes memory _proof,
        address _callbackAddress,
        bytes memory _callbackData
    ) external {
        bytes32 inputHash = sha256(_input);
        bytes32 outputHash = sha256(_output);

        // Verify the proof.
        address verifier = verifiers[r.functionId];
        if (
            !IFunctionVerifier(verifier).verify(inputHash, outputHash, _proof)
        ) {
            revert InvalidProof(
                address(verifier),
                inputHash,
                _outputHash,
                _proof
            );
        }

        if (_callbackAddress == address(0)) {
            // If there is no callback, then just store the result
            verifiedResults[functionId][inputHash] = outputHash;
            return;
        }

        VerifiedCall currentVerifiedCall = VerifiedCall({
            functionId: _functionId,
            inputHash: inputHash,
            outputHash: outputHash,
            input: _input,
            output: _output
        });

        // Call the callback.
        (bool status, ) = _callbackAddress.call(_callbackData);
        if (!status) {
            revert CallbackFailed(_callbackAddress, _callbackData);
        }

        delete currentVerifiedCall;
        // emit CallbackFulfilled(_requestId, _output, _context);
    }

    /// @notice Update the scalar.
    function updateScalar(uint256 _scalar) external onlyGuardian {
        scalar = _scalar;

        emit ScalarUpdated(_scalar);
    }

    /// @notice Calculates the feeAmount for the default gasLimit.
    function calculateFeeAmount() external view returns (uint256 feeAmount) {
        return calculateFeeAmount(DEFAULT_GAS_LIMIT);
    }

    /// @notice Calculates the feeAmount for a given gasLimit.
    function calculateFeeAmount(
        uint256 _gasLimit
    ) public view returns (uint256 feeAmount) {
        if (scalar == 0) {
            feeAmount = tx.gasprice * _gasLimit;
        } else {
            feeAmount = tx.gasprice * _gasLimit * scalar;
        }
    }

    /// @dev Calculates the feeAmount for the request, sends the feeAmount to the FeeVault, and
    ///      sends the excess amount as a refund to the refundAccount.
    function _handlePayment(
        uint256 _gasLimit,
        address _refundAccount,
        address _senderAccount,
        uint256 _value
    ) private returns (uint256 feeAmount) {
        feeAmount = calculateFeeAmount(_gasLimit);
        if (_value < feeAmount) {
            revert InsufficientFeeAmount(feeAmount, _value);
        }

        // Send the feeAmount amount to the fee vault.
        if (feeAmount > 0 && feeVault != address(0)) {
            IFeeVault(feeVault).depositNative{value: feeAmount}(_senderAccount);
        }

        // Send the excess amount to the refund account.
        uint256 refundAmount = _value - feeAmount;
        if (refundAmount > 0) {
            (bool success, ) = _refundAccount.call{value: refundAmount}("");
            if (!success) {
                revert RefundFailed(_refundAccount, refundAmount);
            }
        }
    }

    /// @dev This empty reserved space to add new variables without shifting down storage.
    ///      See: https://docs.openzeppelin.com/contracts/4.x/upgradeable#storage_gaps
    uint256[50] private __gap;
}
