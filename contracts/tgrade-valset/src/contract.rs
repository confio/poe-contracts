use std::cmp::{max, min};
use std::collections::BTreeSet;
use std::convert::{TryFrom, TryInto};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, BlockInfo, CustomQuery, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, QueryRequest, Reply, StdError, StdResult, Timestamp, WasmMsg,
};

use cw2::set_contract_version;
use cw_controllers::AdminError;
use cw_storage_plus::Bound;
use cw_utils::{maybe_addr, parse_reply_instantiate_data};

use tg4::{Member, Tg4Contract};
use tg_bindings::{
    request_privileges, Ed25519Pubkey, Evidence, EvidenceType, Privilege, PrivilegeChangeMsg,
    Pubkey, TgradeMsg, TgradeQuery, TgradeSudoMsg, ToAddress, ValidatorDiff, ValidatorUpdate,
    ValidatorVoteResponse,
};
use tg_utils::{ensure_from_older_version, JailingDuration, SlashMsg, ADMIN};

use crate::error::ContractError;
use crate::migration::{migrate_jailing_period, migrate_verify_validators};
use crate::msg::{
    EpochResponse, ExecuteMsg, InstantiateMsg, InstantiateResponse, JailingEnd, JailingPeriod,
    ListActiveValidatorsResponse, ListValidatorResponse, ListValidatorSlashingResponse, MigrateMsg,
    OperatorResponse, QueryMsg, RewardsDistribution, RewardsInstantiateMsg, ValidatorMetadata,
    ValidatorResponse,
};
use crate::rewards::pay_block_rewards;
use crate::state::{
    export, import, operators, Config, EpochInfo, OperatorInfo, ValidatorInfo, ValidatorSlashing,
    ValsetState, BLOCK_SIGNERS, CONFIG, EPOCH, JAIL, VALIDATORS, VALIDATOR_SLASHING,
    VALIDATOR_START_HEIGHT,
};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:tgrade-valset";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const REWARDS_INIT_REPLY_ID: u64 = 1;

/// Missed blocks interval a validator can be jailed for.
pub const MISSED_BLOCKS: u64 = 1000;

/// We use this custom message everywhere
pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let denom = msg.epoch_reward.denom.clone();

    // verify the message and contract address are valid
    msg.validate()?;
    let membership = Tg4Contract(deps.api.addr_validate(&msg.membership)?);
    membership
        .total_points(&deps.querier)
        .map_err(|_| ContractError::InvalidTg4Contract {})?;
    let distribution_contracts = msg.distribution_contracts.validate(deps.api)?;

    let cfg = Config {
        membership,
        min_points: msg.min_points,
        max_validators: msg.max_validators,
        scaling: msg.scaling,
        epoch_reward: msg.epoch_reward,
        fee_percentage: msg.fee_percentage,
        auto_unjail: msg.auto_unjail,
        double_sign_slash_ratio: msg.double_sign_slash_ratio,
        distribution_contracts,
        // Will be overwritten in reply for rewards contract instantiation
        validator_group: Addr::unchecked(""),
        verify_validators: msg.verify_validators,
        offline_jail_duration: msg.offline_jail_duration,
    };
    CONFIG.save(deps.storage, &cfg)?;

    let epoch = EpochInfo {
        epoch_length: msg.epoch_length,
        current_epoch: 0,
        last_update_time: 0,
        last_update_height: 0,
    };
    EPOCH.save(deps.storage, &epoch)?;

    VALIDATORS.save(deps.storage, &vec![])?;

    for op in msg.initial_keys.into_iter() {
        let oper = deps.api.addr_validate(&op.operator)?;
        let pubkey: Ed25519Pubkey = op.validator_pubkey.try_into()?;
        op.metadata.validate()?;
        let info = OperatorInfo {
            pubkey,
            metadata: op.metadata,
            active_validator: false,
        };
        operators().save(deps.storage, &oper, &info)?;
    }

    if let Some(admin) = &msg.admin {
        let admin = deps.api.addr_validate(admin)?;
        ADMIN.set(deps, Some(admin))?;
    }

    let rewards_init = RewardsInstantiateMsg {
        admin: env.contract.address.clone(),
        denom,
        members: vec![],
    };

    let instantiate_rewards_msg = WasmMsg::Instantiate {
        admin: msg.admin,
        code_id: msg.validator_group_code_id,
        msg: to_binary(&rewards_init)?,
        funds: vec![],
        label: format!("rewards_distribution_{}", env.contract.address),
    };

    let add_slasher = SlashMsg::AddSlasher {
        addr: env.contract.address.to_string(),
    };
    let add_slasher_msg = WasmMsg::Execute {
        contract_addr: msg.membership,
        msg: to_binary(&add_slasher)?,
        funds: vec![],
    };

    let resp = Response::new()
        .add_submessage(SubMsg::reply_on_success(
            instantiate_rewards_msg,
            REWARDS_INIT_REPLY_ID,
        ))
        .add_submessage(SubMsg::new(add_slasher_msg));

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let api = deps.api;

    match msg {
        ExecuteMsg::UpdateAdmin { admin } => Ok(ADMIN.execute_update_admin(
            deps,
            info,
            admin.map(|admin| api.addr_validate(&admin)).transpose()?,
        )?),
        ExecuteMsg::UpdateConfig {
            min_points,
            max_validators,
        } => execute_update_config(deps, info, min_points, max_validators),

        ExecuteMsg::RegisterValidatorKey { pubkey, metadata } => {
            execute_register_validator_key(deps, env, info, pubkey, metadata)
        }
        ExecuteMsg::UpdateMetadata(metadata) => execute_update_metadata(deps, env, info, metadata),
        ExecuteMsg::Jail { operator, duration } => {
            execute_jail(deps, env, info, operator, duration)
        }
        ExecuteMsg::Unjail { operator } => execute_unjail(deps, env, info, operator),
        ExecuteMsg::Slash { addr, portion } => execute_slash(deps, env, info, addr, portion),
        #[cfg(debug_assertions)]
        ExecuteMsg::SimulateValidatorSet { validators } => {
            execute_simulate_validators(deps, info, validators)
        }
        ExecuteMsg::PubkeyToAddress { pubkey } => execute_pubkey_to_address(deps, info, pubkey),
    }
}

