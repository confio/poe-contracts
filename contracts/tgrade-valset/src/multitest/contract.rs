use crate::error::ContractError;
use crate::msg::{
    EpochResponse, ValidatorMetadata, MAX_METADATA_SIZE, MIN_METADATA_SIZE, MIN_MONIKER_LENGTH,
};
use crate::state::Config;

use super::helpers::{addr_to_pubkey, assert_active_validators, assert_operators, members_init};
use super::suite::SuiteBuilder;
use assert_matches::assert_matches;
use cosmwasm_std::{coin, Decimal};
use tg_utils::Duration;

#[test]
fn initialization() {
    let members = vec!["member1", "member2", "member3", "member4"];

    let suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .with_epoch_reward(coin(100, "eth"))
        .with_max_validators(10)
        .with_min_points(5)
        .with_epoch_length(3600)
        .build();

    let config = suite.config().unwrap();
    assert_eq!(
        config,
        Config {
            // This one it is basically assumed is set correctly. Other tests tests if behavior
            // of relation between those contract is correct
            membership: config.membership.clone(),
            min_points: 5,
            max_validators: 10,
            epoch_reward: coin(100, "eth"),
            scaling: None,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            double_sign_slash_ratio: Decimal::percent(50),
            distribution_contracts: vec![],
            // This one it is basically assumed is set correctly. Other tests tests if behavior
            // of relation between those contract is correct
            validator_group: config.validator_group.clone(),
            verify_validators: false,
            offline_jail_duration: Duration::new(0),
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
        &suite.list_active_validators(None, None).unwrap(),
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
fn validators_query_pagination() {
    let members = vec!["member1", "member2", "member3", "member4", "member5"];

    let suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8, 4]))
        .with_operators(&members)
        .with_epoch_reward(coin(100, "eth"))
        .with_max_validators(10)
        .with_min_points(2)
        .with_epoch_length(3600)
        .build();

    // Query without pagination
    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
        &[
            (members[0], 2),
            (members[1], 3),
            (members[2], 5),
            (members[3], 8),
            (members[4], 4),
        ],
    );

    // List only first 3
    let page = suite.list_active_validators(None, 3).unwrap();
    assert_active_validators(&page, &[(members[2], 5), (members[3], 8), (members[4], 4)]);

    // List 2 entries after 2rd validator
    assert_active_validators(
        &suite
            .list_active_validators(page.last().unwrap().operator.to_string(), 2)
            .unwrap(),
        &[(members[0], 2), (members[1], 3)],
    );

    // Starting at unknown validator will return empty query result
    assert_active_validators(
        &suite
            .list_active_validators("unknown_member".to_owned(), None)
            .unwrap(),
        &[],
    );
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
        .with_min_points(5)
        .build();

    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[4], 13), (members[5], 21)],
    );

    assert_active_validators(
        &suite.list_active_validators(None, None).unwrap(),
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
    assert_eq!(
        ContractError::InvalidMetadata {
            data: "moniker",
            min: MIN_MONIKER_LENGTH,
            max: MAX_METADATA_SIZE,
        },
        resp.downcast().unwrap()
    );

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
        .with_min_points(5)
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
        .with_min_points(5)
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

#[test]
fn register_key_invalid_metadata() {
    let members = vec!["member1"];

    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8, 13, 21]))
        .with_operators(&members)
        .with_min_points(5)
        .build();

    let meta = ValidatorMetadata {
        moniker: "example".to_owned(),
        identity: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        website: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        security_contact: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        details: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
    };
    let pubkey = addr_to_pubkey(members[0]);
    let resp = suite
        .register_validator_key(members[0], pubkey.clone(), meta.clone())
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidMetadata {
            data: "identity",
            min: MIN_METADATA_SIZE,
            max: MAX_METADATA_SIZE
        },
        resp.downcast().unwrap()
    );

    let meta = ValidatorMetadata {
        identity: Some(String::new()),
        website: Some(String::new()),
        security_contact: Some(String::new()),
        details: Some(String::new()),
        ..meta
    };
    let resp = suite
        .register_validator_key(members[0], pubkey, meta)
        .unwrap_err();
    assert_eq!(
        ContractError::InvalidMetadata {
            data: "identity",
            min: MIN_METADATA_SIZE,
            max: MAX_METADATA_SIZE
        },
        resp.downcast().unwrap()
    );
}

