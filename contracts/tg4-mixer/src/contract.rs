#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Order, StdError, StdResult,
};

use cw2::set_contract_version;
use cw_storage_plus::{Bound, PrimaryKey};
use cw_utils::maybe_addr;

use tg_bindings::TgradeMsg;
use tg_utils::{
    members, validate_portion, SlashMsg, HOOKS, PREAUTH_HOOKS, PREAUTH_SLASHING, SLASHERS, TOTAL,
};

use tg4::{
    HooksResponse, Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    Tg4Contract, TotalWeightResponse,
};

use crate::error::ContractError;
use crate::functions::PoEFunction;
use crate::msg::{
    ExecuteMsg, GroupsResponse, InstantiateMsg, PoEFunctionType, PreauthResponse, QueryMsg,
    RewardFunctionResponse,
};
use crate::state::{Groups, GROUPS, POE_FUNCTION_TYPE};

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tg4-mixer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    PREAUTH_HOOKS.set_auth(deps.storage, msg.preauths_hooks)?;
    PREAUTH_SLASHING.set_auth(deps.storage, msg.preauths_slashing)?;

    SLASHERS.instantiate(deps.storage)?;

    // Store the PoE function type / params
    POE_FUNCTION_TYPE.save(deps.storage, &msg.function_type)?;

    // validate the two input groups and save
    let left = verify_tg4_input(deps.as_ref(), &msg.left_group)?;
    let right = verify_tg4_input(deps.as_ref(), &msg.right_group)?;
    let groups = Groups { left, right };
    GROUPS.save(deps.storage, &groups)?;

    // Register this contract as a slasher in left and right contracts
    // Slashing is not part of the TG4 spec - it's a separate interface
    // that these contracts must implement.
    let slash_msg = to_binary(&SlashMsg::AddSlasher {
        addr: env.contract.address.to_string(),
    })?;

    // add hooks to listen for all changes
    // TODO: what events to return here?
    let res = Response::new()
        .add_submessage(groups.left.add_hook(&env.contract.address)?)
        .add_submessage(groups.right.add_hook(&env.contract.address)?)
        .add_submessage(groups.left.encode_raw_msg(slash_msg.clone())?)
        .add_submessage(groups.right.encode_raw_msg(slash_msg)?);

    // Instantiate PoE function
    let poe_function = msg.function_type.to_poe_fn()?;

    // calculate initial state from current members on both sides
    initialize_members(deps, groups, &*poe_function, env.block.height)?;
    Ok(res)
}

fn verify_tg4_input(deps: Deps, addr: &str) -> Result<Tg4Contract, ContractError> {
    let contract = Tg4Contract(deps.api.addr_validate(addr)?);
    if !contract.is_tg4(&deps.querier) {
        return Err(ContractError::NotTg4(addr.into()));
    };
    Ok(contract)
}

const QUERY_LIMIT: Option<u32> = Some(30);

fn initialize_members(
    deps: DepsMut,
    groups: Groups,
    poe_function: &dyn PoEFunction,
    height: u64,
) -> Result<(), ContractError> {
    let mut total = 0u64;
    // we query all members of left group - for each non-None value, we check the value of right group and mix it.
    // Either as None means "not a member"
    let mut batch = groups.left.list_members(&deps.querier, None, QUERY_LIMIT)?;
    while !batch.is_empty() {
        let last = Some(batch.last().unwrap().addr.clone());
        // check it's weight in the other group, and calculate/save the mixed weight if in both
        for member in batch.into_iter() {
            let addr = deps.api.addr_validate(&member.addr)?;
            // note that this is a *raw query* and therefore quite cheap compared to a *smart query*
            // like calling `list_members` on the right side as well
            let other = groups.right.is_member(&deps.querier, &addr)?;
            if let Some(right) = other {
                let weight = poe_function.rewards(member.weight, right)?;
                total += weight;
                members().save(deps.storage, &addr, &weight, height)?;
            }
        }
        // and get the next page
        batch = groups.left.list_members(&deps.querier, last, QUERY_LIMIT)?;
    }
    TOTAL.save(deps.storage, &total)?;
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::MemberChangedHook(changes) => execute_member_changed(deps, env, info, changes),
        ExecuteMsg::AddHook { addr } => execute_add_hook(deps, info, addr),
        ExecuteMsg::RemoveHook { addr } => execute_remove_hook(deps, info, addr),
        ExecuteMsg::AddSlasher { addr } => execute_add_slasher(deps, info, addr),
        ExecuteMsg::RemoveSlasher { addr } => execute_remove_slasher(deps, info, addr),
        ExecuteMsg::Slash { addr, portion } => execute_slash(deps, env, info, addr, portion),
    }
}