fn execute_update_config<Q: CustomQuery>(
    deps: DepsMut<Q>,
    info: MessageInfo,
    min_points: Option<u64>,
    max_validators: Option<u32>,
) -> Result<Response, ContractError> {
    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    CONFIG.update::<_, StdError>(deps.storage, |mut cfg| {
        if let Some(min_points) = min_points {
            cfg.min_points = min_points;
        }
        if let Some(max_validators) = max_validators {
            cfg.max_validators = max_validators;
        }
        Ok(cfg)
    })?;

    let res = Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("operator", &info.sender);

    Ok(res)
}

fn execute_register_validator_key<Q: CustomQuery>(
    deps: DepsMut<Q>,
    _env: Env,
    info: MessageInfo,
    pubkey: Pubkey,
    metadata: ValidatorMetadata,
) -> Result<Response, ContractError> {
    metadata.validate()?;

    let pubkey: Ed25519Pubkey = pubkey.try_into()?;
    let moniker = metadata.moniker.clone();

    let operator = OperatorInfo {
        pubkey,
        metadata,
        active_validator: false,
    };
    match operators().may_load(deps.storage, &info.sender)? {
        Some(_) => return Err(ContractError::OperatorRegistered {}),
        None => operators().save(deps.storage, &info.sender, &operator)?,
    };

    let res = Response::new()
        .add_attribute("action", "register_validator_key")
        .add_attribute("operator", &info.sender)
        .add_attribute("pubkey_type", "ed25519")
        .add_attribute("pubkey_value", operator.pubkey.to_base64())
        .add_attribute("moniker", moniker);

    Ok(res)
}

fn execute_update_metadata<Q: CustomQuery>(
    deps: DepsMut<Q>,
    _env: Env,
    info: MessageInfo,
    metadata: ValidatorMetadata,
) -> Result<Response, ContractError> {
    metadata.validate()?;
    let moniker = metadata.moniker.clone();

    operators().update(deps.storage, &info.sender, |info| match info {
        Some(mut old) => {
            old.metadata = metadata;
            Ok(old)
        }
        None => Err(ContractError::Unauthorized(
            "No operator info found".to_owned(),
        )),
    })?;

    let res = Response::new()
        .add_attribute("action", "update_metadata")
        .add_attribute("operator", &info.sender)
        .add_attribute("moniker", moniker);
    Ok(res)
}

fn execute_jail<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    operator: String,
    duration: JailingDuration,
) -> Result<Response, ContractError> {
    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    let expiration = JailingPeriod::from_duration(duration, &env.block);

    JAIL.save(
        deps.storage,
        &deps.api.addr_validate(&operator)?,
        &expiration,
    )?;

    let until_attr = match expiration.end {
        JailingEnd::Until(expires) => Timestamp::from(expires).to_string(),
        JailingEnd::Forever {} => "forever".to_owned(),
    };

    let res = Response::new()
        .add_attribute("action", "jail")
        .add_attribute("operator", &operator)
        .add_attribute("until", &until_attr);

    Ok(res)
}

fn execute_unjail<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    operator: Option<String>,
) -> Result<Response, ContractError> {
    // It is OK to get unchecked address here - invalid address would just not occur in the JAIL
    let operator = operator.map(|op| Addr::unchecked(&op));
    let operator = operator.as_ref().unwrap_or(&info.sender);

    let is_admin = ADMIN.is_admin(deps.as_ref(), &info.sender)?;

    if operator != &info.sender && !is_admin {
        return Err(AdminError::NotAdmin {}.into());
    }

    // if this is `None`, the validator was not unjailed and unjailing succeeds
    if let Some(expiration) = JAIL.may_load(deps.storage, operator)? {
        if expiration.is_forever() {
            return Err(ContractError::UnjailFromJailForeverForbidden {});
        }

        if expiration.is_expired(&env.block) || is_admin {
            JAIL.remove(deps.storage, operator);
        } else {
            return Err(ContractError::JailDidNotExpire {});
        }
    }

    let res = Response::new()
        .add_attribute("action", "unjail")
        .add_attribute("operator", operator.as_str());

    Ok(res)
}