#[test]
fn update_metadata_invalid_metadata() {
    let members = vec!["member1"];

    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8, 13, 21]))
        .with_operators(&members)
        .with_min_points(5)
        .build();

    let meta = ValidatorMetadata {
        moniker: "example".to_owned(),
        identity: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        website: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        security_contact: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        details: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
    };
    let resp = suite.update_metadata(members[0], &meta).unwrap_err();
    assert_eq!(
        ContractError::InvalidMetadata {
            data: "identity",
            min: MIN_METADATA_SIZE,
            max: MAX_METADATA_SIZE
        },
        resp.downcast().unwrap()
    );

    let meta = ValidatorMetadata {
        identity: Some(String::new()),
        website: Some(String::new()),
        security_contact: Some(String::new()),
        details: Some(String::new()),
        ..meta
    };
    let resp = suite.update_metadata(members[0], &meta).unwrap_err();
    assert_eq!(
        ContractError::InvalidMetadata {
            data: "identity",
            min: MIN_METADATA_SIZE,
            max: MAX_METADATA_SIZE
        },
        resp.downcast().unwrap()
    );
}

mod instantiate {
    use cosmwasm_std::{coin, Addr, Decimal, Uint128};
    use cw_multi_test::{AppBuilder, BasicApp, Executor};
    use tg_bindings::{TgradeMsg, TgradeQuery};
    use tg_utils::Duration;

    use crate::error::ContractError;
    use crate::msg::{
        InstantiateMsg, OperatorInitInfo, UnvalidatedDistributionContracts, ValidatorMetadata,
        MAX_METADATA_SIZE, MIN_METADATA_SIZE,
    };
    use crate::multitest::suite::{contract_stake, contract_valset};
    use crate::test_helpers::mock_pubkey;

    #[test]
    fn instantiate_invalid_metadata() {
        let mut app: BasicApp<TgradeMsg, TgradeQuery> =
            AppBuilder::new_custom().build(|_, _, _| ());

        let stake_id = app.store_code(contract_stake());
        let admin = "steakhouse owner".to_owned();
        let msg = tg4_stake::msg::InstantiateMsg {
            denom: "james bond denom".to_owned(),
            tokens_per_point: Uint128::new(10),
            min_bond: Uint128::new(1),
            unbonding_period: 1234,
            admin: Some(admin.clone()),
            preauths_hooks: 0,
            preauths_slashing: 1,
            auto_return_limit: 0,
        };
        let stake_addr = app
            .instantiate_contract(
                stake_id,
                Addr::unchecked(admin.clone()),
                &msg,
                &[],
                "stake",
                Some(admin.clone()),
            )
            .unwrap();

        let valset_id = app.store_code(contract_valset());

        let member = OperatorInitInfo {
            operator: "example".to_owned(),
            validator_pubkey: mock_pubkey("example".as_bytes()),
            metadata: ValidatorMetadata {
                moniker: "example".into(),
                details: Some(String::new()), // <- invalid (empty) details field in metadata
                ..ValidatorMetadata::default()
            },
        };
        let msg = InstantiateMsg {
            admin: None,
            membership: stake_addr.into(),
            min_points: 1,
            max_validators: 120,
            epoch_length: 10,
            epoch_reward: coin(1, "denom"),
            initial_keys: [member].to_vec(),
            scaling: None,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            double_sign_slash_ratio: Decimal::percent(50),
            distribution_contracts: UnvalidatedDistributionContracts::default(),
            validator_group_code_id: 1,
            verify_validators: false,
            offline_jail_duration: Duration::new(0),
        };

        let err = app
            .instantiate_contract(valset_id, Addr::unchecked(admin), &msg, &[], "valset", None)
            .unwrap_err();
        assert_eq!(
            ContractError::InvalidMetadata {
                data: "details",
                min: MIN_METADATA_SIZE,
                max: MAX_METADATA_SIZE
            },
            err.downcast().unwrap()
        );
    }
}
