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
            ValidatorProposal::SetContractAdmin {
                contract: _contract,
                new_admin,
            } => {
                if new_admin.is_empty() {
                    return Err(ContractError::EmptyAdmin {});
                }
            }
            ValidatorProposal::ClearContractAdmin { .. }
            | ValidatorProposal::PromoteToPrivilegedContract { .. }
            | ValidatorProposal::DemotePrivilegedContract { .. }
            | ValidatorProposal::CancelUpgrade {}
            | ValidatorProposal::Text {} => {}
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
    use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{
        from_slice, to_binary, Addr, Binary, ContractInfoResponse, ContractResult, Empty,
        OwnedDeps, Querier, QuerierResult, QueryRequest, Storage, SystemError, SystemResult,
        WasmQuery,
    };
    use std::marker::PhantomData;
    use tg_bindings::{ParamChange, TgradeQuery};

    use crate::ContractError;
    use tg_bindings_test::mock_deps_tgrade;

    use crate::msg::ValidatorProposal;

    #[derive(serde::Serialize)]
    struct DummyMigrateMsg {}

    const MIGRATE_CONTRACT: &str = "target_contract";

    // For `MigrateContract` validation
    struct CustomMockQuerier {
        contract: String,
        contract_admin: Option<String>,
        storage: MockStorage,
    }

    impl CustomMockQuerier {
        pub fn new(contract: &Addr, admin: Option<Addr>) -> Self {
            let storage = MockStorage::new();

            CustomMockQuerier {
                contract: contract.to_string(),
                contract_admin: admin.map(|a| a.to_string()),
                storage,
            }
        }

        fn handle_query(&self, request: QueryRequest<Empty>) -> QuerierResult {
            match request {
                QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                    self.query_wasm(contract_addr, key)
                }
                QueryRequest::Wasm(WasmQuery::Smart { .. }) => {
                    SystemResult::Err(SystemError::UnsupportedRequest {
                        kind: "WasmQuery::Smart".to_string(),
                    })
                }
                QueryRequest::Wasm(WasmQuery::ContractInfo { contract_addr }) => {
                    self.query_contract_info(contract_addr)
                }
                _ => SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: "not wasm".to_string(),
                }),
            }
        }

        // TODO: we should be able to add a custom wasm handler to MockQuerier from cosmwasm_std::mock
        fn query_wasm(&self, contract_addr: String, key: Binary) -> QuerierResult {
            if contract_addr != self.contract {
                SystemResult::Err(SystemError::NoSuchContract {
                    addr: contract_addr,
                })
            } else {
                let bin = self.storage.get(&key).unwrap_or_default();
                SystemResult::Ok(ContractResult::Ok(bin.into()))
            }
        }

        fn query_contract_info(&self, contract_addr: String) -> QuerierResult {
            if contract_addr != self.contract {
                SystemResult::Err(SystemError::NoSuchContract {
                    addr: contract_addr,
                })
            } else {
                let mut res = ContractInfoResponse::new(1, "dummy_creator");
                res.admin = self.contract_admin.clone();
                let bin = to_binary(&res).unwrap();
                SystemResult::Ok(ContractResult::Ok(bin))
            }
        }
    }

    impl Querier for CustomMockQuerier {
        fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
            let request: QueryRequest<Empty> = match from_slice(bin_request) {
                Ok(v) => v,
                Err(e) => {
                    return SystemResult::Err(SystemError::InvalidRequest {
                        error: format!("Parsing query request: {:?}", e),
                        request: bin_request.into(),
                    })
                }
            };
            self.handle_query(request)
        }
    }

    fn custom_mock_deps_tgrade(
        contract_addr: &str,
        contract_admin: Option<&str>,
    ) -> OwnedDeps<MockStorage, MockApi, CustomMockQuerier, TgradeQuery> {
        let querier = CustomMockQuerier::new(
            &Addr::unchecked(contract_addr),
            contract_admin.map(Addr::unchecked),
        );

        OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier,
            custom_query_type: PhantomData,
        }
    }

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
        proposal
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

        proposal
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

        proposal
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

        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_migrate_contract_works() {
        // No migration admin address set by default
        let deps = custom_mock_deps_tgrade(MIGRATE_CONTRACT, None);
        let env = mock_env();

        // Zero code id
        let proposal = ValidatorProposal::MigrateContract {
            contract: MIGRATE_CONTRACT.to_owned(),
            code_id: 0,
            migrate_msg: to_binary(&DummyMigrateMsg {}).unwrap(),
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(res.unwrap_err(), ContractError::ZeroCodes {});

        // Empty migrate msg
        let proposal = ValidatorProposal::MigrateContract {
            contract: MIGRATE_CONTRACT.to_owned(),
            code_id: 1,
            migrate_msg: Binary(vec![]),
        };

        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert_eq!(
            res.unwrap_err(),
            ContractError::MigrateMsgCannotBeEmptyString {}
        );

        // Valid (but no migration admin)
        let proposal = ValidatorProposal::MigrateContract {
            contract: MIGRATE_CONTRACT.to_owned(),
            code_id: 1,
            migrate_msg: to_binary(&DummyMigrateMsg {}).unwrap(),
        };
        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        assert!(matches!(res.unwrap_err(), ContractError::Unauthorized(_)));

        // Valid (but migration admin is some other contract)
        // Sets other contract as migration admin of the contract to migrate
        let deps = custom_mock_deps_tgrade(MIGRATE_CONTRACT, Some("other_contract"));
        let res = proposal.validate(deps.as_ref(), &env, "title", "description");
        // Also fails
        assert!(matches!(res.unwrap_err(), ContractError::Unauthorized(_)));

        // Valid (and migration admin is this contract)
        let deps = custom_mock_deps_tgrade(MIGRATE_CONTRACT, Some(MOCK_CONTRACT_ADDR));
        // Now it works
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
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

        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_cancel_upgrade_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Valid
        let proposal = ValidatorProposal::CancelUpgrade {};

        proposal
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
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: max_gas set
        let proposal = ValidatorProposal::UpdateConsensusBlockParams {
            max_bytes: None,
            max_gas: Some(3290),
        };
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: both max_bytes and max_gas set
        let proposal = ValidatorProposal::UpdateConsensusBlockParams {
            max_bytes: Some(1),
            max_gas: Some(2),
        };
        proposal
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
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: max_age_duration set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: None,
            max_age_duration: Some(0),
            max_bytes: None,
        };
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: max_age_num_blocks set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: Some(1),
            max_age_duration: None,
            max_bytes: None,
        };
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();

        // Valid: all set
        let proposal = ValidatorProposal::UpdateConsensusEvidenceParams {
            max_age_num_blocks: Some(1),
            max_age_duration: Some(2),
            max_bytes: Some(3),
        };
        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }

    #[test]
    fn validate_text_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        // Valid
        let proposal = ValidatorProposal::Text {};

        proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }
}
