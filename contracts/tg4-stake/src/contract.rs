#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_binary, Addr, BankMsg, Binary, Coin, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, StdResult, Storage, Uint128,
};

use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey};
use cw_utils::maybe_addr;
use tg4::{
    HooksResponse, Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
};
use tg_bindings::{request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg, TgradeSudoMsg};
use tg_utils::{
    members, validate_portion, Duration, ADMIN, HOOKS, PREAUTH_HOOKS, PREAUTH_SLASHING, SLASHERS,
    TOTAL,
};

use crate::error::ContractError;
use crate::msg::{
    ClaimsResponse, ExecuteMsg, InstantiateMsg, PreauthResponse, QueryMsg, StakedResponse,
    TotalWeightResponse, UnbondingPeriodResponse,
};
use crate::state::{claims, Config, CONFIG, STAKE};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tg4-stake";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let api = deps.api;
    ADMIN.set(deps.branch(), maybe_addr(api, msg.admin)?)?;

    PREAUTH_HOOKS.set_auth(deps.storage, msg.preauths_hooks)?;
    PREAUTH_SLASHING.set_auth(deps.storage, msg.preauths_slashing)?;

    // min_bond is at least 1, so 0 stake -> non-membership
    let min_bond = if msg.min_bond == Uint128::zero() {
        Uint128::new(1)
    } else {
        msg.min_bond
    };

    let config = Config {
        denom: msg.denom,
        tokens_per_weight: msg.tokens_per_weight,
        min_bond,
        unbonding_period: Duration::new(msg.unbonding_period),
        auto_return_limit: msg.auto_return_limit,
    };
    CONFIG.save(deps.storage, &config)?;
    TOTAL.save(deps.storage, &0)?;
    SLASHERS.instantiate(deps.storage)?;

    Ok(Response::default())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let api = deps.api;
    match msg {
        ExecuteMsg::UpdateAdmin { admin } => ADMIN
            .execute_update_admin(deps, info, maybe_addr(api, admin)?)
            .map_err(Into::into),
        ExecuteMsg::AddHook { addr } => execute_add_hook(deps, info, addr),
        ExecuteMsg::RemoveHook { addr } => execute_remove_hook(deps, info, addr),
        ExecuteMsg::Bond {} => execute_bond(deps, env, info),
        ExecuteMsg::Unbond {
            tokens: Coin { amount, denom },
        } => execute_unbond(deps, env, info, amount, denom),
        ExecuteMsg::Claim {} => execute_claim(deps, env, info),
        ExecuteMsg::AddSlasher { addr } => execute_add_slasher(deps, info, addr),
        ExecuteMsg::RemoveSlasher { addr } => execute_remove_slasher(deps, info, addr),
        ExecuteMsg::Slash { addr, portion } => execute_slash(deps, env, info, addr, portion),
    }
}

pub fn execute_add_hook(
    deps: DepsMut,
    info: MessageInfo,
    hook: String,
) -> Result<Response, ContractError> {
    // custom guard: using a preauth OR being admin
    if !ADMIN.is_admin(deps.as_ref(), &info.sender)? {
        PREAUTH_HOOKS.use_auth(deps.storage)?;
    }

    // add the hook
    HOOKS.add_hook(deps.storage, deps.api.addr_validate(&hook)?)?;

    // response
    let res = Response::new()
        .add_attribute("action", "add_hook")
        .add_attribute("hook", hook)
        .add_attribute("sender", info.sender);
    Ok(res)
}

pub fn execute_remove_hook(
    deps: DepsMut,
    info: MessageInfo,
    hook: String,
) -> Result<Response, ContractError> {
    // custom guard: self-removal OR being admin
    let hook_addr = deps.api.addr_validate(&hook)?;
    if info.sender != hook_addr && !ADMIN.is_admin(deps.as_ref(), &info.sender)? {
        return Err(ContractError::Unauthorized(
            "Hook address is not same as sender's and sender is not an admin".to_owned(),
        ));
    }

    // remove the hook
    HOOKS.remove_hook(deps.storage, hook_addr)?;

    // response
    let res = Response::new()
        .add_attribute("action", "remove_hook")
        .add_attribute("hook", hook)
        .add_attribute("sender", info.sender);
    Ok(res)
}

pub fn execute_bond(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let amount = validate_funds(&info.funds, &cfg.denom)?;

    // update the sender's stake
    let new_stake = STAKE.update(deps.storage, &info.sender, |stake| -> StdResult<_> {
        Ok(stake.unwrap_or_default() + amount)
    })?;

    let mut res = Response::new()
        .add_attribute("action", "bond")
        .add_attribute("amount", amount)
        .add_attribute("sender", &info.sender);
    res.messages = update_membership(deps.storage, info.sender, new_stake, &cfg, env.block.height)?;

    Ok(res)
}

pub fn execute_unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    denom: String,
) -> Result<Response, ContractError> {
    // provide them a claim
    let cfg = CONFIG.load(deps.storage)?;

    if cfg.denom != denom {
        return Err(ContractError::InvalidDenom(denom));
    }

    // reduce the sender's stake - aborting if insufficient
    let new_stake = STAKE.update(deps.storage, &info.sender, |stake| -> StdResult<_> {
        Ok(stake.unwrap_or_default().checked_sub(amount)?)
    })?;

    let completion = cfg.unbonding_period.after(&env.block);
    claims().create_claim(
        deps.storage,
        info.sender.clone(),
        amount,
        completion,
        env.block.height,
    )?;

    let mut res = Response::new()
        .add_attribute("action", "unbond")
        .add_attribute("amount", amount)
        .add_attribute("denom", &denom)
        .add_attribute("sender", &info.sender)
        .add_attribute("completion_time", completion.time().nanos().to_string());
    res.messages = update_membership(deps.storage, info.sender, new_stake, &cfg, env.block.height)?;

    Ok(res)
}

pub fn execute_add_slasher(
    deps: DepsMut,
    info: MessageInfo,
    slasher: String,
) -> Result<Response, ContractError> {
    // custom guard: using a preauth OR being admin
    if !ADMIN.is_admin(deps.as_ref(), &info.sender)? {
        PREAUTH_SLASHING.use_auth(deps.storage)?;
    }

    // add the slasher
    SLASHERS.add_slasher(deps.storage, deps.api.addr_validate(&slasher)?)?;

    // response
    let res = Response::new()
        .add_attribute("action", "add_slasher")
        .add_attribute("slasher", slasher)
        .add_attribute("sender", info.sender);
    Ok(res)
}

pub fn execute_remove_slasher(
    deps: DepsMut,
    info: MessageInfo,
    slasher: String,
) -> Result<Response, ContractError> {
    // custom guard: self-removal OR being admin
    let slasher_addr = Addr::unchecked(&slasher);
    if info.sender != slasher_addr && !ADMIN.is_admin(deps.as_ref(), &info.sender)? {
        return Err(ContractError::Unauthorized(
            "Only slasher might remove himself and sender is not an admin".to_owned(),
        ));
    }

    // remove the slasher
    SLASHERS.remove_slasher(deps.storage, slasher_addr)?;

    // response
    let res = Response::new()
        .add_attribute("action", "remove_slasher")
        .add_attribute("slasher", slasher)
        .add_attribute("sender", info.sender);
    Ok(res)
}

