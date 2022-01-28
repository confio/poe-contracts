use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    System(String),

    #[error("{0}")]
    Contract(String),

    #[error("{0}")]
    Voting(tg_voting_contract::ContractError),

    #[error("Received system callback we didn't expect")]
    UnsupportedSudoType {},

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Migrate message cannot be an empty string")]
    MigrateMsgCannotBeEmptyString {},
}

impl From<tg_voting_contract::ContractError> for ContractError {
    fn from(err: tg_voting_contract::ContractError) -> Self {
        match err {
            tg_voting_contract::ContractError::Std(err) => Self::Std(err),
            err => Self::Voting(err),
        }
    }
}
