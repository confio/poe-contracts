use crate::msg::{DistributionMsg, RewardsDistribution};
use crate::state::Config;
use cosmwasm_std::{
    coins, to_binary, Addr, Coin, DepsMut, Env, StdResult, SubMsg, Uint128, WasmMsg,
};
use tg_bindings::TgradeMsg;

#[derive(Clone)]
pub struct DistributionInfo {
    pub addr: Addr,
    pub weight: u64,
}

/// Ensure you pass in non-empty pay-validators, it will panic if total validator weight is 0
/// This handles all deps and calls into pure functions
pub fn pay_block_rewards(
    deps: DepsMut,
    env: Env,
    pay_epochs: u64,
    config: &Config,
) -> StdResult<Vec<SubMsg<TgradeMsg>>> {
    // calculate the desired block reward
    let mut block_reward = config.epoch_reward.clone();
    block_reward.amount = Uint128::new(block_reward.amount.u128() * (pay_epochs as u128));
    let denom = block_reward.denom.clone();

    // query existing balance
    let balances = deps.querier.query_all_balances(&env.contract.address)?;
    let fees_amount = get_fees_amount(&balances, &denom);

    let amount = block_reward
        .amount
        .saturating_sub(config.fee_percentage * fees_amount);
    block_reward.amount = amount + fees_amount;

    let mut reward_pool = block_reward.amount;

    // create the distribution messages
    let mut messages = vec![];

    // create a minting action if needed (and do this first)
    if amount > Uint128::zero() {
        let minting = SubMsg::new(TgradeMsg::MintTokens {
            denom,
            amount,
            recipient: env.contract.address.into(),
        });
        messages.push(minting);
    }

    for contract in &config.distribution_contracts {
        let reward = block_reward.amount * contract.ratio;
        if reward > Uint128::zero() {
            reward_pool -= reward;
            messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: contract.contract.to_string(),
                msg: to_binary(&DistributionMsg::DistributeFunds {})?,
                funds: coins(reward.into(), &block_reward.denom),
            }));
        }
    }

    // After rewarding all non-validators, the remainder goes to validators.
    if reward_pool > Uint128::zero() {
        messages.push(SubMsg::new(WasmMsg::Execute {
            contract_addr: config.rewards_contract.to_string(),
            msg: to_binary(&RewardsDistribution::DistributeFunds {})?,
            funds: coins(reward_pool.into(), &block_reward.denom),
        }));
    }

    Ok(messages)
}

fn get_fees_amount(coins: &[Coin], denom: &str) -> Uint128 {
    coins
        .iter()
        .find(|coin| coin.denom == denom)
        .map(|coin| coin.amount)
        .unwrap_or_else(Uint128::zero)
}
