use crate::{Hooks, Preauth, Slashers};
use cosmwasm_std::Addr;
use cw_controllers::Admin;
use cw_storage_plus::{Index, IndexList, IndexedSnapshotMap, Item, MultiIndex, Strategy};
use tg4::TOTAL_KEY;

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("tg4-hooks");
pub const PREAUTH_HOOKS: Preauth = Preauth::new("tg4-preauth");
pub const SLASHERS: Slashers = Slashers::new("tg4-slashers");
pub const PREAUTH_SLASHING: Preauth = Preauth::new("tg4-preauth_slashing");
pub const TOTAL: Item<u64> = Item::new(TOTAL_KEY);

pub struct MemberIndexes<'a> {
    // Weights (multi-)index (deserializing the (hidden) pk to Addr)
    pub weight: MultiIndex<'a, u64, u64, Addr>,
}

impl<'a> IndexList<u64> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<u64>> + '_> {
        let v: Vec<&dyn Index<u64>> = vec![&self.weight];
        Box::new(v.into_iter())
    }
}

/// Indexed snapshot map for members.
/// This allows to query the map members, sorted by weight.
/// The weight index is a `MultiIndex`, as there can be multiple members with the same weight.
/// The weight index is not snapshotted; only the current weights are indexed at any given time.
pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, u64, MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        weight: MultiIndex::new(|&w| w, tg4::MEMBERS_KEY, "members__weight"),
    };
    IndexedSnapshotMap::new(
        tg4::MEMBERS_KEY,
        tg4::MEMBERS_CHECKPOINTS,
        tg4::MEMBERS_CHANGELOG,
        Strategy::EveryBlock,
        indexes,
    )
}