pub fn execute_member_changed(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    changes: MemberChangedHookMsg,
) -> Result<Response, ContractError> {
    let mut res = Response::new()
        .add_attribute("action", "update_members")
        .add_attribute("changed", changes.diffs.len().to_string())
        .add_attribute("sender", &info.sender);

    let groups = GROUPS.load(deps.storage)?;

    // authorization check
    let diff = if info.sender == groups.left.addr() {
        let poe_function = POE_FUNCTION_TYPE.load(deps.storage)?.to_poe_fn()?;
        update_members(
            deps.branch(),
            env.block.height,
            groups.right,
            changes.diffs,
            &*poe_function,
        )
    } else if info.sender == groups.right.addr() {
        let poe_function = POE_FUNCTION_TYPE.load(deps.storage)?.to_poe_fn()?;
        update_members(
            deps.branch(),
            env.block.height,
            groups.left,
            changes.diffs,
            &*poe_function,
        )
    } else {
        Err(ContractError::Unauthorized(
            "Sender is neither left nor right side group contract".to_owned(),
        ))
    }?;

    // call all registered hooks
    res.messages = HOOKS.prepare_hooks(deps.storage, |h| {
        diff.clone().into_cosmos_msg(h).map(SubMsg::new)
    })?;
    Ok(res)
}

// the logic from execute_update_members extracted for easier re-usability
pub fn update_members(
    deps: DepsMut,
    height: u64,
    query_group: Tg4Contract,
    changes: Vec<MemberDiff>,
    poe_function: &dyn PoEFunction,
) -> Result<MemberChangedHookMsg, ContractError> {
    let mut total = TOTAL.load(deps.storage)?;
    let mut diffs: Vec<MemberDiff> = vec![];

    // add all new members and update total
    for change in changes {
        let member_addr = deps.api.addr_validate(&change.key)?;
        let new_weight = match change.new {
            Some(x) => match query_group.is_member(&deps.querier, &member_addr)? {
                Some(y) => Some(poe_function.rewards(x, y)?),
                None => None,
            },
            None => None,
        };
        let mems = members();

        // update the total with changes.
        // to calculate this, we need to load the old weight before saving the new weight
        let prev_weight = mems.may_load(deps.storage, &member_addr)?;
        total -= prev_weight.unwrap_or_default();
        total += new_weight.unwrap_or_default();

        // store the new value
        match new_weight {
            Some(weight) => mems.save(deps.storage, &member_addr, &weight, height)?,
            None => mems.remove(deps.storage, &member_addr, height)?,
        };

        // return the diff
        diffs.push(MemberDiff::new(member_addr, prev_weight, new_weight));
    }

    TOTAL.save(deps.storage, &total)?;
    Ok(MemberChangedHookMsg { diffs })
}

