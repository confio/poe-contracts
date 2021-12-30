use cosmwasm_std::StdError;
use tg_utils::{HookError, PreauthError, SlasherError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hook(#[from] HookError),

    #[error("{0}")]
    Slasher(#[from] SlasherError),

    #[error("{0}")]
    Preauth(#[from] PreauthError),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// TODO: Remove this when we are ready to ensure we finished implementing everything
    #[error("Unimplemented")]
    Unimplemented {},

    #[error("Contract {0} doesn't fulfill the tg4 interface")]
    NotTg4(String),

    #[error("Overflow when multiplying group weights - the product must be less than 10^18")]
    WeightOverflow {},

    #[error("Overflow when computing: {0}")]
    ComputationOverflow(&'static str),

    #[error("Overflow when computing rewards - the result cannot be represented as u64")]
    RewardOverflow {},

    #[error("The parameter '{0}' is out of range: {1}")]
    ParameterRange(&'static str, String),
}
