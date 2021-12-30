#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_slice, to_binary, to_vec, Binary, ContractInfoResponse, ContractResult, Deps, DepsMut,
    Empty, Env, MessageInfo, QueryRequest, StdResult, SystemResult, WasmMsg, WasmQuery,
};

use cw2::set_contract_version;
use cw3::Status;
use tg_bindings::{
    request_privileges, BlockParams, ConsensusParams, EvidenceParams, GovProposal, Privilege,
    PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, ValidatorProposal};
use crate::ContractError;

use tg_voting_contract::state::proposals;
use tg_voting_contract::{
    close as execute_close, list_proposals, list_voters, list_votes, propose as execute_propose,
    query_group_contract, query_proposal, query_rules, query_vote, query_voter, reverse_proposals,
    vote as execute_vote,
};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_validator_voting_proposals";
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
    use ExecuteMsg::*;

    match msg {
        Propose {
            title,
            description,
            proposal,
        } => {
            // Migrate contract needs confirming that sender (proposing member) is an admin
            // of target contract
            if let ValidatorProposal::MigrateContract { ref contract, .. } = proposal {
                confirm_admin_in_contract(deps.as_ref(), &env, contract.clone())?;
            };
            execute_propose(deps, env, info, title, description, proposal)
                .map_err(ContractError::from)
        }
        Vote { proposal_id, vote } => {
            execute_vote::<ValidatorProposal>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        Execute { proposal_id } => execute_execute(deps, info, proposal_id),
        Close { proposal_id } => execute_close::<ValidatorProposal>(deps, env, info, proposal_id)
            .map_err(ContractError::from),
    }
}

fn confirm_admin_in_contract(
    deps: Deps,
    env: &Env,
    contract_addr: String,
) -> Result<(), ContractError> {
    use ContractError::*;

    let contract_query = QueryRequest::<Empty>::Wasm(WasmQuery::ContractInfo { contract_addr });
    let response = match deps.querier.raw_query(&to_vec(&contract_query)?) {
        SystemResult::Err(system_err) => {
            Err(System(format!("Querier system error: {}", system_err)))
        }
        SystemResult::Ok(ContractResult::Err(contract_err)) => Err(Contract(format!(
            "Querier contract error: {}",
            contract_err
        ))),
        SystemResult::Ok(ContractResult::Ok(value)) => Ok(value),
    }?;

    let response = from_slice::<Option<ContractInfoResponse>>(&response)?
        .ok_or_else(|| Contract("Contract query provided no results!".to_owned()))?;

    if let Some(admin) = response.admin {
        if admin == env.contract.address {
            return Ok(());
        }
    }

    Err(Unauthorized(
        "Validator Proposal contract is not an admin of contract proposed to migrate".to_owned(),
    ))
}

pub fn execute_execute(
    deps: DepsMut,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    use ValidatorProposal::*;
    // anyone can trigger this if the vote passed

    let mut proposal = proposals::<ValidatorProposal>().load(deps.storage, proposal_id)?;

    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if proposal.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    let prop = proposal.clone();
    let msg = match prop.proposal {
        RegisterUpgrade { name, height, info } => SubMsg::new(TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::RegisterUpgrade { name, height, info },
        }),
        CancelUpgrade {} => SubMsg::new(TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::CancelUpgrade {},
        }),
        PinCodes(code_ids) => SubMsg::new(TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::PinCodes { code_ids },
        }),
        UnpinCodes(code_ids) => SubMsg::new(TgradeMsg::ExecuteGovProposal {
            title: prop.title,
            description: prop.description,
            proposal: GovProposal::UnpinCodes { code_ids },
        }),
        UpdateConsensusBlockParams { max_bytes, max_gas } => {
            SubMsg::new(TgradeMsg::ConsensusParams(ConsensusParams {
                block: Some(BlockParams { max_bytes, max_gas }),
                evidence: None,
            }))
        }
        UpdateConsensusEvidenceParams {
            max_age_num_blocks,
            max_age_duration,
            max_bytes,
        } => SubMsg::new(TgradeMsg::ConsensusParams(ConsensusParams {
            block: None,
            evidence: Some(EvidenceParams {
                max_age_num_blocks,
                max_age_duration,
                max_bytes,
            }),
        })),
        MigrateContract {
            contract,
            code_id,
            migrate_msg,
        } => SubMsg::new(WasmMsg::Migrate {
            contract_addr: contract,
            new_code_id: code_id,
            msg: migrate_msg,
        }),
    };

    // set it to executed
    proposal.status = Status::Executed;
    proposals::<ValidatorProposal>().save(deps.storage, proposal_id, &proposal)?;

    Ok(Response::new()
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_submessage(msg))
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
        Proposal { proposal_id } => to_binary(&query_proposal::<ValidatorProposal>(
            deps,
            env,
            proposal_id,
        )?),
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => to_binary(&list_proposals::<ValidatorProposal>(
            deps,
            env,
            start_after,
            align_limit(limit),
        )?),
        ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<ValidatorProposal>(
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: TgradeSudoMsg) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(deps, change)),
        _ => Err(ContractError::UnsupportedSudoType {}),
    }
}

