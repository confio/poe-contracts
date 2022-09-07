pub mod ballots;
pub mod error;
pub mod msg;
#[cfg(test)]
mod multitest;
pub mod state;

use serde::de::DeserializeOwned;
use serde::Serialize;

use ballots::ballots;
pub use error::ContractError;
use state::{
    next_id, proposals, Config, Proposal, ProposalListResponse, ProposalResponse,
    TextProposalListResponse, Votes, VotingRules, CONFIG, TEXT_PROPOSALS,
};

use cosmwasm_std::{
    Addr, BlockInfo, CustomQuery, Deps, DepsMut, Env, MessageInfo, Order, StdResult, Storage,
};
use cw_storage_plus::Bound;
use cw_utils::maybe_addr;
use tg3::{
    Status, Vote, VoteInfo, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse,
    VoterResponse,
};
use tg4::{Member, Tg4Contract};
use tg_bindings::TgradeMsg;
use tg_utils::Expiration;

type Response = cosmwasm_std::Response<TgradeMsg>;

pub fn instantiate<Q: CustomQuery>(
    deps: DepsMut<Q>,
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

pub fn propose<P, Q: CustomQuery>(
    deps: DepsMut<Q>,
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
    // Additional check if points >= 1
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
        created_by: info.sender.to_string(),
        start_height: env.block.height,
        expires,
        proposal,
        status: Status::Open,
        votes: Votes::yes(vote_power),
        rules: cfg.rules,
        total_points: cfg.group_contract.total_points(&deps.querier)?,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    proposals().save(deps.storage, id, &prop)?;

    // add the first yes vote from voter
    ballots().create_ballot(deps.storage, &info.sender, id, vote_power, Vote::Yes)?;

    let resp = msg::ProposalCreationResponse { proposal_id: id };

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status))
        .set_data(cosmwasm_std::to_binary(&resp)?))
}

pub fn vote<P, Q: CustomQuery>(
    deps: DepsMut<Q>,
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

    if ![Status::Open, Status::Passed, Status::Rejected].contains(&prop.status) {
        return Err(ContractError::NotOpen {});
    }

    // if they are not expired
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
    ballots().create_ballot(deps.storage, &info.sender, proposal_id, vote_power, vote)?;

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

/// Checks if a given proposal is passed and can then be executed, and returns it.
/// Notice that this call is mutable, so, better execute the returned proposal after this succeeds,
/// as you you wouldn't be able to execute it in the future (If the contract call errors, this status
/// change will be reverted / ignored).
pub fn mark_executed<P>(
    storage: &mut dyn Storage,
    env: Env,
    proposal_id: u64,
) -> Result<Proposal<P>, ContractError>
where
    P: Serialize + DeserializeOwned,
{
    let mut proposal = proposals::<P>().load(storage, proposal_id)?;
    // Update Status
    proposal.update_status(&env.block);
    // We allow execution even after the proposal "expiration" as long as all votes come in before
    // that point. If it was approved on time, it can be executed any time.
    if proposal.current_status(&env.block) != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // Set it to executed
    proposal.status = Status::Executed;
    proposals::<P>().save(storage, proposal_id, &proposal)?;
    Ok(proposal)
}

pub fn execute_text<P, Q: CustomQuery>(
    deps: DepsMut<Q>,
    id: u64,
    proposal: Proposal<P>,
) -> Result<(), ContractError>
where
    P: Serialize + DeserializeOwned,
{
    TEXT_PROPOSALS.save(deps.storage, id, &proposal.into())?;

    Ok(())
}

pub fn close<P, Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError>
where
    P: Serialize + DeserializeOwned,
{
    // anyone can trigger this if the vote passed

    let mut prop = proposals().load(deps.storage, proposal_id)?;

    if prop.status == Status::Rejected {
        return Err(ContractError::NotOpen {});
    }

    prop.update_status(&env.block);

    if [Status::Executed, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    prop.status = Status::Rejected;
    proposals::<P>().save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn query_rules<Q: CustomQuery>(deps: Deps<Q>) -> StdResult<VotingRules> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.rules)
}

pub fn query_proposal<P, Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    id: u64,
) -> StdResult<ProposalResponse<P>>
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
        created_by: prop.created_by,
        status,
        expires: prop.expires,
        rules,
        total_points: prop.total_points,
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
        created_by: prop.created_by,
        status,
        expires: prop.expires,
        rules: prop.rules,
        total_points: prop.total_points,
        votes: prop.votes,
    })
}

pub fn list_proposals<P, Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<ProposalListResponse<P>>
where
    P: Serialize + DeserializeOwned,
{
    let start = start_after.map(Bound::exclusive);
    let props: StdResult<Vec<_>> = proposals()
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

pub fn list_text_proposals<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<TextProposalListResponse> {
    let start = start_after.map(Bound::exclusive);
    let props: StdResult<Vec<_>> = TEXT_PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|r| r.map(|(_, p)| p))
        .collect();

    Ok(TextProposalListResponse { proposals: props? })
}

pub fn reverse_proposals<P, Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    start_before: Option<u64>,
    limit: usize,
) -> StdResult<ProposalListResponse<P>>
where
    P: Serialize + DeserializeOwned,
{
    let end = start_before.map(Bound::exclusive);
    let props: StdResult<Vec<_>> = proposals()
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

pub fn query_vote<Q: CustomQuery>(
    deps: Deps<Q>,
    proposal_id: u64,
    voter: String,
) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = ballots()
        .ballots
        .may_load(deps.storage, (proposal_id, &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        proposal_id,
        voter,
        vote: b.vote,
        points: b.points,
    });
    Ok(VoteResponse { vote })
}

pub fn list_votes<Q: CustomQuery>(
    deps: Deps<Q>,
    proposal_id: u64,
    start_after: Option<String>,
    limit: usize,
) -> StdResult<VoteListResponse> {
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.as_ref().map(Bound::exclusive);

    let votes: StdResult<Vec<_>> = ballots()
        .ballots
        .prefix(proposal_id)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (voter, ballot) = item?;
            Ok(VoteInfo {
                proposal_id,
                voter: voter.into(),
                vote: ballot.vote,
                points: ballot.points,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

pub fn list_votes_by_voter<Q: CustomQuery>(
    deps: Deps<Q>,
    voter: String,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<VoteListResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    // PrimaryKey of that IndexMap is (proposal_id, voter_address) -> (u64, Addr)
    let start = start_after.map(|m| Bound::exclusive((m, voter_addr.clone())));

    let votes: StdResult<Vec<_>> = ballots()
        .ballots
        .idx
        .voter
        .prefix(voter_addr)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let ((proposal_id, _), ballot) = item?;
            Ok(VoteInfo {
                proposal_id,
                voter: ballot.voter.into(),
                vote: ballot.vote,
                points: ballot.points,
            })
        })
        .collect();

    Ok(VoteListResponse { votes: votes? })
}

pub fn query_voter<Q: CustomQuery>(deps: Deps<Q>, voter: String) -> StdResult<VoterResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voter_addr = deps.api.addr_validate(&voter)?;
    let points = cfg.group_contract.is_member(&deps.querier, &voter_addr)?;

    Ok(VoterResponse { points })
}

pub fn list_voters<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voters = cfg
        .group_contract
        .list_members(&deps.querier, start_after, limit)?
        .into_iter()
        .map(|Member { addr, points, .. }| VoterDetail { addr, points })
        .collect();
    Ok(VoterListResponse { voters })
}

pub fn query_group_contract<Q: CustomQuery>(deps: Deps<Q>) -> StdResult<Addr> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(cfg.group_contract.addr())
}
