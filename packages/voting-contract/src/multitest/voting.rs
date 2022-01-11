use cosmwasm_std::{Decimal, StdError};
use cw3::{Status, Vote, VoteInfo};

use crate::multitest::suite::{get_proposal_id, SuiteBuilder};
use crate::state::{RulesBuilder, Votes};
use crate::ContractError;

#[test]
fn proposal_creator_votes_automatically() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("charlie", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes, Votes::yes(1));
}

#[test]
fn simple_vote() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("charlie", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Bob votes
    suite.vote("bob", proposal_id, Vote::Yes).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 3);
}

#[test]
fn proposal_creator_cannot_vote() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Creator cannot vote (again)
    let err = suite.vote("alice", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(ContractError::AlreadyVoted {}, err.downcast().unwrap());
}

#[test]
fn cannot_double_vote() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("charlie", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Bob votes
    suite.vote("bob", proposal_id, Vote::Yes).unwrap();

    // Bob cannot vote again
    let err = suite.vote("bob", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(ContractError::AlreadyVoted {}, err.downcast().unwrap());
}

#[test]
fn non_voters_cannot_vote() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules)
        .build();

    let proposal_creation_height = suite.app.block_info().height;

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // A random tries to vote
    let err = suite.vote("some guy", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: format!(
                "Unauthorized: wasn't member of a group at block height: {}",
                proposal_creation_height
            )
        }),
        err.downcast().unwrap()
    );
}

#[test]
fn members_with_no_voting_power_cannot_vote() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("charlie", 0)
        .with_rules(rules)
        .build();

    let proposal_creation_height = suite.app.block_info().height;

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // A random tries to vote
    let err = suite.vote("charlie", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: format!(
                "Unauthorized: member didn't have voting power at block height: {}",
                proposal_creation_height
            )
        }),
        err.downcast().unwrap()
    );
}

#[test]
fn yes_vote_can_pass_proposal() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("charlie", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Charlie votes and passes proposal
    suite.vote("charlie", proposal_id, Vote::Yes).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn expired_proposals_cannot_be_voted_on() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "cool proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Move time forward so proposal expires
    suite.app.advance_seconds(rules.voting_period_secs());

    // Bob can't vote on the expired proposal
    let err = suite.vote("bob", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(ContractError::Expired {}, err.downcast().unwrap());
}

#[test]
fn query_individual_votes() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
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

    suite.vote("bob", proposal_id, Vote::No).unwrap();

    // Creator of proposal
    let vote = suite.query_vote_info(proposal_id, "alice").unwrap();
    assert_eq!(
        vote,
        Some(VoteInfo {
            voter: "alice".to_string(),
            vote: Vote::Yes,
            weight: 1
        })
    );

    // First no vote
    let vote = suite.query_vote_info(proposal_id, "bob").unwrap();
    assert_eq!(
        vote,
        Some(VoteInfo {
            voter: "bob".to_owned(),
            vote: Vote::No,
            weight: 2
        })
    );

    // Non-voter
    let vote = suite.query_vote_info(proposal_id, "carol").unwrap();
    assert!(vote.is_none());
}
