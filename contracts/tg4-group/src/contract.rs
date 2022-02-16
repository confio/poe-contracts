#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdResult,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use cw_utils::maybe_addr;
use tg4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalPointsResponse,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{ADMIN, MEMBERS, TOTAL};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:tg4-group";
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
    create(deps, msg.admin, msg.members, env.block.height)?;
    Ok(Response::default())
}

// create is the instantiation logic with set_contract_version removed so it can more
// easily be imported in other contracts
pub fn create(
    mut deps: DepsMut,
    admin: Option<String>,
    members: Vec<Member>,
    height: u64,
) -> Result<(), ContractError> {
    let admin_addr = admin
        .map(|admin| deps.api.addr_validate(&admin))
        .transpose()?;
    ADMIN.set(deps.branch(), admin_addr)?;

    let mut total = 0u64;
    for member in members.into_iter() {
        total += member.points;
        let member_addr = deps.api.addr_validate(&member.addr)?;
        MEMBERS.save(deps.storage, &member_addr, &member.points, height)?;
    }
    TOTAL.save(deps.storage, &total)?;

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
    let api = deps.api;
    match msg {
        ExecuteMsg::UpdateAdmin { admin } => Ok(ADMIN.execute_update_admin(
            deps,
            info,
            admin.map(|admin| api.addr_validate(&admin)).transpose()?,
        )?),
        ExecuteMsg::UpdateMembers { add, remove } => {
            execute_update_members(deps, env, info, add, remove)
        }
    }
}

pub fn execute_update_members(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Vec<Member>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let attributes = vec![
        attr("action", "update_members"),
        attr("added", add.len().to_string()),
        attr("removed", remove.len().to_string()),
        attr("sender", &info.sender),
    ];

    // make the local update
    update_members(deps.branch(), env.block.height, info.sender, add, remove)?;
    let res = Response::new().add_attributes(attributes);
    Ok(res)
}

// the logic from execute_update_members extracted for easier import
pub fn update_members(
    deps: DepsMut,
    height: u64,
    sender: Addr,
    to_add: Vec<Member>,
    to_remove: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    ADMIN.assert_admin(deps.as_ref(), &sender)?;

    let mut total = TOTAL.load(deps.storage)?;
    let mut diffs: Vec<MemberDiff> = vec![];

    // add all new members and update total
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add.addr)?;
        MEMBERS.update(deps.storage, &add_addr, height, |old| -> StdResult<_> {
            total -= old.unwrap_or_default();
            total += add.points;
            diffs.push(MemberDiff::new(add.addr, old, Some(add.points)));
            Ok(add.points)
        })?;
    }

    for remove in to_remove.into_iter() {
        let remove_addr = deps.api.addr_validate(&remove)?;
        let old = MEMBERS.may_load(deps.storage, &remove_addr)?;
        // Only process this if they were actually in the list before
        if let Some(points) = old {
            diffs.push(MemberDiff::new(remove, Some(points), None));
            total -= points;
            MEMBERS.remove(deps.storage, &remove_addr, height)?;
        }
    }

    TOTAL.save(deps.storage, &total)?;
    Ok(MemberChangedHookMsg { diffs })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Member {
            addr,
            at_height: height,
        } => to_binary(&query_member(deps, addr, height)?),
        QueryMsg::ListMembers { start_after, limit } => {
            to_binary(&list_members(deps, start_after, limit)?)
        }
        QueryMsg::TotalPoints {} => to_binary(&query_total_points(deps)?),
        QueryMsg::Admin {} => to_binary(&ADMIN.query_admin(deps)?),
    }
}

fn query_total_points(deps: Deps) -> StdResult<TotalPointsResponse> {
    let points = TOTAL.load(deps.storage)?;
    Ok(TotalPointsResponse { points })
}

