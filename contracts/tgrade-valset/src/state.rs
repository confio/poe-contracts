use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use cosmwasm_std::Order::Ascending;
use cosmwasm_std::{to_binary, Addr, Coin, Decimal, Deps, DepsMut, Response, StdResult};
use cw2::{get_contract_version, set_contract_version, ContractVersion};
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, Map, UniqueIndex};
use tg4::Tg4Contract;

use crate::error::ContractError;
use crate::msg::{default_fee_percentage, JailingPeriod, OperatorResponse, ValidatorMetadata};
use tg_bindings::{Ed25519Pubkey, Pubkey, TgradeMsg, TgradeQuery};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// address of a tg4 contract with the raw membership used to feed the validator set
    pub membership: Tg4Contract,
    /// minimum points needed by an address in `membership` to be considered for the validator set.
    /// 0-point members are always filtered out.
    /// TODO: if we allow sub-1 scaling factors, determine if this is pre-/post- scaling
    /// (use points for tg4, power for tendermint)
    pub min_points: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership points
    /// descending. (In case of ties at the last slot, select by "first" tendermint pubkey
    /// lexicographically sorted).
    pub max_validators: u32,
    /// A scaling factor to multiply tg4-engagement points to produce the tendermint validator power
    /// (TODO: should we allow this to reduce points? Like 1/1000?)
    pub scaling: Option<u32>,
    /// Total reward paid out each epoch. This will be split among all validators during the last
    /// epoch.
    /// (epoch_reward.amount * 86_400 * 30 / epoch_length) is reward tokens to mint each month.
    /// Ensure this is sensible in relation to the total token supply.
    pub epoch_reward: Coin,
    /// Percentage of total accumulated fees which is subtracted from tokens minted as a rewards.
    /// 50% as default. To disable this feature just set it to 0 (which effectively means that fees
    /// doesn't affect the per epoch reward).
    #[serde(default = "default_fee_percentage")]
    pub fee_percentage: Decimal,
    /// Flag determining if validators should be automatically unjailed after jailing period, false
    /// by default.
    #[serde(default)]
    pub auto_unjail: bool,

    /// Validators who are caught double signing are jailed forever and their bonded tokens are
    /// slashed based on this value.
    pub double_sign_slash_ratio: Decimal,

    /// Addresses where part of the reward for non-validators is sent for further distribution. These are
    /// required to handle the `Distribute {}` message (eg. tg4-engagement contract) which would
    /// distribute the funds sent with this message.
    /// The sum of ratios here has to be in the [0, 1] range. The remainder is sent to validators via the
    /// rewards contract.
    pub distribution_contracts: Vec<DistributionContract>,

    /// Address of contract for validator group voting.
    pub validator_group: Addr,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct DistributionContract {
    pub contract: Addr,
    pub ratio: Decimal,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EpochInfo {
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    pub epoch_length: u64,
    /// The current epoch # (env.block.time/epoch_length, rounding down)
    pub current_epoch: u64,
    /// The last time we updated the validator set - block time and height
    pub last_update_time: u64,
    pub last_update_height: u64,
}

/// Tendermint public key, Operator SDK address, and tendermint voting power.
/// The order of fields in this struct defines the sort order of ValidatorDiff
/// additions and updates.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, JsonSchema, Debug)]
pub struct ValidatorInfo {
    /// TODO: better name to specify this is the Tendermint pubkey for consensus?
    pub validator_pubkey: Pubkey,
    pub operator: Addr,
    /// The voting power in Tendermint sdk
    pub power: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const EPOCH: Item<EpochInfo> = Item::new("epoch");

/// VALIDATORS is the calculated list of the active validators from the last execution.
/// This will be empty only on the first run.
pub const VALIDATORS: Item<Vec<ValidatorInfo>> = Item::new("validators");

/// Map of operator addr to block height it initially became a validator. If operator doesn't
/// appear in this map, he was never in the validator set.
pub const VALIDATOR_START_HEIGHT: Map<&Addr, u64> = Map::new("start_height");

/// Map of slashing events per operator address.
pub const VALIDATOR_SLASHING: Map<&Addr, Vec<ValidatorSlashing>> = Map::new("validator_slashing");

/// Map of jailed operator addr to jail expiration time. If operator doesn't appear in this map he
/// is not jailed
pub const JAIL: Map<&Addr, JailingPeriod> = Map::new("jail");

/// This stores the info for an operator. Both their Tendermint key as well as
/// their metadata.
#[derive(Serialize, Deserialize, Clone, JsonSchema, Debug, PartialEq)]
pub struct OperatorInfo {
    pub pubkey: Ed25519Pubkey,
    pub metadata: ValidatorMetadata,
    /// Is this currently an active validator?
    pub active_validator: bool,
}

/// This defines the stored and returned data for a slashing event.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValidatorSlashing {
    /// Block height of slashing event
    pub slash_height: u64,
    pub portion: Decimal,
}

/// All this to get a unique secondary index on the pubkey, so we can ensure uniqueness.
/// (It also allows reverse lookup from the pubkey to operator address if needed)
pub fn operators<'a>() -> IndexedMap<'a, &'a Addr, OperatorInfo, OperatorIndexes<'a>> {
    let indexes = OperatorIndexes {
        pubkey: UniqueIndex::new(|d| d.pubkey.to_vec(), "operators__pubkey"),
    };
    IndexedMap::new("operators", indexes)
}

pub struct OperatorIndexes<'a> {
    pub pubkey: UniqueIndex<'a, Vec<u8>, OperatorInfo>,
}

