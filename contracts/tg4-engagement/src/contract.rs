#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, Addr, BankMsg, Binary, Coin, Decimal, Deps, DepsMut, Env, Event, MessageInfo,
    Order, StdResult, Timestamp, Uint128,
};
use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey};
use cw_utils::maybe_addr;
use tg4::{
    HooksResponse, Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};

use crate::error::ContractError;
use crate::msg::{
    DelegatedResponse, ExecuteMsg, FundsResponse, HalflifeInfo, HalflifeResponse, InstantiateMsg,
    PreauthResponse, QueryMsg, SudoMsg,
};
use crate::state::{
    Distribution, Halflife, WithdrawAdjustment, DISTRIBUTION, HALFLIFE, POINTS_SHIFT,
    PREAUTH_SLASHING, SLASHERS, WITHDRAW_ADJUSTMENT,
};
use tg_bindings::{request_privileges, Privilege, PrivilegeChangeMsg, TgradeMsg};
use tg_utils::{members, validate_portion, Duration, ADMIN, HOOKS, PREAUTH_HOOKS, TOTAL};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tg4-engagement";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    create(
        deps,
        msg.admin,
        msg.members,
        msg.preauths_hooks,
        msg.preauths_slashing,
        env.block.height,
        env.block.time,
        msg.halflife,
        msg.denom,
    )?;

    Ok(Response::default())
}

// create is the instantiation logic with set_contract_version removed so it can more
// easily be imported in other contracts
#[allow(clippy::too_many_arguments)]
pub fn create(
    mut deps: DepsMut,
    admin: Option<String>,
    members_list: Vec<Member>,
    preauths_hooks: u64,
    preauths_slashing: u64,
    height: u64,
    time: Timestamp,
    halflife: Option<Duration>,
    denom: String,
) -> Result<(), ContractError> {
    let admin_addr = admin
        .map(|admin| deps.api.addr_validate(&admin))
        .transpose()?;
    ADMIN.set(deps.branch(), admin_addr)?;

    PREAUTH_HOOKS.set_auth(deps.storage, preauths_hooks)?;
    PREAUTH_SLASHING.set_auth(deps.storage, preauths_slashing)?;

    let data = Halflife {
        halflife,
        last_applied: time,
    };
    HALFLIFE.save(deps.storage, &data)?;

    let distribution = Distribution {
        denom,
        points_per_weight: Uint128::zero(),
        points_leftover: 0,
        distributed_total: Uint128::zero(),
        withdrawable_total: Uint128::zero(),
    };
    DISTRIBUTION.save(deps.storage, &distribution)?;

    let mut total = 0u64;

    for member in members_list.into_iter() {
        total += member.weight;
        let member_addr = deps.api.addr_validate(&member.addr)?;
        members().save(deps.storage, &member_addr, &member.weight, height)?;

        let adjustment = WithdrawAdjustment {
            points_correction: 0i128.into(),
            withdrawn_funds: Uint128::zero(),
            delegated: member_addr.clone(),
        };
        WITHDRAW_ADJUSTMENT.save(deps.storage, &member_addr, &adjustment)?;
    }
    TOTAL.save(deps.storage, &total)?;

    SLASHERS.instantiate(deps.storage)?;

    Ok(())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    use ExecuteMsg::*;

    let api = deps.api;
    match msg {
        UpdateAdmin { admin } => Ok(ADMIN.execute_update_admin(
            deps,
            info,
            admin.map(|admin| api.addr_validate(&admin)).transpose()?,
        )?),
        UpdateMembers { add, remove } => execute_update_members(deps, env, info, add, remove),
        AddPoints { addr, points } => execute_add_points(deps, env, info, addr, points),
        AddHook { addr } => execute_add_hook(deps, info, addr),
        RemoveHook { addr } => execute_remove_hook(deps, info, addr),
        DistributeFunds { sender } => execute_distribute_tokens(deps, env, info, sender),
        WithdrawFunds { owner, receiver } => execute_withdraw_tokens(deps, info, owner, receiver),
        DelegateWithdrawal { delegated } => execute_delegate_withdrawal(deps, info, delegated),
        AddSlasher { addr } => execute_add_slasher(deps, info, addr),
        RemoveSlasher { addr } => execute_remove_slasher(deps, info, addr),
        Slash { addr, portion } => execute_slash(deps, env, info, addr, portion),
    }
}

pub fn execute_add_points(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    addr: String,
    points: u64,
) -> Result<Response, ContractError> {
    let mut res = Response::new()
        .add_attribute("action", "add_points")
        .add_attribute("to_member", addr.to_string())
        .add_attribute("amount", points.to_string());

    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    let old_weight = query_member(deps.as_ref(), addr.clone(), None)?
        .weight
        .unwrap_or_default();

    // make the local update
    let diff = update_members(
        deps.branch(),
        env.block.height,
        vec![Member {
            addr,
            weight: old_weight + points,
        }],
        vec![],
    )?;
    // call all registered hooks
    res.messages = HOOKS.prepare_hooks(deps.storage, |h| {
        diff.clone().into_cosmos_msg(h).map(SubMsg::new)
    })?;
    Ok(res)
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
            "Hook address is not same as sender's or sender is not an admin".to_owned(),
        ));
    }

    // remove the hook
    HOOKS.remove_hook(deps.storage, hook_addr)?;

    // response
    let resp = Response::new()
        .add_attribute("action", "remove_hook")
        .add_attribute("hook", hook)
        .add_attribute("sender", info.sender);
    Ok(resp)
}

