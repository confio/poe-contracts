use cosmwasm_std::Decimal;
use tg_bindings_test::UpgradePlan;
use tg_test_utils::RulesBuilder;

use super::suite::{get_proposal_id, SuiteBuilder};

#[test]
fn pin_contract() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member("member", 1)
        .with_voting_rules(rules)
        .build();

    let proposal = suite.propose_pin("member", &[1, 3]).unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    assert!(suite.check_pinned(1).unwrap());
    assert!(!suite.check_pinned(2).unwrap());
    assert!(suite.check_pinned(3).unwrap());
}

#[test]
fn unpin_contract() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member("member", 1)
        .with_voting_rules(rules)
        .build();

    let proposal = suite.propose_pin("member", &[1, 2, 3]).unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    let proposal = suite.propose_unpin("member", &[1, 3]).unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    assert!(!suite.check_pinned(1).unwrap());
    assert!(suite.check_pinned(2).unwrap());
    assert!(!suite.check_pinned(3).unwrap());
}

#[test]
fn upgrade() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member("member", 1)
        .with_voting_rules(rules)
        .build();

    // We haven't executed an upgrade proposal, so nothing yet.
    assert_eq!(suite.check_upgrade().unwrap(), None);

    let proposal = suite
        .propose_upgrade("member", "v2", 333, "detailed info")
        .unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    // There should now be a planned upgrade on the chain.
    assert_eq!(
        suite.check_upgrade().unwrap(),
        Some(UpgradePlan::new("v2", 333, "detailed info"))
    );
}

#[test]
fn cancel_upgrade() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member("member", 1)
        .with_voting_rules(rules)
        .build();

    let proposal = suite
        .propose_upgrade("member", "v2", 333, "detailed info")
        .unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    // There should now be a planned upgrade on the chain.
    assert_eq!(
        suite.check_upgrade().unwrap(),
        Some(UpgradePlan::new("v2", 333, "detailed info"))
    );

    let proposal = suite.propose_cancel_upgrade("member").unwrap();
    let proposal_id = get_proposal_id(&proposal).unwrap();
    suite.execute("member", proposal_id).unwrap();

    // We canceled the upgrade, so there should be no upgrade planned on the chain.
    assert_eq!(suite.check_upgrade().unwrap(), None);
}
