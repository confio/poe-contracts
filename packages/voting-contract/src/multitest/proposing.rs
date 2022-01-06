use cosmwasm_std::StdError;

use crate::{multitest::suite::SuiteBuilder, ContractError};

#[test]
fn member_with_no_voting_power_cannot_propose() {
    let mut suite = SuiteBuilder::new()
        .with_member("alice", 0)
        .with_member("bob", 3)
        .build();

    let err = suite
        .propose("alice", "thing", "best proposal", "do the thing")
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
        .propose("bob", "thing", "best proposal", "do the thing")
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
        .propose("alice", "thing", "best proposal", "do the thing")
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
        .propose("bob", "thing", "best proposal", "do the thing")
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
