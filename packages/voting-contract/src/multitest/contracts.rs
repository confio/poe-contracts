use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, StdError};
use cw_multi_test::{Contract, ContractWrapper};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeMsg;

pub fn engagement_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

pub fn voting_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(voting::execute, voting::instantiate, voting::query);
    Box::new(contract)
}

pub mod voting {
    use crate::{state::VotingRules, ContractError};

    use super::*;

    type Response = cosmwasm_std::Response<TgradeMsg>;

    #[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
    #[serde(rename_all = "snake_case")]
    pub struct InstantiateMsg {
        pub rules: VotingRules,
        pub group_addr: String,
    }

    #[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
    pub struct ExecuteMsg {}

    #[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
    pub struct QueryMsg {}

    pub fn instantiate(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> Result<Response, ContractError> {
        crate::instantiate(deps, msg.rules, &msg.group_addr)
    }

    pub fn execute(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: ExecuteMsg,
    ) -> Result<Response, StdError> {
        todo!()
    }

    pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> Result<Binary, StdError> {
        todo!()
    }
}
