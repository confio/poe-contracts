use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::claim::Claims;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use tg_utils::Duration;

/// Builds a claims map as it cannot be done in const time
pub fn claims() -> Claims<'static> {
    Claims::new("claims", "claims__release")
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// denom of the token to stake
    pub denom: String,
    pub tokens_per_weight: Uint128,
    pub min_bond: Uint128,
    /// time in seconds
    pub unbonding_period: Duration,
    /// limits of how much claims can be automatically returned at end of block
    pub auto_return_limit: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STAKE: Map<&Addr, Uint128> = Map::new("stake");
