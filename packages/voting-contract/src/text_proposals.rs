use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, Order, StdResult};
use cw_storage_plus::Bound;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::state::{proposals, Proposal, ProposalListResponse, TextProposal};

pub fn list_text_proposals<P>(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: usize,
) -> StdResult<ProposalListResponse<P>>
where
    P: Serialize + DeserializeOwned + TextProposal,
{
    let start = start_after.map(Bound::exclusive_int);
    let props: StdResult<Vec<_>> = proposals()
        .range(deps.storage, start, None, Order::Ascending)
        .filter(|r: &Result<(u64, Proposal<P>), cosmwasm_std::StdError>| {
            if let Ok((_, p)) = r {
                p.proposal.is_text()
            } else {
                true
            }
        })
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}
