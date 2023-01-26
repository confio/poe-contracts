//! This example tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo example` will validate we can properly call into that generated Wasm.

use cosmwasm_std::{Decimal, Uint64};
use cosmwasm_vm::testing::{
    mock_env, mock_instance_with_options, query, MockApi, MockInstanceOptions, MockQuerier,
    MockStorage,
};
use cosmwasm_vm::{capabilities_from_csv, Instance};

use tg4_mixer::msg::PoEFunctionType::Sigmoid;
use tg4_mixer::msg::QueryMsg;

fn mock_instance_on_tgrade(wasm: &[u8]) -> Instance<MockApi, MockStorage, MockQuerier> {
    mock_instance_with_options(
        wasm,
        MockInstanceOptions {
            available_capabilities: capabilities_from_csv("iterator,tgrade"),
            gas_limit: 100_000_000_000_000_000,
            ..Default::default()
        },
    )
}

// Output of cargo wasm
static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tg4_mixer.wasm");

fn main() {
    const MAX_POINTS: u64 = 1000;
    const ENGAGEMENT: u64 = 50000;

    let mut deps = mock_instance_on_tgrade(WASM);

    let max_points = Uint64::new(MAX_POINTS);

    // Fixed engagement
    let engagement = Uint64::new(ENGAGEMENT);

    let min_stake = 1000000;
    let step_stake = 10000;

    println!();
    println!("Sigmoid function breaking checks:");
    for (s, p) in [
        (
            Decimal::from_ratio(3u128, 100000u128),
            Decimal::from_ratio(68u128, 100u128),
        ),
        (
            Decimal::from_ratio(5u128, 100000u128),
            Decimal::from_ratio(55u128, 100u128),
        ),
        (
            Decimal::from_ratio(2u128, 100000u128),
            Decimal::from_ratio(57u128, 100u128),
        ),
        (
            Decimal::from_ratio(1u128, 1000000u128),
            Decimal::from_ratio(72u128, 100u128),
        ),
        (
            Decimal::from_ratio(3u128, 100000u128),
            Decimal::from_ratio(59u128, 100u128),
        ),
        (
            Decimal::from_ratio(1u128, 100000u128),
            Decimal::from_ratio(62u128, 100u128),
        ),
        (
            Decimal::from_ratio(1u128, 1000000u128),
            Decimal::from_ratio(741u128, 1000u128),
        ),
        (
            Decimal::from_ratio(5u128, 100000u128),
            Decimal::from_ratio(54u128, 100u128),
        ),
        (
            Decimal::from_ratio(2u128, 100000u128),
            Decimal::from_ratio(56u128, 100u128),
        ),
        (
            Decimal::from_ratio(1u128, 1000000u128),
            Decimal::from_ratio(707u128, 1000u128),
        ),
    ] {
        let sigmoid_fn = Sigmoid { max_points, p, s };

        for stake in (min_stake..).step_by(step_stake) {
            let breaking_msg = QueryMsg::MixerFunction {
                stake: Uint64::new(stake),
                engagement,
                poe_function: Some(sigmoid_fn.clone()),
            };

            let res = query(&mut deps, mock_env(), breaking_msg);

            if res.is_err() {
                println!(
                    "Sigmoid(p={}, s={})(stake={}, engagement={}) broke.",
                    p, s, stake, ENGAGEMENT
                );
                break;
            }
        }
    }
}
