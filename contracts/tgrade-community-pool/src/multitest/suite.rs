use anyhow::{anyhow, Result as AnyResult};

use cosmwasm_std::{coin, Addr, CosmosMsg, StdResult};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use tg4::{Member, Tg4ExecuteMsg};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;

use tg_test_utils::RulesBuilder;
use tg_voting_contract::state::VotingRules;

use crate::msg::{ExecuteMsg, Proposal};

fn contract_validator_proposals() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

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
    contract_weight: u64,
    group_token: String,
}

impl SuiteBuilder {
    pub fn new() -> SuiteBuilder {
        SuiteBuilder {
            engagement_members: vec![],
            group_members: vec![],
            rules: RulesBuilder::new().build(),
            contract_weight: 0,
            group_token: "GROUP".to_owned(),
        }
    }

    pub fn with_group_member(mut self, addr: &str, weight: u64) -> Self {
        self.group_members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    pub fn with_community_pool_as_member(mut self, weight: u64) -> Self {
        self.contract_weight = weight;
        self
    }

    pub fn with_group_token(mut self, token: &str) -> Self {
        self.group_token = token.to_owned();
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
                    denom: self.group_token.clone(),
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

        if self.contract_weight > 0 {
            app.execute_contract(
                owner.clone(),
                group_contract.clone(),
                &tg4_engagement::ExecuteMsg::UpdateMembers {
                    remove: vec![],
                    add: vec![Member {
                        addr: contract.to_string(),
                        weight: self.contract_weight,
                    }],
                },
                &[],
            )
            .unwrap();
        };

        app.next_block().unwrap();

        Suite {
            app,
            contract,
            group_contract,
            owner,
            group_token: self.group_token,
        }
    }
}

pub struct Suite {
    app: TgradeApp,
    pub contract: Addr,
    group_contract: Addr,
    owner: Addr,
    group_token: String,
}

impl Suite {
    pub fn distribute_engagement_rewards(&mut self, amount: u128) -> AnyResult<AppResponse> {
        let block_info = self.app.block_info();
        let owner = self.owner.clone();
        let denom = self.group_token.to_string();

        self.app
            .init_modules(|router, api, storage| -> AnyResult<()> {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    owner.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom,
                        amount: amount.into(),
                        recipient: owner.to_string(),
                    })
                    .into(),
                )?;

                Ok(())
            })?;

        self.app.next_block().unwrap();

        self.app.execute_contract(
            self.owner.clone(),
            self.group_contract.clone(),
            &tg4_engagement::ExecuteMsg::DistributeFunds { sender: None },
            &[coin(amount, self.group_token.clone())],
        )
    }

    pub fn distribute_funds(&mut self, amount: u128) -> AnyResult<AppResponse> {
        let block_info = self.app.block_info();
        let owner = self.owner.clone();
        let denom = self.group_token.to_string();

        self.app
            .init_modules(|router, api, storage| -> AnyResult<()> {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    owner.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom,
                        amount: amount.into(),
                        recipient: owner.to_string(),
                    })
                    .into(),
                )?;

                Ok(())
            })?;

        self.app.next_block().unwrap();

        self.app.execute_contract(
            self.owner.clone(),
            self.contract.clone(),
            &ExecuteMsg::DistributeFunds {},
            &[coin(amount, self.group_token.clone())],
        )
    }

    pub fn withdraw_community_pool_rewards(&mut self, executor: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.contract.clone(),
            &ExecuteMsg::WithdrawEngagementRewards {},
            &[],
        )
    }

    pub fn propose(
        &mut self,
        sender: &str,
        title: &str,
        description: &str,
        proposal: Proposal,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.contract.clone(),
            &ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal,
            },
            &[],
        )
    }

    pub fn execute(&mut self, sender: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.contract.clone(),
            &ExecuteMsg::Execute { proposal_id },
            &[],
        )
    }

    /// Shortcut for querying distributable token balance of contract
    pub fn token_balance(&self, owner: Addr) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(owner, self.group_token.clone())?
            .amount;
        Ok(amount.into())
    }
}

pub fn created_proposal_id(resp: &AppResponse) -> AnyResult<u64> {
    let wasm_ev = resp
        .events
        .iter()
        .find(|ev| &ev.ty == "wasm")
        .ok_or_else(|| anyhow!("No wasm event on response"))?;

    let proposal_id: u64 = wasm_ev
        .attributes
        .iter()
        .find(|attr| &attr.key == "proposal_id")
        .ok_or_else(|| anyhow!("No proposal_id on wasm event"))?
        .value
        .parse()?;

    Ok(proposal_id)
}
