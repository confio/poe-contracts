use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::Item;
use tg_utils::Expiration;

/// If vesting account is discrete, tokens can't be transferred
/// until given point of time.
/// If account is continuous, then tokens will be released lineary
/// starting at pre-defined point.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum VestingPlan {
    Discrete {
        release_at: Expiration,
    },
    Continuous {
        start_at: Expiration,
        /// end_at allows linear interpolation between these points.
        end_at: Expiration,
    },
}

impl VestingPlan {
    pub fn is_expired(&self, timestamp: Timestamp) -> bool {
        match self {
            VestingPlan::Discrete { release_at } => release_at.is_expired_time(timestamp),
            VestingPlan::Continuous { end_at, .. } => end_at.is_expired_time(timestamp),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccount {
    pub denom: String,
    pub recipient: Addr,
    pub operator: Addr,
    pub oversight: Addr,
    pub vesting_plan: VestingPlan,
    /// Number of currently frozen tokens
    pub frozen_tokens: Uint128,
    /// Number of tokens that has been paid so far
    pub paid_tokens: Uint128,
    /// Number of initial tokens
    pub initial_tokens: Uint128,
    /// Has hand over been completed
    pub handed_over: bool,
}

pub const VESTING_ACCOUNT: Item<VestingAccount> = Item::new("vesting_account");
