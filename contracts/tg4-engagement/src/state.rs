use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::i128::Int128;
use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use tg_utils::Duration;
pub use tg_utils::{PREAUTH_SLASHING, SLASHERS};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Halflife {
    /// if set to None then there's no half life
    pub halflife: Option<Duration>,

    pub last_applied: Timestamp,
}

impl Halflife {
    pub fn should_apply(&self, t: Timestamp) -> bool {
        if let Some(halflife) = self.halflife {
            halflife.after_time(self.last_applied).is_expired_time(t)
        } else {
            false
        }
    }
}

/// How much points is the worth of single token in token distribution.
/// The scaling is performed to have better precision of fixed point division.
/// This value is not actually the scaling itself, but how much bits value should be shifted
/// (for way more efficient division).
///
/// `32, to have those 32 bits, but it reduces how much tokens may be handled by this contract
/// (it is now 96-bit integer instead of 128). In original ERC2222 it is handled by 256-bit
/// calculations, but I256 is missing and it is required for this.
pub const POINTS_SHIFT: u8 = 32;

pub const HALFLIFE: Item<Halflife> = Item::new("halflife");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Distribution {
    /// Tokens can be distributed by this denom.
    pub denom: String,
    /// How much points is single point of weight worth at this point.
    pub points_per_weight: Uint128,
    /// Points which were not fully distributed on previous distributions, and should be redistributed
    pub points_leftover: u64,
    /// Total funds distributed by this contract.
    pub distributed_total: Uint128,
    /// Total funds not yet withdrawn.
    pub withdrawable_total: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct WithdrawAdjustment {
    /// How much points should be added/removed from calculated funds while withdrawal.
    pub points_correction: Int128,
    /// How much funds addresses already withdrawn.
    pub withdrawn_funds: Uint128,
    /// User delegated for funds withdrawal
    pub delegated: Addr,
}

/// Tokens distribution data
pub const DISTRIBUTION: Item<Distribution> = Item::new("distribution");
/// Information how to exactly adjust tokens while withdrawal
pub const WITHDRAW_ADJUSTMENT: Map<&Addr, WithdrawAdjustment> = Map::new("withdraw_adjustment");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halflife_should_apply() {
        let epoch = 123456789;
        let hf = Halflife {
            halflife: None,
            last_applied: Timestamp::from_seconds(epoch),
        };
        assert!(!hf.should_apply(Timestamp::from_seconds(epoch)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch),
        };
        assert!(!hf.should_apply(Timestamp::from_seconds(epoch)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch),
        };
        // because halflife + last_applied + 1 = one second after half life is expected to be met
        assert!(hf.should_apply(Timestamp::from_seconds(epoch * 2 + 1)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch + 2),
        };
        assert!(!hf.should_apply(Timestamp::from_seconds(epoch + 2)));

        let hf = Halflife {
            halflife: Some(Duration::new(epoch + 1)),
            last_applied: Timestamp::from_seconds(epoch + 2),
        };
        assert!(hf.should_apply(Timestamp::from_seconds(epoch * 2 + 3)));
    }
}
