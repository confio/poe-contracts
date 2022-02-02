use cosmwasm_std::Decimal;

use crate::multitest::suite::{get_proposal_id, SuiteBuilder};
use crate::state::RulesBuilder;
use crate::ContractError;

#[test]
fn expired_proposals_can_be_closed() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(35))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "cool proposal", "cool").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Move time forward so proposal expires
    suite.app.advance_seconds(rules.voting_period_secs());

    // Expired proposals can be closed
    let response = suite.close("anybody", proposal_id).unwrap();
    assert_eq!(
        response.custom_attrs(1),
        [
            ("action", "close"),
            ("sender", "anybody"),
            ("proposal_id", proposal_id.to_string().as_str()),
        ],
    );
}

#[test]
fn active_proposals_cannot_be_closed() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(35))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "cool proposal", "cool").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Non-expired proposals cannot be closed
    let err = suite.close("anybody", proposal_id).unwrap_err();
    assert_eq!(ContractError::NotExpired {}, err.downcast().unwrap());
}

#[test]
fn passed_proposals_cannot_be_closed() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules)
        .build();

    // Create proposal with 2 voting power - should pass immediately
    let response = suite.propose("bob", "cool proposal", "cool").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Passed proposals cannot be closed
    let err = suite.close("anybody", proposal_id).unwrap_err();
    assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());
}

#[test]
fn expired_proposals_cannot_be_closed_twice() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(60))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "cool proposal", "cool").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Move time forward so proposal expires
    suite.app.advance_seconds(rules.voting_period_secs());

    // Expired proposal can be closed...
    suite.close("anybody", proposal_id).unwrap();
    // ...but a closed one can't be closed again
    let err = suite.close("anybody", proposal_id).unwrap_err();
    assert_eq!(ContractError::NotOpen {}, err.downcast().unwrap());
}
