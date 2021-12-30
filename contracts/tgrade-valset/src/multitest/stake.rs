#![cfg(test)]
use cosmwasm_std::{coin, Addr, Decimal};

use crate::multitest::suite::SuiteBuilder;
use crate::state::{Config, ValidatorInfo};
use crate::test_helpers::{addrs, valid_operator};

#[test]
fn init_and_query_state() {
    let ops_owned = addrs(24);
    let ops: Vec<_> = ops_owned.iter().map(String::as_str).collect();

    let epoch_reward = coin(50_000, "usdc");

    let suite = SuiteBuilder::new()
        .with_stake("usdc", 100u128)
        .with_operators(&ops)
        .with_min_weight(5)
        .with_max_validators(10)
        .with_epoch_reward(epoch_reward.clone())
        .build();

    // check config
    let cfg = suite.config().unwrap();
    assert_eq!(
        cfg,
        Config {
            membership: cfg.membership.clone(),
            min_weight: 5,
            max_validators: 10,
            scaling: None,
            epoch_reward,
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            double_sign_slash_ratio: Decimal::percent(50),
            distribution_contracts: vec![],
            rewards_contract: cfg.rewards_contract.clone(),
        }
    );

    // no initial active set
    let active = suite.list_active_validators().unwrap();
    assert_eq!(active, vec![]);

    // check a validator is set
    let op = addrs(4)
        .into_iter()
        .map(|s| valid_operator(&s))
        .last()
        .unwrap();

    let val = suite.validator(&op.operator).unwrap();
    let val = val.validator.unwrap();
    assert_eq!(val.pubkey, op.validator_pubkey);
    assert_eq!(val.metadata, op.metadata);
}

#[test]
fn simulate_validators() {
    let bond_denom = "tgrade";
    let tokens_per_weight = 100u128;
    let min_weight = 2;

    let ops_owned = addrs(24);
    let operators: Vec<_> = ops_owned.iter().map(String::as_str).collect();

    let operator_funds = cosmwasm_std::coins(1_000, bond_denom);
    let operator_balances: Vec<_> = operators
        .iter()
        .copied()
        .zip(std::iter::repeat(operator_funds.as_slice()))
        .collect();

    let mut suite = SuiteBuilder::new()
        .with_stake(bond_denom, tokens_per_weight)
        .with_operators(&operators)
        .with_funds(&operator_balances)
        .with_min_weight(min_weight)
        .with_max_validators(10)
        .with_epoch_reward(coin(50_000, "usdc"))
        .build();

    // what do we expect?
    // 1..24 have pubkeys registered, we take the top 10, but none have stake yet, so zero
    let active = suite.list_active_validators().unwrap();
    assert_eq!(0, active.len());

    // One member bonds needed tokens to have enough weight
    let op1_addr = Addr::unchecked(operators[0]);

    // First, he does not bond enough tokens
    let stake = cosmwasm_std::coins(tokens_per_weight * min_weight as u128 - 1u128, bond_denom);
    suite.bond(&op1_addr, &stake).unwrap();

    // what do we expect?
    // 1..24 have pubkeys registered, we take the top 10, only one has stake but not enough of it, so zero
    let active = suite.simulate_active_validators().unwrap();
    assert_eq!(0, active.len());

    // Now, he bonds just enough tokens of the right denom
    let stake = cosmwasm_std::coins(1, bond_denom);
    suite.bond(&op1_addr, &stake).unwrap();

    // what do we expect?
    // only one have enough stake now, so one
    let active = suite.simulate_active_validators().unwrap();
    assert_eq!(1, active.len());

    let expected: Vec<_> = vec![ValidatorInfo {
        operator: op1_addr.clone(),
        validator_pubkey: valid_operator(op1_addr.as_ref()).validator_pubkey,
        metadata: valid_operator(op1_addr.as_ref()).metadata,
        power: min_weight,
    }];
    assert_eq!(expected, active);

    // Other member bonds twice the minimum amount
    let op2_addr = Addr::unchecked(operators[1]);

    let stake = cosmwasm_std::coins(tokens_per_weight * min_weight as u128 * 2u128, bond_denom);
    suite.bond(&op2_addr, &stake).unwrap();

    // what do we expect?
    // two have stake, so two
    let active = suite.simulate_active_validators().unwrap();
    assert_eq!(2, active.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(op2_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op2_addr.as_ref()).metadata,
            power: min_weight * 2,
        },
        ValidatorInfo {
            operator: op1_addr.clone(),
            validator_pubkey: valid_operator(op1_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op1_addr.as_ref()).metadata,
            power: min_weight,
        },
    ];
    assert_eq!(expected, active);

    // Other member bonds almost thrice the minimum amount
    let op3_addr = Addr::unchecked(operators[2]);

    let stake = cosmwasm_std::coins(
        tokens_per_weight * min_weight as u128 * 3u128 - 1u128,
        bond_denom,
    );
    suite.bond(&op3_addr, &stake).unwrap();

    // what do we expect?
    // three have stake, so three
    let active = suite.simulate_active_validators().unwrap();
    assert_eq!(3, active.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op3_addr.clone(),
            validator_pubkey: valid_operator(op3_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op3_addr.as_ref()).metadata,
            power: min_weight * 3 - 1,
        },
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(op2_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op2_addr.as_ref()).metadata,
            power: min_weight * 2,
        },
        ValidatorInfo {
            operator: op1_addr.clone(),
            validator_pubkey: valid_operator(op1_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op1_addr.as_ref()).metadata,
            power: min_weight,
        },
    ];
    assert_eq!(expected, active);

    // Now, op1 unbonds some tokens
    let tokens = cosmwasm_std::coin(1, bond_denom);
    suite.unbond(&op1_addr, tokens).unwrap();

    // what do we expect?
    // only two have enough stake, so two
    let active = suite.simulate_active_validators().unwrap();
    assert_eq!(2, active.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op3_addr.clone(),
            validator_pubkey: valid_operator(op3_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op3_addr.as_ref()).metadata,
            power: min_weight * 3 - 1,
        },
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(op2_addr.as_ref()).validator_pubkey,
            metadata: valid_operator(op2_addr.as_ref()).metadata,
            power: min_weight * 2,
        },
    ];
    assert_eq!(expected, active);
}
