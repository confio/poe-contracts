use cosmwasm_std::{Addr, Decimal};
use cw_controllers::AdminError;

use crate::error::ContractError;
use crate::multitest::suite::Suite;
use crate::state::DistributionContract;

use super::suite::SuiteBuilder;

#[test]
fn update_cfg() {
    let mut suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_points, 3);

    suite
        .update_config(
            &admin,
            Some(5),
            Some(10),
            vec![DistributionContract {
                contract: Addr::unchecked("contract1"),
                ratio: Decimal::percent(15),
            }],
        )
        .unwrap();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 10);
    assert_eq!(cfg.min_points, 5);
    assert_eq!(
        cfg.distribution_contracts,
        vec![DistributionContract {
            contract: Addr::unchecked("contract1"),
            ratio: Decimal::percent(15)
        }]
    );
}

#[test]
fn none_values_do_not_alter_cfg() {
    let mut suite: Suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .with_distribution(Decimal::percent(50), &[("engagement1", 20)], None)
        .build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_points, 3);
    assert_eq!(
        cfg.distribution_contracts,
        vec![DistributionContract {
            contract: Addr::unchecked("contract1"),
            ratio: Decimal::percent(50)
        }]
    );

    suite.update_config(&admin, None, None, None).unwrap();

    // Make sure the values haven't changed.
    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_points, 3);
    assert_eq!(
        cfg.distribution_contracts,
        vec![DistributionContract {
            contract: Addr::unchecked("contract1"),
            ratio: Decimal::percent(50)
        }]
    );
}

#[test]
fn non_admin_cannot_update_cfg() {
    let mut suite = SuiteBuilder::new().build();

    let err = suite
        .update_config("random fella", Some(5), Some(10), None)
        .unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );
}