pub fn execute_update_members(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Vec<Member>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let mut res = Response::new()
        .add_attribute("action", "update_members")
        .add_attribute("added", add.len().to_string())
        .add_attribute("removed", remove.len().to_string())
        .add_attribute("sender", &info.sender);

    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    // make the local update
    let diff = update_members(deps.branch(), env.block.height, add, remove)?;
    // call all registered hooks
    res.messages = HOOKS.prepare_hooks(deps.storage, |h| {
        diff.clone().into_cosmos_msg(h).map(SubMsg::new)
    })?;
    Ok(res)
}

pub fn execute_distribute_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Option<String>,
) -> Result<Response, ContractError> {
    let total = TOTAL.load(deps.storage)? as u128;

    // There are no shares in play - noone to distribute to
    if total == 0 {
        return Err(ContractError::NoMembersToDistributeTo {});
    }

    let sender = sender
        .map(|sender| deps.api.addr_validate(&sender))
        .transpose()?
        .unwrap_or(info.sender);

    let mut distribution = DISTRIBUTION.load(deps.storage)?;

    let withdrawable: u128 = distribution.withdrawable_total.into();
    let balance: u128 = deps
        .querier
        .query_balance(env.contract.address, distribution.denom.clone())?
        .amount
        .into();

    let amount = balance - withdrawable;
    if amount == 0 {
        return Ok(Response::new());
    }

    let leftover: u128 = distribution.points_leftover.into();
    let points = (amount << POINTS_SHIFT) + leftover;
    let points_per_share = points / total;
    distribution.points_leftover = (points % total) as u64;

    // Everything goes back to 128-bits/16-bytes
    // Full amount is added here to total withdrawable, as it should not be considered on its own
    // on future distributions - even if because of calculation offsets it is not fully
    // distributed, the error is handled by leftover.
    distribution.points_per_weight += Uint128::from(points_per_share);
    distribution.distributed_total += Uint128::from(amount);
    distribution.withdrawable_total += Uint128::from(amount);

    DISTRIBUTION.save(deps.storage, &distribution)?;

    let resp = Response::new()
        .add_attribute("action", "distribute_tokens")
        .add_attribute("sender", sender.as_str())
        .add_attribute("denom", &distribution.denom)
        .add_attribute("amount", &amount.to_string());

    Ok(resp)
}

pub fn execute_withdraw_tokens(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    let owner = owner.map_or_else(
        || Ok(info.sender.clone()),
        |owner| deps.api.addr_validate(&owner),
    )?;

    let mut distribution = DISTRIBUTION.load(deps.storage)?;
    let mut adjustment = WITHDRAW_ADJUSTMENT.load(deps.storage, &owner)?;

    if ![&owner, &adjustment.delegated].contains(&&info.sender) {
        return Err(ContractError::Unauthorized(
            "Sender is neither owner or delegated".to_owned(),
        ));
    }

    let token = withdrawable_funds(deps.as_ref(), &owner, &distribution, &adjustment)?;
    let receiver = receiver
        .map(|receiver| deps.api.addr_validate(&receiver))
        .transpose()?
        .unwrap_or_else(|| info.sender.clone());

    if token.amount.is_zero() {
        // Just do nothing
        return Ok(Response::new());
    }

    adjustment.withdrawn_funds += token.amount;
    WITHDRAW_ADJUSTMENT.save(deps.storage, &owner, &adjustment)?;
    distribution.withdrawable_total -= token.amount;
    DISTRIBUTION.save(deps.storage, &distribution)?;

    let resp = Response::new()
        .add_attribute("action", "withdraw_tokens")
        .add_attribute("sender", info.sender.as_str())
        .add_attribute("owner", owner.as_str())
        .add_attribute("receiver", receiver.as_str())
        .add_attribute("token", &token.denom)
        .add_attribute("amount", &token.amount.to_string())
        .add_submessage(SubMsg::new(BankMsg::Send {
            to_address: receiver.to_string(),
            amount: vec![token],
        }));

    Ok(resp)
}

pub fn execute_delegate_withdrawal(
    deps: DepsMut,
    info: MessageInfo,
    delegated: String,
) -> Result<Response, ContractError> {
    let delegated = deps.api.addr_validate(&delegated)?;

    WITHDRAW_ADJUSTMENT.update(deps.storage, &info.sender, |data| -> StdResult<_> {
        Ok(data.map_or_else(
            || WithdrawAdjustment {
                points_correction: 0.into(),
                withdrawn_funds: Uint128::zero(),
                delegated: delegated.clone(),
            },
            |mut data| {
                data.delegated = delegated.clone();
                data
            },
        ))
    })?;

    let resp = Response::new()
        .add_attribute("action", "delegate_withdrawal")
        .add_attribute("sender", info.sender.as_str())
        .add_attribute("delegated", &delegated);

    Ok(resp)
}

