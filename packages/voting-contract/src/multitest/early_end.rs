use cosmwasm_std::Decimal;
use cw3::{Status, Vote};

use crate::multitest::suite::{get_proposal_id, SuiteBuilder};
use crate::state::RulesBuilder;

#[test]
fn yes_vote_can_pass_proposal_early() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_allow_early(true)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Carol votes and passes proposal
    suite.vote("carol", proposal_id, Vote::Yes).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
#[ignore]
fn abstaining_can_cause_early_pass() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(20))
        .with_allow_early(true)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 2 voting power
    let response = suite.propose("bob", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.status, Status::Open);

    // Carol abstains. It's no longer possible to reject the proposal, so it passes immediately
    suite.vote("carol", proposal_id, Vote::Abstain).unwrap();

    assert_eq!(prop.votes.total(), 2);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn early_end_can_be_disabled() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_allow_early(false)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Carol votes. He'd trigger an early end here, but early end is turned off
    suite.vote("carol", proposal_id, Vote::Yes).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Open);
}
