use cosmwasm_std::{Deps, DepsMut, Order, StdResult};

use tg4::{Member, MemberChangedHookMsg, MemberDiff, MemberInfo};
use tg_bindings::TgradeQuery;
use tg_utils::members;

use crate::error::ContractError;
use crate::msg::MigrateMsg;
use crate::state::{Halflife, HALFLIFE};

pub(crate) fn migrate_config(
    deps: DepsMut<TgradeQuery>,
    msg: MigrateMsg,
) -> Result<(), ContractError> {
    if let Some(duration) = msg.halflife {
        // Update half life's duration
        // Zero duration means no / remove half life
        HALFLIFE.update(deps.storage, |hf| -> StdResult<_> {
            Ok(Halflife {
                halflife: if duration.seconds() > 0 {
                    Some(duration)
                } else {
                    None
                },
                last_applied: hf.last_applied,
            })
        })?;
    }
    Ok(())
}

// Helper to repair the half life bug (#203)
pub fn generate_pending_member_updates(
    deps: Deps<TgradeQuery>,
) -> Result<MemberChangedHookMsg, ContractError> {
    // Iterate over all the members, and send an update member message to each of the registered hooks
    let members_to_update: Vec<_> = members()
        .range(deps.storage, None, None, Order::Ascending)
        .filter_map(|item| {
            (move || -> StdResult<Option<_>> {
                let (
                    addr,
                    MemberInfo {
                        points,
                        start_height,
                    },
                ) = item?;
                if points <= 1 {
                    return Ok(None);
                }
                Ok(Some(Member {
                    addr: addr.into(),
                    points,
                    start_height,
                }))
            })()
            .transpose()
        })
        .collect::<StdResult<_>>()?;

    let mut diffs: Vec<MemberDiff> = vec![];
    for member in members_to_update {
        diffs.push(MemberDiff::new(
            member.addr.clone(),
            Some(member.points), // FIXME: These are not the old points on the remote side
            Some(member.points),
        ));
    }

    Ok(MemberChangedHookMsg { diffs })
}
