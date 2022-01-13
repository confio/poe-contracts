use cosmwasm_std::{Decimal, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Group contract invalid address '{addr}'")]
    InvalidGroup { addr: String },

    #[error("Invalid voting quorum percentage, must be 0.01-1.0: {0}")]
    InvalidQuorum(Decimal),

    #[error("Invalid voting threshold percentage, must be 0.5-1.0: {0}")]
    InvalidThreshold(Decimal),

    #[error("Invalid voting period, must be 1-365 days: {0}")]
    InvalidVotingPeriod(u32),

    #[error("Proposal is not open")]
    NotOpen {},

    #[error("Proposal voting period has expired")]
    Expired {},

    #[error("Proposal must expire before you can close it")]
    NotExpired {},

    #[error("Already voted on this proposal")]
    AlreadyVoted {},

    #[error("Cannot close completed or passed proposals")]
    WrongCloseStatus {},

    #[error("Proposal must have passed and not yet been executed")]
    WrongExecuteStatus {},
}
