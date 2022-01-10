use super::contracts::{self, engagement_contract, voting, VotingContract};
use anyhow::Result as AnyResult;
use cosmwasm_std::{Addr, StdResult};
use cw3::{Vote, VoteInfo, VoteResponse, VoterDetail, VoterListResponse};
use cw_multi_test::{AppResponse, Executor};
use derivative::Derivative;

use tg4::Member;
use tg_bindings_test::TgradeApp;

use crate::{
    state::{ProposalResponse, RulesBuilder, VotingRules},
    ContractError,
};

pub fn get_proposal_id(response: &AppResponse) -> Result<u64, std::num::ParseIntError> {
    response.custom_attrs(1)[2].value.parse()
}

pub struct SuiteBuilder {
    members: Vec<Member>,
    rules: VotingRules,
}

impl SuiteBuilder {
    pub fn new() -> Self {
        Self {
            members: vec![],
            rules: RulesBuilder::new().build(),
        }
    }

    pub fn with_member(mut self, addr: &str, weight: u64) -> Self {
        self.members.push(Member {
            addr: addr.to_owned(),
            weight,
        });
        self
    }

    pub fn with_rules(mut self, rules: VotingRules) -> Self {
        self.rules = rules;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");

        let mut app = TgradeApp::new(owner.as_str());

        let engagement_id = app.store_code(engagement_contract());
        let engagement = app
            .instantiate_contract(
                engagement_id,
                owner.clone(),
                &tg4_engagement::msg::InstantiateMsg {
                    admin: Some(owner.to_string()),
                    members: self.members,
                    preauths_hooks: 0,
                    preauths_slashing: 0,
                    halflife: None,
                    denom: "poe-coin".to_string(),
                },
                &[],
                "engagement",
                Some(owner.to_string()),
            )
            .unwrap();

        let voting_id = app.store_code(Box::new(VotingContract));
        let voting = app
            .instantiate_contract(
                voting_id,
                owner.clone(),
                &contracts::voting::InstantiateMsg {
                    rules: self.rules,
                    group_addr: engagement.to_string(),
                },
                &[],
                "voting",
                Some(owner.to_string()),
            )
            .unwrap();

        app.advance_blocks(1);

        Suite {
            app,
            voting,
            engagement,
            owner,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    #[derivative(Debug = "ignore")]
    pub app: TgradeApp,
    /// Voting contract address
    pub voting: Addr,
    /// Engagement contract address
    pub engagement: Addr,
    /// Mixer contract address
    pub owner: Addr,
}

impl Suite {
    pub fn modify_members(
        &mut self,
        executor: &str,
        add: &[(&str, u64)],
        remove: &[&str],
    ) -> AnyResult<AppResponse> {
        let add = add
            .iter()
            .map(|(addr, weight)| Member {
                addr: (*addr).to_owned(),
                weight: *weight,
            })
            .collect();

        let remove = remove.iter().map(|addr| (*addr).to_owned()).collect();

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.engagement.clone(),
            &tg4_engagement::ExecuteMsg::UpdateMembers { add, remove },
            &[],
        )
    }

    pub fn propose_detailed(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
        proposal: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.voting.clone(),
            &voting::ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal: proposal.to_string(),
            },
            &[],
        )
    }

    pub fn propose(&mut self, executor: &str, title: &str) -> AnyResult<AppResponse> {
        self.propose_detailed(executor, title, title, title)
    }

    pub fn vote(&mut self, executor: &str, proposal_id: u64, vote: Vote) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.voting.clone(),
            &voting::ExecuteMsg::Vote { proposal_id, vote },
            &[],
        )
    }

    pub fn close(&mut self, executor: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.voting.clone(),
            &voting::ExecuteMsg::Close { proposal_id },
            &[],
        )
    }

    pub fn query_proposal(&mut self, proposal_id: u64) -> StdResult<ProposalResponse<String>> {
        let prop: ProposalResponse<String> = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::Proposal { proposal_id },
        )?;
        Ok(prop)
    }

    pub fn query_rules(&mut self) -> StdResult<VotingRules> {
        let rules: VotingRules = self
            .app
            .wrap()
            .query_wasm_smart(self.voting.clone(), &voting::QueryMsg::Rules {})?;
        Ok(rules)
    }

    pub fn query_vote_info(
        &mut self,
        proposal_id: u64,
        voter: &str,
    ) -> Result<Option<VoteInfo>, ContractError> {
        let vote: VoteResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::Vote {
                proposal_id,
                voter: voter.to_owned(),
            },
        )?;
        Ok(vote.vote)
    }

    pub fn list_voters(&mut self) -> StdResult<VoterListResponse> {
        let voters: VoterListResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ListVoters {
                start_after: None,
                limit: None,
            },
        )?;
        Ok(voters)
    }

    pub fn assert_voters(&mut self, expected: &[(&str, u64)]) {
        let expected: Vec<_> = expected
            .iter()
            .map(|(addr, weight)| VoterDetail {
                addr: addr.to_string(),
                weight: *weight,
            })
            .collect();

        assert_eq!(expected, self.list_voters().unwrap().voters);
    }
}