fn store_slashing_event<Q: CustomQuery>(
    deps: DepsMut<Q>,
    env: &Env,
    addr: Addr,
    portion: Decimal,
) -> Result<(), ContractError> {
    let mut slashing = VALIDATOR_SLASHING
        .may_load(deps.storage, &addr)?
        .unwrap_or_default();
    slashing.push(ValidatorSlashing {
        slash_height: env.block.height,
        portion,
    });
    VALIDATOR_SLASHING.save(deps.storage, &addr, &slashing)?;
    Ok(())
}

fn execute_slash<Q: CustomQuery>(
    mut deps: DepsMut<Q>,
    env: Env,
    info: MessageInfo,
    operator: String,
    portion: Decimal,
) -> Result<Response, ContractError> {
    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    // Store slashing event
    let addr = Addr::unchecked(&operator);
    store_slashing_event(deps.branch(), &env, addr, portion)?;

    let config = CONFIG.load(deps.storage)?;

    let slash_msg = SlashMsg::Slash {
        addr: operator,
        portion,
    };
    let slash_msg = to_binary(&slash_msg)?;

    let slash_msg = WasmMsg::Execute {
        contract_addr: config.membership.addr().to_string(),
        msg: slash_msg,
        funds: vec![],
    };

    let resp = Response::new().add_submessage(SubMsg::new(slash_msg));

    Ok(resp)
}

#[cfg(debug_assertions)]
fn execute_simulate_validators<Q: CustomQuery>(
    deps: DepsMut<Q>,
    _info: MessageInfo,
    validators: Vec<ValidatorInfo>,
) -> Result<Response, ContractError> {
    // Assert admin is not found (contract not instantiated!) for this to be possible
    let admin_res = ADMIN.get(deps.as_ref());
    if let Err(err) = admin_res {
        let not_found = matches!(err, StdError::NotFound { .. });
        if !not_found {
            return Err(ContractError::AdminError(AdminError::NotAdmin {}));
        }
    } else {
        return Err(ContractError::AdminError(AdminError::NotAdmin {}));
    }

    // Store validators
    VALIDATORS.save(deps.storage, &validators)?;

    Ok(Response::new())
}

