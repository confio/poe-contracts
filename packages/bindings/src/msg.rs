use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Binary, Coin, CosmosMsg, CustomMsg, Uint128};

use crate::gov::GovProposal;
use crate::hooks::PrivilegeMsg;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
/// A number of Custom messages that can be returned by 'privileged' contracts.
/// Returning them from any other contract will return an error and abort the transaction.
pub enum TgradeMsg {
    /// request or release some privileges, such as BeginBlocker or TokenMinter
    Privilege(PrivilegeMsg),
    /// privileged contracts can mint arbitrary native tokens (extends BankMsg)
    MintTokens {
        denom: String,
        amount: Uint128,
        recipient: String,
    },
    /// as well as adjust tendermint consensus params
    ConsensusParams(ConsensusParams),
    /// Run another contract in "sudo" mode (extends WasmMsg)
    WasmSudo {
        contract_addr: String,
        /// msg is the json-encoded SudoMsg struct (as raw Binary).
        /// Note the contract may support different variants than the base TgradeSudoMsg,
        /// which defines the base chain->contract interface
        msg: Binary,
    },
    /// This will execute an approved proposal in the Cosmos SDK "Gov Router".
    /// That allows access to many of the system internals, like sdk params or x/upgrade,
    /// as well as privileged access to the wasm module (eg. mark module privileged)
    ExecuteGovProposal {
        title: String,
        description: String,
        proposal: GovProposal,
    },
    /// This will stake funds from the sender's vesting account. Requires `Delegator` privilege.
    Delegate { funds: Coin, staker: String },
    /// This will unstake funds to the recipient's vesting account. Requires `Delegator` privilege.
    Undelegate { funds: Coin, recipient: String },
}

/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L282-L289
/// These are various Tendermint Consensus Params that can be adjusted by EndBlockers
/// If any of them are set to Some, then the blockchain will set those as new parameter for tendermint consensus.
///
/// Note: we are not including ValidatorParams, which is used to change the allowed pubkey types for validators
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug, Default)]
pub struct ConsensusParams {
    pub block: Option<BlockParams>,
    pub evidence: Option<EvidenceParams>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug, Default)]
pub struct BlockParams {
    /// Maximum number of bytes (over all tx) to be included in a block
    pub max_bytes: Option<i64>,
    /// Maximum gas (over all tx) to be executed in one block.
    /// If set, more txs may be included in a block, but when executing, all tx after this is limit
    /// are consumed will immediately error
    pub max_gas: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug, Default)]
pub struct EvidenceParams {
    /// Max age of evidence, in blocks.
    pub max_age_num_blocks: Option<i64>,

    /// Max age of evidence, in seconds.
    /// It should correspond with an app's "unbonding period"
    pub max_age_duration: Option<i64>,

    /// Maximum number of bytes of evidence to be included in a block
    pub max_bytes: Option<i64>,
}

// we provide some constructor helpers for some common parameter changes
impl ConsensusParams {
    /// set -1 for unlimited, positive number for a gas limit over all txs in a block
    pub fn max_block_gas(gas: i64) -> Self {
        ConsensusParams {
            block: Some(BlockParams {
                max_bytes: None,
                max_gas: Some(gas),
            }),
            evidence: None,
        }
    }

    /// set -1 for unlimited, positive number for a gas limit over all txs in a block
    pub fn max_block_size(bytes: i64) -> Self {
        ConsensusParams {
            block: Some(BlockParams {
                max_bytes: Some(bytes),
                max_gas: None,
            }),
            evidence: None,
        }
    }

    pub fn max_evidence_age(seconds: i64) -> Self {
        ConsensusParams {
            block: None,
            evidence: Some(EvidenceParams {
                max_age_num_blocks: None,
                max_age_duration: Some(seconds),
                max_bytes: None,
            }),
        }
    }
}

impl From<TgradeMsg> for CosmosMsg<TgradeMsg> {
    fn from(msg: TgradeMsg) -> CosmosMsg<TgradeMsg> {
        CosmosMsg::Custom(msg)
    }
}

impl From<PrivilegeMsg> for TgradeMsg {
    fn from(msg: PrivilegeMsg) -> TgradeMsg {
        TgradeMsg::Privilege(msg)
    }
}

impl From<PrivilegeMsg> for CosmosMsg<TgradeMsg> {
    fn from(msg: PrivilegeMsg) -> CosmosMsg<TgradeMsg> {
        CosmosMsg::Custom(TgradeMsg::from(msg))
    }
}

impl From<ConsensusParams> for CosmosMsg<TgradeMsg> {
    fn from(params: ConsensusParams) -> CosmosMsg<TgradeMsg> {
        CosmosMsg::Custom(TgradeMsg::ConsensusParams(params))
    }
}

impl CustomMsg for TgradeMsg {}