/// Adds new slasher to contract
pub fn execute_add_slasher(
    deps: DepsMut,
    info: MessageInfo,
    slasher: String,
) -> Result<Response, ContractError> {
    if !ADMIN.is_admin(deps.as_ref(), &info.sender)? {
        PREAUTH_SLASHING.use_auth(deps.storage)?;
    }

    SLASHERS.add_slasher(deps.storage, deps.api.addr_validate(&slasher)?)?;

    let res = Response::new()
        .add_attribute("action", "add_slasher")
        .add_attribute("slasher", slasher)
        .add_attribute("sender", info.sender);

    Ok(res)
}

/// Removes slasher from contract
pub fn execute_remove_slasher(
    deps: DepsMut,
    info: MessageInfo,
    slasher: String,
) -> Result<Response, ContractError> {
    // Do not need to validate - it would be "verified" on when it is compared to be either admin
    // or slasher which is already verified.
    let slasher_addr = Addr::unchecked(&slasher);

    if info.sender != slasher_addr && !ADMIN.is_admin(deps.as_ref(), &info.sender)? {
        return Err(ContractError::Unauthorized(
            "Only slasher might remove himself or sender is not an admin".to_owned(),
        ));
    }

    SLASHERS.remove_slasher(deps.storage, slasher_addr)?;

    let res = Response::new()
        .add_attribute("action", "remove_slasher")
        .add_attribute("slasher", slasher)
        .add_attribute("sender", info.sender);

    Ok(res)
}

/// Slashes engagement points from address
pub fn execute_slash(
    mut deps: DepsMut,
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
    let addr = Addr::unchecked(&addr);
    // check if address belongs to member, otherwise leave early
    if members().may_load(deps.storage, &addr)?.is_none() {
        return Ok(Response::new());
    };

    validate_portion(portion)?;

    let ppw: u128 = DISTRIBUTION.load(deps.storage)?.points_per_weight.into();

    let mut diff = 0i128;

    members().update(
        deps.storage,
        &addr,
        env.block.height,
        |old| -> StdResult<_> {
            let old = match old {
                Some(old) => Uint128::new(old as _),
                None => Uint128::zero(),
            };

            let slash = old * portion;
            let new = old - slash;

            diff = -(slash.u128() as i128);

            Ok(new.u128() as _)
        },
    )?;
    apply_points_correction(deps.branch(), &addr, ppw, diff)?;

    TOTAL.update(deps.storage, |total| -> StdResult<_> {
        Ok((total as i128 + diff) as _)
    })?;

    let res = Response::new()
        .add_attribute("action", "slash")
        .add_attribute("addr", &addr)
        .add_attribute("sender", info.sender);

    Ok(res)
}

/// Calculates withdrawable funds from distribution and adjustment info.
pub fn withdrawable_funds(
    deps: Deps,
    owner: &Addr,
    distribution: &Distribution,
    adjustment: &WithdrawAdjustment,
) -> StdResult<Coin> {
    let ppw: u128 = distribution.points_per_weight.into();
    let weight: u128 = members()
        .may_load(deps.storage, owner)?
        .unwrap_or_default()
        .into();
    let correction: i128 = adjustment.points_correction.into();
    let withdrawn: u128 = adjustment.withdrawn_funds.into();
    let points = (ppw * weight) as i128;
    let points = points + correction;
    let amount = points as u128 >> POINTS_SHIFT;
    let amount = amount - withdrawn;

    Ok(coin(amount, &distribution.denom))
}

pub fn sudo_add_member(
    mut deps: DepsMut,
    env: Env,
    add: Member,
) -> Result<Response, ContractError> {
    let mut res = Response::new()
        .add_attribute("action", "sudo_add_member")
        .add_attribute("addr", add.addr.clone())
        .add_attribute("weight", add.weight.to_string());

    // make the local update
    let diff = update_members(deps.branch(), env.block.height, vec![add], vec![])?;
    // call all registered hooks
    res.messages = HOOKS.prepare_hooks(deps.storage, |h| {
        diff.clone().into_cosmos_msg(h).map(SubMsg::new)
    })?;
    Ok(res)
}

