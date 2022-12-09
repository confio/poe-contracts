use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Storage};
use cw_storage_plus::{Index, IndexList, IndexedMap, MultiIndex};
use tg3::Vote;

use crate::ContractError;

// we cast a ballot with our chosen vote and a given points
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Ballot {
    pub voter: Addr,
    pub points: u64,
    pub vote: Vote,
}

pub struct BallotIndexes<'a> {
    // This PrimaryKey allows quering over all proposal ids for given voter address
    pub voter: MultiIndex<'a, Addr, Ballot, (u64, Addr)>,
}

impl<'a> IndexList<Ballot> for BallotIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Ballot>> + '_> {
        let v: Vec<&dyn Index<Ballot>> = vec![&self.voter];
        Box::new(v.into_iter())
    }
}

pub struct Ballots<'a> {
    pub ballots: IndexedMap<'a, (u64, &'a Addr), Ballot, BallotIndexes<'a>>,
}

impl<'a> Ballots<'a> {
    pub fn new(storage_key: &'a str, release_subkey: &'a str) -> Self {
        let indexes = BallotIndexes {
            voter: MultiIndex::new(
                |_, ballot| ballot.voter.clone(),
                storage_key,
                release_subkey,
            ),
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
                        points,
                        vote,
                    }),
                }
            },
        )?;
        Ok(())
    }
}

pub fn ballots() -> Ballots<'static> {
    Ballots::new("ballots", "ballots__proposal_id")
}
