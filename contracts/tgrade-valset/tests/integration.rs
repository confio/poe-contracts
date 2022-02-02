//! This integration test tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo integration-test` will validate we can properly call into that generated Wasm.
//!
//! You can easily convert unit tests to integration tests as follows:
//! 1. Copy them over verbatim
//! 2. Then change
//!      let mut deps = mock_dependencies(20, &[]);
//!    to
//!      let mut deps = mock_instance(WASM, &[]);
//! 3. If you access raw storage, where ever you see something like:
//!      deps.storage.get(CONFIG_KEY).expect("no data stored");
//!    replace it with:
//!      deps.with_storage(|store| {
//!          let data = store.get(CONFIG_KEY).expect("no data stored");
//!          //...
//!      });
//! 4. Anywhere you see query(&deps, ...) you must replace it with query(&mut deps, ...)

use cosmwasm_std::{to_binary, Addr, Binary, ContractResult, Empty, Response};
use cosmwasm_vm::testing::{
    execute, mock_env, mock_info, mock_instance_with_options, MockApi, MockInstanceOptions,
    MockQuerier, MockStorage,
};
use tg_bindings::Pubkey;

use cosmwasm_vm::{features_from_csv, Instance};
use tgrade_valset::msg::{ExecuteMsg, ValidatorMetadata};
use tgrade_valset::state::ValidatorInfo;

// Copied from test_helpers
// returns a list of addresses that are set in the tg4-stake contract
fn addrs(count: u32) -> Vec<String> {
    (1..=count).map(|x| format!("operator-{:03}", x)).collect()
}

fn valid_validator(seed: &str, power: u64) -> ValidatorInfo {
    ValidatorInfo {
        operator: Addr::unchecked(seed),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        metadata: mock_metadata(seed),
        power,
    }
}

fn mock_instance_on_tgrade(wasm: &[u8]) -> Instance<MockApi, MockStorage, MockQuerier> {
    mock_instance_with_options(
        wasm,
        MockInstanceOptions {
            supported_features: features_from_csv("iterator,tgrade"),
            gas_limit: 100_000_000_000_000,
            ..Default::default()
        },
    )
}

const ED25519_PUBKEY_LENGTH: usize = 32;

// creates a valid pubkey from a seed
fn mock_pubkey(base: &[u8]) -> Pubkey {
    let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(ED25519_PUBKEY_LENGTH);
    Pubkey::Ed25519(Binary(raw))
}

fn mock_metadata(seed: &str) -> ValidatorMetadata {
    ValidatorMetadata {
        moniker: seed.into(),
        details: Some(format!("I'm really {}", seed)),
        ..ValidatorMetadata::default()
    }
}

static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tgrade_valset.wasm");

const NUM_VALIDATORS: u32 = 534;
const VALIDATOR_POWER: u64 = 1;

#[test]
fn test_validators_storage() {
    let mut deps = mock_instance_on_tgrade(WASM);
    assert_eq!(deps.required_features().len(), 2);

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
#[should_panic]
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
