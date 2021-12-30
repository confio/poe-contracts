use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg};
use tg4::{Member, Tg4Contract};

use crate::msg::ExecuteMsg;

/// Tg4GroupContract is a wrapper around Tg4Contract that provides a lot of helpers
/// for working with tg4-engagement contracts.
///
/// It extends Tg4Contract to add the extra calls from tg4-engagement.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Tg4GroupContract(pub Tg4Contract);

impl Deref for Tg4GroupContract {
    type Target = Tg4Contract;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Tg4GroupContract {
    pub fn new(addr: Addr) -> Self {
        Tg4GroupContract(Tg4Contract(addr))
    }

    fn encode_msg(&self, msg: ExecuteMsg) -> StdResult<CosmosMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg: to_binary(&msg)?,
            funds: vec![],
        }
        .into())
    }

    pub fn update_members(&self, remove: Vec<String>, add: Vec<Member>) -> StdResult<CosmosMsg> {
        let msg = ExecuteMsg::UpdateMembers { remove, add };
        self.encode_msg(msg)
    }
}
