use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Deps, Order, StdResult, Storage};
use cw_storage_plus::Bound;
use cw_storage_plus::{Index, IndexList, IndexedMap, MultiIndex};
use cw_utils::maybe_addr;
use tg3::{Vote, VoteInfo, VoteListResponse, VoteResponse};

use crate::ContractError;

// we cast a ballot with our chosen vote and a given points
// stored under the key that voted
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Ballot {
    pub voter: Addr,
    pub proposal_id: u64,
    pub points: u64,
    pub vote: Vote,
}

struct BallotIndexes<'a> {
    pub voter: MultiIndex<'a, Addr, Ballot>,
}

impl<'a> IndexList<Ballot> for BallotIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Ballot>> + '_> {
        let v: Vec<&dyn Index<Ballot>> = vec![&self.voter];
        Box::new(v.into_iter())
    }
}

pub fn ballots() -> Ballots<'static> {
    Ballots::new("ballots", "ballots__proposal_id")
}

pub struct Ballots<'a> {
    ballots: IndexedMap<'a, (u64, &'a Addr), Ballot, BallotIndexes<'a>>,
}

impl<'a> Ballots<'a> {
    pub fn new(storage_key: &'a str, release_subkey: &'a str) -> Self {
        let indexes = BallotIndexes {
            voter: MultiIndex::new(|ballot| ballot.voter.clone(), storage_key, release_subkey),
        };
        let ballots = IndexedMap::new(storage_key, indexes);

        Self { ballots }
    }

    pub fn create_ballot(
        &self,
        storage: &mut dyn Storage,
        addr: &Addr,
        proposal_id: u64,
        points: u64,
        vote: Vote,
    ) -> Result<(), ContractError> {
        self.ballots.update(
            storage,
            (proposal_id, addr),
            move |ballot| -> Result<_, ContractError> {
                match ballot {
                    Some(_) => Err(ContractError::AlreadyVoted {}),
                    None => Ok(Ballot {
                        voter: addr.clone(),
                        proposal_id,
                        points,
                        vote,
                    }),
                }
            },
        )?;
        Ok(())
    }

    pub fn query_vote(
        &self,
        deps: Deps,
        proposal_id: u64,
        voter: String,
    ) -> StdResult<VoteResponse> {
        let voter_addr = deps.api.addr_validate(&voter)?;
        let prop = self
            .ballots
            .may_load(deps.storage, (proposal_id, &voter_addr))?;
        let vote = prop.map(|b| VoteInfo {
            proposal_id,
            voter,
            vote: b.vote,
            points: b.points,
        });
        Ok(VoteResponse { vote })
    }

    pub fn query_votes(
        &self,
        deps: Deps,
        proposal_id: u64,
        start_after: Option<String>,
        limit: usize,
    ) -> StdResult<VoteListResponse> {
        let addr = maybe_addr(deps.api, start_after)?;
        let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

        let votes: StdResult<Vec<_>> = self
            .ballots
            .prefix(proposal_id)
            .range(deps.storage, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (voter, ballot) = item?;
                Ok(VoteInfo {
                    proposal_id,
                    voter: voter.into(),
                    vote: ballot.vote,
                    points: ballot.points,
                })
            })
            .collect();

        Ok(VoteListResponse { votes: votes? })
    }

    pub fn query_votes_by_voter(
        &self,
        deps: Deps,
        voter: String,
        start_after: Option<u64>,
        limit: usize,
    ) -> StdResult<VoteListResponse> {
        let start = start_after.map(Bound::exclusive_int);
        let voter_addr = deps.api.addr_validate(&voter)?;

        dbg!(start.clone());
        let votes: StdResult<Vec<_>> = self
            .ballots
            .idx
            .voter
            .prefix(voter_addr)
            .range(deps.storage, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                let (_, ballot) = item?;
                Ok(VoteInfo {
                    proposal_id: ballot.proposal_id,
                    voter: ballot.voter.into(),
                    vote: ballot.vote,
                    points: ballot.points,
                })
            })
            .collect();

        Ok(VoteListResponse { votes: votes? })
    }
}
