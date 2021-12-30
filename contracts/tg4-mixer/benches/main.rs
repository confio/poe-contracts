//! This benchmark tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo bench` will validate we can properly call into that generated Wasm.
//!
use cosmwasm_std::{Decimal, Uint64};
use cosmwasm_vm::from_slice;
use cosmwasm_vm::testing::{mock_env, mock_instance, query};

use tg4_mixer::msg::PoEFunctionType::{AlgebraicSigmoid, GeometricMean, Sigmoid, SigmoidSqrt};
use tg4_mixer::msg::{QueryMsg, RewardFunctionResponse};

// Output of cargo wasm
static WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/tg4_mixer.wasm");

const DESERIALIZATION_LIMIT: usize = 20_000;

// From https://github.com/CosmWasm/wasmd/blob/master/x/wasm/keeper/gas_register.go#L31
const GAS_MULTIPLIER: u64 = 140_000_000;

fn main() {
    const MAX_REWARDS: u64 = 1000;

    const STAKE: u64 = 100000;
    const ENGAGEMENT: u64 = 5000;

    let mut deps = mock_instance(WASM, &[]);

    let max_rewards = Uint64::new(MAX_REWARDS);
    let a = Decimal::from_ratio(37u128, 10u128);
    let p = Decimal::from_ratio(68u128, 100u128);
    let s = Decimal::from_ratio(3u128, 100000u128);
    let s_sqrt = Decimal::from_ratio(3u128, 10000u128);

    println!();
    for (poe_fn_name, poe_fn, result, gas) in [
        ("GeometricMean", GeometricMean {}, 22360, 5893350000),
        (
            "Sigmoid",
            Sigmoid { max_rewards, p, s },
            MAX_REWARDS,
            91848300000,
        ),
        (
            "SigmoidSqrt",
            SigmoidSqrt {
                max_rewards,
                s: s_sqrt,
            },
            997,
            21120000000,
        ),
        (
            "AlgebraicSigmoid",
            AlgebraicSigmoid {
                max_rewards,
                a,
                p,
                s,
            },
            996,
            86607900000,
        ),
    ] {
        let benchmark_msg = QueryMsg::RewardFunction {
            stake: Uint64::new(STAKE),
            engagement: Uint64::new(ENGAGEMENT),
            poe_function: Some(poe_fn),
        };

        let gas_before = deps.get_gas_left();
        let raw = query(&mut deps, mock_env(), benchmark_msg).unwrap();
        let res: RewardFunctionResponse = from_slice(&raw, DESERIALIZATION_LIMIT).unwrap();
        let gas_used = gas_before - deps.get_gas_left();
        let sdk_gas = gas_used / GAS_MULTIPLIER;

        println!(
            "{:>16}({}, {}) = {:>5} ({:>3} SDK gas)",
            poe_fn_name, STAKE, ENGAGEMENT, res.reward, sdk_gas
        );

        assert_eq!(
            RewardFunctionResponse { reward: result },
            res,
            "{} result",
            poe_fn_name
        );
        assert_eq!(gas, gas_used, "{} gas", poe_fn_name);
    }
}
