use anyhow::Result as AnyResult;

use cosmwasm_std::{to_binary, Addr, ContractInfoResponse, Decimal};
use cw3::Status;
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use tg4::{Member, Tg4ExecuteMsg};
use tg_bindings::TgradeMsg;
use tg_bindings_test::{TgradeApp, UpgradePlan};

use crate::msg::ValidatorProposal;
use crate::msg::*;
use tg_voting_contract::state::{ProposalResponse, VotingRules};
use tg_voting_contract::ContractError;

pub fn get_proposal_id(response: &AppResponse) -> Result<u64, std::num::ParseIntError> {
    response.custom_attrs(1)[2].value.parse()
}

fn contract_validator_proposals() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo);

    Box::new(contract)
}

fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

pub struct SuiteBuilder {
    engagement_members: Vec<Member>,
    group_members: Vec<Member>,
    rules: VotingRules,
}

impl SuiteBuilder {
    pub fn new() -> SuiteBuilder {
        SuiteBuilder {
            engagement_members: vec![],
            group_members: vec![],
            rules: VotingRules {
                voting_period: 0,
                quorum: Decimal::zero(),
                threshold: Decimal::zero(),
                allow_end_early: false,
            },
        }
    }

    pub fn with_group_member(mut self, addr: &str, weight: u64) -> Self {
        self.group_members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    pub fn with_voting_rules(mut self, voting_rules: VotingRules) -> Self {
        self.rules = voting_rules;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");
        let mut app = TgradeApp::new(owner.as_str());

        // start from genesis
        app.back_to_genesis();

        let engagement_id = app.store_code(contract_engagement());
        let engagement_contract = app
            .instantiate_contract(
                engagement_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.engagement_members,
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: None,
                    denom: "ENGAGEMENT".to_owned(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();

        let group_id = app.store_code(contract_engagement());
        let group_contract = app
            .instantiate_contract(
                group_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.group_members.clone(),
                    preauths_hooks: 0,
                    preauths_slashing: 1,
                    halflife: None,
                    denom: "GROUP".to_owned(),
                },
                &[],
                "group",
                None,
            )
            .unwrap();

        let validator_proposals_id = app.store_code(contract_validator_proposals());
        let contract = app
            .instantiate_contract(
                validator_proposals_id,
                owner.clone(),
                &crate::msg::InstantiateMsg {
                    group_addr: group_contract.to_string(),
                    rules: self.rules,
                },
                &[],
                "validator-proposals",
                None,
            )
            .unwrap();

        // Set validator proposals contract's address as admin of engagement contract
        app.execute_contract(
            owner.clone(),
            engagement_contract,
            &Tg4ExecuteMsg::UpdateAdmin {
                admin: Some(contract.to_string()),
            },
            &[],
        )
        .unwrap();

        // promote the validator voting contract
        app.promote(owner.as_str(), contract.as_str()).unwrap();

        app.next_block().unwrap();

        Suite {
            app,
            contract,
            owner,
        }
    }
}

pub struct Suite {
    pub app: TgradeApp,
    pub contract: Addr,
    pub owner: Addr,
}

impl Suite {
    fn propose(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        proposal: ValidatorProposal,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal,
            },
            &[],
        )
    }

    fn propose_migrate(
        &mut self,
        executor: &str,
        contract: &str,
        code_id: u64,
        migrate_msg: crate::multitest::hackatom::MigrateMsg,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            ValidatorProposal::MigrateContract {
                contract: contract.to_owned(),
                code_id,
                migrate_msg: to_binary(&migrate_msg)?,
            },
        )
    }

    pub fn propose_pin(&mut self, executor: &str, code_ids: &[u64]) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            ValidatorProposal::PinCodes(code_ids.to_vec()),
        )
    }

    pub fn propose_unpin(&mut self, executor: &str, code_ids: &[u64]) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            ValidatorProposal::UnpinCodes(code_ids.to_vec()),
        )
    }

    pub fn propose_upgrade(
        &mut self,
        executor: &str,
        name: &str,
        height: u64,
        info: &str,
    ) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            ValidatorProposal::RegisterUpgrade {
                name: name.to_string(),
                height,
                info: info.to_string(),
            },
        )
    }

    pub fn propose_cancel_upgrade(&mut self, executor: &str) -> AnyResult<AppResponse> {
        self.propose(
            executor,
            "proposal title",
            "proposal description",
            ValidatorProposal::CancelUpgrade {},
        )
    }

    pub fn check_pinned(&self, code_id: u64) -> AnyResult<bool> {
        Ok(self
            .app
            .read_module(|router, _, storage| router.custom.is_pinned(storage, code_id))?)
    }

    pub fn check_upgrade(&self) -> AnyResult<Option<UpgradePlan>> {
        Ok(self
            .app
            .read_module(|router, _, storage| router.custom.upgrade_is_planned(storage))?)
    }

    pub fn execute(&mut self, executor: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::Execute { proposal_id },
            &[],
        )
    }

    pub fn query_proposal_status(&mut self, proposal_id: u64) -> Result<Status, ContractError> {
        let prop: ProposalResponse<ValidatorProposal> = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::Proposal { proposal_id })?;
        Ok(prop.status)
    }

    pub fn instantiate_hackatom_contract(
        &mut self,
        owner: Addr,
        hackatom_id: u64,
        beneficiary: &str,
    ) -> Addr {
        use crate::multitest::hackatom;
        self.app
            .instantiate_contract(
                hackatom_id,
                owner.clone(),
                &hackatom::InstantiateMsg {
                    beneficiary: beneficiary.to_owned(),
                },
                &[],
                "hackatom",
                Some(owner.to_string()),
            )
            .unwrap()
    }

    pub fn query_beneficiary(&mut self, hackatom: Addr) -> Result<String, ContractError> {
        use crate::multitest::hackatom::{InstantiateMsg, QueryMsg};
        let query_result: InstantiateMsg = self
            .app
            .wrap()
            .query_wasm_smart(hackatom, &QueryMsg::Beneficiary {})?;
        Ok(query_result.beneficiary)
    }

    pub fn query_contract_code_id(&mut self, contract: Addr) -> Result<u64, ContractError> {
        use cosmwasm_std::{QueryRequest, WasmQuery};
        let query_result: ContractInfoResponse =
            self.app
                .wrap()
                .query(&QueryRequest::Wasm(WasmQuery::ContractInfo {
                    contract_addr: contract.to_string(),
                }))?;
        Ok(query_result.code_id)
    }

    pub fn propose_migrate_hackatom(
        &mut self,
        sender: Addr,
        contract_to_migrate: Addr,
        new_beneficiary: &str,
        new_code_id: u64,
    ) -> AnyResult<AppResponse> {
        use crate::multitest::hackatom::MigrateMsg;
        self.propose_migrate(
            sender.as_str(),
            contract_to_migrate.as_str(),
            new_code_id,
            MigrateMsg {
                new_guy: new_beneficiary.to_owned(),
            },
        )
    }
}
