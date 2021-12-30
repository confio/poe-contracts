#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, StdResult};

use cw2::set_contract_version;
use cw3::Status;
use tg_bindings::TgradeMsg;

use crate::msg::{ExecuteMsg, InstantiateMsg, Proposal, QueryMsg};
use crate::ContractError;

use tg_voting_contract::state::{proposals, CONFIG as VOTING_CONFIG};
use tg_voting_contract::{
    close as execute_close, list_proposals, list_voters, list_votes, propose, query_group_contract,
    query_proposal, query_rules, query_vote, query_voter, reverse_proposals, vote as execute_vote,
};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_community-pool";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    tg_voting_contract::instantiate(deps, msg.rules, &msg.group_addr).map_err(ContractError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Propose {
            title,
            description,
            proposal,
        } => execute_propose(deps, env, info, title, description, proposal),
        ExecuteMsg::Vote { proposal_id, vote } => {
            execute_vote::<Proposal>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => {
            execute_close::<Proposal>(deps, env, info, proposal_id).map_err(ContractError::from)
        }
        ExecuteMsg::WithdrawEngagementRewards {} => execute_withdraw_engagement_rewards(deps, info),
        ExecuteMsg::DistributeFunds {} => Ok(Response::new()),
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    proposal: Proposal,
) -> Result<Response, ContractError> {
    use Proposal::*;

    match &proposal {
        SendProposal { to_addr, .. } => {
            deps.api.addr_validate(to_addr)?;
        }
    }

    propose(deps, env, info, title, description, proposal).map_err(ContractError::from)
}

pub fn execute_send_proposal(to_address: String, amount: Coin) -> Result<Response, ContractError> {
    let resp = Response::new()
        .add_attribute("proposal", "send_proposal")
        .add_attribute("to_addr", &to_address)
        .add_attribute("amount", amount.amount.to_string())
        .add_attribute("denom", &amount.denom);

    let msg = BankMsg::Send {
        to_address,
        amount: vec![amount],
    };

    let resp = resp.add_message(msg);

    Ok(resp)
}

pub fn execute_execute(
    deps: DepsMut,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    use Proposal::*;

    // anyone can trigger this if the vote passed
    let prop = proposals::<Proposal>().load(deps.storage, proposal_id)?;

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // dispatch all proposed messages
    let resp = match prop.proposal {
        SendProposal { to_addr, amount } => execute_send_proposal(to_addr, amount)?,
    };

    let resp = resp
        .add_attribute("action", "execute")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("sender", info.sender.to_string());

    Ok(resp)
}

pub fn execute_withdraw_engagement_rewards(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let group_contract = VOTING_CONFIG.load(deps.storage)?.group_contract;

    let msg = group_contract.encode_raw_msg(to_binary(
        &tg4_engagement::msg::ExecuteMsg::WithdrawFunds {
            owner: None,
            receiver: None,
        },
    )?)?;

    Ok(Response::new()
        .add_submessage(msg)
        .add_attribute("action", "withdraw_engagement_rewards")
        .add_attribute("sender", info.sender))
}

fn align_limit(limit: Option<u32>) -> usize {
    // settings for pagination
    const MAX_LIMIT: u32 = 30;
    const DEFAULT_LIMIT: u32 = 10;

    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as _
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;

    match msg {
        Rules {} => to_binary(&query_rules(deps)?),
        Proposal { proposal_id } => to_binary(&query_proposal::<crate::msg::Proposal>(
            deps,
            env,
            proposal_id,
        )?),
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => to_binary(&list_proposals::<crate::msg::Proposal>(
            deps,
            env,
            start_after,
            align_limit(limit),
        )?),
        ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<crate::msg::Proposal>(
            deps,
            env,
            start_before,
            align_limit(limit),
        )?),
        ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(
            deps,
            proposal_id,
            start_after,
            align_limit(limit),
        )?),
        Voter { address } => to_binary(&query_voter(deps, address)?),
        ListVoters { start_after, limit } => to_binary(&list_voters(deps, start_after, limit)?),
        GroupContract {} => to_binary(&query_group_contract(deps)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::{
        from_slice,
        testing::{mock_dependencies, mock_env},
        Addr, Decimal,
    };
    use tg_voting_contract::state::VotingRules;

    #[test]
    fn query_group_contract() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let rules = VotingRules {
            voting_period: 1,
            quorum: Decimal::percent(50),
            threshold: Decimal::percent(50),
            allow_end_early: false,
        };
        let group_addr = "group_addr";
        instantiate(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: Addr::unchecked("sender"),
                funds: vec![],
            },
            InstantiateMsg {
                rules,
                group_addr: group_addr.to_owned(),
            },
        )
        .unwrap();

        let query: Addr =
            from_slice(&query(deps.as_ref(), env, QueryMsg::GroupContract {}).unwrap()).unwrap();
        assert_eq!(query, Addr::unchecked(group_addr));
    }
}
