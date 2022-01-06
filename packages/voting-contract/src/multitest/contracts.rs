use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, StdError};
use cw_multi_test::{Contract, ContractWrapper};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tg_bindings::TgradeMsg;

pub fn engagement_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );

    Box::new(contract)
}

pub fn voting_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(voting::execute, voting::instantiate, voting::query);
    Box::new(contract)
}

pub mod voting {
    use cosmwasm_std::to_binary;
    use cw3::Vote;

    use crate::{list_voters, query_rules, state::VotingRules, ContractError};

    use super::*;

    type Response = cosmwasm_std::Response<TgradeMsg>;

    #[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
    #[serde(rename_all = "snake_case")]
    pub struct InstantiateMsg {
        pub rules: VotingRules,
        pub group_addr: String,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum ExecuteMsg {
        Propose {
            title: String,
            description: String,
            proposal: String,
        },
        Vote {
            proposal_id: u64,
            vote: Vote,
        },
        Execute {
            proposal_id: u64,
        },
        Close {
            proposal_id: u64,
        },
        /// The Community Pool may be a participant in engagement and end up
        /// receiving engagement rewards. This endpoint can be used to withdraw
        /// those. Anyone can call it.
        WithdrawEngagementRewards {},
        /// Message comming from valset on funds distribution, just takes funds
        /// send with message and does nothing
        DistributeFunds {},
    }

    #[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum QueryMsg {
        /// Return VotingRules
        Rules {},
        /// Returns ProposalResponse
        Proposal { proposal_id: u64 },
        /// Returns ProposalListResponse
        ListProposals {
            start_after: Option<u64>,
            limit: Option<u32>,
        },
        /// Returns ProposalListResponse
        ReverseProposals {
            start_before: Option<u64>,
            limit: Option<u32>,
        },
        /// Returns VoteResponse
        Vote { proposal_id: u64, voter: String },
        /// Returns VoteListResponse
        ListVotes {
            proposal_id: u64,
            start_after: Option<String>,
            limit: Option<u32>,
        },
        /// Returns VoterResponse
        Voter { address: String },
        /// Returns VoterListResponse
        ListVoters {
            start_after: Option<String>,
            limit: Option<u32>,
        },
        /// Returns address of current's group contract
        GroupContract {},
    }

    pub fn instantiate(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> Result<Response, ContractError> {
        crate::instantiate(deps, msg.rules, &msg.group_addr)
    }

    pub fn execute(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: ExecuteMsg,
    ) -> Result<Response, StdError> {
        todo!()
    }

    pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
        use QueryMsg::*;

        match msg {
            Rules {} => to_binary(&query_rules(deps)?),
            ListVoters { start_after, limit } => to_binary(&list_voters(deps, start_after, limit)?),
            _ => todo!(),
        }
    }
}