fn execute_pubkey_to_address<Q: CustomQuery>(
    _deps: DepsMut<Q>,
    _info: MessageInfo,
    pubkey: Ed25519Pubkey,
) -> Result<Response, ContractError> {
    Ok(Response::new().set_data(pubkey.to_address()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TgradeQuery>, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    use QueryMsg::*;
    match msg {
        Configuration {} => Ok(to_binary(&CONFIG.load(deps.storage)?)?),
        Epoch {} => Ok(to_binary(&query_epoch(deps, env)?)?),
        Validator { operator } => Ok(to_binary(&query_validator_key(deps, env, operator)?)?),
        ListValidators { start_after, limit } => Ok(to_binary(&list_validator_keys(
            deps,
            env,
            start_after,
            limit,
        )?)?),
        ListActiveValidators { start_after, limit } => Ok(to_binary(&list_active_validators(
            deps,
            start_after,
            limit,
        )?)?),
        ListJailedValidators { start_after, limit } => Ok(to_binary(&list_jailed_validators(
            deps,
            env,
            start_after,
            limit,
        )?)?),
        SimulateActiveValidators {} => Ok(to_binary(&simulate_active_validators(deps, env)?)?),
        ListValidatorSlashing { operator } => {
            Ok(to_binary(&list_validator_slashing(deps, env, operator)?)?)
        }
        Admin {} => Ok(to_binary(&ADMIN.query_admin(deps)?)?),
    }
}

fn query_epoch<Q: CustomQuery>(deps: Deps<Q>, env: Env) -> Result<EpochResponse, ContractError> {
    let epoch = EPOCH.load(deps.storage)?;
    let mut next_update_time =
        Timestamp::from_seconds((epoch.current_epoch + 1) * epoch.epoch_length);
    if env.block.time > next_update_time {
        next_update_time = env.block.time;
    }

    let resp = EpochResponse {
        epoch_length: epoch.epoch_length,
        current_epoch: epoch.current_epoch,
        last_update_time: epoch.last_update_time,
        last_update_height: epoch.last_update_height,
        next_update_time: next_update_time.nanos() / 1_000_000_000,
    };
    Ok(resp)
}

fn query_validator_key<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    operator: String,
) -> Result<ValidatorResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let operator_addr = deps.api.addr_validate(&operator)?;
    let info = operators().may_load(deps.storage, &operator_addr)?;

    let jailed_until = JAIL
        .may_load(deps.storage, &operator_addr)?
        .filter(|expires| !(cfg.auto_unjail && expires.is_expired(&env.block)));

    Ok(ValidatorResponse {
        validator: info.map(|i| OperatorResponse::from_info(i, operator, jailed_until)),
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 30;

fn list_validator_keys<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<ListValidatorResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = maybe_addr(deps.api, start_after)?;
    let start = start_after.as_ref().map(Bound::exclusive);

    let operators: StdResult<Vec<_>> = operators()
        .range(deps.storage, start, None, Order::Ascending)
        .map(|r| {
            let (operator, info) = r?;

            let jailed_until = JAIL
                .may_load(deps.storage, &Addr::unchecked(&operator))?
                .filter(|expires| !(cfg.auto_unjail && expires.is_expired(&env.block)));

            Ok(OperatorResponse {
                operator: operator.into(),
                metadata: info.metadata,
                pubkey: info.pubkey.into(),
                jailed_until,
                active_validator: info.active_validator,
            })
        })
        .take(limit)
        .collect();

    Ok(ListValidatorResponse {
        validators: operators?,
    })
}

fn list_active_validators<Q: CustomQuery>(
    deps: Deps<Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = maybe_addr(deps.api, start_after)?;

    let validators = VALIDATORS.load(deps.storage)?;
    // Simulate a range query
    let mut i = 0;
    if let Some(start_after) = start_after {
        for v in &validators {
            if v.operator == start_after {
                i += 1;
                break;
            }
            i += 1;
        }
    }
    let validators = &validators[i..min(i + limit, validators.len())];
    Ok(ListActiveValidatorsResponse {
        validators: Vec::from(validators),
    })
}

fn list_jailed_validators<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<ListValidatorResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = maybe_addr(deps.api, start_after)?;
    let start = start_after.as_ref().map(Bound::exclusive);

    let validators = JAIL
        .range(deps.storage, start, None, Order::Ascending)
        .map(|jail| {
            let (addr, jailing_period) = jail?;
            if !(cfg.auto_unjail && jailing_period.is_expired(&env.block)) {
                Ok(Some((addr, jailing_period)))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<Option<_>>, ContractError>>()?
        .into_iter()
        .flatten()
        .map(|(addr, jailing_period)| {
            let info = operators().load(deps.storage, &Addr::unchecked(&addr))?;
            Ok(OperatorResponse {
                operator: addr.into(),
                metadata: info.metadata,
                pubkey: info.pubkey.into(),
                jailed_until: Some(jailing_period),
                active_validator: info.active_validator,
            })
        })
        .take(limit)
        .collect::<Result<Vec<OperatorResponse>, ContractError>>()?;

    Ok(ListValidatorResponse { validators })
}

fn simulate_active_validators<Q: CustomQuery>(
    deps: Deps<Q>,
    env: Env,
) -> Result<ListActiveValidatorsResponse, ContractError> {
    let (validators, _) = calculate_validators(deps, &env)?;
    Ok(ListActiveValidatorsResponse { validators })
}

fn list_validator_slashing<Q: CustomQuery>(
    deps: Deps<Q>,
    _env: Env,
    operator: String,
) -> Result<ListValidatorSlashingResponse, ContractError> {
    let addr = deps.api.addr_validate(&operator)?;
    // Fails if never a validator (which is correct)
    let start_height = VALIDATOR_START_HEIGHT
        .load(deps.storage, &addr)
        .map_err(|_| ContractError::NeverAValidator(operator.clone()))?;
    let slashing = VALIDATOR_SLASHING
        .may_load(deps.storage, &addr)?
        .unwrap_or_default();
    let (jailed_until, tombstoned) = match JAIL.may_load(deps.storage, &addr)?.map(|j| j.end) {
        Some(JailingEnd::Forever {}) => (None, true),
        Some(JailingEnd::Until(u)) => (Some(u), false),
        None => (None, false),
    };
    Ok(ListValidatorSlashingResponse {
        addr: operator,
        start_height,
        slashing,
        tombstoned,
        jailed_until,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(
    deps: DepsMut<TgradeQuery>,
    env: Env,
    msg: TgradeSudoMsg<ValsetState>,
) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(change) => Ok(privilege_change(deps, change)),
        TgradeSudoMsg::EndWithValidatorUpdate {} => end_block(deps, env),
        TgradeSudoMsg::BeginBlock { evidence } => begin_block(deps, env, evidence),
        TgradeSudoMsg::Export {} => export(deps.as_ref()),
        TgradeSudoMsg::Import(state) => import(deps, state),
        _ => Err(ContractError::UnsupportedSudoType {}),
    }
}

fn privilege_change<Q: CustomQuery>(_deps: DepsMut<Q>, change: PrivilegeChangeMsg) -> Response {
    match change {
        PrivilegeChangeMsg::Promoted {} => {
            let msgs = request_privileges(&[
                Privilege::ValidatorSetUpdater,
                Privilege::TokenMinter,
                Privilege::BeginBlocker,
                Privilege::StateExporterImporter,
            ]);
            Response::new().add_submessages(msgs)
        }
        PrivilegeChangeMsg::Demoted {} => Response::new(),
    }
}

/// returns true if this is an initial block, maybe part of InitGenesis processing,
/// or other bootstrapping.
fn is_genesis_block(block: &BlockInfo) -> bool {
    // not sure if this will manifest as height 0 or 1, so treat them both as startup
    // this will force re-calculation on the end_block, no issues in startup.
    block.height < 2
}

fn end_block(deps: DepsMut<TgradeQuery>, env: Env) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if cfg.verify_validators {
        // Update the block signers height at each block
        deps.querier
            .query::<ValidatorVoteResponse>(&QueryRequest::Custom(TgradeQuery::ValidatorVotes {}))?
            .votes
            .iter()
            .filter(|&v| v.voted)
            .try_for_each(|v| {
                BLOCK_SIGNERS.save(deps.storage, v.address.as_slice(), &env.block.height)
            })?;
    }

    // check if needed and quit early if we didn't hit epoch boundary
    let mut epoch = EPOCH.load(deps.storage)?;
    let cur_epoch = env.block.time.nanos() / (1_000_000_000 * epoch.epoch_length);

    if cur_epoch <= epoch.current_epoch && !is_genesis_block(&env.block) {
        return Ok(Response::default());
    }
    // we don't pay the first epoch, as this may be huge if contract starts at non-zero height
    let pay_epochs = if epoch.current_epoch == 0 {
        0
    } else {
        cur_epoch - epoch.current_epoch
    };

    // ensure to update this so we wait until next epoch to run this again
    epoch.current_epoch = cur_epoch;
    EPOCH.save(deps.storage, &epoch)?;

    if cfg.verify_validators {
        let expiration = JailingPeriod::from_duration(
            JailingDuration::Duration(cfg.offline_jail_duration),
            &env.block,
        );

        VALIDATORS
            .load(deps.storage)?
            .iter()
            .flat_map(|v| match Ed25519Pubkey::try_from(&v.validator_pubkey) {
                Ok(pubkey) => Some((v, pubkey)),
                _ => None, // Silently ignore wrong / different type pubkeys
            })
            .try_for_each(|(v, ed25519_pubkey)| {
                let operator_addr = &v.operator;
                let validator_addr = ed25519_pubkey.to_address();
                let mut height = BLOCK_SIGNERS.may_load(deps.storage, &validator_addr)?;
                if height.is_none() {
                    // Not a block signer yet, check their validator start height instead
                    height = VALIDATOR_START_HEIGHT.may_load(deps.storage, operator_addr)?;
                }
                match height {
                    Some(h) if h > env.block.height.saturating_sub(MISSED_BLOCKS) => Ok(()),
                    _ => {
                        // validator is inactive for at least MISSED_BLOCKS, jail!
                        JAIL.save(deps.storage, operator_addr, &expiration)
                    }
                }
            })?;
    }

    // calculate and store new validator set
    let (validators, auto_unjail) = calculate_validators(deps.as_ref(), &env)?;

    // auto unjailing
    for addr in &auto_unjail {
        JAIL.remove(deps.storage, addr)
    }

    let old_validators = VALIDATORS.load(deps.storage)?;

    // determine the diff to send back to tendermint
    let (diff, add, remove) = calculate_diff(validators.clone(), old_validators);
    let update_members = RewardsDistribution::UpdateMembers {
        add: add.clone(),
        remove: remove.clone(),
    };

    VALIDATORS.save(deps.storage, &validators)?;

    // update operators list with info about whether or not they're active validators
    for op in add {
        operators().update::<_, StdError>(deps.storage, &Addr::unchecked(op.addr), |op| {
            let mut op = op.ok_or_else(|| StdError::generic_err("operator doesn't exist"))?;
            op.active_validator = true;
            Ok(op)
        })?;
    }
    for op in remove {
        operators().update::<_, StdError>(deps.storage, &Addr::unchecked(op), |op| {
            let mut op = op.ok_or_else(|| StdError::generic_err("operator doesn't exist"))?;
            op.active_validator = false;
            Ok(op)
        })?;
    }

    // Store starting heights of new validators
    match &update_members {
        RewardsDistribution::UpdateMembers { add, .. } => {
            for member in add {
                let addr = Addr::unchecked(member.addr.clone());
                if VALIDATOR_START_HEIGHT
                    .may_load(deps.storage, &addr)?
                    .is_none()
                {
                    // see https://github.com/confio/tgrade-contracts/pull/309#discussion_r748164514
                    // for details
                    // Validator is added on an epoch boundary only. But next block contains proof of
                    // the new validator
                    VALIDATOR_START_HEIGHT.save(deps.storage, &addr, &(env.block.height + 1))?;
                }
            }
        }
        _ => unreachable!(),
    }

    // provide payment if there are rewards to give
    let mut res = Response::new().set_data(to_binary(&diff)?);
    if pay_epochs > 0 {
        res.messages = pay_block_rewards(deps, env, pay_epochs, &cfg)?
    };

    let res = res.add_submessage(SubMsg::new(WasmMsg::Execute {
        contract_addr: cfg.validator_group.to_string(),
        msg: to_binary(&update_members)?,
        funds: vec![],
    }));

    Ok(res)
}

const QUERY_LIMIT: Option<u32> = Some(30);

/// Selects validators to be used for incoming epoch. Returns vector of validators info paired
/// with vector of addresses to be un-jailed (always empty if auto un-jailing is disabled).
fn calculate_validators<Q: CustomQuery>(
    deps: Deps<Q>,
    env: &Env,
) -> Result<(Vec<ValidatorInfo>, Vec<Addr>), ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let min_points = max(cfg.min_points, 1);
    let scaling: u64 = cfg.scaling.unwrap_or(1).into();

    // get all validators from the contract, filtered
    let mut validators = vec![];
    let mut batch = cfg
        .membership
        .list_members_by_points(&deps.querier, None, QUERY_LIMIT)?;
    let mut auto_unjail = vec![];

    while !batch.is_empty() && validators.len() < cfg.max_validators as usize {
        let last = Some(batch.last().unwrap().clone());

        let filtered: Vec<_> = batch
            .into_iter()
            .filter(|m| m.points >= min_points)
            .filter_map(|m| -> Option<StdResult<_>> {
                // why do we allow Addr::unchecked here?
                // all valid keys for `operators()` are already validated before insertion
                // we have 3 cases:
                // 1. There is a match with operators().load(), this means it is a valid address and
                //    has a pubkey registered -> count in our group
                // 2. The address is valid, but has no pubkey registered in operators() -> skip
                // 3. The address is invalid -> skip
                //
                // All 3 cases are handled properly below (operators.load() returns an Error on
                // both 2 and 3), so we do not need to perform N addr_validate calls here
                let m_addr = Addr::unchecked(&m.addr);

                // check if address is jailed
                match JAIL.may_load(deps.storage, &m_addr) {
                    Err(err) => return Some(Err(err)),
                    // address not jailed, proceed
                    Ok(None) => (),
                    // address jailed, but period expired and auto unjailing enabled, add to
                    // auto_unjail list
                    Ok(Some(expires)) if cfg.auto_unjail && expires.is_expired(&env.block) => {
                        auto_unjail.push(m_addr.clone())
                    }
                    // address jailed and cannot be unjailed - filter validator out
                    _ => return None,
                };

                operators().load(deps.storage, &m_addr).ok().map(|op| {
                    Ok(ValidatorInfo {
                        operator: m_addr,
                        validator_pubkey: op.pubkey.into(),
                        power: m.points * scaling,
                    })
                })
            })
            .take(cfg.max_validators as usize - validators.len() as usize)
            .collect::<Result<_, _>>()?;
        validators.extend_from_slice(&filtered);

        // and get the next page
        batch = cfg
            .membership
            .list_members_by_points(&deps.querier, last, QUERY_LIMIT)?;
    }

    Ok((validators, auto_unjail))
}

/// Computes validator differences.
///
/// The diffs are calculated by computing two (slightly different) differences:
/// - In `cur` but not in `old` (comparing by `operator` and `power`) => update with `cur` (handles additions and updates).
/// - In `old` but not in `cur` (comparing by `validator_pubkey` only) => update with `old`, set power to zero (handles removals).
///
/// Uses `validator_pubkey` instead of `operator`, to use the derived `Ord` and `PartialOrd` impls for it.
/// `operators` and `pubkeys` are one-to-one, so this is legit.
///
/// Uses a `BTreeSet`, so computed differences are stable / sorted.
/// The order is defined by the order of fields in the `ValidatorInfo` struct, for
/// additions and updates, and by `validator_pubkey`, for removals.
/// Additions and updates (power > 0) come first, and then removals (power == 0);
/// and, each group is ordered in turn by `validator_pubkey` ascending.
fn calculate_diff(
    cur_vals: Vec<ValidatorInfo>,
    old_vals: Vec<ValidatorInfo>,
) -> (ValidatorDiff, Vec<Member>, Vec<String>) {
    // Compute additions and updates
    let cur: BTreeSet<_> = cur_vals.iter().collect();
    let old: BTreeSet<_> = old_vals.iter().collect();
    let (mut diffs, add): (Vec<_>, Vec<_>) = cur
        .difference(&old)
        .map(|vi| {
            let update = ValidatorUpdate {
                pubkey: vi.validator_pubkey.clone(),
                power: vi.power,
            };
            let member = Member {
                addr: vi.operator.to_string(),
                points: vi.power,
                start_height: None,
            };

            (update, member)
        })
        .unzip();

    // Compute removals
    let cur: BTreeSet<_> = cur_vals
        .iter()
        .map(|vi| (&vi.validator_pubkey, &vi.operator))
        .collect();
    let old: BTreeSet<_> = old_vals
        .iter()
        .map(|vi| (&vi.validator_pubkey, &vi.operator))
        .collect();

    let (removed_diff, remove): (Vec<_>, Vec<_>) = old
        .difference(&cur)
        .map(|&(pubkey, operator)| {
            let update = ValidatorUpdate {
                pubkey: pubkey.clone(),
                power: 0,
            };
            let member = operator.to_string();

            (update, member)
        })
        .unzip();

    // Compute, map and append removals to diffs
    diffs.extend(removed_diff);

    (ValidatorDiff { diffs }, add, remove)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    mut deps: DepsMut<TgradeQuery>,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response, ContractError> {
    let original_version =
        ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.update::<_, StdError>(deps.storage, |mut cfg| {
        if let Some(min_points) = msg.min_points {
            cfg.min_points = min_points;
        }
        if let Some(max_validators) = msg.max_validators {
            cfg.max_validators = max_validators;
        }
        Ok(cfg)
    })?;

    migrate_jailing_period(deps.branch(), &original_version)?;

    migrate_verify_validators(deps.branch(), &original_version)?;

    Ok(Response::new())
}

mod evidence {
    use super::*;

    use tg_bindings::{ToAddress, Validator};

    /// Validator struct contains only hash of first 20 bytes of validator's pub key
    /// (sha256), while contract keeps only pub keys. To match potential reported
    /// suspect, this function computes sha256 hashes for all existing validator and
    /// compares result with suspect. It is acceptable approach, since it shouldn't
    /// happen too often.
    pub fn find_matching_validator<Q: CustomQuery>(
        deps: Deps<Q>,
        suspect: &Validator,
        evidence_height: u64,
    ) -> Result<Option<Addr>, cosmwasm_std::StdError> {
        VALIDATOR_START_HEIGHT
            .range(deps.storage, None, None, Order::Ascending)
            .find_map(|r| {
                r.and_then(|(addr, start_height)| {
                    // Makes sure validator was active before evidence was reported
                    if start_height >= evidence_height {
                        return Ok(None);
                    }
                    let operator = operators().load(deps.storage, &addr)?;
                    let hash = Binary::from(operator.pubkey.to_address());
                    if hash == suspect.address {
                        return Ok(Some(addr));
                    }
                    Ok(None)
                })
                .transpose()
            })
            .transpose()
    }

    pub fn slash_validator_msg(config: &Config, addr: String) -> Result<SubMsg, ContractError> {
        let slash_msg = SlashMsg::Slash {
            addr,
            portion: config.double_sign_slash_ratio,
        };
        let slash_msg = to_binary(&slash_msg)?;

        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: config.membership.addr().to_string(),
            msg: slash_msg,
            funds: vec![],
        }))
    }
}

/// If some validators are caught on malicious behavior (for example double signing),
/// they are reported and punished on begin of next block.
fn begin_block(
    mut deps: DepsMut<TgradeQuery>,
    env: Env,
    evidences: Vec<Evidence>,
) -> Result<Response, ContractError> {
    // Early exit saves couple loads from below if there are no evidences at all.
    if evidences.is_empty() {
        return Ok(Response::new());
    }

    let config = CONFIG.load(deps.storage)?;

    let mut response = Response::new();

    evidences
        .iter()
        .flat_map(|e| match e.evidence_type {
            EvidenceType::DuplicateVote => Some((e.validator.clone(), e.height)),
            _ => None,
        })
        .map(|(validator, evidence_height)| {
            // If there's match between evidence validator's hash and one from list of validators,
            // then jail and slash that validator
            if let Some(validator) =
                evidence::find_matching_validator(deps.as_ref(), &validator, evidence_height)?
            {
                let sub_msg = evidence::slash_validator_msg(&config, validator.to_string())?;
                store_slashing_event(
                    deps.branch(),
                    &env,
                    validator.clone(),
                    config.double_sign_slash_ratio,
                )?;

                JAIL.save(
                    deps.storage,
                    &validator,
                    &JailingPeriod::from_duration(JailingDuration::Forever {}, &env.block),
                )?;

                response = response
                    .clone()
                    .add_attribute("action", "slash_and_jail")
                    .add_attribute("validator", validator.as_str())
                    .add_submessage(sub_msg);
            }
            Ok(())
        })
        .collect::<Result<Vec<()>, ContractError>>()?;

    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut<TgradeQuery>, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        REWARDS_INIT_REPLY_ID => rewards_instantiate_reply(deps, env, msg),
        _ => Err(ContractError::UnrecognisedReply(msg.id)),
    }
}

