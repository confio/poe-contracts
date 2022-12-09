use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{BlockInfo, Decimal, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use tg3::{Status, Vote};
use tg4::Tg4Contract;
use tg_utils::Expiration;

use crate::ContractError;

// we multiply by this when calculating needed_votes in order to round up properly
// Note: `10u128.pow(9)` fails as "u128::pow` is not yet stable as a const fn"
const PRECISION_FACTOR: u128 = 1_000_000_000;

/// Contract configuration. Custom config is added to avoid double-fetching config on execution.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Config {
    pub rules: VotingRules,
    // Total points and voters are queried from this contract
    pub group_contract: Tg4Contract,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Proposal<P> {
    pub title: String,
    pub description: String,
    pub start_height: u64,
    pub created_by: String,
    pub expires: Expiration,
    pub proposal: P,
    pub status: Status,
    /// pass requirements
    pub rules: VotingRules,
    // the total number of points when the proposal started (used to calculate percentages)
    pub total_points: u64,
    // summary of existing votes
    pub votes: Votes,
}

impl<P> From<Proposal<P>> for ProposalInfo {
    fn from(p: Proposal<P>) -> Self {
        Self {
            title: p.title,
            description: p.description,
        }
    }
}

impl<P> Proposal<P> {
    /// current_status is non-mutable and returns what the status should be.
    /// (designed for queries)
    pub fn current_status(&self, block: &BlockInfo) -> Status {
        let mut status = self.status;

        // if open, check if voting is passed or timed out
        if status == Status::Open && self.is_passed(block) {
            status = Status::Passed;
        }
        if status == Status::Open && self.expires.is_expired(block) {
            status = Status::Rejected;
        }

        status
    }

    /// update_status sets the status of the proposal to current_status.
    /// (designed for handler logic)
    pub fn update_status(&mut self, block: &BlockInfo) {
        self.status = self.current_status(block);
    }

    // returns true iff this proposal is sure to pass (even before expiration if no future
    // sequence of possible votes can cause it to fail)
    pub fn is_passed(&self, block: &BlockInfo) -> bool {
        let VotingRules {
            quorum,
            threshold,
            allow_end_early,
            ..
        } = self.rules;

        // we always require the quorum
        if self.votes.total() < votes_needed(self.total_points, quorum) {
            return false;
        }
        if self.expires.is_expired(block) {
            // If expired, we compare Yes votes against the total number of votes (minus abstain).
            let opinions = self.votes.total() - self.votes.abstain;
            self.votes.yes >= votes_needed(opinions, threshold)
        } else if allow_end_early {
            // If not expired, we must assume all non-votes will be cast as No.
            // We compare threshold against the total points (minus abstain).
            let possible_opinions = self.total_points - self.votes.abstain;
            self.votes.yes >= votes_needed(possible_opinions, threshold)
        } else {
            false
        }
    }
}

/// Note, if you are storing custom messages in the proposal,
/// the querier needs to know what possible custom message types
/// those are in order to parse the response
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct ProposalResponse<P> {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub created_by: String,
    pub proposal: P,
    pub status: Status,
    pub expires: Expiration,
    pub rules: VotingRules,
    pub total_points: u64,
    pub votes: Votes,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct ProposalListResponse<P> {
    pub proposals: Vec<ProposalResponse<P>>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct TextProposalListResponse {
    pub proposals: Vec<ProposalInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, JsonSchema)]
pub struct VotingRules {
    /// Length of voting period in days.
    pub voting_period: u32,
    /// quorum requirement (0.0-1.0]
    pub quorum: Decimal,
    /// threshold requirement [0.5-1.0]
    pub threshold: Decimal,
    /// If true, and absolute threshold and quorum are met, we can end before voting period finished
    pub allow_end_early: bool,
}

impl VotingRules {
    pub fn validate(&self) -> Result<(), ContractError> {
        let zero = Decimal::percent(0);
        let hundred = Decimal::percent(100);

        if self.quorum == zero || self.quorum > hundred {
            return Err(ContractError::InvalidQuorum(self.quorum));
        }

        if self.threshold < Decimal::percent(50) || self.threshold > hundred {
            return Err(ContractError::InvalidThreshold(self.threshold));
        }

        if self.voting_period == 0 || self.voting_period > 365 {
            return Err(ContractError::InvalidVotingPeriod(self.voting_period));
        }
        Ok(())
    }

    pub fn voting_period_secs(&self) -> u64 {
        self.voting_period as u64 * 86_400
    }
}

pub struct RulesBuilder {
    voting_period: u32,
    quorum: Decimal,
    threshold: Decimal,
    allow_end_early: bool,
}

impl RulesBuilder {
    pub fn new() -> Self {
        Self {
            voting_period: 14,
            quorum: Decimal::percent(20),
            threshold: Decimal::percent(50),
            allow_end_early: true,
        }
    }

    pub fn with_threshold(mut self, threshold: impl Into<Decimal>) -> Self {
        self.threshold = threshold.into();
        self
    }

    pub fn with_quorum(mut self, quorum: impl Into<Decimal>) -> Self {
        self.quorum = quorum.into();
        self
    }

    pub fn with_allow_early(mut self, allow_end_early: bool) -> Self {
        self.allow_end_early = allow_end_early;
        self
    }

    pub fn build(&self) -> VotingRules {
        VotingRules {
            voting_period: self.voting_period,
            quorum: self.quorum,
            threshold: self.threshold,
            allow_end_early: self.allow_end_early,
        }
    }
}

impl Default for RulesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// points of votes for each option
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Votes {
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
    pub veto: u64,
}

impl Votes {
    /// sum of all votes
    pub fn total(&self) -> u64 {
        self.yes + self.no + self.abstain + self.veto
    }

    /// create it with a yes vote for this much
    pub fn yes(init_points: u64) -> Self {
        Votes {
            yes: init_points,
            no: 0,
            abstain: 0,
            veto: 0,
        }
    }

    pub fn add_vote(&mut self, vote: Vote, points: u64) {
        match vote {
            Vote::Yes => self.yes += points,
            Vote::Abstain => self.abstain += points,
            Vote::No => self.no += points,
            Vote::Veto => self.veto += points,
        }
    }
}

// this is a helper function so Decimal works with u64 rather than Uint128
// also, we must *round up* here, as we need 8, not 7 votes to reach 50% of 15 total
fn votes_needed(points: u64, percentage: Decimal) -> u64 {
    let applied = percentage * Uint128::new(PRECISION_FACTOR * points as u128);
    // Divide by PRECISION_FACTOR, rounding up to the nearest integer
    ((applied.u128() + PRECISION_FACTOR - 1) / PRECISION_FACTOR) as u64
}

// unique items
pub const CONFIG: Item<Config> = Item::new("voting_config");
pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

pub fn proposals<'m, P>() -> Map<'m, u64, Proposal<P>> {
    Map::new("proposals")
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct ProposalInfo {
    pub title: String,
    pub description: String,
}

pub const TEXT_PROPOSALS: Map<u64, ProposalInfo> = Map::new("text_proposals");

pub fn next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}
