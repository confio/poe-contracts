pub mod voting;

use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo};
use cw_multi_test::{Contract, ContractWrapper};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeMsg;

pub use voting::VotingContract;

pub fn engagement_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}
