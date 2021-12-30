//! Simplified contract which when executed releases the funds to beneficiary
//! Copied almost 1:1 from https://github.com/CosmWasm/cw-plus/blob/main/packages/multi-test/src/test_helpers/contracts/hackatom.rs

use cosmwasm_std::{to_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, StdError};
use cw_storage_plus::Item;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeMsg;

use cw_multi_test::{Contract, ContractWrapper};

pub type Response = cosmwasm_std::Response<TgradeMsg>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstantiateMsg {
    pub beneficiary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteMsg {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateMsg {
    // just use some other string so we see there are other types
    pub new_guy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // returns InstantiateMsg
    Beneficiary {},
}

const HACKATOM: Item<InstantiateMsg> = Item::new("hackatom");

fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
    HACKATOM.save(deps.storage, &msg)?;
    Ok(Response::default())
}

fn execute(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, StdError> {
    let init = HACKATOM.load(deps.storage)?;
    let balance = deps.querier.query_all_balances(env.contract.address)?;

    let resp = Response::new().add_message(BankMsg::Send {
        to_address: init.beneficiary,
        amount: balance,
    });

    Ok(resp)
}

fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::Beneficiary {} => {
            let res = HACKATOM.load(deps.storage)?;
            to_binary(&res)
        }
    }
}

fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, StdError> {
    HACKATOM.update::<_, StdError>(deps.storage, |mut state| {
        state.beneficiary = msg.new_guy;
        Ok(state)
    })?;
    let resp = Response::new().add_attribute("migrate", "successful");
    Ok(resp)
}

pub fn contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(execute, instantiate, query).with_migrate(migrate);
    Box::new(contract)
}
