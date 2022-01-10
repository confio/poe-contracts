use cosmwasm_std::StdError;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] cw_utils::PaymentError),

    #[error("Unauthorized: action requires sender to be Operator or Oversight")]
    RequireOperator,

    #[error("Unauthorized: action requires sender to be Oversight")]
    RequireOversight,

    #[error("Unauthorized: action requires sender to be Recipient")]
    RequireRecipient,

    #[error("Unauthorized: action requires sender to be Recipient or Oversight")]
    RequireRecipientOrOversight,

    #[error("Not enough tokens available")]
    NotEnoughTokensAvailable,

    #[error("Contract must be expired to proceed with hand over")]
    ContractNotExpired,

    #[error("Unauthorized: hand over not done")]
    HandOverNotCompleted,

    #[error(
        "Unaccessible operation - account has released all available and burnt all frozen tokens"
    )]
    HandOverCompleted,

    #[error("Amount of tokens in operation must be higher then zero")]
    ZeroTokensNotAllowed,

    // TODO: Temporary error to not panic at unimplemented parts - remove when done
    #[error("Not available - implementation is not finished")]
    NotImplemented,
}
