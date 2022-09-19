use super::suite::SuiteBuilder;
use crate::msg::MigrateMsg;
use crate::state::DistributionContract;
use cosmwasm_std::{Addr, Decimal};

#[test]
fn migration_can_alter_cfg() {
    let mut suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_points, 3);

    suite
        .migrate(
            &admin,
            &MigrateMsg {
                min_points: Some(5),
                max_validators: Some(10),
                distribution_contracts: Some(vec![DistributionContract {
                    contract: Addr::unchecked("engagement1".to_string()),
                    ratio: Decimal::percent(50),
                }]),
                verify_validators: Some(true),
            },
        )
        .unwrap();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 10);
    assert_eq!(cfg.min_points, 5);
    assert!(cfg.verify_validators);
    assert_eq!(cfg.distribution_contracts, vec![DistributionContract {
        contract: Addr::unchecked("engagement1".to_string()),
        ratio: Decimal::percent(50),
    }]);
}
