use cosmwasm_std::StdError;
use thiserror::Error;

use cw_controllers::AdminError;
use tg_utils::{HookError, PreauthError, SlasherError};

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    Hook(#[from] HookError),

    #[error("{0}")]
    Slasher(#[from] SlasherError),

    #[error("{0}")]
    Preauth(#[from] PreauthError),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("No claims that can be released currently")]
    NothingToClaim {},

    #[error("Must send '{0}' to stake")]
    MissingDenom(String),

    #[error("Sent unsupported denoms, must send '{0}' to stake")]
    ExtraDenoms(String),

    #[error("Must send valid address to stake")]
    InvalidDenom(String),

    #[error("Missed address or denom")]
    MixedNativeAndCw20(String),

    #[error("No funds sent")]
    NoFunds {},

    #[error("Unrecognized sudo message")]
    UnknownSudoMsg {},

    #[error("Cw20 coins release functionality is in progress")]
    Cw20CoinsRelease {},
}
