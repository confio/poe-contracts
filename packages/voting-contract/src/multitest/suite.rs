use super::contracts::{
    self, engagement_contract,
    voting::{self, Proposal},
    VotingContract,
};
use anyhow::Result as AnyResult;
use cosmwasm_std::{Addr, StdResult};
use cw_multi_test::{AppResponse, Executor};
use derivative::Derivative;
use tg3::{
    Vote, VoteInfo, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse, VoterResponse,
};

use tg4::Member;
use tg_bindings_test::TgradeApp;

use crate::{
    state::{
        ProposalInfo, ProposalListResponse, ProposalResponse, RulesBuilder,
        TextProposalListResponse, VotingRules,
    },
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

    pub fn with_member(mut self, addr: &str, points: u64) -> Self {
        self.members.push(Member {
            addr: addr.to_owned(),
            points,
            start_height: None,
        });
        self
    }

    pub fn with_rules(mut self, rules: VotingRules) -> Self {
        self.rules = rules;
        self
    }

    pub fn build(self) -> Suite {
        let owner = Addr::unchecked("owner");

        let mut app = TgradeApp::new(owner.as_str());

        let group_id = app.store_code(engagement_contract());
        let group = app
            .instantiate_contract(
                group_id,
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
                    group_addr: group.to_string(),
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
            group,
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
    pub group: Addr,
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
            .map(|(addr, points)| Member {
                addr: (*addr).to_owned(),
                points: *points,
                start_height: None,
            })
            .collect();

        let remove = remove.iter().map(|addr| (*addr).to_owned()).collect();

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.group.clone(),
            &tg4_engagement::ExecuteMsg::UpdateMembers { add, remove },
            &[],
        )
    }

    pub fn propose(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.voting.clone(),
            &voting::ExecuteMsg::Propose {
                title: title.to_owned(),
                description: description.to_owned(),
                proposal: Proposal::Text {},
            },
            &[],
        )
    }

    pub fn propose_and_execute(
        &mut self,
        executor: &str,
        title: &str,
        description: &str,
    ) -> AnyResult<AppResponse> {
        let prop = self.propose(executor, title, description)?;
        let id = get_proposal_id(&prop)?;
        self.execute_proposal(executor, id)
    }

    pub fn vote(&mut self, executor: &str, proposal_id: u64, vote: Vote) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.voting.clone(),
            &voting::ExecuteMsg::Vote { proposal_id, vote },
            &[],
        )
    }

    pub fn execute_proposal(&mut self, executor: &str, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.voting.clone(),
            &voting::ExecuteMsg::Execute { proposal_id },
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

    pub fn query_proposal(&self, proposal_id: u64) -> StdResult<ProposalResponse<Proposal>> {
        let prop: ProposalResponse<Proposal> = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::Proposal { proposal_id },
        )?;
        Ok(prop)
    }

    pub fn query_rules(&self) -> StdResult<VotingRules> {
        let rules: VotingRules = self
            .app
            .wrap()
            .query_wasm_smart(self.voting.clone(), &voting::QueryMsg::Rules {})?;
        Ok(rules)
    }

    pub fn query_vote_info(
        &self,
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

    pub fn list_voters(
        &self,
        start_after: impl Into<Option<String>>,
        limit: impl Into<Option<u32>>,
    ) -> StdResult<VoterListResponse> {
        let voters: VoterListResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ListVoters {
                start_after: start_after.into(),
                limit: limit.into(),
            },
        )?;
        Ok(voters)
    }

    pub fn list_proposals(
        &self,
        start_after: impl Into<Option<u64>>,
        limit: impl Into<Option<usize>>,
    ) -> StdResult<Vec<ProposalResponse<Proposal>>> {
        let proposals: ProposalListResponse<Proposal> = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ListProposals {
                start_after: start_after.into(),
                limit: limit.into().unwrap_or(10),
            },
        )?;
        Ok(proposals.proposals)
    }

    pub fn list_text_proposals(
        &self,
        start_after: impl Into<Option<u64>>,
        limit: impl Into<Option<usize>>,
    ) -> StdResult<Vec<ProposalInfo>> {
        let proposals: TextProposalListResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ListTextProposals {
                start_after: start_after.into(),
                limit: limit.into().unwrap_or(10),
            },
        )?;
        Ok(proposals.proposals)
    }

    pub fn reverse_proposals(
        &self,
        start_before: impl Into<Option<u64>>,
        limit: impl Into<Option<usize>>,
    ) -> StdResult<Vec<ProposalResponse<Proposal>>> {
        let proposals: ProposalListResponse<Proposal> = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ReverseProposals {
                start_before: start_before.into(),
                limit: limit.into().unwrap_or(10),
            },
        )?;
        Ok(proposals.proposals)
    }

    pub fn list_votes(
        &self,
        proposal_id: u64,
        start_after: impl Into<Option<String>>,
        limit: impl Into<Option<usize>>,
    ) -> StdResult<Vec<VoteInfo>> {
        let votes: VoteListResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ListVotes {
                proposal_id,
                start_after: start_after.into(),
                limit: limit.into().unwrap_or(10),
            },
        )?;
        Ok(votes.votes)
    }

    pub fn list_votes_by_voter(
        &self,
        voter: &str,
        start_after: impl Into<Option<u64>>,
        limit: impl Into<Option<usize>>,
    ) -> StdResult<Vec<VoteInfo>> {
        let votes: VoteListResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::ListVotesByVoter {
                voter: voter.to_owned(),
                start_after: start_after.into(),
                limit: limit.into().unwrap_or(10),
            },
        )?;
        Ok(votes.votes)
    }

    pub fn query_voter(&self, addr: &str) -> Result<VoterResponse, ContractError> {
        let voter: VoterResponse = self.app.wrap().query_wasm_smart(
            self.voting.clone(),
            &voting::QueryMsg::Voter {
                address: addr.to_string(),
            },
        )?;
        Ok(voter)
    }

    pub fn query_group_contract(&self) -> Result<String, ContractError> {
        let contract: String = self
            .app
            .wrap()
            .query_wasm_smart(self.voting.clone(), &voting::QueryMsg::GroupContract {})?;
        Ok(contract)
    }

    pub fn assert_voters(&mut self, expected: &[(&str, u64)]) {
        let expected: Vec<_> = expected
            .iter()
            .map(|(addr, points)| VoterDetail {
                addr: addr.to_string(),
                points: *points,
            })
            .collect();

        assert_eq!(expected, self.list_voters(None, None).unwrap().voters);
    }
}
