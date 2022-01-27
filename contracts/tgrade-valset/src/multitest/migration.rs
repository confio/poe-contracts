use super::suite::SuiteBuilder;
use crate::msg::MigrateMsg;

#[test]
fn migration_can_alter_cfg() {
    let mut suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_weight(3)
        .build();
    let admin = suite.admin().to_string();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 6);
    assert_eq!(cfg.min_weight, 3);

    suite
        .migrate(
            &admin,
            &MigrateMsg {
                min_weight: Some(5),
                max_validators: Some(10),
            },
        )
        .unwrap();

    let cfg = suite.config().unwrap();
    assert_eq!(cfg.max_validators, 10);
    assert_eq!(cfg.min_weight, 5);
}
