use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tg3::Vote;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalCreationResponse {
    pub proposal_id: u64,
}

// TODO: Remove all underneath when cw-plus equivalent is updated about proposal_id field
// https://github.com/CosmWasm/cw-plus/issues/647
/// Returns the vote (opinion as well as weight counted) as well as
/// the address of the voter who submitted it
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteInfo {
    pub proposal_id: u64,
    pub voter: String,
    pub vote: Vote,
    pub weight: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteListResponse {
    pub votes: Vec<VoteInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteResponse {
    pub vote: Option<VoteInfo>,
}
