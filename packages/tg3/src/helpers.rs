use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg};
use tg_bindings::TgradeMsg;

use crate::msg::{Tg3ExecuteMsg, Vote};
use tg_utils::Expiration;

/// Tg3Contract is a wrapper around Addr that provides a lot of helpers
/// for working with this.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Tg3Contract(pub Addr);

impl Tg3Contract {
    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    pub fn encode_msg(&self, msg: Tg3ExecuteMsg) -> StdResult<CosmosMsg<TgradeMsg>> {
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg: to_binary(&msg)?,
            funds: vec![],
        }
        .into())
    }

    /// helper doesn't support custom messages now
    pub fn proposal(
        &self,
        title: impl Into<String>,
        description: impl Into<String>,
        msgs: Vec<CosmosMsg<TgradeMsg>>,
        earliest: Option<Expiration>,
        latest: Option<Expiration>,
    ) -> StdResult<CosmosMsg<TgradeMsg>> {
        let msg = Tg3ExecuteMsg::Propose {
            title: title.into(),
            description: description.into(),
            msgs,
            earliest,
            latest,
        };
        self.encode_msg(msg)
    }

    pub fn vote(&self, proposal_id: u64, vote: Vote) -> StdResult<CosmosMsg<TgradeMsg>> {
        let msg = Tg3ExecuteMsg::Vote { proposal_id, vote };
        self.encode_msg(msg)
    }

    pub fn execute(&self, proposal_id: u64) -> StdResult<CosmosMsg<TgradeMsg>> {
        let msg = Tg3ExecuteMsg::Execute { proposal_id };
        self.encode_msg(msg)
    }

    pub fn close(&self, proposal_id: u64) -> StdResult<CosmosMsg<TgradeMsg>> {
        let msg = Tg3ExecuteMsg::Close { proposal_id };
        self.encode_msg(msg)
    }
}
