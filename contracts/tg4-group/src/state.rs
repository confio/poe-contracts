use cosmwasm_std::Addr;
use cw_controllers::{Admin, Hooks};
use cw_storage_plus::{Item, SnapshotMap, Strategy};
use tg4::{MemberInfo, TOTAL_KEY};

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("tg4-hooks");

pub const TOTAL: Item<u64> = Item::new(TOTAL_KEY);

pub const MEMBERS: SnapshotMap<&Addr, MemberInfo> = SnapshotMap::new(
    tg4::MEMBERS_KEY,
    tg4::MEMBERS_CHECKPOINTS,
    tg4::MEMBERS_CHANGELOG,
    Strategy::EveryBlock,
);
