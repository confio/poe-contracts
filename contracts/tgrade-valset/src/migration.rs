use cosmwasm_std::{Addr, CustomQuery, DepsMut, Order, Timestamp};
use cw_storage_plus::Map;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use tg_utils::Expiration;

use crate::error::ContractError;
use crate::msg::{JailingEnd, JailingPeriod};
use crate::state::{CONFIG, JAIL};

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

pub fn migrate_verify_validators<Q: CustomQuery>(
    deps: DepsMut<Q>,
    version: &Version,
) -> Result<(), ContractError> {
    let mut config = if *version <= "0.14.0".parse::<Version>().unwrap() {
        CONFIG.load(deps.storage)?
    } else {
        return Ok(());
    };
    config.verify_validators = true;
    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    //! These are very rudimentary tests that only -mock- old state and perform migrations on it.
    //! It's absolutely vital to do more thorough migration testing on some actual old state.

    use cosmwasm_std::{testing::mock_dependencies, StdError, Storage};

    use super::*;

    fn mock_v_0_6_2_jailing_periods(
        store: &mut dyn Storage,
        jailings: &[(&str, JailingPeriodV0_6_2)],
    ) {
        let jail_map: Map<&Addr, JailingPeriodV0_6_2> = Map::new("jail");

        for (addr, period) in jailings.iter().cloned() {
            jail_map
                .update(store, &Addr::unchecked(addr), |_| -> Result<_, StdError> {
                    Ok(period)
                })
                .unwrap();
        }
    }

    #[test]
    fn migrate_jailing_period_v_0_6_2() {
        let mut deps = mock_dependencies();

        mock_v_0_6_2_jailing_periods(
            &mut deps.storage,
            &[
                (
                    "alice",
                    JailingPeriodV0_6_2::Until(Expiration::at_timestamp(Timestamp::from_seconds(
                        123,
                    ))),
                ),
                ("bob", JailingPeriodV0_6_2::Forever {}),
            ],
        );

        migrate_jailing_period(deps.as_mut(), &Version::parse("0.6.2").unwrap()).unwrap();

        // verify the data is what we expect
        let jailed = JAIL
            .range(&deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            jailed,
            [
                (
                    Addr::unchecked("alice"),
                    JailingPeriod {
                        start: Timestamp::from_seconds(0),
                        end: JailingEnd::Until(Expiration::at_timestamp(Timestamp::from_seconds(
                            123
                        )))
                    }
                ),
                (
                    Addr::unchecked("bob"),
                    JailingPeriod {
                        start: Timestamp::from_seconds(0),
                        end: JailingEnd::Forever {}
                    }
                )
            ]
        );
    }
}
