use cosmwasm_std::StdError;
use tg3::Status;
use tg_utils::Expiration;

use crate::multitest::contracts::voting::Proposal;
use crate::multitest::suite::{get_proposal_id, SuiteBuilder};
use crate::state::{ProposalResponse, RulesBuilder, Votes};
use crate::ContractError;

#[test]
fn proposal_creation() {
    let rules = RulesBuilder::new().build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 3)
        .with_rules(rules.clone())
        .build();

    let res = suite
        .propose("alice", "best proposal", "it's just the best")
        .unwrap();

    let id = get_proposal_id(&res).unwrap();
    let proposal = suite.query_proposal(id).unwrap();
    let expected_expiration = Expiration::at_timestamp(
        suite
            .app
            .block_info()
            .time
            .plus_seconds(rules.voting_period_secs()),
    );
    assert_eq!(
        proposal,
        ProposalResponse {
            id: 1,
            title: "best proposal".to_string(),
            description: "it's just the best".to_string(),
            created_by: "alice".to_owned(),
            proposal: Proposal::Text {},
            status: Status::Open,
            expires: expected_expiration,
            rules,
            total_points: 4,
            votes: Votes::yes(1),
        }
    )
}

#[test]
fn member_with_no_voting_power_cannot_propose() {
    let mut suite = SuiteBuilder::new()
        .with_member("alice", 0)
        .with_member("bob", 3)
        .build();

    let err = suite
        .propose("alice", "do the thing", "do the thing")
        .unwrap_err();

    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: "Unauthorized: member doesn't have voting power".to_string()
        }),
        err.downcast().unwrap()
    );
}

#[test]
fn proposal_from_non_voter_is_rejected() {
    let mut suite = SuiteBuilder::new().with_member("alice", 1).build();

    let err = suite
        .propose("bob", "do the thing", "do the thing")
        .unwrap_err();

    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: "Unauthorized: not member of a group".to_string()
        }),
        err.downcast().unwrap()
    );
}

#[test]
fn proposal_from_voter_is_accepted() {
    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 3)
        .build();

    let res = suite
        .propose("alice", "do the thing", "do the thing")
        .unwrap();

    assert_eq!(
        res.custom_attrs(1),
        [
            ("action", "propose"),
            ("sender", "alice"),
            ("proposal_id", "1"),
            ("status", "Open"),
        ],
    );
}

#[test]
fn proposal_from_voter_can_directly_pass() {
    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 3)
        .build();

    let res = suite
        .propose("bob", "do the thing", "do the thing")
        .unwrap();

    assert_eq!(
        res.custom_attrs(1),
        [
            ("action", "propose"),
            ("sender", "bob"),
            ("proposal_id", "1"),
            ("status", "Passed"),
        ],
    );
}
