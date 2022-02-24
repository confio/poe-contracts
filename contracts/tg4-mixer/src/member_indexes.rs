use cosmwasm_std::Addr;
use cw_storage_plus::{Index, IndexList, IndexedSnapshotMap, MultiIndex, Strategy};

use tg4::MemberInfo;

// Copied from `tg-utils` and re-defined here for the extra tie-break index
pub struct MemberIndexes<'a> {
    // Points (multi-)indexes (deserializing the (hidden) pk to Addr)
    pub points: MultiIndex<'a, u64, MemberInfo, Addr>,
    pub points_tie_break: MultiIndex<'a, (u64, i64), MemberInfo, Addr>,
}

impl<'a> IndexList<MemberInfo> for MemberIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<MemberInfo>> + '_> {
        let v: Vec<&dyn Index<MemberInfo>> = vec![&self.points, &self.points_tie_break];
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
pub fn members<'a>() -> IndexedSnapshotMap<'a, &'a Addr, MemberInfo, MemberIndexes<'a>> {
    let indexes = MemberIndexes {
        points: MultiIndex::new(|mi| mi.points, tg4::MEMBERS_KEY, "members__points"),
        points_tie_break: MultiIndex::new(
            |mi| {
                (
                    mi.points,
                    mi.start_height
                        .map_or(i64::MIN, |h| (h as i64).wrapping_neg()),
                )
            }, // Works as long as `start_height <= i64::MAX + 1`
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
