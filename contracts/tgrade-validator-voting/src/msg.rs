use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Binary;
use cw3::Vote;

use tg_voting_contract::state::VotingRules;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    pub rules: VotingRules,
    // this is the group contract that contains the member list
    pub group_addr: String,
}

// TODO: add some T variants? Maybe good enough as fixed Empty for now
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Propose {
        title: String,
        description: String,
        proposal: ValidatorProposal,
    },
    Vote {
        proposal_id: u64,
        vote: Vote,
    },
    Execute {
        proposal_id: u64,
    },
    Close {
        proposal_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidatorProposal {
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
    CancelUpgrade {},
    /// all code ids that should be pinned in cache for high performance
    PinCodes(Vec<u64>),
    /// all code ids that should be removed from cache to free space
    UnpinCodes(Vec<u64>),
    UpdateConsensusBlockParams {
        /// Maximum number of bytes (over all tx) to be included in a block
        max_bytes: Option<i64>,
        /// Maximum gas (over all tx) to be executed in one block.
        /// If set, more txs may be included in a block, but when executing, all tx after this is limit
        /// are consumed will immediately error
        max_gas: Option<i64>,
    },
    UpdateConsensusEvidenceParams {
        /// Max age of evidence, in blocks.
        max_age_num_blocks: Option<i64>,
        /// Max age of evidence, in seconds.
        /// It should correspond with an app's "unbonding period"
        max_age_duration: Option<i64>,
        /// Maximum number of bytes of evidence to be included in a block
        max_bytes: Option<i64>,
    },
    MigrateContract {
        /// the contract address to be migrated
        contract: String,
        /// a reference to the new WASM code that it should be migrated to
        code_id: u64,
        /// encoded message to be passed to perform the migration
        migrate_msg: Binary,
    },
}

// We can also add this as a cw3 extension
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return VotingRules
    Rules {},
    /// Returns ProposalResponse
    Proposal { proposal_id: u64 },
    /// Returns ProposalListResponse
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns ProposalListResponse
    ReverseProposals {
        start_before: Option<u64>,
        limit: Option<u32>,
    },
    /// Returns VoteResponse
    Vote { proposal_id: u64, voter: String },
    /// Returns VoteListResponse
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns VoterResponse
    Voter { address: String },
    /// Returns VoterListResponse
    ListVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns address of current's group contract
    GroupContract {},
}