// the logic from execute_update_members extracted for easier import
pub fn update_members(
    mut deps: DepsMut,
    height: u64,
    to_add: Vec<Member>,
    to_remove: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    let mut total = TOTAL.load(deps.storage)?;
    let mut diffs: Vec<MemberDiff> = vec![];

    let ppw: u128 = DISTRIBUTION.load(deps.storage)?.points_per_weight.into();

    // add all new members and update total
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add.addr)?;

        let mut diff = 0;
        let mut insert_funds = false;
        members().update(deps.storage, &add_addr, height, |old| -> StdResult<_> {
            diffs.push(MemberDiff::new(add.addr, old, Some(add.weight)));
            insert_funds = old.is_none();
            let old = old.unwrap_or_default();
            total -= old;
            total += add.weight;
            diff = add.weight as i128 - old as i128;
            Ok(add.weight)
        })?;
        apply_points_correction(deps.branch(), &add_addr, ppw, diff)?;
    }

    for remove in to_remove.into_iter() {
        let remove_addr = deps.api.addr_validate(&remove)?;
        let old = members().may_load(deps.storage, &remove_addr)?;
        // Only process this if they were actually in the list before
        if let Some(weight) = old {
            diffs.push(MemberDiff::new(remove, Some(weight), None));
            total -= weight;
            members().remove(deps.storage, &remove_addr, height)?;
            apply_points_correction(deps.branch(), &remove_addr, ppw, -(weight as i128))?;
        }
    }

    TOTAL.save(deps.storage, &total)?;
    Ok(MemberChangedHookMsg { diffs })
}

/// Applies points correction for given address.
/// `points_per_weight` is current value from `POINTS_PER_WEIGHT` - not loaded in function, to
/// avoid multiple queries on bulk updates.
/// `diff` is the weight change
pub fn apply_points_correction(
    deps: DepsMut,
    addr: &Addr,
    points_per_weight: u128,
    diff: i128,
) -> StdResult<()> {
    WITHDRAW_ADJUSTMENT.update(deps.storage, addr, |old| -> StdResult<_> {
        let mut old = old.unwrap_or_else(|| {
            // This should never happen, but better this than panic
            WithdrawAdjustment {
                points_correction: 0.into(),
                withdrawn_funds: Uint128::zero(),
                delegated: addr.clone(),
            }
        });
        let points_correction: i128 = old.points_correction.into();
        old.points_correction = (points_correction - points_per_weight as i128 * diff).into();
        Ok(old)
    })?;
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::UpdateMember(member) => sudo_add_member(deps, env, member),
        SudoMsg::PrivilegeChange(PrivilegeChangeMsg::Promoted {}) => privilege_promote(deps),
        SudoMsg::EndBlock {} => end_block(deps, env),
        _ => Err(ContractError::UnknownSudoMsg {}),
    }
}

fn privilege_promote(deps: DepsMut) -> Result<Response, ContractError> {
    if HALFLIFE.load(deps.storage)?.halflife.is_some() {
        let msgs = request_privileges(&[Privilege::EndBlocker]);
        Ok(Response::new().add_submessages(msgs))
    } else {
        Ok(Response::new())
    }
}

fn weight_reduction(weight: u64) -> u64 {
    weight - (weight / 2)
}

fn end_block(mut deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let resp = Response::new();

    // If duration of half life added to timestamp of last applied
    // if lesser then current timestamp, do nothing
    if !HALFLIFE.load(deps.storage)?.should_apply(env.block.time) {
        return Ok(resp);
    }

    let ppw: u128 = DISTRIBUTION.load(deps.storage)?.points_per_weight.into();

    let mut reduction = 0;

    let members_to_update: Vec<_> = members()
        .range(deps.storage, None, None, Order::Ascending)
        .filter_map(|item| {
            (move || -> StdResult<Option<_>> {
                let (addr, weight) = item?;
                if weight <= 1 {
                    return Ok(None);
                }
                Ok(Some(Member {
                    addr: addr.into(),
                    weight,
                }))
            })()
            .transpose()
        })
        .collect::<StdResult<_>>()?;

    for member in members_to_update {
        let diff = weight_reduction(member.weight);
        reduction += diff;
        let addr = Addr::unchecked(member.addr);
        members().replace(
            deps.storage,
            &addr,
            Some(&(member.weight - diff)),
            Some(&member.weight),
            env.block.height,
        )?;
        apply_points_correction(deps.branch(), &addr, ppw, -(diff as i128))?;
    }

    // We need to update half life's last applied timestamp to current one
    HALFLIFE.update(deps.storage, |hf| -> StdResult<_> {
        Ok(Halflife {
            halflife: hf.halflife,
            last_applied: env.block.time,
        })
    })?;

    let mut total = TOTAL.load(deps.storage)?;
    total -= reduction;
    TOTAL.save(deps.storage, &total)?;

    let evt = Event::new("halflife")
        .add_attribute("height", env.block.height.to_string())
        .add_attribute("reduction", reduction.to_string());
    let resp = resp.add_event(evt);

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use QueryMsg::*;
    match msg {
        Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        ListMembers { start_after, limit } => to_binary(&list_members(deps, start_after, limit)?),
        ListMembersByWeight { start_after, limit } => {
            to_binary(&list_members_by_weight(deps, start_after, limit)?)
        }
        TotalWeight {} => to_binary(&query_total_weight(deps)?),
        Admin {} => to_binary(&ADMIN.query_admin(deps)?),
        Hooks {} => {
            let hooks = HOOKS.list_hooks(deps.storage)?;
            to_binary(&HooksResponse { hooks })
        }
        Preauths {} => {
            let preauths = PREAUTH_HOOKS.get_auth(deps.storage)?;
            to_binary(&PreauthResponse { preauths })
        }
        WithdrawableFunds { owner } => to_binary(&query_withdrawable_funds(deps, owner)?),
        DistributedFunds {} => to_binary(&query_distributed_total(deps)?),
        UndistributedFunds {} => to_binary(&query_undistributed_funds(deps, env)?),
        Delegated { owner } => to_binary(&query_delegated(deps, owner)?),
        Halflife {} => to_binary(&query_halflife(deps)?),
        IsSlasher { addr } => {
            let addr = deps.api.addr_validate(&addr)?;
            to_binary(&SLASHERS.is_slasher(deps.storage, &addr)?)
        }
        ListSlashers {} => to_binary(&SLASHERS.list_slashers(deps.storage)?),
        DistributionData {} => to_binary(&DISTRIBUTION.may_load(deps.storage)?),
        WithdrawAdjustmentData { addr } => {
            let addr = deps.api.addr_validate(&addr)?;
            to_binary(&WITHDRAW_ADJUSTMENT.may_load(deps.storage, &addr)?)
        }
    }
}

