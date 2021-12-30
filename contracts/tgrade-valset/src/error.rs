use cosmwasm_std::StdError;
use thiserror::Error;

use cw_controllers::AdminError;
use tg_bindings::Ed25519PubkeyConversionError;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    AdminError(#[from] AdminError),

    #[error("Operator is already registered, cannot change Tendermint pubkey")]
    OperatorRegistered {},

    #[error("Received system callback we didn't expect")]
    UnsupportedSudoType {},

    #[error("The address supplied doesn't implement the tg4 interface")]
    InvalidTg4Contract {},

    #[error("The epoch length must be greater than zero")]
    InvalidEpoch {},

    #[error("You must use a valid denom for the block reward (> 2 chars)")]
    InvalidRewardDenom {},

    #[error("Min_weight must be greater than zero")]
    InvalidMinWeight {},

    #[error("Max validators must be greater than zero")]
    InvalidMaxValidators {},

    #[error("Scaling must be unset or greater than zero")]
    InvalidScaling {},

    #[error("The moniker field must not be empty")]
    InvalidMoniker {},

    #[error("Tendermint pubkey must be 32 bytes long")]
    InvalidPubkey {},

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("No validators")]
    NoValidators {},

    #[error("The sum of distribution contract ratios exceeds 100%")]
    InvalidRewardsRatio {},

    #[error("No distribution contract")]
    NoDistributionContract {},

    #[error("Failure response from submsg: {0}")]
    SubmsgFailure(String),

    #[error("Invalid reply from submessage {id}, {err}")]
    ReplyParseFailure { id: u64, err: String },

    #[error("Unrecognised reply id: {0}")]
    UnrecognisedReply(u64),

    #[error("Never a validator: {0}")]
    NeverAValidator(String),

    #[error("Cannot unjail validator who's been jailed forever")]
    UnjailFromJailForeverForbidden {},
}

impl From<Ed25519PubkeyConversionError> for ContractError {
    fn from(_err: Ed25519PubkeyConversionError) -> Self {
        ContractError::InvalidPubkey {}
    }
}
