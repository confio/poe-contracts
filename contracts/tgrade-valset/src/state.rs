use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use cosmwasm_std::Order::Ascending;
use cosmwasm_std::{to_binary, Addr, Coin, Decimal, Deps, DepsMut, Response, StdResult};
use cw2::{get_contract_version, set_contract_version, ContractVersion};
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, Map, UniqueIndex};
use tg4::Tg4Contract;
use tg_utils::{Duration, ADMIN};

use crate::error::ContractError;
use crate::msg::{default_fee_percentage, JailingPeriod, OperatorResponse, ValidatorMetadata};
use tg_bindings::{Ed25519Pubkey, Pubkey, TgradeMsg, TgradeQuery};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    /// address of a tg4 contract with the raw membership used to feed the validator set
    pub membership: Tg4Contract,
    /// minimum points needed by an address in `membership` to be considered for the validator set.
    /// 0-point members are always filtered out.
    /// (use points for tg4, power for tendermint)
    pub min_points: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership points
    /// descending. (In case of ties at the last slot, select by "first" tendermint pubkey
    /// lexicographically sorted).
    pub max_validators: u32,
    /// A scaling factor to multiply tg4-engagement points to produce the tendermint validator power
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

    /// When a validator joins the valset, verify they sign the first block since joining
    /// or jail them for a period otherwise.
    ///
    /// The verification happens every time the validator becomes an active validator,
    /// including when they are unjailed or when they just gain enough power to participate.
    pub verify_validators: bool,

    /// The duration to jail a validator for in case they don't sign their first epoch
    /// boundary block. After the period, they have to pass verification again, ad infinitum.
    pub offline_jail_duration: Duration,
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

/// A map of validators to block heights they had last signed a block.
/// To verify they're online / active.
/// The key are the first 20 bytes of the SHA-256 hashed validator pubkey (from Cosmos SDK).
pub const BLOCK_SIGNERS: Map<&[u8], u64> = Map::new("block_signers");

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

/// Ancillary struct for exporting validator start height
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StartHeightResponse {
    pub validator: String,
    pub height: u64,
}

/// Ancillary struct for exporting validator slashing
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct SlashingResponse {
    pub validator: String,
    pub slashing: Vec<ValidatorSlashing>,
}

/// Export / Import state
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ValsetState {
    pub contract_version: ContractVersion,
    pub admin: Option<Addr>,
    pub config: Config,
    pub epoch: EpochInfo,
    pub operators: Vec<OperatorResponse>,
    pub validators: Vec<ValidatorInfo>,
    pub validators_start_height: Vec<StartHeightResponse>,
    pub validators_slashing: Vec<SlashingResponse>,
}

/// Export state
pub fn export(deps: Deps<TgradeQuery>) -> Result<Response<TgradeMsg>, ContractError> {
    // Valset state items
    let mut state = ValsetState {
        admin: ADMIN.get(deps)?,
        contract_version: get_contract_version(deps.storage)?,
        config: CONFIG.load(deps.storage)?,
        epoch: EPOCH.load(deps.storage)?,
        operators: vec![],
        validators: VALIDATORS.load(deps.storage)?,
        validators_start_height: vec![],
        validators_slashing: vec![],
    };

    // Operator items
    state.operators = operators()
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (operator, info) = r?;
            let jailed = JAIL.may_load(deps.storage, &operator)?;
            Ok(OperatorResponse::from_info(
                info,
                operator.to_string(),
                jailed,
            ))
        })
        .collect::<StdResult<_>>()?;

    // Validator start height items
    state.validators_start_height = VALIDATOR_START_HEIGHT
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (validator, height) = r?;
            Ok(StartHeightResponse {
                validator: validator.to_string(),
                height,
            })
        })
        .collect::<StdResult<_>>()?;

    // Validator slashing items
    state.validators_slashing = VALIDATOR_SLASHING
        .range(deps.storage, None, None, Ascending)
        .map(|r| {
            let (validator, slashings) = r?;
            Ok(SlashingResponse {
                validator: validator.to_string(),
                slashing: slashings,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(Response::new().set_data(to_binary(&state)?))
}

/// Import state
pub fn import(
    mut deps: DepsMut<TgradeQuery>,
    state: ValsetState,
) -> Result<Response<TgradeMsg>, ContractError> {
    // Valset state items
    set_contract_version(
        deps.storage,
        state.contract_version.contract,
        state.contract_version.version,
    )?;
    ADMIN.set(deps.branch(), state.admin)?;
    CONFIG.save(deps.storage, &state.config)?;
    EPOCH.save(deps.storage, &state.epoch)?;
    VALIDATORS.save(deps.storage, &state.validators)?;

    // Operator items
    // Delete all existing operators
    let ops = operators()
        .keys(deps.storage, None, None, Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    for op in ops.iter() {
        operators().remove(deps.storage, op)?;
    }
    // Delete all existing jails
    let jails = JAIL
        .keys(deps.storage, None, None, Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    for jail in jails.iter() {
        JAIL.remove(deps.storage, jail);
    }
    // Import operators
    for op in state.operators {
        let info = OperatorInfo {
            pubkey: Ed25519Pubkey::try_from(op.pubkey)?,
            metadata: op.metadata,
            active_validator: op.active_validator,
        };
        let addr = Addr::unchecked(&op.operator);
        operators().save(deps.storage, &addr, &info)?;
        op.jailed_until
            .map(|jp| JAIL.save(deps.storage, &addr, &jp))
            .transpose()?;
    }

    // Validator start height items
    // Delete all existing start heights
    let heights = VALIDATOR_START_HEIGHT
        .keys(deps.storage, None, None, Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    for height in heights.iter() {
        VALIDATOR_START_HEIGHT.remove(deps.storage, height);
    }
    // Import start heights
    for start_height in &state.validators_start_height {
        VALIDATOR_START_HEIGHT.save(
            deps.storage,
            &Addr::unchecked(&start_height.validator),
            &start_height.height,
        )?;
    }

    // Validator slashing items
    // Delete all existing slashings
    let slashings = VALIDATOR_SLASHING
        .keys(deps.storage, None, None, Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    for slash in slashings.iter() {
        VALIDATOR_SLASHING.remove(deps.storage, slash);
    }
    // Import slashings
    for slash in &state.validators_slashing {
        VALIDATOR_SLASHING.save(
            deps.storage,
            &Addr::unchecked(&slash.validator),
            &slash.slashing,
        )?;
    }

    Ok(Response::default())
}
