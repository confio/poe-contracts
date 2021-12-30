use super::helpers::{addr_to_pubkey, mock_metadata, mock_pubkey};
use crate::state::Config;
use crate::{msg::*, state::ValidatorInfo};
use anyhow::{bail, Result as AnyResult};
use cosmwasm_std::{
    coin, Addr, BlockInfo, Coin, CosmosMsg, Decimal, StdResult, Timestamp, Uint128,
};
use cw_multi_test::{next_block, AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use derivative::Derivative;
use tg4::{AdminResponse, Member};
use tg_bindings::{Evidence, Pubkey, TgradeMsg, ValidatorDiff};
use tg_bindings_test::TgradeApp;
use tg_utils::{Duration, JailingDuration};

use crate::msg::OperatorInitInfo;

pub fn contract_engagement() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_engagement::contract::execute,
        tg4_engagement::contract::instantiate,
        tg4_engagement::contract::query,
    );
    Box::new(contract)
}

fn contract_stake() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        tg4_stake::contract::execute,
        tg4_stake::contract::instantiate,
        tg4_stake::contract::query,
    );
    Box::new(contract)
}

pub fn contract_valset() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_sudo(crate::contract::sudo)
    .with_reply(crate::contract::reply);

    Box::new(contract)
}

#[derive(Debug, Clone)]
struct DistributionConfig {
    members: Vec<Member>,
    halflife: Option<Duration>,
    reward_ratio: Decimal,
}

#[derive(Derivative, Debug, Clone)]
#[derivative(Default = "new")]
enum GroupConfig {
    /// Use a tg4_engagement contract as the valset group contract
    #[derivative(Default)]
    Engagement {
        /// Engagement members
        members: Vec<(String, u64)>,
    },
    /// Usa a tg4_stake contract as the valset group contract
    Stake {
        denom: String,
        tokens_per_weight: Uint128,
    },
}

#[derive(Derivative, Debug, Clone)]
#[derivative(Default = "new")]
pub struct SuiteBuilder {
    /// Config as to which contract to use as the valset "group" contract,
    /// and with what kind of config
    group_config: GroupConfig,
    /// Valset operators, with optionally provided pubkeys
    operators: Vec<(String, Option<Pubkey>)>,
    #[derivative(Default(value = "1"))]
    min_weight: u64,
    /// Maximum number of validators for single epoch
    #[derivative(Default(value = "u32::MAX"))]
    max_validators: u32,
    /// Epoch length in seconds, 100s by default
    #[derivative(Default(value = "100"))]
    epoch_length: u64,
    /// Base epoch reward, 100uscd by default
    #[derivative(Default(value = "coin(100, \"usdc\")"))]
    epoch_reward: Coin,
    /// Validators weight scaling
    scaling: Option<u32>,
    /// Factor determining how accumulated fees affects base epoch reward
    fee_percentage: Decimal,
    /// Flag determining if jailed operators should be automatically unjailed
    auto_unjail: bool,
    #[derivative(Default(value = "Decimal::percent(50)"))]
    double_sign_slash_ratio: Decimal,
    /// Configuration of `distribution_contract` if any
    distribution_configs: Vec<DistributionConfig>,
    /// Funds to add on init per address
    init_funds: Vec<(String, Vec<Coin>)>,
}

impl SuiteBuilder {
    pub fn with_operators(mut self, operators: &[&str]) -> Self {
        self.operators = operators
            .iter()
            .map(|addr| ((*addr).to_owned(), None))
            .collect();

        self
    }

    // Method generates proper pubkeys, but requires address length to be exactly 32 bytes,
    // otherwise it will panic.
    pub fn with_operators_pubkeys(mut self, operators: &[&str]) -> Self {
        self.operators = operators
            .iter()
            .map(|addr| ((*addr).to_owned(), Some(addr_to_pubkey(addr))))
            .collect();

        self
    }

    /// Use a tg4_engagement contract for membership.
    /// Provide a list of members with assigned weights to initialize it with.
    pub fn with_engagement(mut self, members: &[(&str, u64)]) -> Self {
        self.group_config = GroupConfig::Engagement {
            members: members
                .iter()
                .map(|(addr, weight)| ((*addr).to_owned(), *weight))
                .collect(),
        };

        self
    }

    /// Use a tg4_stake contract for membership.
    /// The staking tokens may have a different denomination from the reward ones.
    pub fn with_stake(
        mut self,
        denom: impl Into<String>,
        tokens_per_weight: impl Into<Uint128>,
    ) -> Self {
        self.group_config = GroupConfig::Stake {
            denom: denom.into(),
            tokens_per_weight: tokens_per_weight.into(),
        };
        self
    }

