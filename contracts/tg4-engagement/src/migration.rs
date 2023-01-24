use crate::error::ContractError;
use crate::msg::MigrateMsg;
use crate::state::{Halflife, HALFLIFE};
use cosmwasm_std::{DepsMut, StdResult};
use tg_bindings::TgradeQuery;

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
