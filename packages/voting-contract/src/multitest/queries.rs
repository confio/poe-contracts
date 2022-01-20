use cosmwasm_std::Decimal;
use cw3::{Status, Vote, VoteInfo};
use tg_utils::Expiration;

use super::contracts::voting::Proposal;
use crate::multitest::suite::{get_proposal_id, SuiteBuilder};
use crate::state::{ProposalResponse, RulesBuilder, Votes};

#[test]
fn query_rules() {
    let rules = RulesBuilder::new().build();
    let suite = SuiteBuilder::new().with_rules(rules.clone()).build();

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
        .modify_members(owner.as_str(), &[("alice", 7), ("charlie", 1)], &["eve"])
        .unwrap();

    suite.assert_voters(&[("alice", 7), ("bob", 3), ("charlie", 1)]);
}

#[test]
fn query_proposal() {
    let rules = RulesBuilder::new().with_allow_early(false).build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_member("dave", 4)
        .with_rules(rules.clone())
        .build();

    let res = suite
        .propose_detailed(
            "alice",
            "best proposal",
            "it's just the best",
            Proposal::DoTheThing {},
        )
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
            proposal: Proposal::DoTheThing {},
            status: Status::Open,
            expires: expected_expiration,
            rules: rules.clone(),
            total_weight: 10,
            votes: Votes {
                yes: 1,
                no: 0,
                abstain: 0,
                veto: 0
            },
        }
    );

    suite.vote("bob", 1, Vote::No).unwrap();
    suite.vote("carol", 1, Vote::Abstain).unwrap();
    suite.vote("dave", 1, Vote::Veto).unwrap();

    let proposal = suite.query_proposal(id).unwrap();
    assert_eq!(
        proposal,
        ProposalResponse {
            id: 1,
            title: "best proposal".to_string(),
            description: "it's just the best".to_string(),
            proposal: Proposal::DoTheThing {},
            status: Status::Open,
            expires: expected_expiration,
            rules: rules.clone(),
            total_weight: 10,
            votes: Votes {
                yes: 1,
                no: 2,
                abstain: 3,
                veto: 4
            },
        }
    );

    suite.app.advance_seconds(rules.voting_period_secs());
    let proposal = suite.query_proposal(id).unwrap();
    assert_eq!(
        proposal,
        ProposalResponse {
            id: 1,
            title: "best proposal".to_string(),
            description: "it's just the best".to_string(),
            proposal: Proposal::DoTheThing {},
            status: Status::Rejected,
            expires: expected_expiration,
            rules,
            total_weight: 10,
            votes: Votes {
                yes: 1,
                no: 2,
                abstain: 3,
                veto: 4
            },
        }
    );
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

#[test]
fn list_proposals() {
    let mut suite = SuiteBuilder::new().with_member("alice", 1).build();

    fn titles(props: Vec<ProposalResponse<Proposal>>) -> Vec<String> {
        props.into_iter().map(|p| p.title).collect()
    }

    suite.propose("alice", "1").unwrap();
    suite.propose("alice", "2").unwrap();
    suite.propose("alice", "3").unwrap();
    suite.propose("alice", "4").unwrap();
    suite.propose("alice", "5").unwrap();

    assert_eq!(
        titles(suite.list_proposals(None, 10).unwrap()),
        ["1", "2", "3", "4", "5"]
    );
    assert_eq!(titles(suite.list_proposals(None, 1).unwrap()), ["1"]);
    assert_eq!(
        titles(suite.list_proposals(None, 3).unwrap()),
        ["1", "2", "3"]
    );
    assert_eq!(titles(suite.list_proposals(1, 3).unwrap()), ["2", "3", "4"]);
    assert_eq!(titles(suite.list_proposals(3, 3).unwrap()), ["4", "5"]);
}

#[test]
fn reverse_proposals() {
    let mut suite = SuiteBuilder::new().with_member("alice", 1).build();

    fn titles(props: Vec<ProposalResponse<Proposal>>) -> Vec<String> {
        props.into_iter().map(|p| p.title).collect()
    }

    suite.propose("alice", "1").unwrap();
    suite.propose("alice", "2").unwrap();
    suite.propose("alice", "3").unwrap();
    suite.propose("alice", "4").unwrap();
    suite.propose("alice", "5").unwrap();

    assert_eq!(
        titles(suite.reverse_proposals(None, 10).unwrap()),
        ["5", "4", "3", "2", "1"]
    );
    assert_eq!(titles(suite.reverse_proposals(None, 1).unwrap()), ["5"]);
    assert_eq!(
        titles(suite.reverse_proposals(None, 3).unwrap()),
        ["5", "4", "3"]
    );
    assert_eq!(
        titles(suite.reverse_proposals(5, 3).unwrap()),
        ["4", "3", "2"]
    );
    assert_eq!(titles(suite.reverse_proposals(3, 3).unwrap()), ["2", "1"]);
}

