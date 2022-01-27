use cw_controllers::AdminError;

use crate::error::ContractError;

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

    suite.update_config(&admin, Some(5), Some(10)).unwrap();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 10);
    assert_eq!(cfg.min_points, 5);
}

#[test]
fn none_values_do_not_alter_cfg() {
    let mut suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_points, 3);

    suite.update_config(&admin, None, None).unwrap();

    // Make sure the values haven't changed.
    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_points, 3);
}

#[test]
fn non_admin_cannot_update_cfg() {
    let mut suite = SuiteBuilder::new().build();

    let err = suite
        .update_config("random fella", Some(5), Some(10))
        .unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );
}