impl<'a> IndexList<OperatorInfo> for OperatorIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<OperatorInfo>> + '_> {
        let v: Vec<&dyn Index<OperatorInfo>> = vec![&self.pubkey];
        Box::new(v.into_iter())
    }
}

/// Export / Import state
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValsetState {
    pub contract_version: ContractVersion,
    pub config: Config,
    pub epoch: EpochInfo,
    pub operators: Vec<OperatorResponse>,
    pub validators: Vec<ValidatorInfo>,
    pub validators_start_height: Vec<(String, u64)>,
    pub validators_slashing: Vec<(String, Vec<ValidatorSlashing>)>,
    pub validators_jail: Vec<(String, JailingPeriod)>,
}

/// Export state
pub fn export(deps: Deps<TgradeQuery>) -> Result<Response<TgradeMsg>, ContractError> {
    // Valset state items
    let mut state = ValsetState {
        contract_version: get_contract_version(deps.storage)?,
        config: CONFIG.load(deps.storage)?,
        epoch: EPOCH.load(deps.storage)?,
        operators: vec![],
        validators: VALIDATORS.load(deps.storage)?,
        validators_start_height: vec![],
        validators_slashing: vec![],
        validators_jail: vec![],
    };

    // Operator items
    state.operators = operators()
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (operator, info) = r?;
            Ok(OperatorResponse::from_info(
                info,
                operator.to_string(),
                None,
            ))
        })
        .collect::<StdResult<_>>()?;

    // Validator start height items
    state.validators_start_height = VALIDATOR_START_HEIGHT
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (validator, height) = r?;
            Ok((validator.to_string(), height))
        })
        .collect::<StdResult<_>>()?;

    // Validator slashing items
    state.validators_slashing = VALIDATOR_SLASHING
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (validator, slashings) = r?;
            Ok((validator.to_string(), slashings))
        })
        .collect::<StdResult<_>>()?;

    // Validator jail items
    state.validators_jail = JAIL
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (validator, period) = r?;
            Ok((validator.to_string(), period))
        })
        .collect::<StdResult<_>>()?;

    Ok(Response::new().set_data(to_binary(&state)?))
}

/// Import state
pub fn import(
    deps: DepsMut<TgradeQuery>,
    state: ValsetState,
) -> Result<Response<TgradeMsg>, ContractError> {
    // Valset state items
    set_contract_version(
        deps.storage,
        state.contract_version.contract,
        state.contract_version.version,
    )?;
    CONFIG.save(deps.storage, &state.config)?;
    EPOCH.save(deps.storage, &state.epoch)?;
    VALIDATORS.save(deps.storage, &state.validators)?;

    // Operator items
    for op in state.operators {
        let info = OperatorInfo {
            pubkey: Ed25519Pubkey::try_from(op.pubkey)?,
            metadata: op.metadata,
            active_validator: op.active_validator,
        };
        operators().save(deps.storage, &Addr::unchecked(&op.operator), &info)?;
    }

    // Validator start height items
    for (k, v) in &state.validators_start_height {
        VALIDATOR_START_HEIGHT.save(deps.storage, &Addr::unchecked(k), v)?;
    }

    // Validator slashing items
    for (k, v) in &state.validators_slashing {
        VALIDATOR_SLASHING.save(deps.storage, &Addr::unchecked(k), v)?;
    }

    // Validator jail items
    for (k, v) in &state.validators_jail {
        JAIL.save(deps.storage, &Addr::unchecked(k), v)?;
    }

    Ok(Response::default())
}
