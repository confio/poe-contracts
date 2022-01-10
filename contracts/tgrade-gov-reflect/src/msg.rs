use cosmwasm_std::SubMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_bindings::{GovProposal, TgradeMsg};

/// Creator is owner and can reflect anything
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Execute {
        msgs: Vec<SubMsg<TgradeMsg>>,
    },
    Proposal {
        title: String,
        description: String,
        proposal: GovProposal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Owner {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OwnerResponse {
    pub owner: String,
}