pub fn execute_slash(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    addr: String,
    portion: Decimal,
) -> Result<Response, ContractError> {
    if !SLASHERS.is_slasher(deps.storage, &info.sender)? {
        return Err(ContractError::Unauthorized(
            "Sender is not on slashers list".to_owned(),
        ));
    }

    validate_portion(portion)?;

    let cfg = CONFIG.load(deps.storage)?;
    let addr = deps.api.addr_validate(&addr)?;

    // update the addr's stake
    // If address doesn't match anyone, leave early
    let stake = match STAKE.may_load(deps.storage, &addr)? {
        Some(s) => s,
        None => return Ok(Response::new()),
    };
    let mut slashed = stake * portion;
    let new_stake = STAKE.update(deps.storage, &addr, |stake| -> StdResult<_> {
        Ok(stake.unwrap_or_default().checked_sub(slashed)?)
    })?;

    // slash the claims
    slashed += claims().slash_claims_for_addr(deps.storage, addr.clone(), portion)?;

    // burn the tokens
    let burn_msg = BankMsg::Burn {
        amount: coins(slashed.u128(), &cfg.denom),
    };

    // response
    let mut res = Response::new()
        .add_attribute("action", "slash")
        .add_attribute("addr", &addr)
        .add_attribute("sender", info.sender)
        .add_message(burn_msg);

    res.messages.extend(update_membership(
        deps.storage,
        addr,
        new_stake,
        &cfg,
        env.block.height,
    )?);

    Ok(res)
}

/// Validates funds send with the message, that they are containing only single denom. Returns
/// amount of funds send, or error if:
/// * No funds are passed with message (`NoFunds` error)
/// * More than single denom  are send (`ExtraDenoms` error)
/// * Invalid single denom is send (`MissingDenom` error)
pub fn validate_funds(funds: &[Coin], stake_denom: &str) -> Result<Uint128, ContractError> {
    match funds {
        [] => Err(ContractError::NoFunds {}),
        [Coin { denom, amount }] if denom == stake_denom => Ok(*amount),
        [_] => Err(ContractError::MissingDenom(stake_denom.to_string())),
        _ => Err(ContractError::ExtraDenoms(stake_denom.to_string())),
    }
}

fn update_membership(
    storage: &mut dyn Storage,
    sender: Addr,
    new_stake: Uint128,
    cfg: &Config,
    height: u64,
) -> StdResult<Vec<SubMsg>> {
    // update their membership weight
    let new = calc_weight(new_stake, cfg);
    let old = members().may_load(storage, &sender)?;

    // short-circuit if no change
    if new == old {
        return Ok(vec![]);
    }
    // otherwise, record change of weight
    match new.as_ref() {
        Some(w) => members().save(storage, &sender, w, height),
        None => members().remove(storage, &sender, height),
    }?;

    // update total
    TOTAL.update(storage, |total| -> StdResult<_> {
        Ok(total + new.unwrap_or_default() - old.unwrap_or_default())
    })?;

    // alert the hooks
    let diff = MemberDiff::new(sender, old, new);
    HOOKS.prepare_hooks(storage, |h| {
        MemberChangedHookMsg::one(diff.clone())
            .into_cosmos_msg(h)
            .map(SubMsg::new)
    })
}

fn calc_weight(stake: Uint128, cfg: &Config) -> Option<u64> {
    if stake < cfg.min_bond {
        None
    } else {
        let w = stake.u128() / (cfg.tokens_per_weight.u128());
        Some(w as u64)
    }
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let release = claims().claim_addr(deps.storage, &info.sender, &env.block, None)?;
    if release.is_zero() {
        return Err(ContractError::NothingToClaim {});
    }

    let config = CONFIG.load(deps.storage)?;
    let amount = coins(release.into(), config.denom);

    let res = Response::new()
        .add_attribute("action", "claim")
        .add_attribute("tokens", coins_to_string(&amount))
        .add_attribute("sender", &info.sender)
        .add_message(BankMsg::Send {
            to_address: info.sender.into(),
            amount,
        });

    Ok(res)
}

// TODO: put in cosmwasm-std
fn coins_to_string(coins: &[Coin]) -> String {
    let strings: Vec<_> = coins
        .iter()
        .map(|c| format!("{}{}", c.amount, c.denom))
        .collect();
    strings.join(",")
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: TgradeSudoMsg) -> Result<Response, ContractError> {
    match msg {
        TgradeSudoMsg::PrivilegeChange(PrivilegeChangeMsg::Promoted {}) => privilege_promote(deps),
        TgradeSudoMsg::EndBlock {} => end_block(deps, env),
        _ => Err(ContractError::UnknownSudoMsg {}),
    }
}

fn privilege_promote(deps: DepsMut) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if config.auto_return_limit > 0 {
        let msgs = request_privileges(&[Privilege::EndBlocker]);
        Ok(Response::new().add_submessages(msgs))
    } else {
        Ok(Response::new())
    }
}

fn end_block(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut resp = Response::new();

    let config = CONFIG.load(deps.storage)?;
    if config.auto_return_limit > 0 {
        let sub_msgs = release_expired_claims(deps, env, config)?;
        resp = resp.add_submessages(sub_msgs);
    }

    Ok(resp)
}

fn release_expired_claims(
    deps: DepsMut,
    env: Env,
    config: Config,
) -> Result<Vec<SubMsg>, ContractError> {
    let releases = claims().claim_expired(deps.storage, &env.block, config.auto_return_limit)?;

    releases
        .into_iter()
        .map(|(addr, amount)| {
            let amount = coins(amount.into(), config.denom.clone());
            Ok(SubMsg::new(BankMsg::Send {
                to_address: addr.into(),
                amount,
            }))
        })
        .collect()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;
    match msg {
        Configuration {} => to_binary(&CONFIG.load(deps.storage)?),
        Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        ListMembers { start_after, limit } => to_binary(&list_members(deps, start_after, limit)?),
        ListMembersByWeight { start_after, limit } => {
            to_binary(&list_members_by_weight(deps, start_after, limit)?)
        }
        TotalWeight {} => to_binary(&query_total_weight(deps)?),
        Claims {
            address,
            limit,
            start_after,
        } => to_binary(&ClaimsResponse {
            claims: claims().query_claims(
                deps,
                deps.api.addr_validate(&address)?,
                limit,
                start_after,
            )?,
        }),
        Staked { address } => to_binary(&query_staked(deps, address)?),
        Admin {} => to_binary(&ADMIN.query_admin(deps)?),
        Hooks {} => {
            let hooks = HOOKS.list_hooks(deps.storage)?;
            to_binary(&HooksResponse { hooks })
        }
        Preauths {} => {
            let preauths_hooks = PREAUTH_HOOKS.get_auth(deps.storage)?;
            to_binary(&PreauthResponse { preauths_hooks })
        }
        UnbondingPeriod {} => {
            let Config {
                unbonding_period, ..
            } = CONFIG.load(deps.storage)?;
            to_binary(&UnbondingPeriodResponse { unbonding_period })
        }
        IsSlasher { addr } => {
            let addr = deps.api.addr_validate(&addr)?;
            to_binary(&SLASHERS.is_slasher(deps.storage, &addr)?)
        }
        ListSlashers {} => to_binary(&SLASHERS.list_slashers(deps.storage)?),
    }
}

