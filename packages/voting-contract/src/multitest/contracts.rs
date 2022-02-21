pub mod voting;

use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo};
use cw_multi_test::{Contract, ContractWrapper};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_bindings::{TgradeMsg, TgradeQuery};

pub use voting::VotingContract;

pub fn engagement_contract() -> Box<dyn Contract<TgradeMsg, TgradeQuery>> {
    let contract = ContractWrapper::<_, _, _, _, _, _, _, TgradeQuery>::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}
