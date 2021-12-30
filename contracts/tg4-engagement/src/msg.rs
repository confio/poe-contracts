use cosmwasm_std::{Addr, Coin, Decimal, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tg4::Member;
use tg_bindings::{Evidence, PrivilegeChangeMsg};
use tg_utils::Duration;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    pub members: Vec<Member>,
    #[serde(default)]
    pub preauths_hooks: u64,
    #[serde(default)]
    pub preauths_slashing: u64,
    pub halflife: Option<Duration>,
    /// Denom of tokens which may be distributed by this contract.
    pub denom: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Change the admin
    UpdateAdmin { admin: Option<String> },
    /// apply a diff to the existing members.
    /// remove is applied after add, so if an address is in both, it is removed
    UpdateMembers {
        remove: Vec<String>,
        add: Vec<Member>,
    },
    /// Add points (weight) to member's address
    AddPoints { addr: String, points: u64 },
    /// Add a new hook to be informed of all membership changes. Must be called by Admin
    AddHook { addr: String },
    /// Remove a hook. Must be called by Admin
    RemoveHook { addr: String },
    /// Distributes funds sent with this message, and all funds transferred since last call of this
    /// to members, proportionally to their weights. Funds are not immediately send to members, but
    /// assigned to them for later withdrawal (see: `ExecuteMsg::WithdrawFunds`)
    DistributeFunds {
        /// Original source of funds, informational. If present overwrites "sender" field on
        /// propagated event.
        sender: Option<String>,
    },
    /// Withdraws funds which were previously distributed and assigned to sender.
    WithdrawFunds {
        /// Account which funds assigned too should be withdrawn, `sender` by default. `sender` has
        /// to be eligible for withdrawal from `owner` address to perform this call (`owner` has to
        /// call `DelegateWithdrawal { delegated: seder }` before)
        owner: Option<String>,
        /// Address where to transfer funds. If not present, funds would be send to `sender`.
        receiver: Option<String>,
    },
    /// Sets given address as allowed for senders funds withdrawal. Funds still can be withdrawn by
    /// sender himself, but additional account is allowed to perform it as well. There can be only
    /// one account delegated for withdrawal for any owner at single time.
    DelegateWithdrawal {
        /// Account delegated for withdrawal. To disallow current withdrawal, best is to set it to
        /// own address.
        delegated: String,
    },
    /// Adds slasher for contract if there are enough `slasher_preauths` left
    AddSlasher { addr: String },
    /// Removes slasher for contract
    RemoveSlasher { addr: String },
    /// Slash engagement points from address
    Slash { addr: String, portion: Decimal },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Return AdminResponse
    Admin {},
    /// Return TotalWeightResponse
    TotalWeight {},
    /// Returns MemberListResponse
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns MemberListResponse, sorted by weight descending
    ListMembersByWeight {
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
    /// Return the current number of preauths. Returns PreauthResponse.
    Preauths {},
    /// Return how much funds are assigned for withdrawal to given address. Returns
    /// `FundsResponse`.
    WithdrawableFunds { owner: String },
    /// Return how much funds were distributed in total by this contract. Returns
    /// `FundsResponse`.
    DistributedFunds {},
    /// Return how much funds were send to this contract since last `ExecuteMsg::DistribtueFunds`,
    /// and wait for distribution. Returns `FundsResponse`.
    UndistributedFunds {},
    /// Returns address allowed for withdrawal funds assigned to owner. Returns `DelegateResponse`
    Delegated { owner: String },
    /// Returns information about the halflife, including the duration in seconds, the last
    /// and the next occurence.
    Halflife {},
    /// Returns information (bool) whether given address is an active slasher
    IsSlasher { addr: String },
    /// Returns all active slashers as vector of addresses
    ListSlashers {},
    /// Returns tokens distribution data
    DistributionData {},
    /// Returns withdraw adjustment
    WithdrawAdjustmentData { addr: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SudoMsg {
    /// This will be delivered every block if the contract is currently registered for Begin Block
    /// types based on subset of https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L81
    BeginBlock {
        /// This is proven evidence of malice and the basis for slashing validators
        evidence: Vec<Evidence>,
    },
    /// This will be delivered every block if the contract is currently registered for End Block
    /// Block height and time is already in Env.
    EndBlock {},
    /// This will be delivered after all end blockers if this is registered for ValidatorUpdates.
    /// If it sets Response.data, it must be a JSON-encoded ValidatorDiff,
    /// which will be used to change the validator set.
    EndWithValidatorUpdate {},
    PrivilegeChange(PrivilegeChangeMsg),
    /// This allows updating group membership via sudo.
    /// Use case: for post-genesis validators, we want to set some initial engagement points / weight.
    /// Note: If the member already exists, its weight will be reset to the weight sent here.
    UpdateMember(Member),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct PreauthResponse {
    pub preauths: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct FundsResponse {
    pub funds: Coin,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct DelegatedResponse {
    pub delegated: Addr,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct HalflifeResponse {
    // `None` means the halflife functionality is disabled for this instance.
    pub halflife_info: Option<HalflifeInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct HalflifeInfo {
    pub last_halflife: Timestamp,
    pub halflife: Duration,
    pub next_halflife: Timestamp,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct SlasherResponse {
    pub is_slasher: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListSlashersResponse {
    pub slashers: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_json_to_sudo_msg() {
        let message = r#"{"update_member": {"addr": "xxx", "weight": 123}}"#;
        assert_eq!(
            SudoMsg::UpdateMember(Member {
                addr: "xxx".to_string(),
                weight: 123
            }),
            cosmwasm_std::from_slice::<SudoMsg>(message.as_bytes()).unwrap()
        );
    }
}
