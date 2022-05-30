use std::convert::TryInto;

use cosmwasm_std::Binary;
use tg_bindings::{Ed25519Pubkey, ToAddress, ValidatorVote};

use crate::multitest::helpers::assert_active_validators;

use super::{
    helpers::{addr_to_pubkey, members_init},
    suite::SuiteBuilder,
};

fn addr_to_vote_addr(addr: &str) -> Binary {
    let pubkey = addr_to_pubkey(addr);
    let pubkey: Ed25519Pubkey = pubkey.try_into().unwrap();
    Binary(pubkey.to_address().to_vec())
}

// Unreadable tests ahead! This deserves a refactor.

#[test]
fn verify_validators_works() {
    let members = vec![
        "member1member1member1member1memb",
        "member2member2member2member2memb",
    ];

    let mut suite = SuiteBuilder::new()
        .with_operators(&members)
        .with_engagement(&members_init(&members, &[2, 3]))
        .with_min_points(1)
        .with_verify_validators(600)
        .build();

    suite
        .set_votes(&[
            ValidatorVote {
                address: addr_to_vote_addr(members[0]),
                power: 2,
                voted: true,
            },
            ValidatorVote {
                address: addr_to_vote_addr(members[1]),
                power: 3,
                voted: true,
            },
        ])
        .unwrap();

    let info1 = suite.validator(members[0]).unwrap().validator.unwrap();
    let info2 = suite.validator(members[1]).unwrap().validator.unwrap();
    assert!(info1.jailed_until.is_none());
    assert!(info2.jailed_until.is_none());
    assert!(info1.active_validator);
    assert!(info2.active_validator);
    // Validators have min power before they're verified
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 1), (members[1], 1)],
    );

    suite.advance_epoch().unwrap();

    let info1 = suite.validator(members[0]).unwrap().validator.unwrap();
    let info2 = suite.validator(members[1]).unwrap().validator.unwrap();
    assert!(info1.jailed_until.is_none());
    assert!(info2.jailed_until.is_none());
    assert!(info1.active_validator);
    assert!(info2.active_validator);
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2), (members[1], 3)],
    );
}

#[test]
fn verify_validators_jailing() {
    let members = vec![
        "member1member1member1member1memb",
        "member2member2member2member2memb",
    ];

    let mut suite = SuiteBuilder::new()
        .with_operators(&members)
        .with_engagement(&members_init(&members, &[2, 3]))
        .with_verify_validators(600)
        .build();

    suite
        .set_votes(&[ValidatorVote {
            address: addr_to_vote_addr(members[0]),
            power: 2,
            voted: true,
        }])
        .unwrap();

    let info1 = suite.validator(members[0]).unwrap().validator.unwrap();
    let info2 = suite.validator(members[1]).unwrap().validator.unwrap();
    assert!(info1.jailed_until.is_none());
    assert!(info2.jailed_until.is_none());

    suite.advance_epoch().unwrap();

    let info1 = suite.validator(members[0]).unwrap().validator.unwrap();
    let info2 = suite.validator(members[1]).unwrap().validator.unwrap();
    assert!(info1.jailed_until.is_none());
    assert!(info2.jailed_until.is_some());
    assert!(!info2.active_validator);
}

#[test]
fn validator_needs_to_verify_if_unjailed() {
    let members = vec![
        "member1member1member1member1memb",
        "member2member2member2member2memb",
    ];

    let mut suite = SuiteBuilder::new()
        .with_operators(&members)
        .with_engagement(&members_init(&members, &[2, 3]))
        .with_min_points(2)
        .with_verify_validators(600)
        .with_epoch_length(600)
        .build();

    suite
        .set_votes(&[ValidatorVote {
            address: addr_to_vote_addr(members[0]),
            power: 2,
            voted: true,
        }])
        .unwrap();

    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_none());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2), (members[1], 2)],
    );

    suite.advance_epoch().unwrap();

    // Validator 2 failed verification, so is jailed
    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_some());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2)],
    );

    // An epoch passes and the validator gets to unjail themself
    suite.advance_epoch().unwrap();
    suite.unjail(members[1], members[1]).unwrap();

    // Another epoch passes before the validator is added to the valset again
    suite.advance_epoch().unwrap();
    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_none());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2), (members[1], 2)],
    );

    // Validator should be PENDING after being re-added to the valset,
    // so if they fail to sign a block to prove they're online, they get
    // jailed -again-
    suite.advance_epoch().unwrap();
    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_some());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2)],
    );
}

#[test]
fn validator_needs_to_verify_if_unjailed_by_auto_unjail() {
    let members = vec![
        "member1member1member1member1memb",
        "member2member2member2member2memb",
    ];

    let mut suite = SuiteBuilder::new()
        .with_operators(&members)
        .with_engagement(&members_init(&members, &[2, 3]))
        .with_min_points(2)
        .with_auto_unjail()
        .with_verify_validators(600)
        .with_epoch_length(600)
        .build();

    suite
        .set_votes(&[ValidatorVote {
            address: addr_to_vote_addr(members[0]),
            power: 2,
            voted: true,
        }])
        .unwrap();

    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_none());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2), (members[1], 2)],
    );

    suite.advance_epoch().unwrap();

    // Validator 2 failed verification, so is jailed
    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_some());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2)],
    );

    // An epoch passes, and the validator gets auto-unjailed
    suite.advance_epoch().unwrap();

    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_none());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2), (members[1], 2)],
    );

    // Validator should be PENDING after being re-added to the valset,
    // so if they fail to sign a block to prove they're online, they get
    // jailed -again-
    suite.advance_epoch().unwrap();
    assert!(suite
        .validator(members[1])
        .unwrap()
        .validator
        .unwrap()
        .jailed_until
        .is_some());
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[(members[0], 2)],
    );
}
