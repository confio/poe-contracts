use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Tg4QueryMsg {
    /// Return AdminResponse
    Admin {},
    /// Return TotalPointsResponse
    TotalPoints {},
    /// Returns MemberListResponse.
    /// The result is sorted by address ascending
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, sorted by points descending
    ListMembersByPoints {
        start_after: Option<Member>,
        limit: Option<u32>,
    },
    /// Returns MemberResponse
    Member {
        addr: String,
        at_height: Option<u64>,
    },
    /// Shows all registered hooks. Returns HooksResponse.
    Hooks {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct AdminResponse {
    pub admin: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, Debug)]
pub struct MemberInfo {
    pub points: u64,
    pub start_height: Option<u64>,
}

impl MemberInfo {
    pub fn new(points: u64) -> Self {
        Self {
            points,
            start_height: None,
        }
    }

    pub fn new_with_height(points: u64, height: u64) -> Self {
        Self {
            points,
            start_height: Some(height),
        }
    }
}

/// A group member has some points associated with them.
/// This may all be equal, or may have meaning in the app that
/// makes use of the group (eg. voting power)
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Member {
    pub addr: String,
    pub points: u64,
    pub start_height: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MemberListResponse {
    pub members: Vec<Member>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MemberResponse {
    pub points: Option<u64>,
    pub start_height: Option<u64>,
}

impl From<Option<MemberInfo>> for MemberResponse {
    fn from(mi: Option<MemberInfo>) -> Self {
        match mi {
            None => Self {
                points: None,
                start_height: None,
            },
            Some(mi) => Self {
                points: Some(mi.points),
                start_height: mi.start_height,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct TotalPointsResponse {
    pub points: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct HooksResponse {
    pub hooks: Vec<String>,
}

/// TOTAL_KEY is meant for raw queries
pub const TOTAL_KEY: &str = "total";
pub const MEMBERS_KEY: &str = "members";
pub const MEMBERS_CHECKPOINTS: &str = "members__checkpoints";
pub const MEMBERS_CHANGELOG: &str = "members__changelog";

/// member_key is meant for raw queries for one member, given address
pub fn member_key(address: &str) -> Vec<u8> {
    // FIXME: Inlined here to avoid storage-plus import
    let mut key = [b"\x00", &[MEMBERS_KEY.len() as u8], MEMBERS_KEY.as_bytes()].concat();
    key.extend_from_slice(address.as_bytes());
    key
}
