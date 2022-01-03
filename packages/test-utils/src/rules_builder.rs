use cosmwasm_std::Decimal;
use tg_voting_contract::state::VotingRules;

pub struct RulesBuilder {
    pub voting_period: u32,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub allow_end_early: bool,
}

impl RulesBuilder {
    pub fn new() -> Self {
        Self {
            voting_period: 14,
            quorum: Decimal::percent(1),
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
