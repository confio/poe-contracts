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
    // Points (multi-)index (deserializing the (hidden) pk to Addr)
    pub points: MultiIndex<'a, u64, (u64, u64), Addr>,
    pub points_tie_break: MultiIndex<'a, (u64, i64), (u64, u64), Addr>,
}

impl<'a> IndexList<(u64, u64)> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<(u64, u64)>> + '_> {
        let v: Vec<&dyn Index<(u64, u64)>> = vec![&self.points, &self.points_tie_break];
        Box::new(v.into_iter())
    }
}

/// Indexed snapshot map for members.
///
/// - The map primary key is `Addr`, and the value is a tuple of `points`, `start_height` values.
/// - The `points` index is a `MultiIndex`, as there can be multiple members with the
/// same points.
/// - The `(points, -start_height)` index is a `MultiIndex`, as there can be multiple members with the
/// same points, added at the same block height.
/// The second tuple element of the tie-breaking index is negative, so that lower heights
/// (older members) are sorted first, as this will be used as a descending index.
///
/// This allows to query the map members, sorted by points, breaking ties by height, if needed
/// (breaking ties by address in turn).
/// The indexes are not snapshotted; only the current points are indexed at any given time.
pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, (u64, u64), MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        points: MultiIndex::new(|&(w, _)| w, tg4::MEMBERS_KEY, "members__points"),
        points_tie_break: MultiIndex::new(
            |&(w, h)| (w, -(h as i64)),
            tg4::MEMBERS_KEY,
            "members__points_tie_break",
        ),
    };
    IndexedSnapshotMap::new(
        tg4::MEMBERS_KEY,
        tg4::MEMBERS_CHECKPOINTS,
        tg4::MEMBERS_CHANGELOG,
        Strategy::EveryBlock,
        indexes,
    )
}
