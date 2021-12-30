use crate::error::ContractError;
use crate::msg::{EpochResponse, ValidatorMetadata};
use crate::state::Config;

use super::helpers::{assert_active_validators, assert_operators, members_init};
use super::suite::SuiteBuilder;
use assert_matches::assert_matches;
use cosmwasm_std::{coin, Decimal};

#[test]
fn initialization() {
    let members = vec!["member1", "member2", "member3", "member4"];

    let suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .with_epoch_reward(coin(100, "eth"))
        .with_max_validators(10)
        .with_min_weight(5)
        .with_epoch_length(3600)
        .build();

    let config = suite.config().unwrap();
    assert_eq!(
        config,
        Config {
            // This one it is basically assumed is set correctly. Other tests tests if behavior
            // of relation between those contract is correct
            membership: config.membership.clone(),
            min_weight: 5,
            max_validators: 10,
            epoch_reward: coin(100, "eth"),
            scaling: None,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            double_sign_slash_ratio: Decimal::percent(50),
            distribution_contracts: vec![],
            // This one it is basically assumed is set correctly. Other tests tests if behavior
            // of relation between those contract is correct
            rewards_contract: config.rewards_contract.clone(),
        }
    );

    assert_matches!(
        suite.epoch().unwrap(),
        EpochResponse {
            epoch_length,
            last_update_time,
            last_update_height,
            next_update_time,
            ..
        } if
            epoch_length == 3600 &&
            last_update_time == 0 &&
            last_update_height == 0 &&
            (suite.timestamp().seconds()..=suite.timestamp().seconds()+3600)
                .contains(&next_update_time)
    );

    // Validators should be set on genesis processing block
    assert_active_validators(
        &suite.list_active_validators().unwrap(),
        &[(members[2], 5), (members[3], 8)],
    );

    for member in &members {
        assert_eq!(
            suite.validator(member).unwrap().validator.unwrap().operator,
            *member
        );
    }
}

#[test]
fn simulate_validators() {
    let members = vec![
        "member1", "member2", "member3", "member4", "member5", "member6",
    ];

    let suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8, 13, 21]))
        .with_operators(&members)
        .with_max_validators(2)
        .with_min_weight(5)
        .build();

    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[4], 13), (members[5], 21)],
    );

    assert_active_validators(
        &suite.list_active_validators().unwrap(),
        &[(members[4], 13), (members[5], 21)],
    );
}

#[test]
fn update_metadata() {
    let members = vec!["member1"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2]))
        .with_operators(&members)
        .build();

    let meta = ValidatorMetadata {
        moniker: "funny boy".to_owned(),
        identity: Some("Secret identity".to_owned()),
        website: Some("https://www.funny.boy.rs".to_owned()),
        security_contact: Some("funny@boy.rs".to_owned()),
        details: Some("Comedian".to_owned()),
    };

    suite.update_metadata(members[0], &meta).unwrap();

    let resp = suite.validator(members[0]).unwrap();
    assert_eq!(resp.validator.unwrap().metadata, meta);

    let invalid_meta = ValidatorMetadata {
        moniker: "".to_owned(),
        identity: Some("Magic identity".to_owned()),
        website: Some("https://www.empty.one.rs".to_owned()),
        security_contact: Some("empty@one.rs".to_owned()),
        details: Some("Ghost".to_owned()),
    };

    // Update with invalid meta (empty moniker) fails
    let resp = suite
        .update_metadata(members[0], &invalid_meta)
        .unwrap_err();
    assert_eq!(ContractError::InvalidMoniker {}, resp.downcast().unwrap());

    // Ensure no metadata changed
    let resp = suite.validator(members[0]).unwrap();
    assert_eq!(resp.validator.unwrap().metadata, meta);

    // Update with valid meta on non-member always fail
    let resp = suite.update_metadata("invalid", &meta).unwrap_err();
    assert_eq!(
        ContractError::Unauthorized("No operator info found".to_owned()),
        resp.downcast().unwrap()
    );
}

#[test]
fn list_validators() {
    let members = vec!["member1", "member2", "member3", "member4"];

    let suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8, 13, 21]))
        .with_operators(&members)
        .with_min_weight(5)
        .build();

    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0], None),
            (members[1], None),
            (members[2], None),
            (members[3], None),
        ],
    );
}

#[test]
fn list_validators_paginated() {
    let members = vec!["member1", "member2", "member3", "member4", "member5"];

    let suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8, 13, 21]))
        .with_operators(&members)
        .with_min_weight(5)
        .build();

    let page1 = suite.list_validators(None, 2).unwrap();
    assert_eq!(
        page1.len(),
        2,
        "Invalid page length, 2 expected, got page: {:?}",
        page1
    );
    let page2 = suite
        .list_validators(Some(page1.last().unwrap().operator.clone()), 2)
        .unwrap();
    assert_eq!(
        page2.len(),
        2,
        "Invalid page length, 2 expected, got page: {:?}",
        page2
    );
    let page3 = suite
        .list_validators(Some(page2.last().unwrap().operator.clone()), 2)
        .unwrap();
    assert_eq!(
        page3.len(),
        1,
        "Invalid page length, 1 expected, got page: {:?}",
        page3
    );
    let page4 = suite
        .list_validators(Some(page3.last().unwrap().operator.clone()), 2)
        .unwrap();
    assert_eq!(page4, vec![]);

    assert_operators(
        &[page1, page2, page3, page4].concat(),
        &[
            (members[0], None),
            (members[1], None),
            (members[2], None),
            (members[3], None),
            (members[4], None),
        ],
    );
}