fn query_total_weight(deps: Deps) -> StdResult<TotalWeightResponse> {
    let weight = TOTAL.load(deps.storage)?;
    Ok(TotalWeightResponse { weight })
}

fn query_member(deps: Deps, addr: String, height: Option<u64>) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let weight = match height {
        Some(h) => members().may_load_at_height(deps.storage, &addr, h),
        None => members().may_load(deps.storage, &addr),
    }?;
    Ok(MemberResponse { weight })
}

pub fn query_withdrawable_funds(deps: Deps, owner: String) -> StdResult<FundsResponse> {
    // Not checking address, as if it is ivnalid it is guaranteed not to appear in maps, so
    // `withdrawable_funds` would return error itself.
    let owner = Addr::unchecked(&owner);
    let distribution = DISTRIBUTION.load(deps.storage)?;
    let adjustment = if let Some(adj) = WITHDRAW_ADJUSTMENT.may_load(deps.storage, &owner)? {
        adj
    } else {
        return Ok(FundsResponse {
            funds: coin(0, distribution.denom),
        });
    };

    let token = withdrawable_funds(deps, &owner, &distribution, &adjustment)?;
    Ok(FundsResponse { funds: token })
}

pub fn query_undistributed_funds(deps: Deps, env: Env) -> StdResult<FundsResponse> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    let balance = deps
        .querier
        .query_balance(env.contract.address, distribution.denom.clone())?
        .amount;

    Ok(FundsResponse {
        funds: coin(
            (balance - distribution.withdrawable_total).into(),
            &distribution.denom,
        ),
    })
}

pub fn query_distributed_total(deps: Deps) -> StdResult<FundsResponse> {
    let distribution = DISTRIBUTION.load(deps.storage)?;
    Ok(FundsResponse {
        funds: coin(distribution.distributed_total.into(), &distribution.denom),
    })
}

pub fn query_delegated(deps: Deps, owner: String) -> StdResult<DelegatedResponse> {
    let owner = deps.api.addr_validate(&owner)?;

    let delegated = WITHDRAW_ADJUSTMENT
        .may_load(deps.storage, &owner)?
        .map_or(owner, |data| data.delegated);

    Ok(DelegatedResponse { delegated })
}

