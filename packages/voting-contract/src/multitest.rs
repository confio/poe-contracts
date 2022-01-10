use suite::SuiteBuilder;

use crate::state::RulesBuilder;

mod closing;
mod contracts;
mod proposing;
mod suite;

#[test]
fn simple_instantiate() {
    let rules = RulesBuilder::new().build();
    let mut suite = SuiteBuilder::new().with_rules(rules.clone()).build();

    assert_eq!(rules, suite.query_rules().unwrap());
}

#[test]
fn list_voters() {
    let mut suite = SuiteBuilder::new()
        .with_member("alice", 2)
        .with_member("bob", 3)
        .with_member("eve", 999)
        .build();

    let owner = suite.owner.clone();
    suite.assert_voters(&[("alice", 2), ("bob", 3), ("eve", 999)]);

    suite
        .modify_members(owner.as_str(), &[("charlie", 1)], &["eve"])
        .unwrap();

    suite.assert_voters(&[("alice", 2), ("bob", 3), ("charlie", 1)]);
}
