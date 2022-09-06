use cosmwasm_std::{Decimal, StdError};
use tg3::{Status, Vote};

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
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
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
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
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
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
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
        .with_member("carol", 3)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
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
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
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
        .with_member("carol", 0)
        .with_rules(rules)
        .build();

    let proposal_creation_height = suite.app.block_info().height;

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // A random tries to vote
    let err = suite.vote("carol", proposal_id, Vote::Yes).unwrap_err();
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
fn veto_counts_as_no() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .with_quorum(Decimal::percent(10))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Bob votes VETO - it's 33% yes votes
    suite.vote("bob", proposal_id, Vote::Veto).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 3);
    assert_eq!(prop.status, Status::Open);

    // Proposal is rejected once expired
    suite.app.advance_seconds(rules.voting_period_secs());
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 3);
    assert_eq!(prop.status, Status::Rejected);
}

#[test]
fn proposal_rejected_when_quorum_not_reached() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(40))
        .with_allow_early(false)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_member("dave", 4)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 2 voting power
    let response = suite.propose("bob", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Alice votes no. Quorum isn't reached because 3/10 voting power has been used.
    suite.vote("alice", proposal_id, Vote::No).unwrap();

    suite.app.advance_seconds(rules.voting_period_secs());
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 3);
    assert_eq!(prop.status, Status::Rejected);
}

#[test]
fn abstaining_can_help_reach_quorum() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(40))
        .with_allow_early(false)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_member("dave", 4)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 2 voting power
    let response = suite.propose("bob", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Alice votes no. Quorum isn't reached because 3/10 voting power has been used.
    suite.vote("alice", proposal_id, Vote::No).unwrap();

    // Carol abstains. Quorum is reached because 6/10 voting power has been used.
    suite.vote("carol", proposal_id, Vote::Abstain).unwrap();

    suite.app.advance_seconds(rules.voting_period_secs());
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 6);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn abstaining_does_not_count_as_yes() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .with_quorum(Decimal::percent(40))
        .with_allow_early(false)
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_member("dave", 4)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 2 voting power
    let response = suite.propose("bob", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Carol votes no.
    suite.vote("carol", proposal_id, Vote::No).unwrap();

    // Dave abstains. We have 2 yes, 3 no, 4 abstained. Proposal should be rejected.
    suite.vote("dave", proposal_id, Vote::Abstain).unwrap();

    suite.app.advance_seconds(rules.voting_period_secs());
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 9);
    assert_eq!(prop.status, Status::Rejected);
}

#[test]
fn proposal_can_be_rejected_after_voting_period() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .with_quorum(Decimal::percent(10))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Bob votes NO - it's 33% yes votes
    suite.vote("bob", proposal_id, Vote::No).unwrap();

    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 3);
    assert_eq!(prop.status, Status::Open);

    // Proposal is rejected once expired
    suite.app.advance_seconds(rules.voting_period_secs());
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 3);
    assert_eq!(prop.status, Status::Rejected);
}

#[test]
fn passing_a_proposal_after_voting_period_works() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .with_quorum(Decimal::percent(20))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 4)
        .with_member("bob", 6)
        .with_rules(rules.clone())
        .build();

    // Create proposal with 4 voting power
    let response = suite.propose("alice", "proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Proposal doesn't pass early because Bob could still reject it
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Open);

    // Proposal is passed once voting period is over
    suite.app.advance_seconds(rules.voting_period_secs());
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.votes.total(), 4);
    assert_eq!(prop.status, Status::Passed);
}

#[test]
fn expired_proposals_cannot_be_voted_on() {
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
    let response = suite.propose("alice", "cool proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Move time forward so proposal expires
    suite.app.advance_seconds(rules.voting_period_secs());

    // Bob can't vote on the expired proposal
    let err = suite.vote("bob", proposal_id, Vote::Yes).unwrap_err();
    // proposal that is open and expired is rejected
    assert_eq!(ContractError::Expired {}, err.downcast().unwrap());
}

#[test]
fn proposal_pass_on_expiration() {
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
    let response = suite.propose("alice", "cool proposal", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Bob can vote on the proposal
    suite.vote("bob", proposal_id, Vote::Yes).unwrap();

    // Move time forward so voting ends
    suite.app.advance_seconds(rules.voting_period_secs());

    // Verify proposal is now passed
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.status, Status::Passed);

    // Alice can't vote on the proposal
    let err = suite.vote("alice", proposal_id, Vote::Yes).unwrap_err();

    // cannot vote on proposal as it has expired
    assert_eq!(ContractError::Expired {}, err.downcast().unwrap());

    // But she can execute the proposal
    suite.execute_proposal("alice", proposal_id).unwrap();

    // Verify proposal is now 'executed'
    let prop = suite.query_proposal(proposal_id).unwrap();
    assert_eq!(prop.status, Status::Executed);

    // Closing should NOT be possible
    let err = suite.close("bob", proposal_id).unwrap_err();
    assert_eq!(ContractError::WrongCloseStatus {}, err.downcast().unwrap());
}