fn query_halflife(deps: Deps) -> StdResult<HalflifeResponse> {
    let Halflife {
        halflife,
        last_applied: last_halflife,
    } = HALFLIFE.load(deps.storage)?;

    Ok(HalflifeResponse {
        halflife_info: halflife.map(|d| {
            let next_halflife = last_halflife.plus_seconds(d.seconds());

            HalflifeInfo {
                last_halflife,
                halflife: d,
                next_halflife,
            }
        }),
    })
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
    use super::*;

    use crate::i128::Int128;

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{from_slice, Api, OwnedDeps, Querier, StdError, Storage};
    use cw_controllers::AdminError;
    use cw_storage_plus::Map;
    use tg4::{member_key, TOTAL_KEY};
    use tg_utils::{HookError, PreauthError};

    const INIT_ADMIN: &str = "ADMIN";
    const USER1: &str = "USER1";
    const USER1_WEIGHT: u64 = 11;
    const USER2: &str = "USER2";
    const USER2_WEIGHT: u64 = 6;
    const USER3: &str = "USER3";
    const HALFLIFE: u64 = 180 * 24 * 60 * 60;

    fn mock_env_height(height_offset: u64) -> Env {
        let mut env = mock_env();
        env.block.height += height_offset;
        env
    }

    fn do_instantiate(deps: DepsMut) {
        let msg = InstantiateMsg {
            admin: Some(INIT_ADMIN.into()),
            members: vec![
                Member {
                    addr: USER1.into(),
                    weight: USER1_WEIGHT,
                },
                Member {
                    addr: USER2.into(),
                    weight: USER2_WEIGHT,
                },
            ],
            preauths_hooks: 1,
            preauths_slashing: 0,
            halflife: Some(Duration::new(HALFLIFE)),
            denom: "usdc".to_owned(),
        };
        let info = mock_info("creator", &[]);
        instantiate(deps, mock_env(), info, msg).unwrap();
    }

    #[test]
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // it worked, let's query the state
        let res = ADMIN.query_admin(deps.as_ref()).unwrap();
        assert_eq!(Some(INIT_ADMIN.into()), res.admin);

        let res = query_total_weight(deps.as_ref()).unwrap();
        assert_eq!(17, res.weight);

        let preauths = PREAUTH_HOOKS.get_auth(&deps.storage).unwrap();
        assert_eq!(1, preauths);

        let raw = query(deps.as_ref(), mock_env(), QueryMsg::DistributionData {}).unwrap();
        let res: Distribution = from_slice(&raw).unwrap();
        assert_eq!(
            res,
            Distribution {
                denom: "usdc".to_owned(),
                points_per_weight: Uint128::zero(),
                points_leftover: 0,
                distributed_total: Uint128::zero(),
                withdrawable_total: Uint128::zero(),
            }
        );

        let raw = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::WithdrawAdjustmentData {
                addr: USER1.to_owned(),
            },
        )
        .unwrap();
        let res: WithdrawAdjustment = from_slice(&raw).unwrap();
        assert_eq!(
            res,
            WithdrawAdjustment {
                points_correction: Int128::zero(),
                withdrawn_funds: Uint128::zero(),
                delegated: Addr::unchecked("USER1"),
            }
        );
    }

    #[test]
    fn try_member_queries() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        let member1 = query_member(deps.as_ref(), USER1.into(), None).unwrap();
        assert_eq!(member1.weight, Some(11));

        let member2 = query_member(deps.as_ref(), USER2.into(), None).unwrap();
        assert_eq!(member2.weight, Some(6));

        let member3 = query_member(deps.as_ref(), USER3.into(), None).unwrap();
        assert_eq!(member3.weight, None);

        let members = list_members(deps.as_ref(), None, None).unwrap();
        assert_eq!(members.members.len(), 2);
        // assert the set is proper
        let members = list_members(deps.as_ref(), None, None).unwrap().members;
        assert_eq!(members.len(), 2);
        // Assert the set is proper
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
                weight: 11
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
                weight: 6
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
        do_instantiate(deps.as_mut());

        let members = list_members_by_weight(deps.as_ref(), None, None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 2);
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
        let start_after = Some(members[0].clone());
        let members = list_members_by_weight(deps.as_ref(), start_after, None)
            .unwrap()
            .members;
        assert_eq!(members.len(), 1);
        // Assert the set is proper
        assert_eq!(
            members,
            vec![Member {
                addr: USER2.into(),
                weight: 6
            },]
        );

        // Assert there's no more
        let start_after = Some(members[0].clone());
        let members = list_members_by_weight(deps.as_ref(), start_after, Some(1))
            .unwrap()
            .members;
        assert_eq!(members.len(), 0);
    }

    #[test]
    fn try_halflife_queries() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        let HalflifeInfo {
            last_halflife,
            halflife,
            next_halflife,
        } = query_halflife(deps.as_ref())
            .unwrap()
            .halflife_info
            .unwrap();

        // Last halflife event.
        let env_block_time = mock_env().block.time;
        assert_eq!(last_halflife, env_block_time);

        // Halflife duration.
        assert_eq!(halflife, Duration::new(HALFLIFE));

        // Next halflife event.
        let expected_next_halflife = last_halflife.plus_seconds(halflife.seconds());
        assert_eq!(expected_next_halflife, next_halflife);
    }

    #[test]
    fn try_halflife_query_when_no_halflife() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            admin: Some(INIT_ADMIN.into()),
            members: vec![
                Member {
                    addr: USER1.into(),
                    weight: USER1_WEIGHT,
                },
                Member {
                    addr: USER2.into(),
                    weight: USER2_WEIGHT,
                },
            ],
            preauths_hooks: 1,
            preauths_slashing: 0,
            halflife: None,
            denom: "usdc".to_owned(),
        };
        let info = mock_info("creator", &[]);

        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(query_halflife(deps.as_ref()).unwrap().halflife_info, None);
    }

    #[test]
    fn handle_non_utf8_in_members_list() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // make sure we get 2 members as expected, no error
        let members = list_members(deps.as_ref(), None, None).unwrap().members;
        assert_eq!(members.len(), 2);

        // we write some garbage non-utf8 key in the same key space as members, with some tricks
        const BIN_MEMBERS: Map<Vec<u8>, u64> = Map::new(tg4::MEMBERS_KEY);
        BIN_MEMBERS
            .save(&mut deps.storage, vec![226, 130, 40], &123)
            .unwrap();

        // this should now error when trying to parse the invalid data (in the same keyspace)
        let err = list_members(deps.as_ref(), None, None).unwrap_err();
        assert!(matches!(err, StdError::InvalidUtf8 { .. }));
    }

    #[track_caller]
    fn assert_users<S: Storage, A: Api, Q: Querier>(
        deps: &OwnedDeps<S, A, Q>,
        user1_weight: Option<u64>,
        user2_weight: Option<u64>,
        user3_weight: Option<u64>,
        height: Option<u64>,
    ) {
        let member1 = query_member(deps.as_ref(), USER1.into(), height).unwrap();
        assert_eq!(member1.weight, user1_weight);

        let member2 = query_member(deps.as_ref(), USER2.into(), height).unwrap();
        assert_eq!(member2.weight, user2_weight);

        let member3 = query_member(deps.as_ref(), USER3.into(), height).unwrap();
        assert_eq!(member3.weight, user3_weight);

        // this is only valid if we are not doing a historical query
        if height.is_none() {
            // compute expected metrics
            let weights = vec![user1_weight, user2_weight, user3_weight];
            let sum: u64 = weights.iter().map(|x| x.unwrap_or_default()).sum();
            let count = weights.iter().filter(|x| x.is_some()).count();

            // TODO: more detailed compare?
            let members = list_members(deps.as_ref(), None, None).unwrap();
            assert_eq!(count, members.members.len());

            let total = query_total_weight(deps.as_ref()).unwrap();
            assert_eq!(sum, total.weight); // 17 - 11 + 15 = 21
        }
    }

    #[test]
    fn add_new_remove_old_member() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // add a new one and remove existing one
        let add = vec![Member {
            addr: USER3.into(),
            weight: 15,
        }];
        let remove = vec![USER1.into()];

        // non-admin cannot update
        let env = mock_env_height(5);
        let info = mock_info(USER1, &[]);
        let height = env.block.height - 5;

        let err = execute_update_members(deps.as_mut(), env, info, add.clone(), remove.clone())
            .unwrap_err();
        assert_eq!(err, AdminError::NotAdmin {}.into());

        // Test the values from instantiate
        assert_users(&deps, Some(11), Some(6), None, None);
        // Note all values were set at height, the beginning of that block was all None
        assert_users(&deps, None, None, None, Some(height));
        // This will get us the values at the start of the block after instantiate (expected initial values)
        assert_users(&deps, Some(11), Some(6), None, Some(height + 1));

        let env = mock_env_height(10);
        let info = mock_info(INIT_ADMIN, &[]);
        // admin updates properly
        execute_update_members(deps.as_mut(), env, info, add, remove).unwrap();

        // updated properly
        assert_users(&deps, None, Some(6), Some(15), None);

        // snapshot still shows old value
        assert_users(&deps, Some(11), Some(6), None, Some(height + 1));
    }

    #[test]
    fn add_old_remove_new_member() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // add a new one and remove existing one
        let add = vec![Member {
            addr: USER1.into(),
            weight: 4,
        }];
        let remove = vec![USER3.into()];

        let env = mock_env();
        let info = mock_info(INIT_ADMIN, &[]);

        // admin updates properly
        execute_update_members(deps.as_mut(), env, info, add, remove).unwrap();
        assert_users(&deps, Some(4), Some(6), None, None);
    }

    #[test]
    fn add_and_remove_same_member() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // USER1 is updated and remove in the same call, we should remove this an add member3
        let add = vec![
            Member {
                addr: USER1.into(),
                weight: 20,
            },
            Member {
                addr: USER3.into(),
                weight: 5,
            },
        ];
        let remove = vec![USER1.into()];

        let env = mock_env();
        let info = mock_info(INIT_ADMIN, &[]);

        // admin updates properly
        execute_update_members(deps.as_mut(), env, info, add, remove).unwrap();
        assert_users(&deps, None, Some(6), Some(5), None);
    }

    #[test]
    fn sudo_add_new_member() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // add a new member
        let add = Member {
            addr: USER3.into(),
            weight: 15,
        };

        let env = mock_env();
        let height = env.block.height;

        // Test the values from instantiate
        assert_users(&deps, Some(11), Some(6), None, None);
        // Note all values were set at height, the beginning of that block was all None
        assert_users(&deps, None, None, None, Some(height));
        // This will get us the values at the start of the block after instantiate (expected initial values)
        assert_users(&deps, Some(11), Some(6), None, Some(height + 1));

        let env = mock_env_height(10);

        sudo_add_member(deps.as_mut(), env, add).unwrap();

        // snapshot still shows old value
        assert_users(&deps, Some(11), Some(6), None, Some(height + 10));

        // updated properly in next snapshot
        assert_users(&deps, Some(11), Some(6), Some(15), Some(height + 11));

        // updated properly
        assert_users(&deps, Some(11), Some(6), Some(15), None);
    }

    #[test]
    fn sudo_update_existing_member() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // update an existing member
        let add = Member {
            addr: USER2.into(),
            weight: 1,
        };

        let env = mock_env();
        let height = env.block.height;

        // Test the values from instantiate
        assert_users(&deps, Some(11), Some(6), None, None);
        // Note all values were set at height, the beginning of that block was all None
        assert_users(&deps, None, None, None, Some(height));
        // This will get us the values at the start of the block after instantiate (expected initial values)
        assert_users(&deps, Some(11), Some(6), None, Some(height + 1));

        let env = mock_env_height(10);

        sudo_add_member(deps.as_mut(), env, add).unwrap();

        // snapshot still shows old value
        assert_users(&deps, Some(11), Some(6), None, Some(height + 10));

        // updated properly in next snapshot
        assert_users(&deps, Some(11), Some(1), None, Some(height + 11));

        // updated properly
        assert_users(&deps, Some(11), Some(1), None, None);
    }

    #[test]
    fn add_remove_hooks() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

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
                "Hook address is not same as sender's or sender is not an admin".to_owned()
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

    #[test]
    fn hooks_fire() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

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

        // make some changes - add 3, remove 2, and update 1
        // USER1 is updated and remove in the same call, we should remove this an add member3
        let add = vec![
            Member {
                addr: USER1.into(),
                weight: 20,
            },
            Member {
                addr: USER3.into(),
                weight: 5,
            },
        ];
        let remove = vec![USER2.into()];
        let msg = ExecuteMsg::UpdateMembers { remove, add };

        // admin updates properly
        assert_users(&deps, Some(11), Some(6), None, None);
        let res = execute(deps.as_mut(), mock_env(), admin_info, msg).unwrap();
        assert_users(&deps, Some(20), None, Some(5), None);

        // ensure 2 messages for the 2 hooks
        assert_eq!(res.messages.len(), 2);
        // same order as in the message (adds first, then remove)
        let diffs = vec![
            MemberDiff::new(USER1, Some(11), Some(20)),
            MemberDiff::new(USER3, None, Some(5)),
            MemberDiff::new(USER2, Some(6), None),
        ];
        let hook_msg = MemberChangedHookMsg { diffs };
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
    fn raw_queries_work() {
        // add will over-write and remove have no effect
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

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

    #[test]
    fn halflife_workflow() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());
        let mut env = mock_env();

        // end block just before half life time is met - do nothing
        env.block.time = env.block.time.plus_seconds(HALFLIFE - 2);
        assert_eq!(end_block(deps.as_mut(), env.clone()), Ok(Response::new()));
        assert_users(&deps, Some(USER1_WEIGHT), Some(USER2_WEIGHT), None, None);

        // end block at half life
        env.block.time = env.block.time.plus_seconds(HALFLIFE);
        let expected_reduction = weight_reduction(USER1_WEIGHT) + weight_reduction(USER2_WEIGHT);
        let evt = Event::new("halflife")
            .add_attribute("height", env.block.height.to_string())
            .add_attribute("reduction", expected_reduction.to_string());
        let resp = Response::new().add_event(evt);
        assert_eq!(end_block(deps.as_mut(), env.clone()), Ok(resp));
        assert_users(
            &deps,
            Some(USER1_WEIGHT / 2),
            Some(USER2_WEIGHT / 2),
            None,
            None,
        );

        // end block at same timestamp after last half life was met - do nothing
        end_block(deps.as_mut(), env.clone()).unwrap();
        assert_users(
            &deps,
            Some(USER1_WEIGHT / 2),
            Some(USER2_WEIGHT / 2),
            None,
            None,
        );

        // after two more iterations of halftime + end block, both users should have weight 1
        env.block.time = env.block.time.plus_seconds(HALFLIFE);
        end_block(deps.as_mut(), env.clone()).unwrap();
        env.block.time = env.block.time.plus_seconds(HALFLIFE);
        end_block(deps.as_mut(), env).unwrap();
        assert_users(&deps, Some(1), Some(1), None, None);
    }

    mod points {
        use super::*;

        #[test]
        fn add_to_existing_member() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut());

            let env = mock_env();
            let info = mock_info(INIT_ADMIN, &[]);

            // Originally USER1 has 11 points of weight
            execute_add_points(deps.as_mut(), env, info, "USER1".to_string(), 10).unwrap();
            assert_users(&deps, Some(21), Some(6), None, None);
        }

        #[test]
        fn add_to_nonexisting_member() {
            let mut deps = mock_dependencies();
            do_instantiate(deps.as_mut());

            let env = mock_env();
            let info = mock_info(INIT_ADMIN, &[]);

            let new_user = "USER111".to_owned();
            execute_add_points(deps.as_mut(), env, info, new_user.clone(), 10).unwrap();
            let new_member = query_member(deps.as_ref(), new_user, None).unwrap();
            assert_eq!(new_member.weight, Some(10));
        }
    }

    #[test]
    fn slash_nonexisting_user() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        let user1 = Addr::unchecked(USER1);
        SLASHERS
            .add_slasher(&mut deps.storage, user1.clone())
            .unwrap();

        // Trying to slash nonexisting user will result in no-op
        let res = execute_slash(
            deps.as_mut(),
            mock_env(),
            MessageInfo {
                sender: user1,
                funds: vec![],
            },
            "nonexisting_user".to_owned(),
            Decimal::percent(50),
        )
        .unwrap();
        assert_eq!(res, Response::new());
    }
}
