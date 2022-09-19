//! This benchmark tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo bench` will validate we can properly call into that generated Wasm.
//!
use cosmwasm_std::{Decimal, Uint64};
use cosmwasm_vm::testing::{
    mock_env, mock_instance_with_options, query, MockApi, MockInstanceOptions, MockQuerier,
    MockStorage,
};
use cosmwasm_vm::{capabilities_from_csv, from_slice, Instance};

use tg4_mixer::msg::PoEFunctionType::{AlgebraicSigmoid, GeometricMean, Sigmoid, SigmoidSqrt};
use tg4_mixer::msg::{MixerFunctionResponse, QueryMsg};

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

// Output of cargo wasm
static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tg4_mixer.wasm");

const DESERIALIZATION_LIMIT: usize = 20_000;

// From https://github.com/CosmWasm/wasmd/blob/master/x/wasm/keeper/gas_register.go#L31
const GAS_MULTIPLIER: u64 = 140_000_000;

fn main() {
    const MAX_POINTS: u64 = 1000;

    const STAKE: u64 = 100000;
    const ENGAGEMENT: u64 = 5000;

    let mut deps = mock_instance_on_tgrade(WASM);

    let max_points = Uint64::new(MAX_POINTS);
    let a = Decimal::from_ratio(37u128, 10u128);
    let p = Decimal::from_ratio(68u128, 100u128);
    let s = Decimal::from_ratio(3u128, 100000u128);
    let s_sqrt = Decimal::from_ratio(3u128, 10000u128);

    println!();
    for (poe_fn_name, poe_fn, result, gas) in [
        ("GeometricMean", GeometricMean {}, 22360, 5729550000i64),
        (
            "Sigmoid",
            Sigmoid { max_points, p, s },
            MAX_POINTS,
            89533650000,
        ),
        (
            "SigmoidSqrt",
            SigmoidSqrt {
                max_points,
                s: s_sqrt,
            },
            997,
            20300700000,
        ),
        (
            "AlgebraicSigmoid",
            AlgebraicSigmoid {
                max_points,
                a,
                p,
                s,
            },
            996,
            84850050000,
        ),
    ] {
        let benchmark_msg = QueryMsg::MixerFunction {
            stake: Uint64::new(STAKE),
            engagement: Uint64::new(ENGAGEMENT),
            poe_function: Some(poe_fn),
        };

        let gas_before = deps.get_gas_left();
        let raw = query(&mut deps, mock_env(), benchmark_msg).unwrap();
        let res: MixerFunctionResponse = from_slice(&raw, DESERIALIZATION_LIMIT).unwrap();
        let gas_used = gas_before - deps.get_gas_left();
        let sdk_gas = gas_used / GAS_MULTIPLIER;

        println!(
            "{:>16}({}, {}) = {:>5} ({:>3} SDK gas)",
            poe_fn_name, STAKE, ENGAGEMENT, res.points, sdk_gas
        );

        assert_eq!(
            MixerFunctionResponse { points: result },
            res,
            "{} result",
            poe_fn_name
        );
        assert!(
            (gas - gas_used as i64).abs() < gas / 10,
            "{} gas",
            poe_fn_name
        );
    }
}
