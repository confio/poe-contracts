use serde::{Deserialize, Serialize};

use crate::msg::PoEFunctionType;
use cw_storage_plus::Item;
use tg4::Tg4Contract;

pub const POE_FUNCTION_TYPE: Item<PoEFunctionType> = Item::new("poe-function-type");

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Groups {
    pub left: Tg4Contract,
    pub right: Tg4Contract,
}

pub const GROUPS: Item<Groups> = Item::new("groups");