fn query_member(deps: Deps, addr: String, height: Option<u64>) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    let points = match height {
        Some(h) => MEMBERS.may_load_at_height(deps.storage, &addr, h),
        None => MEMBERS.may_load(deps.storage, &addr),
    }?;
    Ok(MemberResponse { points })
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

    let members = MEMBERS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, points)| Member {
                addr: addr.into(),
                points,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(MemberListResponse { members })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{from_slice, Api, OwnedDeps, Querier, Storage};
    use cw_controllers::AdminError;
    use tg4::{member_key, TOTAL_KEY};

    const INIT_ADMIN: &str = "juan";
    const USER1: &str = "somebody";
    const USER2: &str = "else";
    const USER3: &str = "funny";

    fn do_instantiate(deps: DepsMut) {
        let msg = InstantiateMsg {
            admin: Some(INIT_ADMIN.into()),
            members: vec![
                Member {
                    addr: USER1.into(),
                    points: 11,
                },
                Member {
                    addr: USER2.into(),
                    points: 6,
                },
            ],
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

        let res = query_total_points(deps.as_ref()).unwrap();
        assert_eq!(17, res.points);
    }

    #[test]
    fn try_member_queries() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        let member1 = query_member(deps.as_ref(), USER1.into(), None).unwrap();
        assert_eq!(member1.points, Some(11));

        let member2 = query_member(deps.as_ref(), USER2.into(), None).unwrap();
        assert_eq!(member2.points, Some(6));

        let member3 = query_member(deps.as_ref(), USER3.into(), None).unwrap();
        assert_eq!(member3.points, None);

        let members = list_members(deps.as_ref(), None, None).unwrap();
        assert_eq!(members.members.len(), 2);
        // TODO: assert the set is proper
    }

    fn assert_users<S: Storage, A: Api, Q: Querier>(
        deps: &OwnedDeps<S, A, Q>,
        user1_points: Option<u64>,
        user2_points: Option<u64>,
        user3_points: Option<u64>,
        height: Option<u64>,
    ) {
        let member1 = query_member(deps.as_ref(), USER1.into(), height).unwrap();
        assert_eq!(member1.points, user1_points);

        let member2 = query_member(deps.as_ref(), USER2.into(), height).unwrap();
        assert_eq!(member2.points, user2_points);

        let member3 = query_member(deps.as_ref(), USER3.into(), height).unwrap();
        assert_eq!(member3.points, user3_points);

        // this is only valid if we are not doing a historical query
        if height.is_none() {
            // compute expected metrics
            let points = vec![user1_points, user2_points, user3_points];
            let sum: u64 = points.iter().map(|x| x.unwrap_or_default()).sum();
            let count = points.iter().filter(|x| x.is_some()).count();

            // TODO: more detailed compare?
            let members = list_members(deps.as_ref(), None, None).unwrap();
            assert_eq!(count, members.members.len());

            let total = query_total_points(deps.as_ref()).unwrap();
            assert_eq!(sum, total.points); // 17 - 11 + 15 = 21
        }
    }

    #[test]
    fn add_new_remove_old_member() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut());

        // add a new one and remove existing one
        let add = vec![Member {
            addr: USER3.into(),
            points: 15,
        }];
        let remove = vec![USER1.into()];

        // non-admin cannot update
        let height = mock_env().block.height;
        let err = update_members(
            deps.as_mut(),
            height + 5,
            Addr::unchecked(USER1),
            add.clone(),
            remove.clone(),
        )
        .unwrap_err();
        assert_eq!(err, AdminError::NotAdmin {}.into());

        // Test the values from instantiate
        assert_users(&deps, Some(11), Some(6), None, None);
        // Note all values were set at height, the beginning of that block was all None
        assert_users(&deps, None, None, None, Some(height));
        // This will get us the values at the start of the block after instantiate (expected initial values)
        assert_users(&deps, Some(11), Some(6), None, Some(height + 1));

        // admin updates properly
        update_members(
            deps.as_mut(),
            height + 10,
            Addr::unchecked(INIT_ADMIN),
            add,
            remove,
        )
        .unwrap();

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
            points: 4,
        }];
        let remove = vec![USER3.into()];

        // admin updates properly
        let height = mock_env().block.height;
        update_members(
            deps.as_mut(),
            height,
            Addr::unchecked(INIT_ADMIN),
            add,
            remove,
        )
        .unwrap();
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
                points: 20,
            },
            Member {
                addr: USER3.into(),
                points: 5,
            },
        ];
        let remove = vec![USER1.into()];

        // admin updates properly
        let height = mock_env().block.height;
        update_members(
            deps.as_mut(),
            height,
            Addr::unchecked(INIT_ADMIN),
            add,
            remove,
        )
        .unwrap();
        assert_users(&deps, None, Some(6), Some(5), None);
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
}
