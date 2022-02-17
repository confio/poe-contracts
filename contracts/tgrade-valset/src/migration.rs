use cosmwasm_std::{Addr, CustomQuery, DepsMut, Order, Timestamp};
use cw_storage_plus::Map;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use tg_utils::Expiration;

use crate::error::ContractError;
use crate::msg::{JailingEnd, JailingPeriod};
use crate::state::JAIL;

/// `crate::msg::JailingPeriod` version from v0.6.2 and before
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum JailingPeriodV0_6_2 {
    Until(Expiration),
    Forever {},
}

impl JailingPeriodV0_6_2 {
    fn update(self) -> JailingPeriod {
        JailingPeriod {
            start: Timestamp::from_seconds(0),
            end: match self {
                JailingPeriodV0_6_2::Until(u) => JailingEnd::Until(u),
                JailingPeriodV0_6_2::Forever {} => JailingEnd::Forever {},
            },
        }
    }
}

pub fn migrate_jailing_period<Q: CustomQuery>(
    deps: DepsMut<Q>,
    version: &Version,
) -> Result<(), ContractError> {
    let jailings: Vec<_> = if *version <= "0.6.2".parse::<Version>().unwrap() {
        let jailings: Map<&Addr, JailingPeriodV0_6_2> = Map::new("jail");

        jailings
            .range(deps.storage, None, None, Order::Ascending)
            .map(|record| record.map(|(key, jailing_period)| (key, jailing_period.update())))
            .collect::<Result<_, _>>()?
    } else {
        return Ok(());
    };

    for (addr, jailing_period) in jailings {
        JAIL.save(deps.storage, &addr, &jailing_period)?;
    }

    Ok(())
}
