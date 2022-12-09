use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use cosmwasm_std::{Addr, Decimal, StdError, StdResult, Storage};
use cw_storage_plus::Item;

// store all slasher addresses in one item.
pub struct Slashers<'a>(Item<'a, Vec<Addr>>);

impl<'a> Slashers<'a> {
    pub const fn new(storage_key: &'a str) -> Self {
        Slashers(Item::new(storage_key))
    }

    pub fn instantiate(&self, storage: &mut dyn Storage) -> StdResult<()> {
        self.0.save(storage, &vec![])
    }

    pub fn add_slasher(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), SlasherError> {
        let mut slashers = self.0.load(storage)?;
        if !slashers.iter().any(|h| h == &addr) {
            slashers.push(addr);
        } else {
            return Err(SlasherError::SlasherAlreadyRegistered(addr.to_string()));
        }
        Ok(self.0.save(storage, &slashers)?)
    }

    pub fn remove_slasher(
        &self,
        storage: &mut dyn Storage,
        addr: Addr,
    ) -> Result<(), SlasherError> {
        let mut slashers = self.0.load(storage)?;
        if let Some(p) = slashers.iter().position(|x| x == &addr) {
            slashers.remove(p);
        } else {
            return Err(SlasherError::SlasherNotRegistered(addr.to_string()));
        }
        Ok(self.0.save(storage, &slashers)?)
    }

    pub fn is_slasher(&self, storage: &dyn Storage, addr: &Addr) -> StdResult<bool> {
        let slashers = self.0.load(storage)?;
        Ok(slashers.contains(addr))
    }

    pub fn list_slashers(&self, storage: &dyn Storage) -> StdResult<Vec<String>> {
        let slashers = self.0.load(storage)?;
        Ok(slashers.into_iter().map(String::from).collect())
    }
}

/// A common (sort of) interface for adding/removing slashers and slashing.
/// This type exists so that contracts have an easy way of serializing these messages
/// and using something like `encode_raw_msg`.
///
/// # Examples
///
/// ```
/// use tg4::Tg4Contract;
/// use tg_utils::SlashMsg;
/// use cosmwasm_std::{to_binary, Addr, Response};
///
/// let tg_contract = Tg4Contract::new(Addr::unchecked("some_contract"));
///
/// let slash_msg = to_binary(&SlashMsg::AddSlasher {
///     addr: "some_other_contract".to_string(),
/// }).unwrap();
///
/// let res = Response::new()
///     .add_submessage(tg_contract.encode_raw_msg(slash_msg).unwrap());
/// ```
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SlashMsg {
    /// Adds slasher for contract if there are enough `slasher_preauths` left
    AddSlasher { addr: String },
    /// Removes slasher for contract
    RemoveSlasher { addr: String },
    /// Slash engagement points from address
    Slash { addr: String, portion: Decimal },
}

#[derive(Error, Debug, PartialEq)]
pub enum SlasherError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Given address already registered as a hook")]
    SlasherAlreadyRegistered(String),

    #[error("Given address not registered as a hook")]
    SlasherNotRegistered(String),

    #[error("Invalid portion {0}, must be (0, 1]")]
    InvalidPortion(Decimal),
}

pub fn validate_portion(portion: Decimal) -> Result<(), SlasherError> {
    match portion.is_zero() || portion > Decimal::one() {
        true => Err(SlasherError::InvalidPortion(portion)),
        false => Ok(()),
    }
}
