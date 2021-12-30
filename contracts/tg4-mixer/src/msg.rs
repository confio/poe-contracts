use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal as StdDecimal, Uint64};
use tg4::{Member, MemberChangedHookMsg};

use crate::error::ContractError;
use crate::functions::{AlgebraicSigmoid, GeometricMean, PoEFunction, Sigmoid, SigmoidSqrt};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    /// One of the groups we feed to the mixer function
    pub left_group: String,
    /// The other group we feed to the mixer function
    pub right_group: String,
    /// Preauthorize some hooks on init (only way to add them)
    #[serde(default)]
    pub preauths_hooks: u64,
    /// Preauthorize slasher registration on init (only way to add them)
    #[serde(default)]
    pub preauths_slashing: u64,
    /// Enum to store the proof-of-engagement function parameters used for this contract
    pub function_type: PoEFunctionType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoEFunctionType {
    /// GeometricMean returns the geometric mean of staked amount and engagement points
    GeometricMean {},
    /// Sigmoid returns a sigmoid-like value of staked amount times engagement points.
    /// See the Proof-of-Engagement whitepaper for details
    Sigmoid {
        max_rewards: Uint64,
        p: StdDecimal,
        s: StdDecimal,
    },
    /// SigmoidSqrt returns a sigmoid-like value of the geometric mean of staked amount and
    /// engagement points.
    /// It is equal to `Sigmoid` with `p = 0.5`, but implemented using integer sqrt instead of
    /// fixed-point fractional power.
    SigmoidSqrt { max_rewards: Uint64, s: StdDecimal },
    /// `AlgebraicSigmoid` returns a sigmoid-like value of staked amount times engagement points.
    /// It is similar to `Sigmoid`, but uses integer sqrt instead of a fixed-point exponential.
    AlgebraicSigmoid {
        max_rewards: Uint64,
        a: StdDecimal,
        p: StdDecimal,
        s: StdDecimal,
    },
}

impl PoEFunctionType {
    pub fn to_poe_fn(&self) -> Result<Box<dyn PoEFunction>, ContractError> {
        match self.clone() {
            PoEFunctionType::GeometricMean {} => Ok(Box::new(GeometricMean::new())),
            PoEFunctionType::Sigmoid { max_rewards, p, s } => {
                Ok(Box::new(Sigmoid::new(max_rewards, p, s)?))
            }
            PoEFunctionType::SigmoidSqrt { max_rewards, s } => {
                Ok(Box::new(SigmoidSqrt::new(max_rewards, s)?))
            }
            PoEFunctionType::AlgebraicSigmoid {
                max_rewards,
                a,
                p,
                s,
            } => Ok(Box::new(AlgebraicSigmoid::new(max_rewards, a, p, s)?)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// This handles a callback from one of the linked groups
    MemberChangedHook(MemberChangedHookMsg),
    /// Add a new hook to be informed of all membership changes.
    AddHook { addr: String },
    /// Remove a hook. Must be called by the contract being removed
    RemoveHook { addr: String },
    /// Adds slasher for contract if there are enough `slasher_preauths` left
    AddSlasher { addr: String },
    /// Removes slasher for contract
    RemoveSlasher { addr: String },
    /// Slash engagement points from address
    Slash { addr: String, portion: StdDecimal },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MemberListResponse
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, sorted by weight descending
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
    /// Which contracts we are listening to
    Groups {},
    /// Return the current number of preauths. Returns PreauthResponse.
    Preauths {},
    /// Rewards of a PoE function (used for benchmarking).
    /// Returns RewardsResponse.
    RewardFunction {
        stake: Uint64,
        engagement: Uint64,
        poe_function: Option<PoEFunctionType>,
    },
    /// Returns information (bool) whether given address is an active slasher
    IsSlasher { addr: String },
    /// Shows all active slashers as vector of addresses
    ListSlashers {},
}

/// Return the two groups we are listening to
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GroupsResponse {
    pub left: String,
    pub right: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PreauthResponse {
    pub preauths_hooks: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RewardFunctionResponse {
    pub reward: u64,
}
