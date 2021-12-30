mod error;
pub mod msg;
pub mod state;

pub use error::ContractError;

use cosmwasm_std::{Addr, BlockInfo, Deps, DepsMut, Env, MessageInfo, Order, StdResult};
use cw3::{
    Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse,
    VoterResponse,
};
use cw_storage_plus::Bound;
use cw_utils::maybe_addr;
use serde::de::DeserializeOwned;
use serde::Serialize;
use state::{
    next_id, proposals, Ballot, Config, Proposal, ProposalListResponse, ProposalResponse, Votes,
    VotingRules, BALLOTS, CONFIG,
};
use tg4::Tg4Contract;
use tg_bindings::TgradeMsg;
use tg_utils::Expiration;

type Response = cosmwasm_std::Response<TgradeMsg>;

pub fn instantiate(
    deps: DepsMut,
    rules: VotingRules,
    group_addr: &str,
) -> Result<Response, ContractError> {
    let group_contract = Tg4Contract(deps.api.addr_validate(group_addr).map_err(|_| {
        ContractError::InvalidGroup {
            addr: group_addr.to_owned(),
        }
    })?);

    let cfg = Config {
        rules,
        group_contract,
    };

    cfg.rules.validate()?;
    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default())
}

pub fn propose<P>(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: P,
) -> Result<Response, ContractError>
where
    P: DeserializeOwned + Serialize,
{
    let cfg = CONFIG.load(deps.storage)?;

    // Only members of the multisig can create a proposal
    // Additional check if weight >= 1
    let vote_power = cfg
        .group_contract
        .is_voting_member(&deps.querier, info.sender.as_str())?;

    // calculate expiry time
    let expires =
        Expiration::at_timestamp(env.block.time.plus_seconds(cfg.rules.voting_period_secs()));

    // create a proposal
    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires,
        proposal,
        status: Status::Open,
        votes: Votes::yes(vote_power),
        rules: cfg.rules,
        total_weight: cfg.group_contract.total_weight(&deps.querier)?,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    proposals().save(deps.storage, id, &prop)?;

    // add the first yes vote from voter
    let ballot = Ballot {
        weight: vote_power,
        vote: Vote::Yes,
    };
    BALLOTS.save(deps.storage, (id, &info.sender), &ballot)?;

    let resp = msg::ProposalCreationResponse { proposal_id: id };

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status))
        .set_data(cosmwasm_std::to_binary(&resp)?))
}

pub fn vote<P>(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response, ContractError>
where
    P: Serialize + DeserializeOwned,
{
    // ensure proposal exists and can be voted on
    let mut prop = proposals().load(deps.storage, proposal_id)?;
    if prop.status != Status::Open {
        return Err(ContractError::NotOpen {});
    }
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // use a snapshot of "start of proposal"
    // Must be a member of voting group and have voting power >= 1
    let cfg = CONFIG.load(deps.storage)?;
    let vote_power =
        cfg.group_contract
            .was_voting_member(&deps.querier, &info.sender, prop.start_height)?;

    // cast vote if no vote previously cast
    BALLOTS.update(deps.storage, (proposal_id, &info.sender), |bal| match bal {
        Some(_) => Err(ContractError::AlreadyVoted {}),
        None => Ok(Ballot {
            weight: vote_power,
            vote,
        }),
    })?;

    // update vote tally
    prop.votes.add_vote(vote, vote_power);
    prop.update_status(&env.block);
    proposals::<P>().save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn close<P>(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError>
where
    P: Serialize + DeserializeOwned,
{
    // anyone can trigger this if the vote passed

    let mut prop = proposals::<P>().load(deps.storage, proposal_id)?;
    if [Status::Executed, Status::Rejected, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;
    proposals::<P>().save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn query_rules(deps: Deps) -> StdResult<VotingRules> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.rules)
}

pub fn query_proposal<P>(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse<P>>
where
    P: Serialize + DeserializeOwned,
{
    let prop = proposals().load(deps.storage, id)?;
    let status = prop.current_status(&env.block);
    let rules = prop.rules;
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules,
        total_weight: prop.total_weight,
        votes: prop.votes,
    })
}

fn map_proposal<P>(
    block: &BlockInfo,
    item: StdResult<(u64, Proposal<P>)>,
) -> StdResult<ProposalResponse<P>> {
    let (id, prop) = item?;
    let status = prop.current_status(block);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        proposal: prop.proposal,
        status,
        expires: prop.expires,
        rules: prop.rules,
        total_weight: prop.total_weight,
        votes: prop.votes,
    })
}

pub fn list_proposals<P>(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<ProposalListResponse<P>>
where
    P: Serialize + DeserializeOwned,
{
    let start = start_after.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = proposals()
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

pub fn reverse_proposals<P>(
    deps: Deps,
    env: Env,
    start_before: Option<u64>,
    limit: usize,
) -> StdResult<ProposalListResponse<P>>
where
    P: Serialize + DeserializeOwned,
{
    let end = start_before.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = proposals()
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

pub fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = BALLOTS.may_load(deps.storage, (proposal_id, &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        voter,
        vote: b.vote,
        weight: b.weight,
    });
    Ok(VoteResponse { vote })
}

pub fn list_votes(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: usize,
) -> StdResult<VoteListResponse> {
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let votes: StdResult<Vec<_>> = BALLOTS
        .prefix(proposal_id)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (voter, ballot) = item?;
            Ok(VoteInfo {
                voter: voter.into(),
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

pub fn query_voter(deps: Deps, voter: String) -> StdResult<VoterResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voter_addr = deps.api.addr_validate(&voter)?;
    let weight = cfg.group_contract.is_member(&deps.querier, &voter_addr)?;

    Ok(VoterResponse { weight })
}

pub fn list_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voters = cfg
        .group_contract
        .list_members(&deps.querier, start_after, limit)?
        .into_iter()
        .map(|member| VoterDetail {
            addr: member.addr,
            weight: member.weight,
        })
        .collect();
    Ok(VoterListResponse { voters })
}

pub fn query_group_contract(deps: Deps) -> StdResult<Addr> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.group_contract.addr())
}
