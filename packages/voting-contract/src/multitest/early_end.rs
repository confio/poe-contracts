use cosmwasm_std::Decimal;
use tg3::{Status, Vote};

use crate::multitest::suite::{get_proposal_id, SuiteBuilder};
use crate::state::RulesBuilder;
use crate::ContractError;

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
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Carol votes and passes proposal
    suite.vote("carol", proposal_id, Vote::Yes).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn passed_on_expiration_can_be_executed() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .with_quorum(Decimal::percent(20))
        .with_allow_early(true)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 4)
        .with_member("bob", 6)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 4 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.status, Status::Open);

    // Proposal cannot be executed yet
    let err = suite.execute_proposal("anybody", proposal_id).unwrap_err();
    assert_eq!(
        ContractError::WrongExecuteStatus {},
        err.downcast().unwrap()
    );

    // Proposal can be executed once expired
    suite.app.advance_seconds(rules.voting_period_secs());
    suite.execute_proposal("anybody", proposal_id).unwrap();

    // Proposal cannot be executed again
    let err = suite.execute_proposal("anybody", proposal_id).unwrap_err();
    assert_eq!(
        ContractError::WrongExecuteStatus {},
        err.downcast().unwrap()
    );
}

#[test]
fn abstaining_can_cause_early_pass() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(50))
        .with_allow_early(true)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 2 voting power
    let response = suite.propose("bob", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.status, Status::Open);

    // Carol abstains. It's no longer possible to reject the proposal, so it passes immediately
    suite.vote("carol", proposal_id, Vote::Abstain).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 5);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn no_can_cause_early_pass() {
    // The way a NO vote can cause an early pass is if there's still a majority of YES votes,
    // but the NO vote caused the quorum to be met.
    //
    // This kind of early pass is only possible if the quorum requirement is higher than
    // the threshold.

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(70))
        .with_allow_early(true)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 2)
        .with_member("bob", 2)
        .with_member("carol", 6)
        .with_rules(rules)
        .build();

    // Create proposal with 3 voting power
    let response = suite.propose("carol", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.status, Status::Open);

    // Alice votes no. Quorum has been met and this proposal passes.
    suite.vote("alice", proposal_id, Vote::No).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
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
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Carol votes. He'd trigger an early end here, but early end is turned off
    suite.vote("carol", proposal_id, Vote::Yes).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Open);
}
