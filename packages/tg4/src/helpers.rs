use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_binary, to_vec, Addr, Binary, ContractResult, CustomQuery, QuerierWrapper,
    QueryRequest, StdError, StdResult, SystemResult, WasmMsg, WasmQuery,
};
use tg_bindings::TgradeMsg;

use crate::msg::Tg4ExecuteMsg;
use crate::query::HooksResponse;
use crate::{
    member_key, AdminResponse, Member, MemberInfo, MemberListResponse, MemberResponse, Tg4QueryMsg,
    TOTAL_KEY,
};

pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

/// Tg4Contract is a wrapper around Addr that provides a lot of helpers
/// for working with tg4 contracts
///
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Tg4Contract(pub Addr);

impl Tg4Contract {
    pub fn new(addr: Addr) -> Self {
        Tg4Contract(addr)
    }

    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    fn encode_msg(&self, msg: Tg4ExecuteMsg) -> StdResult<SubMsg> {
        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg: to_binary(&msg)?,
            funds: vec![],
        }))
    }

    pub fn encode_raw_msg<T: Into<Binary>>(&self, msg: T) -> StdResult<SubMsg> {
        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg: msg.into(),
            funds: vec![],
        }))
    }

    pub fn add_hook<T: Into<String>>(&self, addr: T) -> StdResult<SubMsg> {
        let msg = Tg4ExecuteMsg::AddHook { addr: addr.into() };
        self.encode_msg(msg)
    }

    pub fn remove_hook<T: Into<String>>(&self, addr: T) -> StdResult<SubMsg> {
        let msg = Tg4ExecuteMsg::RemoveHook { addr: addr.into() };
        self.encode_msg(msg)
    }

    pub fn update_admin<T: Into<String>>(&self, admin: Option<T>) -> StdResult<SubMsg> {
        let msg = Tg4ExecuteMsg::UpdateAdmin {
            admin: admin.map(|x| x.into()),
        };
        self.encode_msg(msg)
    }

    fn encode_smart_query<Q: CustomQuery>(&self, msg: Tg4QueryMsg) -> StdResult<QueryRequest<Q>> {
        Ok(WasmQuery::Smart {
            contract_addr: self.addr().into(),
            msg: to_binary(&msg)?,
        }
        .into())
    }

    fn encode_raw_query<T: Into<Binary>, Q: CustomQuery>(&self, key: T) -> QueryRequest<Q> {
        WasmQuery::Raw {
            contract_addr: self.addr().into(),
            key: key.into(),
        }
        .into()
    }

    /// Show the hooks
    pub fn hooks<Q: CustomQuery>(&self, querier: &QuerierWrapper<Q>) -> StdResult<Vec<String>> {
        let query = self.encode_smart_query(Tg4QueryMsg::Hooks {})?;
        let res: HooksResponse = querier.query(&query)?;
        Ok(res.hooks)
    }

    /// Read the total points
    pub fn total_points<Q: CustomQuery>(&self, querier: &QuerierWrapper<Q>) -> StdResult<u64> {
        let query = self.encode_raw_query(TOTAL_KEY.as_bytes());
        querier.query(&query)
    }

    /// Check if this address is a member, and if so, with which points
    pub fn is_member<Q: CustomQuery>(
        &self,
        querier: &QuerierWrapper<Q>,
        addr: &Addr,
    ) -> StdResult<Option<u64>> {
        let path = member_key(addr.as_ref());
        let query = self.encode_raw_query::<_, Q>(path);

        // We have to copy the logic of Querier.query to handle the empty case, and not
        // try to decode an empty result into a `MemberInfo`.
        // TODO: add similar API on Querier - this is not the first time I came across it
        let raw = to_vec(&query)?;
        match querier.raw_query(&raw) {
            SystemResult::Err(system_err) => Err(StdError::generic_err(format!(
                "Querier system error: {}",
                system_err
            ))),
            SystemResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(
                format!("Querier contract error: {}", contract_err),
            )),
            SystemResult::Ok(ContractResult::Ok(value)) => {
                // This is the only place we customize
                if value.is_empty() {
                    Ok(None)
                } else {
                    Ok(from_slice::<MemberInfo>(&value)?.points.into())
                }
            }
        }
    }

    /// Check if this address is a member
    pub fn is_voting_member<Q: CustomQuery>(
        &self,
        querier: &QuerierWrapper<Q>,
        member: &str,
    ) -> StdResult<u64> {
        self.is_member(querier, &Addr::unchecked(member))?.map_or(
            Err(StdError::generic_err("Unauthorized: not member of a group")),
            |member_points| {
                if member_points < 1 {
                    Err(StdError::generic_err(
                        "Unauthorized: member doesn't have voting power",
                    ))
                } else {
                    Ok(member_points)
                }
            },
        )
    }

    /// Check if this address was a member, and if its points is >= 1
    pub fn was_voting_member<T: Into<String>, Q: CustomQuery>(
        &self,
        querier: &QuerierWrapper<Q>,
        member: T,
        height: u64,
    ) -> StdResult<u64> {
        self.member_at_height(querier, member, height)?.map_or(
            Err(StdError::generic_err(format!(
                "Unauthorized: wasn't member of a group at block height: {}",
                height
            ))),
            |member_points| {
                if member_points < 1 {
                    Err(StdError::generic_err(format!(
                        "Unauthorized: member didn't have voting power at block height: {}",
                        height
                    )))
                } else {
                    Ok(member_points)
                }
            },
        )
    }

    /// Return the member's points at the given snapshot - requires a smart query
    pub fn member_at_height<T: Into<String>, Q: CustomQuery>(
        &self,
        querier: &QuerierWrapper<Q>,
        member: T,
        height: u64,
    ) -> StdResult<Option<u64>> {
        let query = self.encode_smart_query(Tg4QueryMsg::Member {
            addr: member.into(),
            at_height: Some(height),
        })?;
        let res: MemberResponse = querier.query(&query)?;
        Ok(res.points)
    }

    pub fn list_members<Q: CustomQuery>(
        &self,
        querier: &QuerierWrapper<Q>,
        start_after: Option<String>,
        limit: Option<u32>,
    ) -> StdResult<Vec<Member>> {
        let query = self.encode_smart_query(Tg4QueryMsg::ListMembers { start_after, limit })?;
        let res: MemberListResponse = querier.query(&query)?;
        Ok(res.members)
    }

    pub fn list_members_by_points<Q: CustomQuery>(
        &self,
        querier: &QuerierWrapper<Q>,
        start_after: Option<Member>,
        limit: Option<u32>,
    ) -> StdResult<Vec<Member>> {
        let query =
            self.encode_smart_query(Tg4QueryMsg::ListMembersByPoints { start_after, limit })?;
        let res: MemberListResponse = querier.query(&query)?;
        Ok(res.members)
    }

    /// This will make some queries to ensure that the target contract is tg4-compatible.
    /// It returns `true` iff it appears to be compatible.
    pub fn is_tg4<Q: CustomQuery>(&self, querier: &QuerierWrapper<Q>) -> bool {
        self.list_members(querier, None, Some(1)).is_ok()
    }

    /// Read the admin
    pub fn admin<Q: CustomQuery>(&self, querier: &QuerierWrapper<Q>) -> StdResult<Option<String>> {
        let query = self.encode_smart_query(Tg4QueryMsg::Admin {})?;
        let res: AdminResponse = querier.query(&query)?;
        Ok(res.admin)
    }
}
