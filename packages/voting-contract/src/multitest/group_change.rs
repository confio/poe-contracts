use super::suite::{get_proposal_id, SuiteBuilder};
use cosmwasm_std::{Decimal, StdError};
use tg3::{Status, Vote};

use crate::{state::RulesBuilder, ContractError};

#[test]
fn group_change_does_not_affect_old_proposals() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("eve", 5)
        .with_rules(rules)
        .build();

    let proposal_creation_height = suite.app.block_info().height;

    let owner = suite.owner.clone();
    let response = suite
        .propose("alice", "great proposal", "proposal")
        .unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    suite
        .modify_members(owner.as_str(), &[("bob", 10), ("newdude", 2)], &["eve"])
        .unwrap();

    // Proposal is still open
    let proposal_status = suite.query_proposal(proposal_id).unwrap().status;
    assert_eq!(proposal_status, Status::Open);

    // Bob can only vote on the first proposal with 2 points
    // (not enough to pass - alice + bob constitute 3/6 voting power at proposal creation time)
    suite.vote("bob", proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal(proposal_id).unwrap().status;
    assert_eq!(proposal_status, Status::Open);

    // newdude can't vote - wasn't a member at proposal creation time
    let err = suite.vote("newdude", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: format!(
                "Unauthorized: wasn't member of a group at block height: {}",
                proposal_creation_height
            )
        }),
        err.downcast().unwrap()
    );

    // Recently removed Eve can still vote on the old proposal and pass it
    suite.vote("eve", proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal(proposal_id).unwrap().status;
    assert_eq!(proposal_status, Status::Passed);
}

#[test]
fn new_proposals_follow_updated_membership() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 5)
        .with_member("bob", 2)
        .with_member("eve", 2)
        .with_rules(rules)
        .build();

    let owner = suite.owner.clone();

    suite
        .modify_members(
            owner.as_str(),
            &[("bob", 10), ("charlie", 8), ("newdude", 2)],
            &["eve"],
        )
        .unwrap();

    suite.app.advance_blocks(1);

    let proposal_creation_height = suite.app.block_info().height;

    let response = suite
        .propose("alice", "great proposal", "proposal")
        .unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    // Proposal is still open - alice's 5 voting power is no longer the majority, it's only 5/25
    let proposal_status = suite.query_proposal(proposal_id).unwrap().status;
    assert_eq!(proposal_status, Status::Open);

    // newdude can vote
    suite.vote("newdude", proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal(proposal_id).unwrap().status;
    assert_eq!(proposal_status, Status::Open);

    // eve was kicked out - can't vote here
    let err = suite.vote("eve", proposal_id, Vote::Yes).unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::GenericErr {
            msg: format!(
                "Unauthorized: wasn't member of a group at block height: {}",
                proposal_creation_height
            )
        }),
        err.downcast().unwrap()
    );

    // Bob can push the proposal to passed status with his new points of 10
    suite.vote("bob", proposal_id, Vote::Yes).unwrap();
    let proposal_status = suite.query_proposal(proposal_id).unwrap().status;
    assert_eq!(proposal_status, Status::Passed);
}