    pub fn with_funds(mut self, funds: &[(&str, &[Coin])]) -> Self {
        self.init_funds.extend(
            funds
                .iter()
                .map(|(addr, funds)| (addr.to_string(), funds.to_vec())),
        );
        self
    }

    pub fn with_auto_unjail(mut self) -> Self {
        self.auto_unjail = true;
        self
    }

    pub fn with_epoch_reward(mut self, epoch_reward: Coin) -> Self {
        self.epoch_reward = epoch_reward;
        self
    }

    pub fn with_distribution(
        mut self,
        reward_ratio: Decimal,
        members: &[(&str, u64)],
        halflife: impl Into<Option<Duration>>,
    ) -> Self {
        let config = DistributionConfig {
            members: members
                .iter()
                .map(|(addr, weight)| Member {
                    addr: (*addr).to_owned(),
                    weight: *weight,
                })
                .collect(),
            halflife: halflife.into(),
            reward_ratio,
        };
        self.distribution_configs.push(config);
        self
    }

    pub fn with_fee_percentage(mut self, fee_percentage: Decimal) -> Self {
        self.fee_percentage = fee_percentage;
        self
    }

    pub fn with_max_validators(mut self, max_validators: u32) -> Self {
        self.max_validators = max_validators;
        self
    }

    pub fn with_min_weight(mut self, min_weight: u64) -> Self {
        self.min_weight = min_weight;
        self
    }

    pub fn with_epoch_length(mut self, epoch_length: u64) -> Self {
        self.epoch_length = epoch_length;
        self
    }

