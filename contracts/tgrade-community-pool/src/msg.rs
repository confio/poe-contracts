use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Coin;
use cw3::Vote;

use tg_voting_contract::state::VotingRules;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub rules: VotingRules,
    // this is the group contract that contains the member list
    pub group_addr: String,
}

/// The type of proposal to vote on
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Proposal {
    /// Proposal to send some tokens from community pool contract to given address
    SendProposal {
        /// Address to send tokens to
        to_addr: String,
        /// Funds to send
        amount: Coin,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Propose {
        title: String,
        description: String,
        proposal: Proposal,
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
    /// The Community Pool may be a participant in engagement and end up
    /// receiving engagement rewards. This endpoint can be used to withdraw
    /// those. Anyone can call it.
    WithdrawEngagementRewards {},
    /// Message comming from valset on funds distribution, just takes funds
    /// send with message and does nothing
    DistributeFunds {},
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
