use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Binary, Coin};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
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
    // ClientUpdateProposal is a governance proposal. If it passes, the substitute
    // client's latest consensus state is copied over to the subject client. The proposal
    // handler may fail if the subject and the substitute do not match in client and
    // chain parameters (with exception to latest height, frozen height, and chain-id).
    ClientUpdate {
        // the client identifier for the client to be updated if the proposal passes
        subject_client_id: String,
        // the substitute client identifier for the client standing in for the subject
        // client
        substitute_client_id: String,
    },
    // UpgradeProposal is a gov Content type for initiating an IBC breaking
    // upgrade.
    Upgrade {
        plan: UpgradePlan,
        // An UpgradedClientState must be provided to perform an IBC breaking upgrade.
        // This will make the chain commit to the correct upgraded (self) client state
        // before the upgrade occurs, so that connecting chains can verify that the
        // new upgraded client is valid by verifying a proof on the previous version
        // of the chain. This will allow IBC connections to persist smoothly across
        // planned chain upgrades
        upgraded_client_state: ProtoAny,
    },
}

// Plan specifies information about a planned upgrade and when it should occur.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, JsonSchema, Debug)]
pub struct UpgradePlan {
    // Sets the name for the upgrade. This name will be used by the upgraded
    // version of the software to apply any special "on-upgrade" commands during
    // the first BeginBlock method after the upgrade is applied. It is also used
    // to detect whether a software version can handle a given upgrade. If no
    // upgrade handler with this name has been set in the software, it will be
    // assumed that the software is out-of-date when the upgrade Time or Height is
    // reached and the software will exit.
    pub name: String,
    // The height at which the upgrade must be performed.
    // Only used if Time is not set.
    pub height: u64,
    // Any application specific upgrade info to be included on-chain
    // such as a git commit that validators could automatically upgrade to
    pub info: String,
}

/// ParamChange defines an individual parameter change, for use in ParameterChangeProposal.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, JsonSchema, Debug)]
pub struct ParamChange {
    pub subspace: String,
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProtoAny {
    type_url: String,
    value: Binary,
}