pub fn execute_add_hook(
    deps: DepsMut,
    info: MessageInfo,
    hook: String,
) -> Result<Response, ContractError> {
    // custom guard: only preauth
    PREAUTH_HOOKS.use_auth(deps.storage)?;

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
    // custom guard: only self-removal
    let hook_addr = deps.api.addr_validate(&hook)?;
    if info.sender != hook_addr {
        return Err(ContractError::Unauthorized(
            "Hook address must be same as sender".to_owned(),
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

pub fn execute_add_slasher(
    deps: DepsMut,
    info: MessageInfo,
    slasher: String,
) -> Result<Response, ContractError> {
    // custom guard: using a preauth
    PREAUTH_SLASHING.use_auth(deps.storage)?;

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
    // custom guard: self-removal only
    let slasher_addr = Addr::unchecked(&slasher);
    if info.sender != slasher_addr {
        return Err(ContractError::Unauthorized(
            "Slasher address and sender's must be the same".to_owned(),
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
    _env: Env,
    info: MessageInfo,
    addr: String,
    portion: Decimal,
) -> Result<Response, ContractError> {
    if !SLASHERS.is_slasher(deps.storage, &info.sender)? {
        return Err(ContractError::Unauthorized(
            "Sender is not in slashers list".to_owned(),
        ));
    }
    validate_portion(portion)?;
    let addr = deps.api.addr_validate(&addr)?;
    let groups = GROUPS.load(deps.storage)?;

    let slash_msg = to_binary(&SlashMsg::Slash {
        addr: addr.to_string(),
        portion,
    })?;

    // response
    let res = Response::new()
        .add_attribute("action", "slash")
        .add_attribute("addr", &addr)
        .add_attribute("sender", info.sender)
        .add_submessage(groups.left.encode_raw_msg(slash_msg.clone())?)
        .add_submessage(groups.right.encode_raw_msg(slash_msg)?);
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
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
        Groups {} => to_binary(&query_groups(deps)?),
        Hooks {} => {
            let hooks = HOOKS.list_hooks(deps.storage)?;
            to_binary(&HooksResponse { hooks })
        }
        Preauths {} => {
            let preauths_hooks = PREAUTH_HOOKS.get_auth(deps.storage)?;
            to_binary(&PreauthResponse { preauths_hooks })
        }
        RewardFunction {
            stake,
            engagement,
            poe_function,
        } => {
            let reward = query_reward_function(deps, stake.u64(), engagement.u64(), poe_function)
                .map_err(|err| StdError::generic_err(err.to_string()))?;
            to_binary(&RewardFunctionResponse { reward })
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
    Ok(TotalWeightResponse { weight })
}

fn query_groups(deps: Deps) -> StdResult<GroupsResponse> {
    let groups = GROUPS.load(deps.storage)?;
    Ok(GroupsResponse {
        left: groups.left.0.into(),
        right: groups.right.0.into(),
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
    let start = start_after.map(|m| Bound::exclusive((m.weight, m.addr).joined_key()));
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

pub fn query_reward_function(
    deps: Deps,
    stake: u64,
    engagement: u64,
    poe_function: Option<PoEFunctionType>,
) -> Result<u64, ContractError> {
    let poe_function = match poe_function {
        Some(poe_function_type) => poe_function_type,
        None => POE_FUNCTION_TYPE.load(deps.storage)?,
    }
    .to_poe_fn()?;

    poe_function.rewards(stake, engagement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::PoEFunctionType;
    use cosmwasm_std::{coins, Addr, BankMsg, Uint128};
    use cw_multi_test::{next_block, AppBuilder, BasicApp, Contract, ContractWrapper, Executor};
    use tg_bindings::TgradeMsg;

    const STAKE_DENOM: &str = "utgd";
    const OWNER: &str = "owner";
    const VOTER1: &str = "voter0001";
    const VOTER2: &str = "voter0002";
    const VOTER3: &str = "voter0003";
    const VOTER4: &str = "voter0004";
    const VOTER5: &str = "voter0005";
    const RESERVE: &str = "reserve";
    const SLASHER: &str = "slasher";

    fn member<T: Into<String>>(addr: T, weight: u64) -> Member {
        Member {
            addr: addr.into(),
            weight,
        }
    }

    pub fn contract_mixer() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    pub fn contract_group() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new(
            tg4_engagement::contract::execute,
            tg4_engagement::contract::instantiate,
            tg4_engagement::contract::query,
        );
        Box::new(contract)
    }

    pub fn contract_staking() -> Box<dyn Contract<TgradeMsg>> {
        let contract = ContractWrapper::new(
            tg4_stake::contract::execute,
            tg4_stake::contract::instantiate,
            tg4_stake::contract::query,
        );
        Box::new(contract)
    }

    // uploads code and returns address of group contract
    fn instantiate_group(app: &mut BasicApp<TgradeMsg>, members: Vec<Member>) -> Addr {
        let admin = Some(OWNER.into());
        let group_id = app.store_code(contract_group());
        let msg = tg4_engagement::msg::InstantiateMsg {
            admin: admin.clone(),
            members,
            preauths_hooks: 1,
            preauths_slashing: 1,
            halflife: None,
            denom: STAKE_DENOM.to_owned(),
        };
        app.instantiate_contract(group_id, Addr::unchecked(OWNER), &msg, &[], "group", admin)
            .unwrap()
    }

    // uploads code and returns address of group contract
    fn instantiate_staking(app: &mut BasicApp<TgradeMsg>, stakers: Vec<Member>) -> Addr {
        let admin = Some(OWNER.into());
        let group_id = app.store_code(contract_staking());
        let msg = tg4_stake::msg::InstantiateMsg {
            denom: STAKE_DENOM.to_owned(),
            tokens_per_weight: Uint128::new(1),
            min_bond: Uint128::new(100),
            unbonding_period: 3600,
            admin: admin.clone(),
            preauths_hooks: 1,
            preauths_slashing: 1,
            auto_return_limit: 0,
        };
        let contract = app
            .instantiate_contract(
                group_id,
                Addr::unchecked(OWNER),
                &msg,
                &[],
                "staking",
                admin,
            )
            .unwrap();

        // stake any needed tokens
        for staker in stakers {
            // give them a balance
            let balance = coins(staker.weight as u128, STAKE_DENOM);
            let caller = Addr::unchecked(staker.addr);

            // they stake to the contract
            let msg = tg4_stake::msg::ExecuteMsg::Bond {};
            app.execute_contract(caller.clone(), contract.clone(), &msg, &balance)
                .unwrap();
        }

        contract
    }

    fn instantiate_mixer(app: &mut BasicApp<TgradeMsg>, left: &Addr, right: &Addr) -> Addr {
        let flex_id = app.store_code(contract_mixer());
        let msg = crate::msg::InstantiateMsg {
            left_group: left.to_string(),
            right_group: right.to_string(),
            preauths_hooks: 0,
            preauths_slashing: 1,
            function_type: PoEFunctionType::GeometricMean {},
        };
        app.instantiate_contract(flex_id, Addr::unchecked(OWNER), &msg, &[], "mixer", None)
            .unwrap()
    }

    /// this will set up all 3 contracts contracts, instantiating the group with
    /// all the constant members, setting the staking contract with a definable set of stakers,
    /// and connectioning them all to the mixer.
    ///
    /// Returns (mixer address, group address, staking address).
    fn setup_test_case(app: &mut BasicApp<TgradeMsg>, stakers: Vec<Member>) -> (Addr, Addr, Addr) {
        // 1. Instantiate group contract with members (and OWNER as admin)
        let members = vec![
            member(OWNER, 0),
            member(VOTER1, 100),
            member(VOTER2, 200),
            member(VOTER3, 300),
            member(VOTER4, 400),
            member(VOTER5, 500),
        ];
        let group_addr = instantiate_group(app, members);
        app.update_block(next_block);

        // 2. set up staking contract
        let stake_addr = instantiate_staking(app, stakers);
        app.update_block(next_block);

        // 3. Set up mixer backed by these two groups
        let mixer_addr = instantiate_mixer(app, &group_addr, &stake_addr);
        app.update_block(next_block);

        (mixer_addr, group_addr, stake_addr)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_membership(
        app: &BasicApp<TgradeMsg>,
        mixer_addr: &Addr,
        owner: Option<u64>,
        voter1: Option<u64>,
        voter2: Option<u64>,
        voter3: Option<u64>,
        voter4: Option<u64>,
        voter5: Option<u64>,
    ) {
        let weight = |addr: &str| -> Option<u64> {
            let o: MemberResponse = app
                .wrap()
                .query_wasm_smart(
                    mixer_addr,
                    &QueryMsg::Member {
                        addr: addr.into(),
                        at_height: None,
                    },
                )
                .unwrap();
            o.weight
        };

        assert_eq!(weight(OWNER), owner);
        assert_eq!(weight(VOTER1), voter1);
        assert_eq!(weight(VOTER2), voter2);
        assert_eq!(weight(VOTER3), voter3);
        assert_eq!(weight(VOTER4), voter4);
        assert_eq!(weight(VOTER5), voter5);
    }

    #[test]
    fn basic_init() {
        let stakers = vec![
            member(OWNER, 88888888888), // 0 weight -> 0 mixed
            member(VOTER1, 10000),      // 10000 stake, 100 weight -> 1000 mixed
            member(VOTER3, 7500),       // 7500 stake, 300 weight -> 1500 mixed
        ];

        let mut app = AppBuilder::new_custom().build(|router, _, storage| {
            for staker in &stakers {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(&staker.addr),
                        coins(staker.weight as u128, STAKE_DENOM),
                    )
                    .unwrap();
            }
        });

        let (mixer_addr, _, _) = setup_test_case(&mut app, stakers);

        // query the membership values
        check_membership(
            &app,
            &mixer_addr,
            Some(0),
            Some(1000),
            None,
            Some(1500),
            None,
            None,
        );
    }

    #[test]
    fn update_with_upstream_change() {
        let stakers = vec![
            member(VOTER1, 10000), // 10000 stake, 100 weight -> 1000 mixed
            member(VOTER3, 7500),  // 7500 stake, 300 weight -> 1500 mixed
            member(VOTER5, 50),    // below stake threshold -> None
        ];

        let mut app = AppBuilder::new_custom().build(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &Addr::unchecked(RESERVE), coins(450, STAKE_DENOM))
                .unwrap();

            for staker in &stakers {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(&staker.addr),
                        coins(staker.weight as u128, STAKE_DENOM),
                    )
                    .unwrap();
            }
        });

        let (mixer_addr, group_addr, staker_addr) = setup_test_case(&mut app, stakers);

        // query the membership values
        check_membership(
            &app,
            &mixer_addr,
            None,
            Some(1000),
            None,
            Some(1500),
            None,
            None,
        );

        // stake some tokens, update the values
        let balance = coins(450, STAKE_DENOM);
        app.execute(
            Addr::unchecked(RESERVE),
            BankMsg::Send {
                to_address: VOTER5.to_owned(),
                amount: balance.clone(),
            }
            .into(),
        )
        .unwrap();
        let msg = tg4_stake::msg::ExecuteMsg::Bond {};
        app.execute_contract(Addr::unchecked(VOTER5), staker_addr, &msg, &balance)
            .unwrap();

        // check updated weights
        check_membership(
            &app,
            &mixer_addr,
            None,
            Some(1000),
            None,
            Some(1500),
            None,
            // sqrt(500 * 500) = 500
            Some(500),
        );

        // add, remove, and adjust member
        // voter1 => None, voter2 => 300 (still mixed to None), voter3 => 1200 (mixed = 3000)
        let msg = tg4_engagement::msg::ExecuteMsg::UpdateMembers {
            remove: vec![VOTER1.into()],
            add: vec![
                Member {
                    addr: VOTER2.into(),
                    weight: 300,
                },
                Member {
                    addr: VOTER3.into(),
                    weight: 1200,
                },
            ],
        };
        app.execute_contract(Addr::unchecked(OWNER), group_addr, &msg, &[])
            .unwrap();

        // check updated weights
        check_membership(
            &app,
            &mixer_addr,
            None,
            // Removed -> None
            None,
            // Changed, but other None -> None
            None,
            // Changed, other Some -> sqrt(1200 * 7500) = sqrt(9000000)
            Some(3000),
            None,
            Some(500),
        );
    }

    #[test]
    fn hook_on_engagement() {
        let stakers = vec![
            member(VOTER1, 10000), // 10000 stake, 100 weight -> 1000 mixed
            member(VOTER3, 7500),  // 7500 stake, 300 weight -> 1500 mixed
            member(VOTER5, 50),    // below stake threshold -> None
        ];

        let mut app = AppBuilder::new_custom().build(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &Addr::unchecked(RESERVE), coins(450, STAKE_DENOM))
                .unwrap();

            for staker in &stakers {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(&staker.addr),
                        coins(staker.weight as u128, STAKE_DENOM),
                    )
                    .unwrap();
            }
        });

        let (mixer_addr, group_addr, _) = setup_test_case(&mut app, stakers);

        // query the membership values
        check_membership(
            &app,
            &mixer_addr,
            None,
            Some(1000),
            None,
            Some(1500),
            None,
            None,
        );

        // Update members on group, should update mixer as well
        app.execute_contract(
            Addr::unchecked(OWNER),
            group_addr,
            &tg4_engagement::msg::ExecuteMsg::UpdateMembers {
                add: vec![
                    Member {
                        addr: VOTER2.to_owned(),
                        weight: 400,
                    },
                    Member {
                        addr: VOTER1.to_owned(),
                        weight: 8000,
                    },
                ],
                remove: vec![VOTER3.to_owned()],
            },
            &[],
        )
        .unwrap();

        // query the membership values
        check_membership(
            &app,
            &mixer_addr,
            None,
            Some(8944), // (8k, 100) mixed
            None,
            None,
            None,
            None,
        );
    }

    #[test]
    fn slashing_works() {
        let stakers = vec![
            member(VOTER1, 10000), // 10000 stake, 100 weight -> 1000 mixed
            member(VOTER2, 20000), // 20000 stake, 200 weight -> 2000 mixed
            member(VOTER3, 7500),  // 7500 stake, 300 weight -> 1500 mixed
        ];

        let mut app = AppBuilder::new_custom().build(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &Addr::unchecked(RESERVE), coins(450, STAKE_DENOM))
                .unwrap();

            for staker in &stakers {
                router
                    .bank
                    .init_balance(
                        storage,
                        &Addr::unchecked(&staker.addr),
                        coins(staker.weight as u128, STAKE_DENOM),
                    )
                    .unwrap();
            }
        });

        let (mixer_addr, _, _) = setup_test_case(&mut app, stakers);

        // Register our slasher using the preauth.
        app.execute_contract(
            Addr::unchecked(SLASHER),
            mixer_addr.clone(),
            &ExecuteMsg::AddSlasher {
                addr: SLASHER.to_string(),
            },
            &[],
        )
        .unwrap();

        // Slash VOTER1 by 40 percent
        app.execute_contract(
            Addr::unchecked(SLASHER),
            mixer_addr.clone(),
            &ExecuteMsg::Slash {
                addr: VOTER1.to_string(),
                portion: Decimal::percent(40),
            },
            &[],
        )
        .unwrap();
        // We don't slash VOTER2
        // Slash VOTER3 by 20 percent
        app.execute_contract(
            Addr::unchecked(SLASHER),
            mixer_addr.clone(),
            &ExecuteMsg::Slash {
                addr: VOTER3.to_string(),
                portion: Decimal::percent(20),
            },
            &[],
        )
        .unwrap();

        check_membership(
            &app,
            &mixer_addr,
            None,
            Some(600),
            Some(2000),
            Some(1200),
            None,
            None,
        );
    }

    // TODO: multi-test to init!
}