pub fn rewards_instantiate_reply<Q: CustomQuery>(
    deps: DepsMut<Q>,
    _env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let id = msg.id;
    let res =
        parse_reply_instantiate_data(msg).map_err(|err| ContractError::ReplyParseFailure {
            id,
            err: err.to_string(),
        })?;

    let addr = deps.api.addr_validate(&res.contract_address)?;
    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.validator_group = addr.clone();
        Ok(config)
    })?;

    let data = InstantiateResponse {
        validator_group: addr,
    };

    let resp = Response::new().set_data(to_binary(&data)?);

    Ok(resp)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_helpers::{addrs, valid_validator};

    // Number of validators for tests
    const VALIDATORS: usize = 32;

    fn validators(count: usize) -> Vec<ValidatorInfo> {
        let mut p: u64 = 0;
        let vals: Vec<_> = addrs(count as u32)
            .into_iter()
            .map(|s| {
                p += 1;
                valid_validator(&s, p)
            })
            .collect();
        vals
    }

    fn members(members: Vec<(&str, u64)>) -> Vec<Member> {
        members
            .into_iter()
            .map(|(addr, points)| Member {
                addr: addr.to_owned(),
                points,
                start_height: None,
            })
            .collect()
    }

    // Unit tests for calculate_diff()
    // TODO: Split it to actual unit tests. This single test has over 100 lines of code and 7 calls
    // to tested function - it should be 7 unit tests.
    #[test]
    fn test_calculate_diff_simple() {
        let empty: Vec<_> = vec![];
        let vals: Vec<_> = vec![
            ValidatorInfo {
                operator: Addr::unchecked("op1"),
                validator_pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 1,
            },
            ValidatorInfo {
                operator: Addr::unchecked("op2"),
                validator_pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                power: 2,
            },
        ];

        // diff with itself must be empty
        let (diff, add, remove) = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs, vec![]);
        assert_eq!((add, remove), (vec![], vec![]));

        // diff with empty must be itself (additions)
        let (diff, add, remove) = calculate_diff(vals.clone(), empty.clone());
        assert_eq!(
            vec![
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                    power: 1,
                },
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                    power: 2,
                },
            ],
            diff.diffs
        );
        assert!(remove.is_empty());
        assert_eq!(add, members(vec![("op1", 1), ("op2", 2)]));

        // diff between empty and vals must be removals
        let (diff, add, remove) = calculate_diff(empty, vals.clone());
        assert_eq!(
            vec![
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                    power: 0,
                },
                ValidatorUpdate {
                    pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                    power: 0,
                },
            ],
            diff.diffs
        );
        assert!(add.is_empty());
        assert_eq!(remove, ["op1", "op2"]);

        // Add a new member
        let mut cur = vals.clone();
        cur.push(ValidatorInfo {
            operator: Addr::unchecked("op3"),
            validator_pubkey: Pubkey::Ed25519(b"pubkey3".into()),
            power: 3,
        });

        // diff must be add last
        let (diff, add, remove) = calculate_diff(cur, vals.clone());
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey3".into()),
                power: 3,
            },],
            diff.diffs
        );
        assert!(remove.is_empty());
        assert_eq!(add, members(vec![("op3", 3)]));

        // add all but (one) last member
        let old: Vec<_> = vals.iter().skip(1).cloned().collect();

        // diff must be add all but last
        let (diff, add, remove) = calculate_diff(vals.clone(), old);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 1,
            },],
            diff.diffs
        );
        assert!(remove.is_empty());
        assert_eq!(add, members(vec![("op1", 1)]));

        // remove last member
        let cur: Vec<_> = vals.iter().take(1).cloned().collect();
        // diff must be remove last
        let (diff, add, remove) = calculate_diff(cur, vals.clone());
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey2".into()),
                power: 0,
            },],
            diff.diffs
        );
        assert!(add.is_empty());
        assert_eq!(remove, ["op2"]);

        // remove all but last member
        let cur: Vec<_> = vals.iter().skip(1).cloned().collect();
        // diff must be remove all but last
        let (diff, add, remove) = calculate_diff(cur, vals);
        assert_eq!(
            vec![ValidatorUpdate {
                pubkey: Pubkey::Ed25519(b"pubkey1".into()),
                power: 0,
            },],
            diff.diffs
        );
        assert!(add.is_empty());
        assert_eq!(remove, ["op1"]);
    }

    // TODO: Another 7 in 1 test to be split
    #[test]
    fn test_calculate_diff() {
        let empty: Vec<_> = vec![];
        let vals = validators(VALIDATORS);

        // diff with itself must be empty
        let (diff, add, remove) = calculate_diff(vals.clone(), vals.clone());
        assert_eq!(diff.diffs, vec![]);
        assert!(add.is_empty());
        assert!(remove.is_empty());

        // diff with empty must be itself (additions)
        let (diff, add, remove) = calculate_diff(vals.clone(), empty.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: vi.power,
                    })
                    .collect()
            },
            diff
        );
        assert!(remove.is_empty());
        assert_eq!(
            add,
            members(
                vals.iter()
                    .map(|vi| (vi.operator.as_str(), vi.power))
                    .collect()
            )
        );

        // diff between empty and vals must be removals
        let (diff, add, remove) = calculate_diff(empty, vals.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: 0,
                    })
                    .collect()
            },
            diff
        );
        assert!(add.is_empty());
        assert_eq!(
            remove,
            vals.iter()
                .map(|vi| vi.operator.as_str())
                .collect::<Vec<_>>()
        );

        // Add a new member
        let cur = validators(VALIDATORS + 1);

        // diff must be add last
        let (diff, add, remove) = calculate_diff(cur.clone(), vals.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vec![ValidatorUpdate {
                    pubkey: cur.last().as_ref().unwrap().validator_pubkey.clone(),
                    power: (VALIDATORS + 1) as u64,
                }]
            },
            diff
        );
        assert!(remove.is_empty());
        assert_eq!(
            add,
            members(vec![(
                cur.last().as_ref().unwrap().operator.as_str(),
                (VALIDATORS + 1) as u64
            )])
        );

        // add all but (one) last member
        let old: Vec<_> = vals.iter().skip(VALIDATORS - 1).cloned().collect();

        // diff must be add all but last
        let (diff, add, remove) = calculate_diff(vals.clone(), old);
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: vi.power,
                    })
                    .collect()
            },
            diff
        );
        assert!(remove.is_empty());
        assert_eq!(
            add,
            members(
                vals.iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| (vi.operator.as_ref(), vi.power))
                    .collect()
            )
        );

        // remove last member
        let cur: Vec<_> = vals.iter().take(VALIDATORS - 1).cloned().collect();
        // diff must be remove last
        let (diff, add, remove) = calculate_diff(cur, vals.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vec![ValidatorUpdate {
                    pubkey: vals.last().unwrap().validator_pubkey.clone(),
                    power: 0,
                }]
            },
            diff
        );
        assert!(add.is_empty());
        assert_eq!(remove, vec![vals.last().unwrap().operator.as_ref()]);

        // remove all but last member
        let cur: Vec<_> = vals.iter().skip(VALIDATORS - 1).cloned().collect();
        // diff must be remove all but last
        let (diff, add, remove) = calculate_diff(cur, vals.clone());
        assert_eq!(
            ValidatorDiff {
                diffs: vals
                    .iter()
                    .take(VALIDATORS - 1)
                    .map(|vi| ValidatorUpdate {
                        pubkey: vi.validator_pubkey.clone(),
                        power: 0,
                    })
                    .collect()
            },
            diff
        );
        assert!(add.is_empty());
        assert_eq!(
            remove,
            vals.iter()
                .take(VALIDATORS - 1)
                .map(|vi| vi.operator.as_ref())
                .collect::<Vec<_>>()
        );
    }
}
