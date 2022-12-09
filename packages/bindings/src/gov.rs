use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Binary, Coin};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GovProposal {
    /// Signaling proposal, the text and description field will be recorded
    Text {},
    /// Register an "live upgrade" on the x/upgrade module
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/upgrade/v1beta1/upgrade.proto#L12-L53
    RegisterUpgrade {
        /// Sets the name for the upgrade. This name will be used by the upgraded
        /// version of the software to apply any special "on-upgrade" commands during
        /// the first BeginBlock method after the upgrade is applied.
        name: String,
        /// The height at which the upgrade must be performed.
        /// (Time-based upgrades are not supported due to instability)
        height: u64,
        /// Any application specific upgrade info to be included on-chain
        /// such as a git commit that validators could automatically upgrade to
        info: String,
    },
    /// There can only be one pending upgrade at a given time. This cancels the pending upgrade, if any.
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/upgrade/v1beta1/upgrade.proto#L57-L62
    CancelUpgrade {},
    /// Defines a proposal to change one or more parameters.
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/cosmos/params/v1beta1/params.proto#L9-L27
    ChangeParams(Vec<ParamChange>),
    /// Updates the matching client to set a new trusted header.
    /// This can be used by governance to restore a client that has timed out or forked or otherwise broken.
    /// See https://github.com/cosmos/cosmos-sdk/blob/v0.42.3/proto/ibc/core/client/v1/client.proto#L36-L49
    IbcClientUpdate { client_id: String, header: ProtoAny },

    /// See https://github.com/confio/tgrade/blob/privileged_contracts_5/proto/confio/twasm/v1beta1/proposal.proto
    PromoteToPrivilegedContract { contract: String },
    /// See https://github.com/confio/tgrade/blob/privileged_contracts_5/proto/confio/twasm/v1beta1/proposal.proto
    DemotePrivilegedContract { contract: String },

    /// See https://github.com/CosmWasm/wasmd/blob/master/proto/cosmwasm/wasm/v1beta1/proposal.proto#L32-L54
    InstantiateContract {
        /// the address that is passed to the contract's environment as sender
        run_as: String,
        /// Admin is an optional address that can execute migrations
        admin: String,
        /// the reference to the stored WASM code
        code_id: u64,
        /// metadata to be stored with a contract instance.
        label: String,
        /// json encoded message to be passed to the contract on instantiation
        init_msg: Binary,
        /// coins that are transferred to the contract on instantiation
        funds: Vec<Coin>,
    },
    /// See https://github.com/CosmWasm/wasmd/blob/master/proto/cosmwasm/wasm/v1beta1/proposal.proto#L56-L70
    MigrateContract {
        /// the address that is passed to the contract's environment as sender
        run_as: String,
        /// the contract address to be migrated
        contract: String,
        /// a reference to the new WASM code that it should be migrated to
        code_id: u64,
        /// json encoded message to be passed to the new WASM code to perform the migration
        migrate_msg: Binary,
    },
    /// See https://github.com/CosmWasm/wasmd/blob/master/proto/cosmwasm/wasm/v1beta1/proposal.proto#L72-L82
    SetContractAdmin {
        /// the contract address to be updated
        contract: String,
        /// the account address to become admin of this contract
        new_admin: String,
    },
    /// See https://github.com/CosmWasm/wasmd/blob/master/proto/cosmwasm/wasm/v1beta1/proposal.proto#L84-L93
    ClearContractAdmin {
        /// the contract address to be cleared
        contract: String,
    },
    /// See https://github.com/CosmWasm/wasmd/blob/master/proto/cosmwasm/wasm/v1beta1/proposal.proto#L95-L107
    PinCodes {
        /// all code ideas that should be pinned in cache for high performance
        code_ids: Vec<u64>,
    },
    /// See https://github.com/CosmWasm/wasmd/blob/master/proto/cosmwasm/wasm/v1beta1/proposal.proto#L109-L121
    UnpinCodes {
        /// all code ideas that should be removed from cache to free space
        code_ids: Vec<u64>,
    },
}

/// ParamChange defines an individual parameter change, for use in ParameterChangeProposal.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, JsonSchema, Debug)]
pub struct ParamChange {
    pub subspace: String,
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct ProtoAny {
    type_url: String,
    value: Binary,
}
