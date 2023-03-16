#![cfg(feature = "integration")]
//! This integration test tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo integration-test` will validate we can properly call into that generated Wasm.
//!
use bech32::{ToBase32, Variant};

use cosmwasm_std::{coin, to_binary, Addr, ContractResult, Decimal, Empty, Response};
use cosmwasm_vm::testing::{
    execute, instantiate, mock_env, mock_info, mock_instance_with_options, MockApi,
    MockInstanceOptions, MockQuerier, MockStorage,
};
use cosmwasm_vm::{capabilities_from_csv, Instance};
use tg_bindings::TgradeMsg;
use tg_utils::Duration;

use tgrade_valset::msg::ExecuteMsg;
use tgrade_valset::msg::InstantiateMsg;
use tgrade_valset::state::ValidatorInfo;
use tgrade_valset::test_helpers::mock_pubkey;

// Copied from test_helpers
// returns a list of addresses that are set in the tg4-stake contract
fn addrs(count: u32) -> Vec<String> {
    (1..=count)
        .map(|x| {
            bech32::encode(
                "tgrade",
                format!("operator-{:03}", x).to_base32(),
                Variant::Bech32,
            )
            .unwrap()
        })
        .collect()
}

fn valid_validator(seed: &str, power: u64) -> ValidatorInfo {
    ValidatorInfo {
        operator: Addr::unchecked(seed),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        power,
    }
}

fn mock_instance_on_tgrade(wasm: &[u8]) -> Instance<MockApi, MockStorage, MockQuerier> {
    mock_instance_with_options(
        wasm,
        MockInstanceOptions {
            available_capabilities: capabilities_from_csv("iterator,tgrade"),
            gas_limit: 100_000_000_000_000,
            ..Default::default()
        },
    )
}

pub fn mock_instantiate_msg(admin: Addr) -> InstantiateMsg {
    InstantiateMsg {
        admin: Some(admin.to_string()),
        membership: "membership".into(),
        min_points: 1,
        max_validators: 100,
        epoch_length: 120,
        epoch_reward: coin(100, "utgd"),
        initial_keys: vec![],
        scaling: None,
        fee_percentage: Decimal::percent(0),
        auto_unjail: false,
        double_sign_slash_ratio: Decimal::percent(50),
        distribution_contracts: Default::default(),
        validator_group_code_id: 0,
        verify_validators: false,
        offline_jail_duration: Duration::new(86400),
    }
}

static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tgrade_valset.wasm");

const NUM_VALIDATORS: u32 = 956;
const VALIDATOR_POWER: u64 = 1;

#[test]
fn instantiate_works() {
    let mut deps = mock_instance_with_options(
        WASM,
        MockInstanceOptions {
            available_capabilities: capabilities_from_csv("iterator,tgrade"),
            ..Default::default()
        },
    );
    assert_eq!(deps.required_capabilities().len(), 2);

    let info = mock_info("creator", &[]);

    let instantiate_msg = mock_instantiate_msg(Addr::unchecked("owner"));
    instantiate::<_, _, _, _, TgradeMsg>(&mut deps, mock_env(), info, instantiate_msg).unwrap();
}

#[test]
fn test_validators_storage() {
    let mut deps = mock_instance_on_tgrade(WASM);
    assert_eq!(deps.required_capabilities().len(), 2);

    let info = mock_info("creator", &[]);

    let validators = addrs(NUM_VALIDATORS)
        .iter()
        .map(|s| valid_validator(s, VALIDATOR_POWER))
        .collect::<Vec<ValidatorInfo>>();

    // Report serialized validators size
    let serialized_validator = to_binary(&validators[0]).unwrap();
    let serialized_validators = to_binary(&validators).unwrap();
    println!();
    println!("Number of validators: {}", NUM_VALIDATORS);
    println!("Size of a validator: {}", serialized_validator.len());
    println!(
        "Size of the validators list: {}",
        serialized_validators.len()
    );

    let msg = ExecuteMsg::SimulateValidatorSet { validators };

    let res: Response = execute(&mut deps, mock_env(), info, msg).unwrap();

    assert_eq!(res.messages.len(), 0);
}

#[test]
#[should_panic(expected = "Region length too big")]
fn check_validators_storage_breaks() {
    let mut deps = mock_instance_on_tgrade(WASM);

    let info = mock_info("creator", &[]);

    // One more validator this size breaks the validator set storages
    let validators = addrs(NUM_VALIDATORS + 1)
        .iter()
        .map(|s| valid_validator(s, VALIDATOR_POWER))
        .collect::<Vec<ValidatorInfo>>();

    let msg = ExecuteMsg::SimulateValidatorSet { validators };

    let _: ContractResult<Response<Empty>> = execute(&mut deps, mock_env(), info, msg);
}
