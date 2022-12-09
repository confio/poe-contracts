use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use cosmwasm_std::{Addr, StdError, StdResult, Storage};
use cw_storage_plus::Item;
use tg_bindings::TgradeMsg;

type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// this is copied from cw4
// TODO: pull into cw_utils as common dep
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct HooksResponse {
    pub hooks: Vec<String>,
}

#[derive(Error, Debug, PartialEq)]
pub enum HookError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Given address already registered as a hook")]
    HookAlreadyRegistered {},

    #[error("Given address not registered as a hook")]
    HookNotRegistered {},

    #[error("You can only unregister yourself from a hook, not other contracts")]
    OnlyRemoveSelf {},
}

// store all hook addresses in one item. We cannot have many of them before the contract becomes unusable anyway.
pub struct Hooks<'a>(Item<'a, Vec<Addr>>);

impl<'a> Hooks<'a> {
    pub const fn new(hook_key: &'a str) -> Self {
        Hooks(Item::new(hook_key))
    }

    pub fn add_hook(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), HookError> {
        let mut hooks = self.0.may_load(storage)?.unwrap_or_default();
        if !hooks.iter().any(|h| h == &addr) {
            hooks.push(addr);
        } else {
            return Err(HookError::HookAlreadyRegistered {});
        }
        Ok(self.0.save(storage, &hooks)?)
    }

    pub fn remove_hook(&self, storage: &mut dyn Storage, addr: Addr) -> Result<(), HookError> {
        let mut hooks = self.0.load(storage)?;
        if let Some(p) = hooks.iter().position(|x| x == &addr) {
            hooks.remove(p);
        } else {
            return Err(HookError::HookNotRegistered {});
        }
        Ok(self.0.save(storage, &hooks)?)
    }

    pub fn list_hooks(&self, storage: &dyn Storage) -> StdResult<Vec<String>> {
        let hooks = self.0.may_load(storage)?.unwrap_or_default();
        Ok(hooks.into_iter().map(String::from).collect())
    }

    pub fn prepare_hooks<F: Fn(Addr) -> StdResult<SubMsg>>(
        &self,
        storage: &dyn Storage,
        prep: F,
    ) -> StdResult<Vec<SubMsg>> {
        self.0
            .may_load(storage)?
            .unwrap_or_default()
            .into_iter()
            .map(prep)
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{coins, BankMsg, CosmosMsg, Deps};

    const HOOKS: Hooks = Hooks::new("hooks");

    fn assert_count(deps: Deps, expected: usize) {
        let hooks = HOOKS.list_hooks(deps.storage).unwrap();
        assert_eq!(hooks.len(), expected);
    }

    #[test]
    fn add_and_remove_hooks() {
        let mut deps = mock_dependencies();
        assert_count(deps.as_ref(), 0);

        // add a new hook
        let first = Addr::unchecked("first");
        HOOKS
            .add_hook(deps.as_mut().storage, first.clone())
            .unwrap();
        assert_count(deps.as_ref(), 1);

        // cannot add twice
        let err = HOOKS
            .add_hook(deps.as_mut().storage, first.clone())
            .unwrap_err();
        assert_eq!(err, HookError::HookAlreadyRegistered {});
        assert_count(deps.as_ref(), 1);

        // add a different hook
        let bar = Addr::unchecked("bar");
        HOOKS.add_hook(deps.as_mut().storage, bar).unwrap();
        assert_count(deps.as_ref(), 2);

        // cannot remove a non-registered hook
        let boom = Addr::unchecked("boom");
        let err = HOOKS.remove_hook(deps.as_mut().storage, boom).unwrap_err();
        assert_eq!(err, HookError::HookNotRegistered {});
        assert_count(deps.as_ref(), 2);

        // can remove one of the existing hooks
        HOOKS.remove_hook(deps.as_mut().storage, first).unwrap();
        assert_count(deps.as_ref(), 1);
    }

    #[test]
    fn prepare_hook() {
        let payout = |addr: Addr| {
            Ok(SubMsg::new(BankMsg::Send {
                to_address: addr.into(),
                amount: coins(12345, "bonus"),
            }))
        };
        let mut deps = mock_dependencies();
        let storage = deps.as_mut().storage;

        HOOKS.add_hook(storage, Addr::unchecked("some")).unwrap();
        HOOKS.add_hook(storage, Addr::unchecked("one")).unwrap();

        let mut msgs = HOOKS.prepare_hooks(storage, payout).unwrap();
        assert_eq!(msgs.len(), 2);
        // get the last message
        match msgs.pop().unwrap().msg {
            CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
                assert_eq!(to_address.as_str(), "one");
                assert_eq!(amount, coins(12345, "bonus"));
            }
            _ => panic!("bad message"),
        }
    }
}
