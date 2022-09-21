#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, CustomQuery, Deps, DepsMut, Empty, Env, MessageInfo, StdResult, WasmMsg,
};

use cw2::set_contract_version;
use cw_utils::ensure_from_older_version;
use tg_bindings::{
    request_privileges, BlockParams, ConsensusParams, EvidenceParams, GovProposal, Privilege,
    PrivilegeChangeMsg, TgradeMsg, TgradeQuery, TgradeSudoMsg,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, ValidatorProposal};
use crate::ContractError;

use tg_voting_contract::{
    close as execute_close, execute_text, list_proposals, list_text_proposals, list_voters,
    list_votes, list_votes_by_voter, mark_executed, propose as execute_propose,
    query_group_contract, query_proposal, query_rules, query_vote, query_voter, reverse_proposals,
    vote as execute_vote,
};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tgrade_validator_voting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<TgradeQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    tg_voting_contract::instantiate(deps, msg.rules, &msg.group_addr).map_err(ContractError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<TgradeQuery>,
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
            proposal.validate(deps.as_ref(), &env, &title, &description)?;
            execute_propose(deps, env, info, title, description, proposal)
                .map_err(ContractError::from)
        }
        Vote { proposal_id, vote } => {
            execute_vote::<ValidatorProposal, TgradeQuery>(deps, env, info, proposal_id, vote)
                .map_err(ContractError::from)
        }
        Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        Close { proposal_id } => {
            execute_close::<ValidatorProposal, TgradeQuery>(deps, env, info, proposal_id)
                .map_err(ContractError::from)
        }
    }
}

pub fn execute_execute<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    use ValidatorProposal::*;
    // anyone can trigger this if the vote passed
    let proposal = mark_executed::<ValidatorProposal>(deps.storage, env, proposal_id)?;

    let mut res = Response::new();

    match proposal.proposal {
        RegisterUpgrade { name, height, info } => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::RegisterUpgrade { name, height, info },
            })
        }
        CancelUpgrade {} => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::CancelUpgrade {},
            })
        }
        PinCodes(code_ids) => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::PinCodes { code_ids },
            })
        }
        UnpinCodes(code_ids) => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::UnpinCodes { code_ids },
            })
        }
        UpdateConsensusBlockParams { max_bytes, max_gas } => {
            res = res.add_message(TgradeMsg::ConsensusParams(ConsensusParams {
                block: Some(BlockParams { max_bytes, max_gas }),
                evidence: None,
            }))
        }
        UpdateConsensusEvidenceParams {
            max_age_num_blocks,
            max_age_duration,
            max_bytes,
        } => {
            res = res.add_message(TgradeMsg::ConsensusParams(ConsensusParams {
                block: None,
                evidence: Some(EvidenceParams {
                    max_age_num_blocks,
                    max_age_duration,
                    max_bytes,
                }),
            }))
        }
        MigrateContract {
            contract,
            code_id,
            migrate_msg,
        } => {
            res = res.add_message(WasmMsg::Migrate {
                contract_addr: contract,
                new_code_id: code_id,
                msg: migrate_msg,
            })
        }
        Text {} => execute_text(deps, proposal_id, proposal)?,
        ChangeParams(params) => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::ChangeParams(params),
            })
        }
        PromoteToPrivilegedContract { contract } => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::PromoteToPrivilegedContract { contract },
            })
        }
        DemotePrivilegedContract { contract } => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::DemotePrivilegedContract { contract },
            })
        }
        SetContractAdmin {
            contract,
            new_admin,
        } => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::SetContractAdmin {
                    contract,
                    new_admin,
                },
            })
        }
        ClearContractAdmin { contract } => {
            res = res.add_message(TgradeMsg::ExecuteGovProposal {
                title: proposal.title,
                description: proposal.description,
                proposal: GovProposal::ClearContractAdmin { contract },
            })
        }
    };

    Ok(res
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender))
}

