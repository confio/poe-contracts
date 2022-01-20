use cosmwasm_std::{StdError, StdResult, Storage};
use cw2::{get_contract_version, set_contract_version};
use semver::Version;

pub fn ensure_from_older_version(storage: &dyn Storage, name: &str, new_version: &str) -> StdResult<()> {
    let version: Version = new_version.parse()?;
    let stored = get_contract_version(deps.storage)?;
    let storage_version: Version = stored.version.parse()?;

    if name != stored.contract {
        let msg = format!("Cannot migrate from {} to {}", stored.contract, name);
        return Err(StdError::generic_err(msg));
    }

    if storage_version > version {
        let msg = format!("Cannot migrate from newer version ({}) to older ({})", stored.version, new_version);
        return Err(StdError::generic_err(msg));
    } else if storage_version < version {
        // we don't need to save anything if migrating from the same version
        set_contract_version(deps.storage, name, new_version)?;
    }

    Ok(())
}