fn privilege_change(_deps: DepsMut, change: PrivilegeChangeMsg) -> Response {
    match change {
        PrivilegeChangeMsg::Promoted {} => {
            let msgs = request_privileges(&[
                Privilege::GovProposalExecutor,
                Privilege::ConsensusParamChanger,
            ]);
            Response::new().add_submessages(msgs)
        }
        PrivilegeChangeMsg::Demoted {} => Response::new(),
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info},
        Addr, CosmosMsg, Decimal, SubMsg,
    };
    use tg_utils::Expiration;
    use tg_voting_contract::state::{Proposal, Votes, VotingRules};

    use super::*;

    #[derive(serde::Serialize)]
    struct DummyMigrateMsg {}

    #[test]
    fn register_migrate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "MigrateContract".to_owned(),
                    description: "MigrateContract testing proposal".to_owned(),
                    start_height: env.block.height,
                    // Aaaaall the seconds
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::MigrateContract {
                        contract: "target_contract".to_owned(),
                        code_id: 13,
                        migrate_msg: to_binary(&DummyMigrateMsg {}).unwrap(),
                    },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(WasmMsg::Migrate {
                contract_addr: "target_contract".to_owned(),
                new_code_id: 13,
                msg: Binary(vec![123, 125])
            })]
        );
    }

    #[test]
    fn register_cancel_upgrade() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "CancelUpgrade".to_owned(),
                    description: "CancelUpgrade testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::CancelUpgrade {},
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "CancelUpgrade".to_owned(),
                    description: "CancelUpgrade testing proposal".to_owned(),
                    proposal: GovProposal::CancelUpgrade {}
                }
            ))]
        );
    }

    #[test]
    fn register_pin_codes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "PinCodes".to_owned(),
                    description: "PinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::PinCodes(vec![]),
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "PinCodes".to_owned(),
                    description: "PinCodes testing proposal".to_owned(),
                    proposal: GovProposal::PinCodes { code_ids: vec![] }
                }
            ))]
        );
    }

    #[test]
    fn register_unpin_codes() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::UnpinCodes(vec![]),
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    proposal: GovProposal::UnpinCodes { code_ids: vec![] }
                }
            ))]
        );
    }

    #[test]
    fn update_consensus_block_params() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::UpdateConsensusBlockParams {
                        max_bytes: Some(120),
                        max_gas: Some(240),
                    },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(TgradeMsg::ConsensusParams(
                ConsensusParams {
                    block: Some(BlockParams {
                        max_bytes: Some(120),
                        max_gas: Some(240),
                    }),
                    evidence: None,
                }
            )))]
        );
    }

    #[test]
    fn update_consensus_evidence_params() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::UpdateConsensusEvidenceParams {
                        max_age_num_blocks: Some(10),
                        max_age_duration: Some(100),
                        max_bytes: Some(256),
                    },
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_weight: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(TgradeMsg::ConsensusParams(
                ConsensusParams {
                    block: None,
                    evidence: Some(EvidenceParams {
                        max_age_num_blocks: Some(10),
                        max_age_duration: Some(100),
                        max_bytes: Some(256),
                    }),
                }
            )))]
        );
    }

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