fn align_limit(limit: Option<u32>) -> usize {
    // settings for pagination
    const MAX_LIMIT: u32 = 100;
    const DEFAULT_LIMIT: u32 = 30;

    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as _
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TgradeQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;

    match msg {
        Rules {} => to_binary(&query_rules(deps)?),
        Proposal { proposal_id } => to_binary(&query_proposal::<ValidatorProposal, TgradeQuery>(
            deps,
            env,
            proposal_id,
        )?),
        Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        ListProposals { start_after, limit } => {
            to_binary(&list_proposals::<ValidatorProposal, TgradeQuery>(
                deps,
                env,
                start_after,
                align_limit(limit),
            )?)
        }
        ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals::<ValidatorProposal, TgradeQuery>(
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
        ListVotesByVoter {
            voter,
            start_after,
            limit,
        } => to_binary(&list_votes_by_voter(
            deps,
            voter,
            start_after,
            align_limit(limit),
        )?),
        Voter { address } => to_binary(&query_voter(deps, address)?),
        ListVoters { start_after, limit } => to_binary(&list_voters(deps, start_after, limit)?),
        GroupContract {} => to_binary(&query_group_contract(deps)?),
        ListTextProposals { start_after, limit } => {
            to_binary(&list_text_proposals(deps, start_after, align_limit(limit))?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(
    _deps: DepsMut<TgradeQuery>,
    _env: Env,
    msg: TgradeSudoMsg,
) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(change)),
        _ => Err(ContractError::UnsupportedSudoType {}),
    }
}

fn privilege_change(change: PrivilegeChangeMsg) -> Response {
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        from_slice,
        testing::{mock_env, mock_info},
        Addr, CosmosMsg, Decimal, SubMsg,
    };
    use tg_utils::Expiration;
    use tg_voting_contract::state::{proposals, Proposal, Votes, VotingRules};

    use super::*;
    use tg3::Status;
    use tg_bindings::ParamChange;
    use tg_bindings_test::mock_deps_tgrade;

    #[derive(serde::Serialize)]
    struct DummyMigrateMsg {}

    #[test]
    fn register_migrate() {
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "MigrateContract".to_owned(),
                    description: "MigrateContract testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
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
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
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
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "CancelUpgrade".to_owned(),
                    description: "CancelUpgrade testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
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
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
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
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "PinCodes".to_owned(),
                    description: "PinCodes testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
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
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
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
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
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
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
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
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "UpdateConsensusBlockParams".to_owned(),
                    description: "BlockParams testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
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
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
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
    fn change_params() {
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "ChangeParams".to_owned(),
                    description: "Change params testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
                    start_height: env.block.height,
                    expires: Expiration::at_timestamp(env.block.time.plus_seconds(66666)),
                    proposal: ValidatorProposal::ChangeParams(vec![ParamChange {
                        subspace: "foo".to_string(),
                        key: "bar".to_string(),
                        value: "baz".to_string(),
                    }]),
                    status: Status::Passed,
                    rules: VotingRules {
                        voting_period: 1,
                        quorum: Decimal::percent(50),
                        threshold: Decimal::percent(40),
                        allow_end_early: true,
                    },
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Custom(
                TgradeMsg::ExecuteGovProposal {
                    title: "ChangeParams".to_string(),
                    description: "Change params testing proposal".to_string(),
                    proposal: GovProposal::ChangeParams(vec![ParamChange {
                        subspace: "foo".to_string(),
                        key: "bar".to_string(),
                        value: "baz".to_string()
                    }])
                }
            ))]
        );
    }

    #[test]
    fn update_consensus_evidence_params() {
        let mut deps = mock_deps_tgrade();
        let env = mock_env();
        proposals()
            .save(
                &mut deps.storage,
                1,
                &Proposal {
                    title: "UnpinCodes".to_owned(),
                    description: "UnpinCodes testing proposal".to_owned(),
                    created_by: "mock_person".to_owned(),
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
                    total_points: 20,
                    votes: Votes {
                        yes: 20,
                        no: 0,
                        abstain: 0,
                        veto: 0,
                    },
                },
            )
            .unwrap();

        let res = execute_execute(deps.as_mut(), env, mock_info("sender", &[]), 1).unwrap();
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
        let mut deps = mock_deps_tgrade();
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