fn query_total_weight(deps: Deps) -> StdResult<TotalWeightResponse> {
    let weight = TOTAL.load(deps.storage)?;
    let denom = CONFIG.load(deps.storage)?.denom;
    Ok(TotalWeightResponse { weight, denom })
}

pub fn query_staked(deps: Deps, addr: String) -> StdResult<StakedResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let stake = STAKE.may_load(deps.storage, &addr)?.unwrap_or_default();
    let config = CONFIG.load(deps.storage)?;

    Ok(StakedResponse {
        stake: coin(stake.u128(), config.denom),
    })
}

fn query_member(deps: Deps, addr: String, height: Option<u64>) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let weight = match height {
        Some(h) => members().may_load_at_height(deps.storage, &addr, h),
        None => members().may_load(deps.storage, &addr),
    }?;
    Ok(MemberResponse { weight })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_members(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let members: StdResult<Vec<_>> = members()
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (addr, weight) = item?;
            Ok(Member {
                addr: addr.into(),
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

fn list_members_by_weight(
    deps: Deps,
    start_after: Option<Member>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|m| Bound::exclusive((m.weight, m.addr.as_str()).joined_key()));

    let members: StdResult<Vec<_>> = members()
        .idx
        .weight
        .range(deps.storage, None, start, Order::Descending)
        .take(limit)
        .map(|item| {
            let (addr, weight) = item?;
            Ok(Member {
                addr: addr.into(),
                weight,
            })
        })
        .collect();

    Ok(MemberListResponse { members: members? })
}

#[cfg(test)]
mod tests {
    use crate::claim::Claim;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{
        from_slice, CosmosMsg, OverflowError, OverflowOperation, StdError, Storage,
    };
    use tg4::{member_key, TOTAL_KEY};
    use tg_utils::{Expiration, HookError, PreauthError, SlasherError};

    use crate::error::ContractError;

    use super::*;

    const INIT_ADMIN: &str = "juan";
    const USER1: &str = "user1";
    const USER2: &str = "user2";
    const USER3: &str = "user3";
    const DENOM: &str = "stake";
    const TOKENS_PER_WEIGHT: Uint128 = Uint128::new(1_000);
    const MIN_BOND: Uint128 = Uint128::new(5_000);
    const UNBONDING_DURATION: u64 = 100;

    fn default_instantiate(deps: DepsMut) {
        do_instantiate(deps, TOKENS_PER_WEIGHT, MIN_BOND, UNBONDING_DURATION, 0)
    }

    fn do_instantiate(
        deps: DepsMut,
        tokens_per_weight: Uint128,
        min_bond: Uint128,
        unbonding_period: u64,
        auto_return_limit: u64,
    ) {
        let msg = InstantiateMsg {
            denom: "stake".to_owned(),
            tokens_per_weight,
            min_bond,
            unbonding_period,
            admin: Some(INIT_ADMIN.into()),
            preauths_hooks: 1,
            preauths_slashing: 1,
            auto_return_limit,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps, mock_env(), info, msg).unwrap();
    }

    fn bond(mut deps: DepsMut, user1: u128, user2: u128, user3: u128, height_delta: u64) {
        let mut env = mock_env();
        env.block.height += height_delta;

        for (addr, stake) in &[(USER1, user1), (USER2, user2), (USER3, user3)] {
            if *stake != 0 {
                let msg = ExecuteMsg::Bond {};
                let info = mock_info(addr, &coins(*stake, DENOM));
                execute(deps.branch(), env.clone(), info, msg).unwrap();
            }
        }
    }

    fn unbond(
        mut deps: DepsMut,
        user1: u128,
        user2: u128,
        user3: u128,
        height_delta: u64,
        time_delta: u64,
    ) {
        let mut env = mock_env();
        env.block.height += height_delta;
        env.block.time = env.block.time.plus_seconds(time_delta);

        for (addr, stake) in &[(USER1, user1), (USER2, user2), (USER3, user3)] {
            if *stake != 0 {
                let msg = ExecuteMsg::Unbond {
                    tokens: coin(*stake, DENOM),
                };
                let info = mock_info(addr, &[]);
                execute(deps.branch(), env.clone(), info, msg).unwrap();
            }
        }
    }

    #[test]
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        // it worked, let's query the state
        let res = ADMIN.query_admin(deps.as_ref()).unwrap();
        assert_eq!(Some(INIT_ADMIN.into()), res.admin);

        let res = query_total_weight(deps.as_ref()).unwrap();
        assert_eq!(0, res.weight);
        assert_eq!("stake".to_owned(), res.denom);

        let raw = query(deps.as_ref(), mock_env(), QueryMsg::Configuration {}).unwrap();
        let res: Config = from_slice(&raw).unwrap();
        assert_eq!(
            res,
            Config {
                denom: "stake".to_owned(),
                tokens_per_weight: TOKENS_PER_WEIGHT,
                min_bond: MIN_BOND,
                unbonding_period: Duration::new(UNBONDING_DURATION),
                auto_return_limit: 0,
            }
        );
    }

    #[test]
    fn unbonding_period_query_works() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        let raw = query(deps.as_ref(), mock_env(), QueryMsg::UnbondingPeriod {}).unwrap();
        let res: UnbondingPeriodResponse = from_slice(&raw).unwrap();
        assert_eq!(res.unbonding_period, Duration::new(UNBONDING_DURATION));
    }

    fn get_member(deps: Deps, addr: String, at_height: Option<u64>) -> Option<u64> {
        let raw = query(deps, mock_env(), QueryMsg::Member { addr, at_height }).unwrap();
        let res: MemberResponse = from_slice(&raw).unwrap();
        res.weight
    }

    // this tests the member queries
    fn assert_users(
        deps: Deps,
        user1_weight: Option<u64>,
        user2_weight: Option<u64>,
        user3_weight: Option<u64>,
        height: Option<u64>,
    ) {
        let member1 = get_member(deps, USER1.into(), height);
        assert_eq!(member1, user1_weight);

        let member2 = get_member(deps, USER2.into(), height);
        assert_eq!(member2, user2_weight);

        let member3 = get_member(deps, USER3.into(), height);
        assert_eq!(member3, user3_weight);

        // this is only valid if we are not doing a historical query
        if height.is_none() {
            // compute expected metrics
            let weights = vec![user1_weight, user2_weight, user3_weight];
            let sum: u64 = weights.iter().map(|x| x.unwrap_or_default()).sum();
            let count = weights.iter().filter(|x| x.is_some()).count();

            // TODO: more detailed compare?
            let msg = QueryMsg::ListMembers {
                start_after: None,
                limit: None,
            };
            let raw = query(deps, mock_env(), msg).unwrap();
            let members: MemberListResponse = from_slice(&raw).unwrap();
            assert_eq!(count, members.members.len());

            let raw = query(deps, mock_env(), QueryMsg::TotalWeight {}).unwrap();
            let total: TotalWeightResponse = from_slice(&raw).unwrap();
            assert_eq!(sum, total.weight); // 17 - 11 + 15 = 21
        }
    }

    // this tests the member queries
    fn assert_stake(deps: Deps, user1_stake: u128, user2_stake: u128, user3_stake: u128) {
        let stake1 = query_staked(deps, USER1.into()).unwrap();
        assert_eq!(stake1.stake, coin(user1_stake, DENOM));

        let stake2 = query_staked(deps, USER2.into()).unwrap();
        assert_eq!(stake2.stake, coin(user2_stake, DENOM));

        let stake3 = query_staked(deps, USER3.into()).unwrap();
        assert_eq!(stake3.stake, coin(user3_stake, DENOM));
    }

    #[test]
    fn bond_stake_adds_membership() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());
        let height = mock_env().block.height;

        // Assert original weights
        assert_users(deps.as_ref(), None, None, None, None);

        // ensure it rounds down, and respects cut-off
        bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);

        // Assert updated weights
        assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);
        assert_users(deps.as_ref(), Some(12), Some(7), None, None);

        // add some more, ensure the sum is properly respected (7.5 + 7.6 = 15 not 14)
        bond(deps.as_mut(), 0, 7_600, 1_200, 2);

        // Assert updated weights
        assert_stake(deps.as_ref(), 12_000, 15_100, 5_200);
        assert_users(deps.as_ref(), Some(12), Some(15), Some(5), None);

        // check historical queries all work
        assert_users(deps.as_ref(), None, None, None, Some(height + 1)); // before first stake
        assert_users(deps.as_ref(), Some(12), Some(7), None, Some(height + 2)); // after first stake
        assert_users(deps.as_ref(), Some(12), Some(15), Some(5), Some(height + 3));
        // after second stake
    }

    #[test]
    fn try_member_queries() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);

        let member1 = query_member(deps.as_ref(), USER1.into(), None).unwrap();
        assert_eq!(member1.weight, Some(12));

        let member2 = query_member(deps.as_ref(), USER2.into(), None).unwrap();
        assert_eq!(member2.weight, Some(7));

        let member3 = query_member(deps.as_ref(), USER3.into(), None).unwrap();
        assert_eq!(member3.weight, None);

        let members = list_members(deps.as_ref(), None, None).unwrap().members;
        assert_eq!(members.len(), 2);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![
                Member {
                    addr: USER1.into(),
                    weight: 12
                },
                Member {
                    addr: USER2.into(),
                    weight: 7
                },
            ]
        );

        // Test pagination / limits
        let members = list_members(deps.as_ref(), None, Some(1)).unwrap().members;
        assert_eq!(members.len(), 1);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![Member {
                addr: USER1.into(),
                weight: 12
            },]
        );

        // Next page
        let start_after = Some(members[0].addr.clone());
        let members = list_members(deps.as_ref(), start_after, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 1);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![Member {
                addr: USER2.into(),
                weight: 7
            },]
        );

        // Assert there's no more
        let start_after = Some(members[0].addr.clone());
        let members = list_members(deps.as_ref(), start_after, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 0);
    }

    #[test]
    fn try_list_members_by_weight() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        bond(deps.as_mut(), 11_000, 6_500, 5_000, 1);

        let members = list_members_by_weight(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 3);
        // Assert the set is sorted by (descending) weight
        assert_eq!(
            members,
            vec![
                Member {
                    addr: USER1.into(),
                    weight: 11
                },
                Member {
                    addr: USER2.into(),
                    weight: 6
                },
                Member {
                    addr: USER3.into(),
                    weight: 5
                }
            ]
        );

        // Test pagination / limits
        let members = list_members_by_weight(deps.as_ref(), None, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 1);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![Member {
                addr: USER1.into(),
                weight: 11
            },]
        );

        // Next page
        let last = members.last().unwrap();
        let start_after = Some(last.clone());
        let members = list_members_by_weight(deps.as_ref(), start_after, None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 2);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![
                Member {
                    addr: USER2.into(),
                    weight: 6
                },
                Member {
                    addr: USER3.into(),
                    weight: 5
                }
            ]
        );

        // Assert there's no more
        let last = members.last().unwrap();
        let start_after = Some(last.clone());
        let members = list_members_by_weight(deps.as_ref(), start_after, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 0);
    }

    #[test]
    fn unbond_stake_update_membership() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());
        let height = mock_env().block.height;

        // ensure it rounds down, and respects cut-off
        bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
        unbond(deps.as_mut(), 4_500, 2_600, 1_111, 2, 0);

        // Assert updated weights
        assert_stake(deps.as_ref(), 7_500, 4_900, 2_889);
        assert_users(deps.as_ref(), Some(7), None, None, None);

        // Adding a little more returns weight
        bond(deps.as_mut(), 600, 100, 2_222, 3);

        // Assert updated weights
        assert_users(deps.as_ref(), Some(8), Some(5), Some(5), None);

        // check historical queries all work
        assert_users(deps.as_ref(), None, None, None, Some(height + 1)); // before first stake
        assert_users(deps.as_ref(), Some(12), Some(7), None, Some(height + 2)); // after first bond
        assert_users(deps.as_ref(), Some(7), None, None, Some(height + 3)); // after first unbond
        assert_users(deps.as_ref(), Some(8), Some(5), Some(5), Some(height + 4)); // after second bond

        // error if try to unbond more than stake (USER2 has 5000 staked)
        let msg = ExecuteMsg::Unbond {
            tokens: coin(5100, DENOM),
        };
        let mut env = mock_env();
        env.block.height += 5;
        let info = mock_info(USER2, &[]);
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::Std(StdError::overflow(OverflowError::new(
                OverflowOperation::Sub,
                5000,
                5100
            )))
        );
    }

    #[test]
    fn raw_queries_work() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());
        // Set values as (11, 6, None)
        bond(deps.as_mut(), 11_000, 6_000, 0, 1);

        // get total from raw key
        let total_raw = deps.storage.get(TOTAL_KEY.as_bytes()).unwrap();
        let total: u64 = from_slice(&total_raw).unwrap();
        assert_eq!(17, total);

        // get member votes from raw key
        let member2_raw = deps.storage.get(&member_key(USER2)).unwrap();
        let member2: u64 = from_slice(&member2_raw).unwrap();
        assert_eq!(6, member2);

        // and execute misses
        let member3_raw = deps.storage.get(&member_key(USER3));
        assert_eq!(None, member3_raw);
    }

    #[track_caller]
    fn get_claims(
        deps: Deps,
        addr: Addr,
        limit: Option<u32>,
        start_after: Option<Expiration>,
    ) -> Vec<Claim> {
        claims()
            .query_claims(deps, addr, limit, start_after)
            .unwrap()
    }

    #[test]
    fn unbond_claim_workflow() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        // create some data
        bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
        let height_delta = 2;
        unbond(deps.as_mut(), 4_500, 2_600, 0, height_delta, 0);
        let mut env = mock_env();
        env.block.height += height_delta;

        // check the claims for each user
        let expires = Duration::new(UNBONDING_DURATION).after(&env.block);
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER1), None, None),
            vec![Claim::new(
                Addr::unchecked(USER1),
                4_500,
                expires,
                env.block.height
            )]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER2), None, None),
            vec![Claim::new(
                Addr::unchecked(USER2),
                2_600,
                expires,
                env.block.height
            )]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER3), None, None),
            vec![]
        );

        // do another unbond later on
        let mut env2 = mock_env();
        let height_delta = 22;
        env2.block.height += height_delta;
        let time_delta = 50;
        unbond(deps.as_mut(), 0, 1_345, 1_500, height_delta, time_delta);

        // with updated claims
        let expires2 = Duration::new(UNBONDING_DURATION + time_delta).after(&env2.block);
        assert_ne!(expires, expires2);
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER1), None, None),
            vec![Claim::new(
                Addr::unchecked(USER1),
                4_500,
                expires,
                env.block.height
            )]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER2), None, None),
            vec![
                Claim::new(Addr::unchecked(USER2), 2_600, expires, env.block.height),
                Claim::new(Addr::unchecked(USER2), 1_345, expires2, env2.block.height)
            ]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER3), None, None),
            vec![Claim::new(
                Addr::unchecked(USER3),
                1_500,
                expires2,
                env2.block.height
            )]
        );

        // nothing can be withdrawn yet
        let err = execute(
            deps.as_mut(),
            env,
            mock_info(USER1, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::NothingToClaim {});

        // now mature first section, withdraw that
        let mut env3 = mock_env();
        env3.block.time = env3.block.time.plus_seconds(UNBONDING_DURATION);
        // first one can now release
        let res = execute(
            deps.as_mut(),
            env3.clone(),
            mock_info(USER1, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: USER1.into(),
                amount: coins(4_500, DENOM),
            })]
        );

        // second releases partially
        let res = execute(
            deps.as_mut(),
            env3.clone(),
            mock_info(USER2, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: USER2.into(),
                amount: coins(2_600, DENOM),
            })]
        );

        // but the third one cannot release
        let err = execute(
            deps.as_mut(),
            env3,
            mock_info(USER3, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::NothingToClaim {});

        // claims updated properly
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER1), None, None),
            vec![]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER2), None, None),
            vec![Claim::new(
                Addr::unchecked(USER2),
                1_345,
                expires2,
                env2.block.height
            )]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER3), None, None),
            vec![Claim::new(
                Addr::unchecked(USER3),
                1_500,
                expires2,
                env2.block.height
            )]
        );

        // add another few claims for 2
        unbond(deps.as_mut(), 0, 600, 0, 30, 0);
        unbond(deps.as_mut(), 0, 1_005, 0, 50, 0);

        // ensure second can claim all tokens at once
        let mut env4 = mock_env();
        env4.block.time = env4
            .block
            .time
            .plus_seconds(UNBONDING_DURATION + time_delta);
        let res = execute(
            deps.as_mut(),
            env4,
            mock_info(USER2, &[]),
            ExecuteMsg::Claim {},
        )
        .unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: USER2.into(),
                // 1_345 + 600 + 1_005
                amount: coins(2_950, DENOM),
            })]
        );
        assert_eq!(
            get_claims(deps.as_ref(), Addr::unchecked(USER2), None, None),
            vec![]
        );
    }

    #[test]
    fn add_remove_hooks() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        let hooks = HOOKS.list_hooks(&deps.storage).unwrap();
        assert!(hooks.is_empty());

        let contract1 = String::from("hook1");
        let contract2 = String::from("hook2");

        let add_msg = ExecuteMsg::AddHook {
            addr: contract1.clone(),
        };

        // anyone can add the first one, until preauth is consume
        assert_eq!(1, PREAUTH_HOOKS.get_auth(&deps.storage).unwrap());
        let user_info = mock_info(USER1, &[]);
        let _ = execute(deps.as_mut(), mock_env(), user_info, add_msg.clone()).unwrap();
        let hooks = HOOKS.list_hooks(&deps.storage).unwrap();
        assert_eq!(hooks, vec![contract1.clone()]);

        // non-admin cannot add hook without preauth
        assert_eq!(0, PREAUTH_HOOKS.get_auth(&deps.storage).unwrap());
        let user_info = mock_info(USER1, &[]);
        let err = execute(
            deps.as_mut(),
            mock_env(),
            user_info.clone(),
            add_msg.clone(),
        )
        .unwrap_err();
        assert_eq!(err, PreauthError::NoPreauth {}.into());

        // cannot remove a non-registered contract
        let admin_info = mock_info(INIT_ADMIN, &[]);
        let remove_msg = ExecuteMsg::RemoveHook {
            addr: contract2.clone(),
        };
        let err = execute(deps.as_mut(), mock_env(), admin_info.clone(), remove_msg).unwrap_err();
        assert_eq!(err, HookError::HookNotRegistered {}.into());

        // admin can second contract, and it appears in the query
        let add_msg2 = ExecuteMsg::AddHook {
            addr: contract2.clone(),
        };
        execute(deps.as_mut(), mock_env(), admin_info.clone(), add_msg2).unwrap();
        let hooks = HOOKS.list_hooks(&deps.storage).unwrap();
        assert_eq!(hooks, vec![contract1.clone(), contract2.clone()]);

        // cannot re-add an existing contract
        let err = execute(deps.as_mut(), mock_env(), admin_info.clone(), add_msg).unwrap_err();
        assert_eq!(err, HookError::HookAlreadyRegistered {}.into());

        // non-admin cannot remove
        let remove_msg = ExecuteMsg::RemoveHook { addr: contract1 };
        let err = execute(deps.as_mut(), mock_env(), user_info, remove_msg.clone()).unwrap_err();
        assert_eq!(
            err,
            ContractError::Unauthorized(
                "Hook address is not same as sender's and sender is not an admin".to_owned()
            )
        );

        // remove the original
        execute(deps.as_mut(), mock_env(), admin_info, remove_msg).unwrap();
        let hooks = HOOKS.list_hooks(&deps.storage).unwrap();
        assert_eq!(hooks, vec![contract2.clone()]);

        // contract can self-remove
        let contract_info = mock_info(&contract2, &[]);
        let remove_msg2 = ExecuteMsg::RemoveHook { addr: contract2 };
        execute(deps.as_mut(), mock_env(), contract_info, remove_msg2).unwrap();
        let hooks = HOOKS.list_hooks(&deps.storage).unwrap();
        assert_eq!(hooks, Vec::<String>::new());
    }

    mod slash {
        use super::*;

        fn query_is_slasher(deps: Deps, env: Env, addr: String) -> StdResult<bool> {
            let msg = QueryMsg::IsSlasher { addr };
            let raw = query(deps, env, msg)?;
            let is_slasher: bool = from_slice(&raw)?;
            Ok(is_slasher)
        }

        fn query_list_slashers(deps: Deps, env: Env) -> StdResult<Vec<String>> {
            let msg = QueryMsg::ListSlashers {};
            let raw = query(deps, env, msg)?;
            let slashers: Vec<String> = from_slice(&raw)?;
            Ok(slashers)
        }

        fn add_slasher(deps: DepsMut) -> String {
            let slasher = String::from("slasher");
            let add_msg = ExecuteMsg::AddSlasher {
                addr: slasher.clone(),
            };
            let user_info = mock_info(USER1, &[]);
            execute(deps, mock_env(), user_info, add_msg).unwrap();

            slasher
        }

        fn remove_slasher(deps: DepsMut, slasher: &str) {
            let add_msg = ExecuteMsg::RemoveSlasher {
                addr: slasher.to_string(),
            };
            let user_info = mock_info(INIT_ADMIN, &[]);
            execute(deps, mock_env(), user_info, add_msg).unwrap();
        }

        fn slash(
            deps: DepsMut,
            slasher: &str,
            addr: &str,
            portion: Decimal,
        ) -> Result<Response, ContractError> {
            let msg = ExecuteMsg::Slash {
                addr: addr.to_string(),
                portion,
            };
            let slasher_info = mock_info(slasher, &[]);

            execute(deps, mock_env(), slasher_info, msg)
        }

        fn assert_burned(res: Response, expected_amount: Vec<Coin>) {
            // Find all instances of BankMsg::Burn in the response and extract the burned amounts
            let burned_amounts: Vec<_> = res
                .messages
                .iter()
                .filter_map(|sub_msg| match &sub_msg.msg {
                    CosmosMsg::Bank(BankMsg::Burn { amount }) => Some(amount),
                    _ => None,
                })
                .collect();

            assert_eq!(
                burned_amounts.len(),
                1,
                "Expected exactly 1 Bank::Burn message, got {}",
                burned_amounts.len()
            );
            assert_eq!(
                burned_amounts[0], &expected_amount,
                "Expected to burn {}, burned {}",
                expected_amount[0], burned_amounts[0][0]
            );
        }

        #[test]
        fn add_remove_slashers() {
            let mut deps = mock_dependencies();
            let env = mock_env();
            default_instantiate(deps.as_mut());

            let slashers = query_list_slashers(deps.as_ref(), env.clone()).unwrap();
            assert!(slashers.is_empty());

            let contract1 = String::from("slasher1");
            let contract2 = String::from("slasher2");

            let add_msg = ExecuteMsg::AddSlasher {
                addr: contract1.clone(),
            };

            // anyone can add the first one, until preauth is consumed
            assert_eq!(1, PREAUTH_SLASHING.get_auth(&deps.storage).unwrap());
            let user_info = mock_info(USER1, &[]);
            let _ = execute(deps.as_mut(), mock_env(), user_info, add_msg.clone()).unwrap();
            let slashers = query_list_slashers(deps.as_ref(), env.clone()).unwrap();
            assert_eq!(slashers, vec![contract1.clone()]);

            // non-admin cannot add slasher without preauth
            assert_eq!(0, PREAUTH_SLASHING.get_auth(&deps.storage).unwrap());
            let user_info = mock_info(USER1, &[]);
            let err = execute(
                deps.as_mut(),
                mock_env(),
                user_info.clone(),
                add_msg.clone(),
            )
            .unwrap_err();
            assert_eq!(err, PreauthError::NoPreauth {}.into());

            // cannot remove a non-registered slasher
            let admin_info = mock_info(INIT_ADMIN, &[]);
            let remove_msg = ExecuteMsg::RemoveSlasher {
                addr: contract2.clone(),
            };
            let err =
                execute(deps.as_mut(), mock_env(), admin_info.clone(), remove_msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::Slasher(SlasherError::SlasherNotRegistered(contract2.clone()))
            );

            // admin can add a second slasher, and it appears in the query
            let add_msg2 = ExecuteMsg::AddSlasher {
                addr: contract2.clone(),
            };
            execute(deps.as_mut(), mock_env(), admin_info.clone(), add_msg2).unwrap();
            let slashers = query_list_slashers(deps.as_ref(), env.clone()).unwrap();
            assert_eq!(slashers, vec![contract1.clone(), contract2.clone()]);

            // cannot re-add an existing contract
            let err = execute(deps.as_mut(), mock_env(), admin_info.clone(), add_msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::Slasher(SlasherError::SlasherAlreadyRegistered(contract1.clone()))
            );

            // non-admin cannot remove
            let remove_msg = ExecuteMsg::RemoveSlasher { addr: contract1 };
            let err =
                execute(deps.as_mut(), mock_env(), user_info, remove_msg.clone()).unwrap_err();
            assert_eq!(
                err,
                ContractError::Unauthorized(
                    "Only slasher might remove himself and sender is not an admin".to_owned()
                )
            );

            // remove the original
            execute(deps.as_mut(), mock_env(), admin_info, remove_msg).unwrap();
            let slashers = query_list_slashers(deps.as_ref(), env.clone()).unwrap();
            assert_eq!(slashers, vec![contract2.clone()]);

            // contract can self-remove
            let contract_info = mock_info(&contract2, &[]);
            let remove_msg2 = ExecuteMsg::RemoveSlasher { addr: contract2 };
            execute(deps.as_mut(), mock_env(), contract_info, remove_msg2).unwrap();
            let slashers = query_list_slashers(deps.as_ref(), env).unwrap();
            assert_eq!(slashers, Vec::<String>::new());
        }

        #[test]
        fn slashing_nonexisting_member() {
            let mut deps = mock_dependencies();
            default_instantiate(deps.as_mut());

            // confirm address doesn't return true on slasher query
            assert!(!query_is_slasher(deps.as_ref(), mock_env(), "slasher".to_owned()).unwrap());

            let slasher = add_slasher(deps.as_mut());
            assert!(query_is_slasher(deps.as_ref(), mock_env(), slasher.clone()).unwrap());

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);

            // Trying to slash nonexisting user will result in no-op
            let res = slash(deps.as_mut(), &slasher, "nonexisting", Decimal::percent(20)).unwrap();
            assert_eq!(res, Response::new());
        }

        #[test]
        fn slashing_bonded_tokens_works() {
            let mut deps = mock_dependencies();
            default_instantiate(deps.as_mut());
            let cfg = CONFIG.load(&deps.storage).unwrap();
            let slasher = add_slasher(deps.as_mut());

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);

            // The slasher we added can slash
            let res1 = slash(deps.as_mut(), &slasher, USER1, Decimal::percent(20)).unwrap();
            let res2 = slash(deps.as_mut(), &slasher, USER3, Decimal::percent(50)).unwrap();
            assert_stake(deps.as_ref(), 9_600, 7_500, 2_000);

            // Tokens are burned
            assert_burned(res1, coins(2_400, &cfg.denom));
            assert_burned(res2, coins(2_000, &cfg.denom));
        }

        #[test]
        fn slashing_claims_works() {
            let mut deps = mock_dependencies();
            default_instantiate(deps.as_mut());
            let cfg = CONFIG.load(&deps.storage).unwrap();
            let slasher = add_slasher(deps.as_mut());

            // create some data
            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;
            unbond(deps.as_mut(), 12_000, 2_600, 0, height_delta, 0);
            let mut env = mock_env();
            env.block.height += height_delta;

            // check the claims for each user
            let expires = Duration::new(UNBONDING_DURATION).after(&env.block);
            assert_eq!(
                get_claims(deps.as_ref(), Addr::unchecked(USER1), None, None),
                vec![Claim::new(
                    Addr::unchecked(USER1),
                    12_000,
                    expires,
                    env.block.height
                )]
            );

            let res = slash(deps.as_mut(), &slasher, USER1, Decimal::percent(20)).unwrap();

            assert_eq!(
                get_claims(deps.as_ref(), Addr::unchecked(USER1), None, None),
                vec![Claim::new(
                    Addr::unchecked(USER1),
                    9_600,
                    expires,
                    env.block.height
                )]
            );
            assert_burned(res, coins(2_400, &cfg.denom));
        }

        #[test]
        fn random_user_cannot_slash() {
            let mut deps = mock_dependencies();
            default_instantiate(deps.as_mut());
            let _slasher = add_slasher(deps.as_mut());

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);

            let res = slash(deps.as_mut(), USER2, USER1, Decimal::percent(20));
            assert_eq!(
                res,
                Err(ContractError::Unauthorized(
                    "Sender is not on slashers list".to_owned()
                ))
            );
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);
        }

        #[test]
        fn admin_cannot_slash() {
            let mut deps = mock_dependencies();
            default_instantiate(deps.as_mut());
            let _slasher = add_slasher(deps.as_mut());

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);

            let res = slash(deps.as_mut(), INIT_ADMIN, USER1, Decimal::percent(20));
            assert_eq!(
                res,
                Err(ContractError::Unauthorized(
                    "Sender is not on slashers list".to_owned()
                ))
            );
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);
        }

        #[test]
        fn removed_slasher_cannot_slash() {
            let mut deps = mock_dependencies();
            default_instantiate(deps.as_mut());

            // Add, then remove a slasher
            let slasher = add_slasher(deps.as_mut());
            remove_slasher(deps.as_mut(), &slasher);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);

            let res = slash(deps.as_mut(), &slasher, USER1, Decimal::percent(20));
            assert_eq!(
                res,
                Err(ContractError::Unauthorized(
                    "Sender is not on slashers list".to_owned()
                ))
            );
            assert_stake(deps.as_ref(), 12_000, 7_500, 4_000);
        }
    }

    #[test]
    fn hooks_fire() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        let hooks = HOOKS.list_hooks(&deps.storage).unwrap();
        assert!(hooks.is_empty());

        let contract1 = String::from("hook1");
        let contract2 = String::from("hook2");

        // register 2 hooks
        let admin_info = mock_info(INIT_ADMIN, &[]);
        let add_msg = ExecuteMsg::AddHook {
            addr: contract1.clone(),
        };
        let add_msg2 = ExecuteMsg::AddHook {
            addr: contract2.clone(),
        };
        for msg in vec![add_msg, add_msg2] {
            let _ = execute(deps.as_mut(), mock_env(), admin_info.clone(), msg).unwrap();
        }

        // check firing on bond
        assert_users(deps.as_ref(), None, None, None, None);
        let info = mock_info(USER1, &coins(13_800, DENOM));
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Bond {}).unwrap();
        assert_users(deps.as_ref(), Some(13), None, None, None);

        // ensure messages for each of the 2 hooks
        assert_eq!(res.messages.len(), 2);
        let diff = MemberDiff::new(USER1, None, Some(13));
        let hook_msg = MemberChangedHookMsg::one(diff);
        let msg1 = hook_msg
            .clone()
            .into_cosmos_msg(contract1.clone())
            .map(SubMsg::new)
            .unwrap();
        let msg2 = hook_msg
            .into_cosmos_msg(contract2.clone())
            .map(SubMsg::new)
            .unwrap();
        assert_eq!(res.messages, vec![msg1, msg2]);

        // check firing on unbond
        let msg = ExecuteMsg::Unbond {
            tokens: coin(7_300, DENOM),
        };
        let info = mock_info(USER1, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_users(deps.as_ref(), Some(6), None, None, None);

        // ensure messages for each of the 2 hooks
        assert_eq!(res.messages.len(), 2);
        let diff = MemberDiff::new(USER1, Some(13), Some(6));
        let hook_msg = MemberChangedHookMsg::one(diff);
        let msg1 = hook_msg
            .clone()
            .into_cosmos_msg(contract1)
            .map(SubMsg::new)
            .unwrap();
        let msg2 = hook_msg
            .into_cosmos_msg(contract2)
            .map(SubMsg::new)
            .unwrap();
        assert_eq!(res.messages, vec![msg1, msg2]);
    }

    #[test]
    fn only_bond_valid_coins() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        // cannot bond with 0 coins
        let info = mock_info(USER1, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Bond {}).unwrap_err();
        assert_eq!(err, ContractError::NoFunds {});

        // cannot bond with incorrect denom
        let info = mock_info(USER1, &[coin(500, "FOO")]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Bond {}).unwrap_err();
        assert_eq!(err, ContractError::MissingDenom(DENOM.to_string()));

        // cannot bond with 2 coins (even if one is correct)
        let info = mock_info(USER1, &[coin(1234, DENOM), coin(5000, "BAR")]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Bond {}).unwrap_err();
        assert_eq!(err, ContractError::ExtraDenoms(DENOM.to_string()));

        // can bond with just the proper denom
        // cannot bond with incorrect denom
        let info = mock_info(USER1, &[coin(500, DENOM)]);
        execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Bond {}).unwrap();
    }

    #[test]
    fn ensure_bonding_edge_cases() {
        // use min_bond 0, tokens_per_weight 500
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut(), Uint128::new(100), Uint128::zero(), 5, 0);

        // setting 50 tokens, gives us Some(0) weight
        // even setting to 1 token
        bond(deps.as_mut(), 50, 1, 102, 1);
        assert_users(deps.as_ref(), Some(0), Some(0), Some(1), None);

        // reducing to 0 token makes us None even with min_bond 0
        unbond(deps.as_mut(), 49, 1, 102, 2, 0);
        assert_users(deps.as_ref(), Some(0), None, None, None);
    }

    #[test]
    fn paginated_claim_query() {
        let mut deps = mock_dependencies();
        default_instantiate(deps.as_mut());

        // create some data
        let mut env = mock_env();
        let msg = ExecuteMsg::Bond {};
        let info = mock_info(USER1, &coins(500, DENOM));
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        let info = mock_info(USER1, &[]);
        for _ in 0..10 {
            env.block.time = env.block.time.plus_seconds(10);
            let msg = ExecuteMsg::Unbond {
                tokens: coin(10, DENOM),
            };
            execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // check is number of claims is properly limited
        let claims = get_claims(deps.as_ref(), Addr::unchecked(USER1), Some(6), None);
        assert_eq!(claims.len(), 6);
        // check if rest is equal to remainder
        let next = get_claims(
            deps.as_ref(),
            Addr::unchecked(USER1),
            None,
            Some(claims[5].release_at),
        );
        assert_eq!(next.len(), 4);

        // check if joining and sorting both vectors equal number from start
        let mut all_claims = get_claims(deps.as_ref(), Addr::unchecked(USER1), None, None);
        all_claims.sort_by_key(|claim| claim.addr.clone());

        let mut concatenated = [claims, next].concat();
        concatenated.sort_by_key(|claim| claim.addr.clone());
        assert_eq!(concatenated, all_claims);
    }

    mod auto_release_claims {
        // Because of tests framework limitations at the point of implementing this test, it is
        // difficult to actually test reaction for tgrade sudo messages. Instead to check the
        // auto-release functionality, there are assumptions made:
        // * Registration to sudo events is correct
        // * Auto-releasing claims occurs on sudo EndBlock message, and the message is purely
        // calling the `end_block` function - calling this function in test is simulating actual
        // end block

        use cosmwasm_std::CosmosMsg;

        use super::*;

        fn do_instantiate(deps: DepsMut, limit: u64) {
            super::do_instantiate(deps, TOKENS_PER_WEIGHT, MIN_BOND, UNBONDING_DURATION, limit)
        }

        /// Helper for asserting if expected transfers occurred in response. Panics if any non
        /// `BankMsg::Send` occurred, or transfers are different than expected.
        ///
        /// Transfers are passed in form of pairs `(addr, amount)`, as for all test in this module
        /// expected denom is fixed
        #[track_caller]
        fn assert_transfers(response: Response, mut expected_transfers: Vec<(&str, u128)>) {
            let mut sends: Vec<_> = response
                .messages
                .into_iter()
                .map(|msg| match msg.msg {
                    // Trick is used here - bank send messages are filtered out, and mapped to tripple
                    // `(addr, amount_sum, msg)` - `addr` and `amount_sum` would be used only to
                    // properly sort messages, then they would be discarded. As in expected messages
                    // always only one coin is expected for all send messages, taking sum for sorting
                    // is good enough - in case of multiple of invalid denoms it would be visible on
                    // comparison.
                    //
                    // Possibly in future it would be possible for another messages to occur - in such
                    // case instead of returning err and panicking from this function, such messages
                    // should be filtered out.
                    CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => Ok((
                        to_address.clone(),
                        amount.iter().map(|c| c.amount).sum::<Uint128>(),
                        BankMsg::Send { to_address, amount },
                    )),
                    msg => Err(format!(
                        "Unexpected message on response, expected only bank send messages: {:?}",
                        msg
                    )),
                })
                .collect::<Result<_, _>>()
                .unwrap();

            sends.sort_by_key(|(addr, amount_sum, _)| (addr.clone(), *amount_sum));
            // Drop  addr and amount_sum for comparison
            let sends: Vec<_> = sends.into_iter().map(|(_, _, msg)| msg).collect();

            // Tuples are sorted simply first by addresses, then by amount
            expected_transfers.sort_unstable();

            // Build messages for comparison
            let expected_transfers: Vec<_> = expected_transfers
                .into_iter()
                .map(|(addr, amount)| BankMsg::Send {
                    to_address: addr.to_owned(),
                    amount: coins(amount, DENOM),
                })
                .collect();

            assert_eq!(sends, expected_transfers);
        }

        #[test]
        fn single_claim() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 2);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;

            unbond(deps.as_mut(), 1000, 0, 0, height_delta, 0);
            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 1000)]);
        }

        #[test]
        fn multiple_users_claims() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 4);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;

            unbond(deps.as_mut(), 1000, 500, 0, height_delta, 0);
            unbond(deps.as_mut(), 0, 0, 200, height_delta, 1);
            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 1);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 1000), (USER2, 500), (USER3, 200)]);
        }

        #[test]
        fn single_user_multiple_claims() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 3);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;

            unbond(deps.as_mut(), 1000, 0, 0, height_delta, 0);
            unbond(deps.as_mut(), 500, 0, 0, height_delta, 1);
            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 1);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 1500)]);
        }

        #[test]
        fn only_expired_claims() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 3);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;

            // Claims to be returned
            unbond(deps.as_mut(), 1000, 0, 0, height_delta, 0);
            unbond(deps.as_mut(), 500, 600, 0, height_delta, 1);

            // Clams not yet expired
            unbond(deps.as_mut(), 200, 300, 400, height_delta, 2);
            unbond(deps.as_mut(), 700, 0, 0, height_delta, 3);
            unbond(deps.as_mut(), 0, 100, 50, height_delta, 4);

            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 1);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 1500), (USER2, 600)]);
        }

        #[test]
        fn claim_returned_once() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 5);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;

            // Claims to be returned
            unbond(deps.as_mut(), 1000, 0, 0, height_delta, 0);
            unbond(deps.as_mut(), 500, 600, 0, height_delta, 1);

            // Clams not yet expired
            unbond(deps.as_mut(), 200, 300, 400, height_delta, 2);

            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 1);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 1500), (USER2, 600)]);

            // Some additional claims
            unbond(deps.as_mut(), 700, 0, 0, height_delta, 3);
            unbond(deps.as_mut(), 0, 100, 50, height_delta, 4);

            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 3);

            // Expected that claims at time offset 2 and 3 are returned (0 and 1 are already
            // returned, 4 is not yet expired)
            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 900), (USER2, 300), (USER3, 400)]);
        }

        #[test]
        fn up_to_limit_claims_returned() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 2);

            bond(deps.as_mut(), 12_000, 7_500, 4_000, 1);
            let height_delta = 2;

            // Claims to be returned
            unbond(deps.as_mut(), 1000, 500, 0, height_delta, 0);
            unbond(deps.as_mut(), 0, 600, 0, height_delta, 1);
            unbond(deps.as_mut(), 200, 0, 0, height_delta, 2);
            unbond(deps.as_mut(), 0, 0, 300, height_delta, 3);

            // Even if all claims are already expired, only two of them (time offset 0) should be
            // returned
            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 3);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 1000), (USER2, 500)]);

            // Then on next block next batch is returned (time offset 1 and 2)
            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 4);

            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 200), (USER2, 600)]);

            // Some additional claims
            unbond(deps.as_mut(), 700, 0, 0, height_delta, 5);
            unbond(deps.as_mut(), 0, 100, 50, height_delta, 6);

            // Claims are returned in batches
            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 6);

            // offset 3 and 5
            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER1, 700), (USER3, 300)]);

            let mut env = mock_env();
            env.block.height += height_delta;
            env.block.time = env.block.time.plus_seconds(UNBONDING_DURATION + 6);

            // offset 6
            let resp = end_block(deps.as_mut(), env).unwrap();
            assert_transfers(resp, vec![(USER2, 100), (USER3, 50)]);
        }

        #[test]
        fn unbound_with_invalid_denom_fails() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut(), 2);

            bond(deps.as_mut(), 5_000, 0, 0, 1);
            let height_delta = 2;

            let mut env = mock_env();
            env.block.height += height_delta;

            let msg = ExecuteMsg::Unbond {
                tokens: coin(5_000, "invalid"),
            };
            let info = mock_info(USER1, &[]);
            let err = execute(deps.as_mut(), env, info, msg).unwrap_err();

            assert_eq!(ContractError::InvalidDenom("invalid".to_owned()), err);
        }
    }
}
