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
            ValidatorProposal::UpdateConsensusBlockParams { .. } => {}
            ValidatorProposal::UpdateConsensusEvidenceParams { .. } => {}
            ValidatorProposal::MigrateContract {
                contract,
                code_id,
                migrate_msg,
            } => {
                // Migrate contract needs confirming that migration sender (validator voting contract) is an admin
                // of target contract
                confirm_admin_in_contract(deps, env, contract.clone())?;
                if code_id == &0 {
                    return Err(ContractError::ZeroCodes {});
                }
                if migrate_msg.is_empty() {
                    return Err(ContractError::MigrateMsgCannotBeEmptyString {});
                };
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
            ValidatorProposal::CancelUpgrade {} => {}
            ValidatorProposal::Text {} => {}
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

    use tg_bindings_test::mock_deps_tgrade;

    use crate::msg::ValidatorProposal;

    #[test]
    fn validate_main_fields_works() {
        let deps = mock_deps_tgrade();
        let env = mock_env();

        let proposal = ValidatorProposal::Text {};

        // Empty title
        let res = proposal.validate(deps.as_ref(), &env, "", "description");
        assert!(res.is_err());

        // Empty description
        let res = proposal.validate(deps.as_ref(), &env, "title", "");
        assert!(res.is_err());

        // Non empty title and description
        let _res = proposal
            .validate(deps.as_ref(), &env, "title", "description")
            .unwrap();
    }
}
