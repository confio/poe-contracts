#![cfg(test)]
use cosmwasm_std::{coin, Addr, Coin, Decimal, Uint128};

use tg4::Tg4Contract;
use tg_bindings::TgradeMsg;

use tg4_stake::msg::ExecuteMsg;

use cw_multi_test::{AppBuilder, BasicApp, Contract, ContractWrapper, Executor};

use crate::msg::{
    EpochResponse, InstantiateMsg, ListActiveValidatorsResponse, QueryMsg,
    UnvalidatedDistributionContracts, ValidatorResponse,
};
use crate::state::{Config, ValidatorInfo};
use crate::test_helpers::{addrs, contract_engagement, contract_valset, valid_operator};

const EPOCH_LENGTH: u64 = 100;

const OPERATOR_FUNDS: u128 = 1_000;

// Stake contract config
const STAKE_OWNER: &str = "admin";
const TOKENS_PER_WEIGHT: u128 = 100;
const BOND_DENOM: &str = "tgrade";
const MIN_BOND: u128 = 100;

// Valset contract config
// these control how many pubkeys get set in the valset init
const PREREGISTER_MEMBERS: u32 = 24;
const MIN_WEIGHT: u64 = 2;

// 500 usdc per block
const REWARD_AMOUNT: u128 = 50_000;
const REWARD_DENOM: &str = "usdc";

fn epoch_reward() -> Coin {
    coin(REWARD_AMOUNT, REWARD_DENOM)
}

fn contract_stake() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_stake::contract::execute,
        tg4_stake::contract::instantiate,
        tg4_stake::contract::query,
    );
    Box::new(contract)
}

// always registers 24 members and 12 non-members with pubkeys
pub fn instantiate_valset(
    app: &mut BasicApp<TgradeMsg>,
    stake: &Addr,
    max_validators: u32,
    min_weight: u64,
) -> Addr {
    let engagement_id = app.store_code(contract_engagement());
    let valset_id = app.store_code(contract_valset());
    let msg = init_msg(
        &stake.to_string(),
        max_validators,
        min_weight,
        engagement_id,
    );
    app.instantiate_contract(
        valset_id,
        Addr::unchecked(STAKE_OWNER),
        &msg,
        &[],
        "flex",
        None,
    )
    .unwrap()
}

fn instantiate_stake(app: &mut BasicApp<TgradeMsg>) -> Addr {
    let stake_id = app.store_code(contract_stake());
    let admin = Some(STAKE_OWNER.into());
    let msg = tg4_stake::msg::InstantiateMsg {
        denom: BOND_DENOM.to_owned(),
        tokens_per_weight: Uint128::new(TOKENS_PER_WEIGHT),
        min_bond: Uint128::new(MIN_BOND),
        unbonding_period: 1234,
        admin: admin.clone(),
        preauths_hooks: 0,
        preauths_slashing: 1,
        auto_return_limit: 0,
    };
    app.instantiate_contract(
        stake_id,
        Addr::unchecked(STAKE_OWNER),
        &msg,
        &[],
        "stake",
        admin,
    )
    .unwrap()
}

// registers first PREREGISTER_MEMBERS members with pubkeys
fn init_msg(
    stake_addr: &str,
    max_validators: u32,
    min_weight: u64,
    rewards_code_id: u64,
) -> InstantiateMsg {
    let members = addrs(PREREGISTER_MEMBERS)
        .into_iter()
        .map(|s| valid_operator(&s))
        .collect();
    InstantiateMsg {
        admin: None,
        membership: stake_addr.into(),
        min_weight,
        max_validators,
        epoch_length: EPOCH_LENGTH,
        epoch_reward: epoch_reward(),
        initial_keys: members,
        scaling: None,
        fee_percentage: Decimal::zero(),
        auto_unjail: false,
        double_sign_slash_ratio: Decimal::percent(50),
        distribution_contracts: UnvalidatedDistributionContracts::default(),
        rewards_code_id,
    }
}

fn bond(app: &mut BasicApp<TgradeMsg>, addr: &Addr, stake_addr: &Addr, stake: &[Coin]) {
    let _ = app
        .execute_contract(
            addr.clone(),
            stake_addr.clone(),
            &ExecuteMsg::Bond {},
            stake,
        )
        .unwrap();
}

fn unbond(app: &mut BasicApp<TgradeMsg>, addr: &Addr, stake_addr: &Addr, tokens: u128) {
    let _ = app
        .execute_contract(
            addr.clone(),
            stake_addr.clone(),
            &ExecuteMsg::Unbond {
                tokens: coin(tokens, BOND_DENOM),
            },
            &[],
        )
        .unwrap();
}

