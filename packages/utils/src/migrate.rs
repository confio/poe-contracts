use cosmwasm_std::{StdError, StdResult, Storage};
use cw2::{get_contract_version, set_contract_version};
use semver::Version;

pub fn ensure_from_older_version(
    storage: &mut dyn Storage,
    name: &str,
    new_version: &str,
) -> StdResult<()> {
    let version: Version = new_version.parse().map_err(from_semver)?;
    let stored = get_contract_version(storage)?;
    let storage_version: Version = stored.version.parse().map_err(from_semver)?;

    if name != stored.contract {
        let msg = format!("Cannot migrate from {} to {}", stored.contract, name);
        return Err(StdError::generic_err(msg));
    }

    if storage_version > version {
        let msg = format!(
            "Cannot migrate from newer version ({}) to older ({})",
            stored.version, new_version
        );
        return Err(StdError::generic_err(msg));
    }
    if storage_version < version {
        // we don't need to save anything if migrating from the same version
        set_contract_version(storage, name, new_version)?;
    }

    Ok(())
}

fn from_semver(err: semver::Error) -> StdError {
    StdError::generic_err(format!("Semver: {}", err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::MockStorage;

    #[test]
    fn accepts_identical_version() {
        let mut storage = MockStorage::new();
        set_contract_version(&mut storage, "demo", "0.1.2").unwrap();
        // ensure this matches
        ensure_from_older_version(&mut storage, "demo", "0.1.2").unwrap();
    }

    #[test]
    fn accepts_and_updates_on_newer_version() {
        let mut storage = MockStorage::new();
        set_contract_version(&mut storage, "demo", "0.4.0").unwrap();
        // ensure this matches
        ensure_from_older_version(&mut storage, "demo", "0.4.2").unwrap();

        // check the version is updated
        let stored = get_contract_version(&storage).unwrap();
        assert_eq!(stored.contract, "demo".to_string());
        assert_eq!(stored.version, "0.4.2".to_string());
    }

    #[test]
    fn errors_on_name_mismatch() {
        let mut storage = MockStorage::new();
        set_contract_version(&mut storage, "demo", "0.1.2").unwrap();
        // ensure this matches
        let err = ensure_from_older_version(&mut storage, "cw20-base", "0.1.2")
            .unwrap_err();
        assert!(err.to_string().contains("cw20-base"), "{}", err);
        assert!(err.to_string().contains("demo"), "{}", err);
    }

    #[test]
    fn errors_on_older_version() {
        let mut storage = MockStorage::new();
        set_contract_version(&mut storage, "demo", "0.10.2").unwrap();
        // ensure this matches
        let err =
            ensure_from_older_version(&mut storage, "demo", "0.9.7").unwrap_err();
        assert!(err.to_string().contains("0.10.2"), "{}", err);
        assert!(err.to_string().contains("0.9.7"), "{}", err);
    }
}
