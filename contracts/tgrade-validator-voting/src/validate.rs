use cosmwasm_std::{
    from_slice, to_vec, ContractInfoResponse, ContractResult, CustomQuery, Deps, Empty, Env,
    QueryRequest, SystemResult, WasmQuery,
};

use tg_bindings::TgradeQuery;

use crate::msg::ValidatorProposal;
use crate::ContractError;

impl ValidatorProposal {
    pub fn validate(
        &self,
        deps: Deps<TgradeQuery>,
        env: &Env,
        title: &str,
        description: &str,
    ) -> Result<(), ContractError> {
        if title.is_empty() {
            return Err(ContractError::EmptyTitle {});
        }
        if description.is_empty() {
            return Err(ContractError::EmptyDescription {});
        }
        match self {
            ValidatorProposal::ChangeParams(params) => {
                if params.is_empty() {
                    return Err(ContractError::EmptyParams {});
                }
                for param in params.iter() {
                    if param.key.is_empty() {
                        return Err(ContractError::EmptyParamKey {});
                    }
                }
            }
            ValidatorProposal::PinCodes(codes) | ValidatorProposal::UnpinCodes(codes) => {
                if codes.is_empty() {
                    return Err(ContractError::EmptyCodes {});
                }
                if codes.len() != codes.iter().filter(|&c| c > &0).count() {
                    return Err(ContractError::ZeroCodes {});
                }
            }
            ValidatorProposal::MigrateContract {
                contract,
                code_id,
                migrate_msg,
            } => {
                if code_id == &0 {
                    return Err(ContractError::ZeroCodes {});
                }
                if migrate_msg.is_empty() {
                    return Err(ContractError::MigrateMsgCannotBeEmptyString {});
                };
                // Migrate contract needs confirming that migration sender (validator voting contract) is an admin
                // of target contract
                confirm_admin_in_contract(deps, env, contract.clone())?;
            }
            ValidatorProposal::RegisterUpgrade {
                name,
                height,
                info: _info,
            } => {
                if name.is_empty() {
                    return Err(ContractError::EmptyUpgradeName {});
                }
                if height < &env.block.height {
                    return Err(ContractError::InvalidUpgradeHeight(*height));
                }
            }
            ValidatorProposal::UpdateConsensusBlockParams { max_bytes, max_gas } => {
                if max_bytes.is_none() && max_gas.is_none() {
                    return Err(ContractError::InvalidConsensusParams {});
                }
            }
            ValidatorProposal::UpdateConsensusEvidenceParams {
                max_age_num_blocks,
                max_age_duration,
                max_bytes,
            } => {
                if max_age_num_blocks.is_none() && max_age_duration.is_none() && max_bytes.is_none()
                {
                    return Err(ContractError::InvalidConsensusParams {});
                }
            }
            ValidatorProposal::CancelUpgrade {} | ValidatorProposal::Text {} => {}
        }
        Ok(())
    }
}

