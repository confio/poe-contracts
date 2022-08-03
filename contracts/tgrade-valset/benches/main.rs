//! This benchmark tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo bench` will validate we can properly call into that generated Wasm.
//!
use hex_literal::hex;
use std::convert::TryFrom;

use cosmwasm_vm::testing::{
    execute, mock_env, mock_info, mock_instance_with_options, MockApi, MockInstanceOptions,
    MockQuerier, MockStorage,
};
use cosmwasm_vm::{features_from_csv, Instance};

use tg_bindings::{Ed25519Pubkey, Pubkey};
use tgrade_valset::contract::Response;
use tgrade_valset::msg::ExecuteMsg;

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

// Output of cargo wasm
static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tgrade_valset.wasm");

// From https://github.com/CosmWasm/wasmd/blob/master/x/wasm/keeper/gas_register.go#L31
const GAS_MULTIPLIER: u64 = 140_000_000;

const PUBKEY_HEX: [u8; 32] =
    hex!("14253d61ef42d166d02e68d540d07fdf8d65a9af0acaa46302688e788a8521e2");

fn main() {
    let pubkey = Pubkey::Ed25519(PUBKEY_HEX.into());
    let ed25519_pubkey = Ed25519Pubkey::try_from(pubkey).unwrap();

    let mut deps = mock_instance_on_tgrade(WASM);

    // No cache
    let to_address_msg = ExecuteMsg::PubkeyToAddress {
        pubkey: ed25519_pubkey.clone(),
        cache: false,
    };
    let gas_before = deps.get_gas_left();
    let res: Response = execute(
        &mut deps,
        mock_env(),
        mock_info("sender", &[]),
        to_address_msg,
    )
    .unwrap();
    let gas_used = gas_before - deps.get_gas_left();
    let address = res.data.unwrap();
    let sdk_gas = gas_used / GAS_MULTIPLIER;

    println!(
        "{} = {} (not cached) ({} SDK gas)",
        [ed25519_pubkey.to_base64(), ".to_address()".to_string()].concat(),
        address,
        sdk_gas
    );

    // Cache
    let to_address_msg = ExecuteMsg::PubkeyToAddress {
        pubkey: ed25519_pubkey.clone(),
        cache: true,
    };
    let gas_before = deps.get_gas_left();
    let res: Response = execute(
        &mut deps,
        mock_env(),
        mock_info("sender", &[]),
        to_address_msg,
    )
    .unwrap();
    let gas_used = gas_before - deps.get_gas_left();
    let address = res.data.unwrap();
    let sdk_gas = gas_used / GAS_MULTIPLIER;

    println!(
        "{} = {} (cached) ({} SDK gas)",
        [ed25519_pubkey.to_base64(), ".to_address()".to_string()].concat(),
        address,
        sdk_gas
    );

    // Cache hit
    let read_address_msg = ExecuteMsg::ReadPubkeyAddress {
        pubkey: ed25519_pubkey.clone(),
    };
    let gas_before = deps.get_gas_left();
    let res: Response = execute(
        &mut deps,
        mock_env(),
        mock_info("sender", &[]),
        read_address_msg,
    )
    .unwrap();
    let gas_used = gas_before - deps.get_gas_left();
    let address = res.data.unwrap();
    let sdk_gas = gas_used / GAS_MULTIPLIER;

    println!(
        "{} = {} (cache hit) ({} SDK gas)",
        [ed25519_pubkey.to_base64(), ".to_address()".to_string()].concat(),
        address,
        sdk_gas
    );
}
