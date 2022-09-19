use cosmwasm_std::Addr;

use cw_controllers::Admin;
use cw_storage_plus::{Index, IndexList, IndexedSnapshotMap, Item, MultiIndex, Strategy};

use tg4::{MemberInfo, TOTAL_KEY};

use crate::{Hooks, Preauth, Slashers};

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("tg4-hooks");
pub const PREAUTH_HOOKS: Preauth = Preauth::new("tg4-preauth");
pub const SLASHERS: Slashers = Slashers::new("tg4-slashers");
pub const PREAUTH_SLASHING: Preauth = Preauth::new("tg4-preauth_slashing");
pub const TOTAL: Item<u64> = Item::new(TOTAL_KEY);

pub struct MemberIndexes<'a> {
    // Points (multi-)index (deserializing the (hidden) pk to Addr)
    pub points: MultiIndex<'a, u64, MemberInfo, Addr>,
}

impl<'a> IndexList<MemberInfo> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<MemberInfo>> + '_> {
        let v: Vec<&dyn Index<MemberInfo>> = vec![&self.points];
        Box::new(v.into_iter())
    }
}

/// Indexed snapshot map for members.
/// This allows to query the map members, sorted by points.
/// The points index is a `MultiIndex`, as there can be multiple members with the same points.
/// The points index is not snapshotted; only the current points are indexed at any given time.
pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, MemberInfo, MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        points: MultiIndex::new(|_, mi| mi.points, tg4::MEMBERS_KEY, "members__points"),
    };
    IndexedSnapshotMap::new(
        tg4::MEMBERS_KEY,
        tg4::MEMBERS_CHECKPOINTS,
        tg4::MEMBERS_CHANGELOG,
        Strategy::EveryBlock,
        indexes,
    )
}