    pub fn build(mut self) -> Suite {
        let admin = Addr::unchecked("admin");
        let denom = self.epoch_reward.denom.clone();

        let mut app = TgradeApp::new(admin.as_str());
        // start from genesis
        app.back_to_genesis();

        let engagement_id = app.store_code(contract_engagement());

        let membership = match self.group_config {
            GroupConfig::Engagement { mut members } => {
                members.sort();
                members.dedup();

                let members = members
                    .into_iter()
                    .map(|(addr, weight)| Member { addr, weight })
                    .collect();

                app.instantiate_contract(
                    engagement_id,
                    admin.clone(),
                    &tg4_engagement::msg::InstantiateMsg {
                        admin: Some(admin.to_string()),
                        members,
                        preauths_hooks: 0,
                        preauths_slashing: 1,
                        halflife: None,
                        denom: denom.clone(),
                    },
                    &[],
                    "group",
                    Some(admin.to_string()),
                )
                .unwrap()
            }
            GroupConfig::Stake {
                denom,
                tokens_per_weight,
            } => {
                let stake_id = app.store_code(contract_stake());
                app.instantiate_contract(
                    stake_id,
                    admin.clone(),
                    &tg4_stake::msg::InstantiateMsg {
                        denom,
                        tokens_per_weight,
                        min_bond: Uint128::zero(),
                        unbonding_period: 0,
                        admin: Some(admin.to_string()),
                        preauths_hooks: 0,
                        preauths_slashing: 1,
                        auto_return_limit: 0,
                    },
                    &[],
                    "group",
                    Some(admin.to_string()),
                )
                .unwrap()
            }
        };

        self.operators.sort();
        self.operators.dedup();
        let operators: Vec<_> = self
            .operators
            .iter()
            .map(|member| {
                // If pubkey was previously generated, assign it
                // Otherwise, mock value
                let pubkey = match member.1.clone() {
                    Some(pubkey) => pubkey,
                    None => mock_pubkey(member.0.as_bytes()),
                };
                OperatorInitInfo {
                    operator: member.0.clone(),
                    validator_pubkey: pubkey,
                    metadata: mock_metadata(&member.0),
                }
            })
            .collect();

        let distribution_configs = self.distribution_configs;
        let distribution_contracts: Vec<_> = distribution_configs
            .iter()
            .cloned()
            .map(|config| {
                app.instantiate_contract(
                    engagement_id,
                    admin.clone(),
                    &tg4_engagement::msg::InstantiateMsg {
                        admin: Some(admin.to_string()),
                        members: config.members,
                        preauths_hooks: 0,
                        preauths_slashing: 1,
                        halflife: config.halflife,
                        denom: denom.clone(),
                    },
                    &[],
                    "distribution",
                    Some(admin.to_string()),
                )
                .unwrap()
            })
            .collect();

        let valset_id = app.store_code(contract_valset());
        let distribution_contract_instantiation_info = distribution_contracts
            .iter()
            .zip(distribution_configs)
            .map(|(addr, cfg)| UnvalidatedDistributionContract {
                contract: addr.to_string(),
                ratio: cfg.reward_ratio,
            })
            .collect();

        let valset = app
            .instantiate_contract(
                valset_id,
                admin.clone(),
                &InstantiateMsg {
                    admin: Some(admin.to_string()),
                    membership: membership.to_string(),
                    min_weight: self.min_weight,
                    max_validators: self.max_validators,
                    epoch_length: self.epoch_length,
                    epoch_reward: self.epoch_reward,
                    initial_keys: operators.clone(),
                    scaling: self.scaling,
                    fee_percentage: self.fee_percentage,
                    auto_unjail: self.auto_unjail,
                    double_sign_slash_ratio: self.double_sign_slash_ratio,
                    distribution_contracts: UnvalidatedDistributionContracts {
                        inner: distribution_contract_instantiation_info,
                    },
                    rewards_code_id: engagement_id,
                },
                &[],
                "valset",
                Some(admin.to_string()),
            )
            .unwrap();

        // Mint initial funds if any were specified
        for (addr, funds) in self.init_funds {
            for coin in funds {
                let block_info = app.block_info();
                let addr = addr.clone();
                let admin = admin.clone();

                app.init_modules(move |router, api, storage| {
                    router.execute(
                        api,
                        storage,
                        &block_info,
                        admin,
                        CosmosMsg::Custom(TgradeMsg::MintTokens {
                            denom: coin.denom,
                            amount: coin.amount,
                            recipient: addr,
                        })
                        .into(),
                    )
                })
                .unwrap();
            }
        }

        // promote the valset contract
        app.promote(admin.as_str(), valset.as_str()).unwrap();

        // process initial genesis block
        app.next_block().unwrap();

        // query for rewards contract
        let resp: Config = app
            .wrap()
            .query_wasm_smart(valset.clone(), &QueryMsg::Configuration {})
            .unwrap();

        Suite {
            app,
            valset,
            membership,
            distribution_contracts,
            admin: admin.to_string(),
            operators: operators.into_iter().map(|o| o.operator).collect(),
            epoch_length: self.epoch_length,
            denom,
            rewards_contract: resp.rewards_contract,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    /// Multitest app
    #[derivative(Debug = "ignore")]
    app: TgradeApp,
    /// tgrade-valset contract address
    valset: Addr,
    /// membership contract address
    membership: Addr,
    /// tg4-engagement contracts used e.g. for engagement distribution
    distribution_contracts: Vec<Addr>,
    /// Admin used for any administrative messages, but also admin of tgrade-valset contract
    admin: String,
    /// Valset operators included in `initial_keys`
    operators: Vec<String>,
    /// Length of an epoch
    epoch_length: u64,
    /// Reward denom
    denom: String,
    /// Rewards distribution contract address
    rewards_contract: Addr,
}

impl Suite {
    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn app(&mut self) -> &mut TgradeApp {
        &mut self.app
    }

    pub fn block_info(&self) -> BlockInfo {
        self.app.block_info()
    }

    pub fn next_block(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.next_block_with_evidence(vec![])
    }

    pub fn next_block_with_evidence(
        &mut self,
        evidences: Vec<Evidence>,
    ) -> AnyResult<Option<ValidatorDiff>> {
        self.app.update_block(next_block);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(evidences)?;
        Ok(diff)
    }

    pub fn advance_epoch(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        self.app.advance_seconds(self.epoch_length);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(diff)
    }

    pub fn advance_seconds(&mut self, seconds: u64) -> AnyResult<Option<ValidatorDiff>> {
        self.app.advance_seconds(seconds);
        let (_, diff) = self.app.end_block()?;
        self.app.begin_block(vec![])?;
        Ok(diff)
    }

    /// Timestamp of current block
    pub fn timestamp(&self) -> Timestamp {
        self.app.block_info().time
    }

    /// Height of current block
    pub fn height(&self) -> u64 {
        self.app.block_info().height
    }

    pub fn jail(
        &mut self,
        executor: &str,
        operator: &str,
        duration: impl Into<JailingDuration>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::Jail {
                operator: operator.to_owned(),
                duration: duration.into(),
            },
            &[],
        )
    }

    pub fn unjail<'a>(
        &mut self,
        executor: &str,
        operator: impl Into<Option<&'a str>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::Unjail {
                operator: operator.into().map(str::to_owned),
            },
            &[],
        )
    }

    pub fn register_validator_key(
        &mut self,
        executor: &str,
        pubkey: Pubkey,
        metadata: ValidatorMetadata,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::RegisterValidatorKey { pubkey, metadata },
            &[],
        )
    }

    pub fn update_metadata(
        &mut self,
        executor: &str,
        metadata: &ValidatorMetadata,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::UpdateMetadata(metadata.clone()),
            &[],
        )
    }

    pub fn update_admin(
        &mut self,
        executor: &str,
        admin: impl Into<Option<String>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::UpdateAdmin {
                admin: admin.into(),
            },
            &[],
        )
    }

    pub fn slash(
        &mut self,
        executor: &str,
        addr: &str,
        portion: Decimal,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.valset.clone(),
            &ExecuteMsg::Slash {
                addr: addr.to_owned(),
                portion,
            },
            &[],
        )
    }

    pub fn withdraw_distribution_reward(
        &mut self,
        executor: &str,
        distribution_contract_ix: usize,
    ) -> AnyResult<AppResponse> {
        if let Some(contract) = self.distribution_contracts.get(distribution_contract_ix) {
            self.app.execute_contract(
                Addr::unchecked(executor),
                contract.clone(),
                &tg4_engagement::msg::ExecuteMsg::WithdrawFunds {
                    owner: None,
                    receiver: None,
                },
                &[],
            )
        } else {
            bail!(
                "Distribution contract with index {} not found",
                distribution_contract_ix
            )
        }
    }

    pub fn withdraw_validation_reward(&mut self, executor: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.rewards_contract.clone(),
            &tg4_engagement::msg::ExecuteMsg::WithdrawFunds {
                owner: None,
                receiver: None,
            },
            &[],
        )
    }

    pub fn mint_rewards(&mut self, amount: u128) -> AnyResult<AppResponse> {
        let block_info = self.app.block_info();
        let denom = self.denom.clone();
        let admin = Addr::unchecked(&self.admin);
        let recipient = self.valset.to_string();
        self.app.init_modules(move |router, api, storage| {
            router.execute(
                api,
                storage,
                &block_info,
                admin,
                CosmosMsg::Custom(TgradeMsg::MintTokens {
                    denom,
                    amount: amount.into(),
                    recipient,
                })
                .into(),
            )
        })
    }

    pub fn query_admin(&self) -> StdResult<Option<String>> {
        let resp: AdminResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::Admin {})?;

        Ok(resp.admin)
    }

    pub fn list_validators(
        &self,
        start_after: impl Into<Option<String>>,
        limit: impl Into<Option<u32>>,
    ) -> StdResult<Vec<OperatorResponse>> {
        let resp: ListValidatorResponse = self.app.wrap().query_wasm_smart(
            self.valset.clone(),
            &QueryMsg::ListValidators {
                start_after: start_after.into(),
                limit: limit.into(),
            },
        )?;

        Ok(resp.validators)
    }

    pub fn list_active_validators(&self) -> StdResult<Vec<ValidatorInfo>> {
        let resp: ListActiveValidatorsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::ListActiveValidators {})?;

        Ok(resp.validators)
    }

    pub fn list_validator_slashing(&self, addr: &str) -> StdResult<ListValidatorSlashingResponse> {
        let resp = self.app.wrap().query_wasm_smart(
            self.valset.clone(),
            &QueryMsg::ListValidatorSlashing {
                operator: addr.to_owned(),
            },
        )?;

        Ok(resp)
    }

    pub fn simulate_active_validators(&self) -> StdResult<Vec<ValidatorInfo>> {
        let resp: ListActiveValidatorsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.valset.clone(), &QueryMsg::SimulateActiveValidators {})?;

        Ok(resp.validators)
    }

    /// Shortcut for querying reward token balance of contract
    pub fn token_balance(&self, owner: &str) -> StdResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(&Addr::unchecked(owner), &self.denom)?
            .amount;
        Ok(amount.into())
    }

    /// Queries valset contract for its config
    pub fn config(&self) -> StdResult<Config> {
        self.app
            .wrap()
            .query_wasm_smart(&self.valset, &QueryMsg::Configuration {})
    }

    /// Queries valset contract for epoch related info
    pub fn epoch(&self) -> StdResult<EpochResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.valset, &QueryMsg::Epoch {})
    }

    /// Queries valset contract for given validator info
    pub fn validator(&self, addr: &str) -> StdResult<ValidatorResponse> {
        self.app.wrap().query_wasm_smart(
            &self.valset,
            &QueryMsg::Validator {
                operator: addr.to_owned(),
            },
        )
    }

    /// Bonds some tokens.
    /// Only works when the membership contract is tg4_stake. Will error otherwise.
    pub fn bond(&mut self, addr: &Addr, stake: &[Coin]) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            addr.clone(),
            self.membership.clone(),
            &tg4_stake::msg::ExecuteMsg::Bond {},
            stake,
        )
    }

    /// Unbonds some tokens.
    /// Only works when the membership contract is tg4_stake. Will error otherwise.
    pub fn unbond(&mut self, addr: &Addr, tokens: Coin) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            addr.clone(),
            self.membership.clone(),
            &tg4_stake::msg::ExecuteMsg::Unbond { tokens },
            &[],
        )
    }
}