#[test]
fn init_and_query_state() {
    let mut app = AppBuilder::new_custom().build(|_, _, _| ());

    // make a simple stake
    let stake_addr = instantiate_stake(&mut app);
    // make a valset that references it (this does init)
    let valset_addr = instantiate_valset(&mut app, &stake_addr, 10, 5);

    // check config
    let cfg: Config = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::Configuration {})
        .unwrap();
    assert_eq!(
        cfg,
        Config {
            membership: Tg4Contract(stake_addr),
            min_weight: 5,
            max_validators: 10,
            scaling: None,
            epoch_reward: epoch_reward(),
            fee_percentage: Decimal::zero(),
            auto_unjail: false,
            double_sign_slash_ratio: Decimal::percent(50),
            distribution_contracts: vec![],
            rewards_contract: cfg.rewards_contract.clone(),
        }
    );

    // check epoch
    let epoch: EpochResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::Epoch {})
        .unwrap();
    assert_eq!(
        epoch,
        EpochResponse {
            epoch_length: EPOCH_LENGTH,
            current_epoch: 0,
            last_update_time: 0,
            last_update_height: 0,
            next_update_time: app.block_info().time.nanos() / 1_000_000_000,
        }
    );

    // no initial active set
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::ListActiveValidators {})
        .unwrap();
    assert_eq!(active.validators, vec![]);

    // check a validator is set
    let op = addrs(4)
        .into_iter()
        .map(|s| valid_operator(&s))
        .last()
        .unwrap();

    let val: ValidatorResponse = app
        .wrap()
        .query_wasm_smart(
            &valset_addr,
            &QueryMsg::Validator {
                operator: op.operator,
            },
        )
        .unwrap();
    let val = val.validator.unwrap();
    assert_eq!(val.pubkey, op.validator_pubkey);
    assert_eq!(val.metadata, op.metadata);
}

#[test]
fn simulate_validators() {
    let operators: Vec<_> = addrs(PREREGISTER_MEMBERS)
        .iter()
        .map(Addr::unchecked)
        .collect();

    let mut app = AppBuilder::new_custom().build(|router, _, storage| {
        let operator_funds = cosmwasm_std::coins(OPERATOR_FUNDS, BOND_DENOM);
        for op_addr in &operators {
            router
                .bank
                .init_balance(storage, op_addr, operator_funds.clone())
                .unwrap();
        }
    });

    // make a simple stake
    let stake_addr = instantiate_stake(&mut app);
    // make a valset that references it (this does init)
    let valset_addr = instantiate_valset(&mut app, &stake_addr, 10, MIN_WEIGHT);

    // what do we expect?
    // 1..24 have pubkeys registered, we take the top 10, but none have stake yet, so zero
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(0, active.validators.len());

    // One member bonds needed tokens to have enough weight
    let op1_addr = &operators[0];

    // First, he does not bond enough tokens
    let stake = cosmwasm_std::coins(TOKENS_PER_WEIGHT * MIN_WEIGHT as u128 - 1u128, BOND_DENOM);
    bond(&mut app, op1_addr, &stake_addr, &stake);

    // what do we expect?
    // 1..24 have pubkeys registered, we take the top 10, only one has stake but not enough of it, so zero
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(0, active.validators.len());

    // Now, he bonds just enough tokens of the right denom
    let stake = cosmwasm_std::coins(1, BOND_DENOM);
    bond(&mut app, op1_addr, &stake_addr, &stake);

    // what do we expect?
    // only one have enough stake now, so one
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(1, active.validators.len());

    let expected: Vec<_> = vec![ValidatorInfo {
        operator: op1_addr.clone(),
        validator_pubkey: valid_operator(op1_addr.as_ref()).validator_pubkey,
        power: MIN_WEIGHT,
    }];
    assert_eq!(expected, active.validators);

    // Other member bonds twice the minimum amount
    let op2_addr = &operators[1];

    let stake = cosmwasm_std::coins(TOKENS_PER_WEIGHT * MIN_WEIGHT as u128 * 2u128, BOND_DENOM);
    bond(&mut app, op2_addr, &stake_addr, &stake);

    // what do we expect?
    // two have stake, so two
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(2, active.validators.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(op2_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT * 2,
        },
        ValidatorInfo {
            operator: op1_addr.clone(),
            validator_pubkey: valid_operator(op1_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT,
        },
    ];
    assert_eq!(expected, active.validators);

    // Other member bonds almost thrice the minimum amount
    let op3_addr = &operators[2];

    let stake = cosmwasm_std::coins(
        TOKENS_PER_WEIGHT * MIN_WEIGHT as u128 * 3u128 - 1u128,
        BOND_DENOM,
    );
    bond(&mut app, op3_addr, &stake_addr, &stake);

    // what do we expect?
    // three have stake, so three
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(3, active.validators.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op3_addr.clone(),
            validator_pubkey: valid_operator(op3_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT * 3 - 1,
        },
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(op2_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT * 2,
        },
        ValidatorInfo {
            operator: op1_addr.clone(),
            validator_pubkey: valid_operator(op1_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT,
        },
    ];
    assert_eq!(expected, active.validators);

    // Now, op1 unbonds some tokens
    let tokens = 1;
    unbond(&mut app, op1_addr, &stake_addr, tokens);

    // what do we expect?
    // only two have enough stake, so two
    let active: ListActiveValidatorsResponse = app
        .wrap()
        .query_wasm_smart(&valset_addr, &QueryMsg::SimulateActiveValidators {})
        .unwrap();
    assert_eq!(2, active.validators.len());

    // Active validators are returned sorted from highest power to lowest
    let expected: Vec<_> = vec![
        ValidatorInfo {
            operator: op3_addr.clone(),
            validator_pubkey: valid_operator(op3_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT * 3 - 1,
        },
        ValidatorInfo {
            operator: op2_addr.clone(),
            validator_pubkey: valid_operator(op2_addr.as_ref()).validator_pubkey,
            power: MIN_WEIGHT * 2,
        },
    ];
    assert_eq!(expected, active.validators);
}
