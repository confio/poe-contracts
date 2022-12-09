use crate::{
    execute_text, list_proposals, list_text_proposals, list_voters, list_votes,
    list_votes_by_voter, propose, query_group_contract, query_proposal, query_rules, query_vote,
    query_voter, reverse_proposals, state::VotingRules, ContractError, Response,
};
use cosmwasm_std::{from_slice, to_binary, CustomQuery};
use tg3::Vote;
use tg_bindings::TgradeQuery;

use super::*;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub rules: VotingRules,
    pub group_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Proposal {
    Text {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
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
    /// Returns VoteListResponse
    ListVotesByVoter {
        voter: String,
        start_after: Option<u64>,
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
    /// Returns ProposalListResponse
    ListTextProposals {
        start_after: Option<u64>,
        limit: usize,
    },
}

pub struct VotingContract;

impl Contract<TgradeMsg, TgradeQuery> for VotingContract {
    fn instantiate(
        &self,
        deps: DepsMut<TgradeQuery>,
        _env: Env,
        _info: MessageInfo,
        msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        let msg: InstantiateMsg = from_slice(&msg)?;

        crate::instantiate(deps, msg.rules, &msg.group_addr).map_err(anyhow::Error::from)
    }

    fn execute(
        &self,
        deps: DepsMut<TgradeQuery>,
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
            Vote { proposal_id, vote } => {
                crate::vote::<Proposal, TgradeQuery>(deps, env, info, proposal_id, vote)
            }
            Execute { proposal_id } => execute(deps, env, info, proposal_id),
            Close { proposal_id } => {
                crate::close::<Proposal, TgradeQuery>(deps, env, info, proposal_id)
            }
        }
        .map_err(anyhow::Error::from)
    }

    fn query(&self, deps: Deps<TgradeQuery>, env: Env, msg: Vec<u8>) -> anyhow::Result<Binary> {
        let msg: QueryMsg = from_slice(&msg)?;

        use QueryMsg::*;
        match msg {
            Rules {} => to_binary(&query_rules(deps)?),
            ListVoters { start_after, limit } => to_binary(&list_voters(deps, start_after, limit)?),
            Proposal { proposal_id } => to_binary(&query_proposal::<self::Proposal, TgradeQuery>(
                deps,
                env,
                proposal_id,
            )?),
            Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
            ListProposals { start_after, limit } => to_binary(&list_proposals::<
                self::Proposal,
                TgradeQuery,
            >(
                deps, env, start_after, limit
            )?),
            ReverseProposals {
                start_before,
                limit,
            } => to_binary(&reverse_proposals::<self::Proposal, TgradeQuery>(
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
            ListVotesByVoter {
                voter,
                start_after,
                limit,
            } => to_binary(&list_votes_by_voter(deps, voter, start_after, limit)?),
            Voter { address } => to_binary(&query_voter(deps, address)?),
            GroupContract {} => to_binary(&query_group_contract(deps)?),
            ListTextProposals { start_after, limit } => {
                to_binary(&list_text_proposals(deps, start_after, limit)?)
            }
        }
        .map_err(anyhow::Error::from)
    }

    fn sudo(
        &self,
        _deps: DepsMut<TgradeQuery>,
        _env: Env,
        _msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        unimplemented!()
    }

    fn reply(
        &self,
        _deps: DepsMut<TgradeQuery>,
        _env: Env,
        _msg: cosmwasm_std::Reply,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        unimplemented!()
    }

    fn migrate(
        &self,
        _deps: DepsMut<TgradeQuery>,
        _env: Env,
        _msg: Vec<u8>,
    ) -> anyhow::Result<cosmwasm_std::Response<TgradeMsg>> {
        unimplemented!()
    }
}

fn execute<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed
    let prop = crate::mark_executed::<Proposal>(deps.storage, env, proposal_id)?;
    execute_text(deps, proposal_id, prop)?;

    Ok(Response::new()
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}