#[test]
fn list_votes() {
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

    let votes = suite.list_votes(proposal_id, None, None).unwrap();
    assert_eq!(
        votes,
        [
            VoteInfo {
                voter: "alice".to_string(),
                vote: Vote::Yes,
                weight: 1
            },
            VoteInfo {
                voter: "bob".to_string(),
                vote: Vote::No,
                weight: 2
            }
        ]
    )
}

#[test]
fn list_votes_pagination() {
    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(51))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_member("alice", 1)
        .with_member("bob", 2)
        .with_member("carol", 3)
        .with_member("dave", 4)
        .with_rules(rules)
        .build();

    // Create proposal with 1 voting power
    let response = suite.propose("alice", "proposal").unwrap();
    let proposal_id: u64 = get_proposal_id(&response).unwrap();

    suite.vote("bob", proposal_id, Vote::No).unwrap();
    suite.vote("carol", proposal_id, Vote::Abstain).unwrap();
    suite.vote("dave", proposal_id, Vote::Veto).unwrap();

    let votes = suite.list_votes(proposal_id, None, 3).unwrap();
    assert_eq!(
        votes,
        [
            VoteInfo {
                voter: "alice".to_string(),
                vote: Vote::Yes,
                weight: 1
            },
            VoteInfo {
                voter: "bob".to_string(),
                vote: Vote::No,
                weight: 2
            },
            VoteInfo {
                voter: "carol".to_string(),
                vote: Vote::Abstain,
                weight: 3
            }
        ]
    );

    let votes = suite.list_votes(proposal_id, "bob".to_string(), 3).unwrap();
    assert_eq!(
        votes,
        [
            VoteInfo {
                voter: "carol".to_string(),
                vote: Vote::Abstain,
                weight: 3
            },
            VoteInfo {
                voter: "dave".to_string(),
                vote: Vote::Veto,
                weight: 4
            },
        ]
    );
}

#[test]
fn voter() {
    let suite = SuiteBuilder::new().with_member("alice", 1).build();
    assert_eq!(suite.query_voter("alice").unwrap().weight, Some(1));
    assert_eq!(suite.query_voter("bob").unwrap().weight, None);
}

#[test]
fn group_contract() {
    let suite = SuiteBuilder::new().build();
    assert_eq!(suite.group, suite.query_group_contract().unwrap())
}

#[test]
fn list_text_proposals() {
    let mut suite = SuiteBuilder::new().with_member("alice", 1).build();

    fn titles(props: Vec<ProposalResponse<Proposal>>) -> Vec<String> {
        props.into_iter().map(|p| p.title).collect()
    }

    suite
        .propose_and_execute("alice", "1", "1", Proposal::Text {})
        .unwrap();
    suite
        .propose_and_execute("alice", "2", "2", Proposal::DoTheThing {})
        .unwrap();
    suite
        .propose_and_execute("alice", "3", "3", Proposal::Text {})
        .unwrap();
    suite
        .propose_and_execute("alice", "4", "4", Proposal::DoTheThing {})
        .unwrap();
    suite
        .propose_and_execute("alice", "5", "5", Proposal::Text {})
        .unwrap();
    suite
        .propose_and_execute("alice", "6", "6", Proposal::Text {})
        .unwrap();

    assert_eq!(
        titles(suite.list_text_proposals(None, 10).unwrap()),
        ["1", "3", "5", "6"]
    );
    assert_eq!(titles(suite.list_text_proposals(None, 1).unwrap()), ["1"]);
    assert_eq!(
        titles(suite.list_text_proposals(None, 3).unwrap()),
        ["1", "3", "5"]
    );
    assert_eq!(titles(suite.list_text_proposals(1, 2).unwrap()), ["3", "5"]);
    assert_eq!(titles(suite.list_text_proposals(3, 2).unwrap()), ["5", "6"]);
}
