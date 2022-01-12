use cosmwasm_std::{from_slice, to_binary};
use cw3::Vote;

use crate::{
    list_proposals, list_voters, list_votes, propose, query_group_contract, query_proposal,
    query_rules, query_vote, query_voter, reverse_proposals, state::VotingRules,
};

use super::*;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub rules: VotingRules,
    pub group_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Propose {
        title: String,
        description: String,
        proposal: String,
    },
    Vote {
        proposal_id: u64,
        vote: Vote,
    },
    Close {
        proposal_id: u64,
    },
}

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
        limit: usize,
    },
    /// Returns ProposalListResponse
    ReverseProposals {
        start_before: Option<u64>,
        limit: usize,
    },
    /// Returns VoteResponse
    Vote { proposal_id: u64, voter: String },
    /// Returns VoteListResponse
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: usize,
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

pub struct VotingContract;

impl Contract<TgradeMsg> for VotingContract {
    fn instantiate(
        &self,
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        let msg: InstantiateMsg = from_slice(&msg)?;

        crate::instantiate(deps, msg.rules, &msg.group_addr).map_err(anyhow::Error::from)
    }

    fn execute(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        let msg: ExecuteMsg = from_slice(&msg)?;

        use ExecuteMsg::*;
        match msg {
            Propose {
                title,
                description,
                proposal,
            } => propose(deps, env, info, title, description, proposal),
            Close { proposal_id } => crate::close::<String>(deps, env, info, proposal_id),
            Vote { proposal_id, vote } => crate::vote::<String>(deps, env, info, proposal_id, vote),
        }
        .map_err(anyhow::Error::from)
    }

    fn query(&self, deps: Deps, env: Env, msg: Vec<u8>) -> anyhow::Result<Binary> {
        let msg: QueryMsg = from_slice(&msg)?;

        use QueryMsg::*;
        match msg {
            Rules {} => to_binary(&query_rules(deps)?),
            ListVoters { start_after, limit } => to_binary(&list_voters(deps, start_after, limit)?),
            Proposal { proposal_id } => {
                to_binary(&query_proposal::<String>(deps, env, proposal_id)?)
            }
            Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
            ListProposals { start_after, limit } => {
                to_binary(&list_proposals::<String>(deps, env, start_after, limit)?)
            }
            ReverseProposals {
                start_before,
                limit,
            } => to_binary(&reverse_proposals::<String>(
                deps,
                env,
                start_before,
                limit,
            )?),
            ListVotes {
                proposal_id,
                start_after,
                limit,
            } => to_binary(&list_votes(deps, proposal_id, start_after, limit)?),
            Voter { address } => to_binary(&query_voter(deps, address)?),
            GroupContract {} => to_binary(&query_group_contract(deps)?),
        }
        .map_err(anyhow::Error::from)
    }

    fn sudo(
        &self,
        _deps: DepsMut,
        _env: Env,
        _msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        unimplemented!()
    }

    fn reply(
        &self,
        _deps: DepsMut,
        _env: Env,
        _msg: cosmwasm_std::Reply,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        unimplemented!()
    }

    fn migrate(
        &self,
        _deps: DepsMut,
        _env: Env,
        _msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        unimplemented!()
    }
}
