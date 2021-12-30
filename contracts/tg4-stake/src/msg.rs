use cosmwasm_std::{Coin, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_utils::{Duration, Expiration};

pub use crate::claim::Claim;
use tg4::Member;

const fn default_auto_return_limit() -> u64 {
    20
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// denom of the token to stake
    pub denom: String,
    pub tokens_per_weight: Uint128,
    pub min_bond: Uint128,
    /// unbounding period in seconds
    pub unbonding_period: u64,

    // admin can only add/remove hooks and slashers, not change other parameters
    pub admin: Option<String>,
    // or you can simply pre-authorize a number of hooks (to be done in following messages)
    #[serde(default)]
    pub preauths_hooks: u64,
    // and you can pre-authorize a number of slashers the same way
    #[serde(default)]
    pub preauths_slashing: u64,
    /// Limits how much claims would be automatically returned at end of block, 20 by default.
    /// Setting this to 0 disables auto returning claims.
    #[serde(default = "default_auto_return_limit")]
    pub auto_return_limit: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Bond will bond all staking tokens sent with the message and update membership weight
    Bond {},
    /// Unbond will start the unbonding process for the given number of tokens.
    /// The sender immediately loses weight from these tokens, and can claim them
    /// back to his wallet after `unbonding_period`
    Unbond { tokens: Coin },
    /// Claim is used to claim your native tokens that you previously "unbonded"
    /// after the contract-defined waiting period (eg. 1 week)
    Claim {},

    /// Change the admin
    UpdateAdmin { admin: Option<String> },
    /// Add a new hook to be informed of all membership changes. Must be called by Admin
    AddHook { addr: String },
    /// Remove a hook. Must be called by Admin
    RemoveHook { addr: String },
    /// Add a new slasher. Must be called by Admin
    AddSlasher { addr: String },
    /// Remove a slasher. Must be called by Admin
    RemoveSlasher { addr: String },
    Slash {
        addr: String,
        // between (0.0, 1.0]
        portion: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns config
    Configuration {},
    /// Claims shows the tokens in process of unbonding for this address
    Claims {
        address: String,
        limit: Option<u32>,
        start_after: Option<Expiration>,
    },
    // Show the number of tokens currently staked by this address.
    Staked {
        address: String,
    },
    // Returns the unbonding period of this contract
    UnbondingPeriod {},

    /// Return AdminResponse
    Admin {},
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MemberListResponse
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, sorted by weight descending.
    ListMembersByWeight {
        start_after: Option<Member>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse
    Member {
        addr: String,
        at_height: Option<u64>,
    },
    /// Shows all registered hooks. Returns HooksResponse.
    Hooks {},
    /// Return the current number of preauths. Returns PreauthResponse.
    Preauths {},
    /// Returns information (bool) whether given address is an active slasher
    IsSlasher {
        addr: String,
    },
    /// Returns all active slashers as vector of addresses
    ListSlashers {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakedResponse {
    pub stake: Coin,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PreauthResponse {
    pub preauths_hooks: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct UnbondingPeriodResponse {
    pub unbonding_period: Duration,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ClaimsResponse {
    pub claims: Vec<Claim>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TotalWeightResponse {
    pub weight: u64,
    pub denom: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::to_vec;
    use tg_utils::Duration;

    #[test]
    fn unbonding_period_serializes_in_seconds() {
        let res = UnbondingPeriodResponse {
            unbonding_period: Duration::new(12345),
        };
        let json = to_vec(&res).unwrap();
        assert_eq!(&json, br#"{"unbonding_period":12345}"#);
    }
}
