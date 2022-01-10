use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, CosmosMsg, Uint128};

use crate::state::VestingPlan;
use tg_bindings::TgradeMsg;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub denom: String,
    /// Account that receives the tokens once they have been vested and released.
    pub recipient: Addr,
    /// Secure multi-sig from SOB, which can be used to change the Operator
    /// or to hald the release of future tokens in the case of misbehavior.
    pub operator: Addr,
    /// Validator or an optional delegation to an "operational" employee from
    /// SOB, which can approve the payout of fully vested tokens to the final
    /// recipient.
    pub oversight: Addr,
    pub vesting_plan: VestingPlan,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Execute regular messages allowing to use vesting account as fully
    /// functional "proxy account"
    Execute {
        msgs: Vec<CosmosMsg<TgradeMsg>>,
    },
    ReleaseTokens {
        amount: Option<Uint128>,
    },
    /// If the recipient violates a contractual agreement, he may get find his
    /// tokens frozen
    FreezeTokens {
        amount: Option<Uint128>,
    },
    UnfreezeTokens {
        amount: Option<Uint128>,
    },

    // TODO: Add Bond/Unbond implementations
    Bond {},
    Unbond {
        amount: Uint128,
    },

    /// Oversight is able to change the operator'a account address.
    ChangeOperator {
        address: Addr,
    },
    /// Once end time of the contract has passed, hand over can be performed.
    /// It will burn all frozen tokens and set Oversight and Operator's addresses
    /// to the Reciepient's key. This marks the contract as Liberated
    HandOver {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Checks whether account has been handed over and if Sender is the Oversight
    CanExecute { sender: String },
    /// Provides information about current recipient/operator/oversight addresses
    /// as well as vesting plan for this account
    AccountInfo {},
    /// Shows current data about tokens from this vesting account.
    TokenInfo {},
    /// Shows whether hand over procedure has been completed or not
    IsHandedOver {},
}

/// Response for CanExecute query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CanExecuteResponse {
    pub can_execute: bool,
}

/// Response for AccountInfo query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AccountInfoResponse {
    pub recipient: Addr,
    pub operator: Addr,
    pub oversight: Addr,
    /// Timestamps for current discrete or continuous vesting plan
    pub vesting_plan: VestingPlan,
}

/// Response for TokenInfo query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfoResponse {
    pub denom: String,
    /// Initial amount of vested tokens
    pub initial: Uint128,
    /// Amount of currently frozen tokens
    pub frozen: Uint128,
    /// Amount of tokens that has been paid so far
    pub released: Uint128,
    /// Amount of all tokens from current contract
    pub balance: Uint128,
}

/// Response for IsLiberated query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct IsHandedOverResponse {
    /// Does this account completed hand over procedure and thus achieved
    /// "liberated" status
    pub is_handed_over: bool,
}
