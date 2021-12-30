use thiserror::Error;

use cosmwasm_std::{StdError, Storage};
use cw_storage_plus::Item;

#[derive(Error, Debug, PartialEq)]
pub enum PreauthError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("No preauthorization available to add hook")]
    NoPreauth {},
}

// store all hook addresses in one item. We cannot have many of them before the contract becomes unusable anyway.
pub struct Preauth<'a>(Item<'a, u64>);

impl<'a> Preauth<'a> {
    pub const fn new(preauth_key: &'a str) -> Self {
        Preauth(Item::new(preauth_key))
    }

    pub fn set_auth(&self, storage: &mut dyn Storage, count: u64) -> Result<(), StdError> {
        self.0.save(storage, &count)
    }

    pub fn get_auth(&self, storage: &dyn Storage) -> Result<u64, StdError> {
        Ok(self.0.may_load(storage)?.unwrap_or_default())
    }

    pub fn add_auth(&self, storage: &mut dyn Storage) -> Result<(), StdError> {
        let count = self.get_auth(storage)?;
        self.set_auth(storage, count + 1)
    }

    pub fn use_auth(&self, storage: &mut dyn Storage) -> Result<(), PreauthError> {
        let count = self.get_auth(storage)?;
        let count = count.checked_sub(1).ok_or(PreauthError::NoPreauth {})?;
        Ok(self.set_auth(storage, count)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::testing::MockStorage;

    const PREAUTH: Preauth = Preauth::new("preauth");

    #[test]
    fn count_preauth() {
        let mut storage = MockStorage::new();

        // start and cannot consume
        assert_eq!(PREAUTH.get_auth(&storage).unwrap(), 0);
        let err = PREAUTH.use_auth(&mut storage).unwrap_err();
        assert_eq!(err, PreauthError::NoPreauth {});

        // add one and use it (only once)
        PREAUTH.add_auth(&mut storage).unwrap();
        assert_eq!(PREAUTH.get_auth(&storage).unwrap(), 1);
        PREAUTH.use_auth(&mut storage).unwrap();
        assert_eq!(PREAUTH.get_auth(&storage).unwrap(), 0);
        let err = PREAUTH.use_auth(&mut storage).unwrap_err();
        assert_eq!(err, PreauthError::NoPreauth {});

        // set to a higher value
        PREAUTH.set_auth(&mut storage, 27).unwrap();
        assert_eq!(PREAUTH.get_auth(&storage).unwrap(), 27);
        PREAUTH.use_auth(&mut storage).unwrap();
        assert_eq!(PREAUTH.get_auth(&storage).unwrap(), 26);
    }
}