fn confirm_admin_in_contract<Q: CustomQuery>(
    deps: Deps<Q>,
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

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{to_binary, Binary};
    use tg_bindings::ParamChange;

    use crate::ContractError;
    use tg_bindings_test::mock_deps_tgrade;

    use crate::msg::ValidatorProposal;

    #[derive(serde::Serialize)]
    struct DummyMigrateMsg {}

    #[test]
    fn validate_main_fields_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        let proposal = ValidatorProposal::Text {};

        // Empty title
        let res = proposal.validate(deps.as_ref(), &env, "", "description");
        assert_eq!(res.unwrap_err(), ContractError::EmptyTitle {});

        // Empty description
        let res = proposal.validate(deps.as_ref(), &env, "title", "");
        assert_eq!(res.unwrap_err(), ContractError::EmptyDescription {});

        // Non empty title and description
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_change_params_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Empty params
        let proposal = ValidatorProposal::ChangeParams(vec![]);

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::EmptyParams {});

        // Empty param key
        let proposal = ValidatorProposal::ChangeParams(vec![ParamChange {
            subspace: "".to_string(),
            key: "".to_string(),
            value: "".to_string(),
        }]);

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::EmptyParamKey {});

        // Non-empty param key
        let proposal = ValidatorProposal::ChangeParams(vec![ParamChange {
            subspace: "".to_string(),
            key: "key".to_string(),
            value: "".to_string(),
        }]);

        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_pin_codes_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Empty codes
        let proposal = ValidatorProposal::PinCodes(vec![]);

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::EmptyCodes {});

        // Zero codes
        let proposal = ValidatorProposal::PinCodes(vec![9, 8, 0, 7]);

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::ZeroCodes {});

        // Non-zero codes
        let proposal = ValidatorProposal::PinCodes(vec![9, 8, 1, 7]);

        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_unpin_codes_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Empty codes
        let proposal = ValidatorProposal::UnpinCodes(vec![]);

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::EmptyCodes {});

        // Zero codes
        let proposal = ValidatorProposal::UnpinCodes(vec![9, 8, 0, 7]);

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::ZeroCodes {});

        // Non-zero codes
        let proposal = ValidatorProposal::UnpinCodes(vec![9, 8, 1, 7]);

        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_migrate_contract_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Zero code id
        let proposal = ValidatorProposal::MigrateContract {
            contract: "target_contract".to_owned(),
            code_id: 0,
            migrate_msg: to_binary(&DummyMigrateMsg {}).unwrap(),
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::ZeroCodes {});

        // Empty migrate msg
        let proposal = ValidatorProposal::MigrateContract {
            contract: "target_contract".to_owned(),
            code_id: 1,
            migrate_msg: Binary(vec![]),
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(
            res.unwrap_err(),
            ContractError::MigrateMsgCannotBeEmptyString {}
        );

        // Valid
        let proposal = ValidatorProposal::MigrateContract {
            contract: "target_contract".to_owned(),
            code_id: 1,
            migrate_msg: to_binary(&DummyMigrateMsg {}).unwrap(),
        };
        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        // FIXME: Requires custom querier / multi-tests
        assert_eq!(
            res.unwrap_err(),
            ContractError::System("Querier system error: No such contract: target_contract".into())
        );
    }

    #[test]
    fn validate_register_upgrade_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Empty name
        let proposal = ValidatorProposal::RegisterUpgrade {
            name: "".to_string(),
            height: 1234,
            info: "info".to_string(),
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::EmptyUpgradeName {});

        // Invalid height
        let proposal = ValidatorProposal::RegisterUpgrade {
            name: "name".to_string(),
            height: env.block.height - 1,
            info: "info".to_string(),
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(
            res.unwrap_err(),
            ContractError::InvalidUpgradeHeight(env.block.height - 1)
        );

        // Valid
        let proposal = ValidatorProposal::RegisterUpgrade {
            name: "name".to_string(),
            height: env.block.height + 1,
            info: "info".to_string(),
        };

        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_cancel_upgrade_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Valid
        let proposal = ValidatorProposal::CancelUpgrade {};

        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_update_consensus_block_params_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Invalid: both none
        let proposal = ValidatorProposal::UpdateConsensusBlockParams {
            max_bytes: None,
            max_gas: None,
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::InvalidConsensusParams {});

        // Valid: max_bytes set
        let proposal = ValidatorProposal::UpdateConsensusBlockParams {
            max_bytes: Some(1234),
            max_gas: None,
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: max_gas set
        let proposal = ValidatorProposal::UpdateConsensusBlockParams {
            max_bytes: None,
            max_gas: Some(3290),
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: both max_bytes and max_gas set
        let proposal = ValidatorProposal::UpdateConsensusBlockParams {
            max_bytes: Some(1),
            max_gas: Some(2),
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_update_consensus_evidence_params_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Invalid: all none
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: None,
            max_age_duration: None,
            max_bytes: None,
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::InvalidConsensusParams {});

        // Valid: max_bytes set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: None,
            max_age_duration: None,
            max_bytes: Some(1234),
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: max_age_duration set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: None,
            max_age_duration: Some(0),
            max_bytes: None,
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: max_age_num_blocks set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: Some(1),
            max_age_duration: None,
            max_bytes: None,
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: all set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: Some(1),
            max_age_duration: Some(2),
            max_bytes: Some(3),
        };
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_text_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Valid
        let proposal = ValidatorProposal::Text {};

        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }
}
