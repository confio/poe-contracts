use anyhow::{bail, Result as AnyResult};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use thiserror::Error;

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cosmwasm_std::OwnedDeps;
use std::marker::PhantomData;

use cosmwasm_std::Order::Ascending;
use cosmwasm_std::{
    from_slice, to_binary, Addr, Api, Binary, BlockInfo, Coin, CustomQuery, Empty, Order, Querier,
    QuerierResult, StdError, StdResult, Storage, Timestamp,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, Executor, Module,
    WasmKeeper, WasmSudo,
};
use cw_storage_plus::{Item, Map};

use tg_bindings::{
    Evidence, GovProposal, ListPrivilegedResponse, Privilege, PrivilegeChangeMsg, PrivilegeMsg,
    TgradeMsg, TgradeQuery, TgradeSudoMsg, ValidatorDiff, ValidatorVote, ValidatorVoteResponse,
};

pub struct TgradeModule {}

pub type Privileges = Vec<Privilege>;

/// How many seconds per block
/// (when we increment block.height, use this multiplier for block.time)
pub const BLOCK_TIME: u64 = 5;

const PRIVILEGES: Map<&Addr, Privileges> = Map::new("privileges");
const VOTES: Item<ValidatorVoteResponse> = Item::new("votes");
const PINNED: Item<Vec<u64>> = Item::new("pinned");
const PLANNED_UPGRADE: Item<UpgradePlan> = Item::new("planned_upgrade");
const PARAMS: Map<String, String> = Map::new("params");

const ADMIN_PRIVILEGES: &[Privilege] = &[
    Privilege::GovProposalExecutor,
    Privilege::Sudoer,
    Privilege::TokenMinter,
    Privilege::ConsensusParamChanger,
];

pub type TgradeDeps = OwnedDeps<MockStorage, MockApi, MockQuerier, TgradeQuery>;

pub fn mock_deps_tgrade() -> TgradeDeps {
    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: MockQuerier::default(),
        custom_query_type: PhantomData,
    }
}

impl TgradeModule {
    /// Intended for init_modules to set someone who can grant privileges or call arbitrary
    /// TgradeMsg externally
    pub fn set_owner(&self, storage: &mut dyn Storage, owner: &Addr) -> StdResult<()> {
        PRIVILEGES.save(storage, owner, &ADMIN_PRIVILEGES.to_vec())?;
        Ok(())
    }

    /// Used to mock out the response for TgradeQuery::ValidatorVotes
    pub fn set_votes(&self, storage: &mut dyn Storage, votes: Vec<ValidatorVote>) -> StdResult<()> {
        VOTES.save(storage, &ValidatorVoteResponse { votes })
    }

    pub fn is_pinned(&self, storage: &dyn Storage, code: u64) -> StdResult<bool> {
        let pinned = PINNED.may_load(storage)?;
        match pinned {
            Some(pinned) => Ok(pinned.contains(&code)),
            None => Ok(false),
        }
    }

    pub fn upgrade_is_planned(&self, storage: &dyn Storage) -> StdResult<Option<UpgradePlan>> {
        PLANNED_UPGRADE.may_load(storage)
    }

    pub fn get_params(&self, storage: &dyn Storage) -> StdResult<Vec<(String, String)>> {
        PARAMS.range(storage, None, None, Ascending).collect()
    }

    fn require_privilege(
        &self,
        storage: &dyn Storage,
        addr: &Addr,
        required: Privilege,
    ) -> AnyResult<()> {
        let allowed = PRIVILEGES
            .may_load(storage, addr)?
            .unwrap_or_default()
            .into_iter()
            .any(|p| p == required);
        if !allowed {
            return Err(TgradeError::Unauthorized("Admin privileges required".to_owned()).into());
        }
        Ok(())
    }
}

impl Module for TgradeModule {
    type ExecT = TgradeMsg;
    type QueryT = TgradeQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: TgradeMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg {
            TgradeMsg::Privilege(PrivilegeMsg::Request(add)) => {
                if add == Privilege::ValidatorSetUpdater {
                    // there can be only one with ValidatorSetUpdater privilege
                    let validator_registered =
                        PRIVILEGES
                            .range(storage, None, None, Order::Ascending)
                            .fold(Ok(false), |val, item| match (val, item) {
                                (Err(e), _) => Err(e),
                                (_, Err(e)) => Err(e),
                                (Ok(found), Ok((_, privs))) => Ok(found
                                    || privs.iter().any(|p| *p == Privilege::ValidatorSetUpdater)),
                            })?;
                    if validator_registered {
                        bail!(
                            "One ValidatorSetUpdater already registered, cannot register a second"
                        );
                    }
                }

                // if we are privileged (even an empty array), we can auto-add more
                let mut powers = PRIVILEGES.may_load(storage, &sender)?.ok_or_else(|| {
                    TgradeError::Unauthorized("Admin privileges required".to_owned())
                })?;
                powers.push(add);
                PRIVILEGES.save(storage, &sender, &powers)?;
                Ok(AppResponse::default())
            }
            TgradeMsg::Privilege(PrivilegeMsg::Release(remove)) => {
                let powers = PRIVILEGES.may_load(storage, &sender)?;
                if let Some(powers) = powers {
                    let updated = powers.into_iter().filter(|p| *p != remove).collect();
                    PRIVILEGES.save(storage, &sender, &updated)?;
                }
                Ok(AppResponse::default())
            }
            TgradeMsg::WasmSudo { contract_addr, msg } => {
                self.require_privilege(storage, &sender, Privilege::Sudoer)?;
                let contract_addr = api.addr_validate(&contract_addr)?;
                let sudo = WasmSudo { contract_addr, msg };
                router.sudo(api, storage, block, sudo.into())
            }
            TgradeMsg::ConsensusParams(_) => {
                // We don't do anything here
                self.require_privilege(storage, &sender, Privilege::ConsensusParamChanger)?;
                Ok(AppResponse::default())
            }
            TgradeMsg::ExecuteGovProposal {
                title: _,
                description: _,
                proposal,
            } => {
                self.require_privilege(storage, &sender, Privilege::GovProposalExecutor)?;
                match proposal {
                    GovProposal::PromoteToPrivilegedContract { contract } => {
                        // update contract state
                        let contract_addr = api.addr_validate(&contract)?;
                        PRIVILEGES.update(storage, &contract_addr, |current| -> StdResult<_> {
                            // if nothing is set, make it an empty array
                            Ok(current.unwrap_or_default())
                        })?;

                        // call into contract
                        let msg = to_binary(&TgradeSudoMsg::<Empty>::PrivilegeChange(
                            PrivilegeChangeMsg::Promoted {},
                        ))?;
                        let sudo = WasmSudo { contract_addr, msg };
                        router.sudo(api, storage, block, sudo.into())
                    }
                    GovProposal::DemotePrivilegedContract { contract } => {
                        // remove contract privileges
                        let contract_addr = api.addr_validate(&contract)?;
                        PRIVILEGES.remove(storage, &contract_addr);

                        // call into contract
                        let msg = to_binary(&TgradeSudoMsg::<Empty>::PrivilegeChange(
                            PrivilegeChangeMsg::Demoted {},
                        ))?;
                        let sudo = WasmSudo { contract_addr, msg };
                        router.sudo(api, storage, block, sudo.into())
                    }
                    GovProposal::PinCodes { code_ids } => {
                        let mut pinned = PINNED.may_load(storage)?.unwrap_or_default();
                        pinned.extend(code_ids);
                        pinned.sort_unstable();
                        pinned.dedup();
                        PINNED.save(storage, &pinned)?;

                        Ok(AppResponse::default())
                    }
                    GovProposal::UnpinCodes { code_ids } => {
                        let pinned = PINNED
                            .may_load(storage)?
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|id| !code_ids.contains(id))
                            .collect();
                        PINNED.save(storage, &pinned)?;

                        Ok(AppResponse::default())
                    }
                    GovProposal::RegisterUpgrade { name, height, info } => {
                        match PLANNED_UPGRADE.may_load(storage)? {
                            Some(_) => Err(anyhow::anyhow!("an upgrade plan already exists")),
                            None => {
                                PLANNED_UPGRADE
                                    .save(storage, &UpgradePlan::new(name, height, info))?;
                                Ok(AppResponse::default())
                            }
                        }
                    }
                    GovProposal::CancelUpgrade {} => match PLANNED_UPGRADE.may_load(storage)? {
                        None => Err(anyhow::anyhow!("an upgrade plan doesn't exist")),
                        Some(_) => {
                            PLANNED_UPGRADE.remove(storage);
                            Ok(AppResponse::default())
                        }
                    },
                    // these are not yet implemented, but should be
                    GovProposal::InstantiateContract { .. } => {
                        bail!("GovProposal::InstantiateContract not implemented")
                    }
                    // these cannot be implemented, should fail
                    GovProposal::MigrateContract { .. } => {
                        bail!("GovProposal::MigrateContract not implemented")
                    }
                    GovProposal::ChangeParams(params) => {
                        let mut sorted_params = params.clone();
                        sorted_params.sort_unstable();
                        sorted_params.dedup_by(|a, b| a.subspace == b.subspace && a.key == b.key);
                        if sorted_params.len() < params.len() {
                            return Err(anyhow::anyhow!(
                                "duplicate subspace + keys in params vector"
                            ));
                        }
                        for p in params {
                            if p.subspace.is_empty() {
                                return Err(anyhow::anyhow!("empty subspace key"));
                            }
                            if p.key.is_empty() {
                                return Err(anyhow::anyhow!("empty key key"));
                            }
                            PARAMS.save(storage, format!("{}/{}", p.subspace, p.key), &p.value)?;
                        }
                        Ok(AppResponse::default())
                    }
                    // most are ignored
                    _ => Ok(AppResponse::default()),
                }
            }
            TgradeMsg::MintTokens {
                denom,
                amount,
                recipient,
            } => {
                self.require_privilege(storage, &sender, Privilege::TokenMinter)?;
                let mint = BankSudo::Mint {
                    to_address: recipient,
                    amount: vec![Coin { denom, amount }],
                };
                router.sudo(api, storage, block, mint.into())
            }
            TgradeMsg::Delegate {
                funds: _funds,
                staker: _staker,
            } => {
                self.require_privilege(storage, &sender, Privilege::Delegator)?;
                // FIXME? We don't do anything here
                Ok(AppResponse::default())
            }
            TgradeMsg::Undelegate {
                funds: _funds,
                recipient: _recipient,
            } => {
                self.require_privilege(storage, &sender, Privilege::Delegator)?;
                // FIXME? We don't do anything here
                Ok(AppResponse::default())
            }
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        bail!("sudo not implemented for TgradeModule")
    }

    fn query(
        &self,
        _api: &dyn Api,
        storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: TgradeQuery,
    ) -> anyhow::Result<Binary> {
        match request {
            TgradeQuery::ListPrivileged(check) => {
                // FIXME: secondary index to make this more efficient
                let privileged = PRIVILEGES
                    .range(storage, None, None, Order::Ascending)
                    .filter_map(|r| {
                        r.map(|(addr, privs)| match privs.iter().any(|p| *p == check) {
                            true => Some(addr),
                            false => None,
                        })
                        .transpose()
                    })
                    .collect::<StdResult<Vec<_>>>()?;
                Ok(to_binary(&ListPrivilegedResponse { privileged })?)
            }
            TgradeQuery::ValidatorVotes {} => {
                let res = VOTES.may_load(storage)?.unwrap_or_default();
                Ok(to_binary(&res)?)
            }
        }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum TgradeError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

pub type TgradeAppWrapped =
    App<BankKeeper, MockApi, MockStorage, TgradeModule, WasmKeeper<TgradeMsg, TgradeQuery>>;

pub struct TgradeApp(TgradeAppWrapped);

impl Deref for TgradeApp {
    type Target = TgradeAppWrapped;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TgradeApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Querier for TgradeApp {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        self.0.raw_query(bin_request)
    }
}

impl TgradeApp {
    pub fn new(owner: &str) -> Self {
        let owner = Addr::unchecked(owner);
        Self(
            BasicAppBuilder::<TgradeMsg, TgradeQuery>::new_custom()
                .with_custom(TgradeModule {})
                .build(|router, _, storage| {
                    router.custom.set_owner(storage, &owner).unwrap();
                }),
        )
    }

    pub fn new_genesis(owner: &str) -> Self {
        let owner = Addr::unchecked(owner);
        let block_info = BlockInfo {
            height: 0,
            time: Timestamp::from_nanos(1_571_797_419_879_305_533),
            chain_id: "tgrade-testnet-14002".to_owned(),
        };

        Self(
            BasicAppBuilder::<TgradeMsg, TgradeQuery>::new_custom()
                .with_custom(TgradeModule {})
                .with_block(block_info)
                .build(|router, _, storage| {
                    router.custom.set_owner(storage, &owner).unwrap();
                }),
        )
    }

    pub fn block_info(&self) -> BlockInfo {
        self.0.block_info()
    }

    pub fn promote(&mut self, owner: &str, contract: &str) -> AnyResult<AppResponse> {
        let msg = TgradeMsg::ExecuteGovProposal {
            title: "Promote Contract".to_string(),
            description: "Promote Contract".to_string(),
            proposal: GovProposal::PromoteToPrivilegedContract {
                contract: contract.to_string(),
            },
        };
        self.execute(Addr::unchecked(owner), msg.into())
    }

    /// This reverses to genesis (based on current time/height)
    pub fn back_to_genesis(&mut self) {
        self.update_block(|block| {
            block.time = block.time.minus_seconds(BLOCK_TIME * block.height);
            block.height = 0;
        });
    }

    /// This advances BlockInfo by given number of blocks.
    /// It does not do any callbacks, but keeps the ratio of seconds/blokc
    pub fn advance_blocks(&mut self, blocks: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(BLOCK_TIME * blocks);
            block.height += blocks;
        });
    }

    /// This advances BlockInfo by given number of seconds.
    /// It does not do any callbacks, but keeps the ratio of seconds/blokc
    pub fn advance_seconds(&mut self, seconds: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(seconds);
            block.height += max(1, seconds / BLOCK_TIME);
        });
    }

    /// next_block will call the end_blocker, increment block info 1 height and 5 seconds,
    /// and then call the begin_blocker (with no evidence) in the next block.
    /// It returns the validator diff if any.
    ///
    /// Simple iterator when you don't care too much about the details and just want to
    /// simulate forward motion.
    pub fn next_block(&mut self) -> AnyResult<Option<ValidatorDiff>> {
        let (_, diff) = self.end_block()?;
        self.update_block(|block| {
            block.time = block.time.plus_seconds(BLOCK_TIME);
            block.height += 1;
        });
        self.begin_block(vec![])?;
        Ok(diff)
    }

    /// Returns a list of all contracts that have the requested privilege
    pub fn with_privilege(&self, requested: Privilege) -> AnyResult<Vec<Addr>> {
        let ListPrivilegedResponse { privileged } = self
            .wrap()
            .query(&TgradeQuery::ListPrivileged(requested).into())?;
        Ok(privileged)
    }

    fn valset_updater(&self) -> AnyResult<Option<Addr>> {
        let mut updaters = self.with_privilege(Privilege::ValidatorSetUpdater)?;
        if updaters.len() > 1 {
            bail!("Multiple ValidatorSetUpdater registered")
        } else {
            Ok(updaters.pop())
        }
    }

    /// Make the BeginBlock sudo callback on all contracts that have registered
    /// with the BeginBlocker Privilege
    pub fn begin_block(&mut self, evidence: Vec<Evidence>) -> AnyResult<Vec<AppResponse>> {
        let to_call = self.with_privilege(Privilege::BeginBlocker)?;
        let msg = TgradeSudoMsg::<Empty>::BeginBlock { evidence };
        let res = to_call
            .into_iter()
            .map(|contract| self.wasm_sudo(contract, &msg))
            .collect::<AnyResult<_>>()?;
        Ok(res)
    }

    /// Make the EndBlock sudo callback on all contracts that have registered
    /// with the EndBlocker Privilege. Then makes the EndWithValidatorUpdate callback
    /// on any registered valset_updater.
    pub fn end_block(&mut self) -> AnyResult<(Vec<AppResponse>, Option<ValidatorDiff>)> {
        let to_call = self.with_privilege(Privilege::EndBlocker)?;
        let msg = TgradeSudoMsg::<Empty>::EndBlock {};

        let mut res: Vec<AppResponse> = to_call
            .into_iter()
            .map(|contract| self.wasm_sudo(contract, &msg))
            .collect::<AnyResult<_>>()?;

        let diff = match self.valset_updater()? {
            Some(contract) => {
                let mut r =
                    self.wasm_sudo(contract, &TgradeSudoMsg::<Empty>::EndWithValidatorUpdate {})?;
                let data = r.data.take();
                res.push(r);
                match data {
                    Some(b) if !b.is_empty() => Some(from_slice(&b)?),
                    _ => None,
                }
            }
            None => None,
        };
        Ok((res, diff))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct UpgradePlan {
    name: String,
    height: u64,
    info: String,
}

impl UpgradePlan {
    pub fn new(name: impl ToString, height: u64, info: impl ToString) -> Self {
        Self {
            name: name.to_string(),
            height,
            info: info.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::coin;
    use cw_multi_test::Executor;

    #[test]
    fn init_and_owner_mints_tokens() {
        let owner = Addr::unchecked("govner");
        let rcpt = Addr::unchecked("townies");

        let mut app = TgradeApp::new(owner.as_str());

        // no tokens
        let start = app.wrap().query_all_balances(rcpt.as_str()).unwrap();
        assert_eq!(start, vec![]);

        // prepare to mint
        let mintable = coin(123456, "shilling");
        let msg = TgradeMsg::MintTokens {
            denom: mintable.denom.clone(),
            amount: mintable.amount,
            recipient: rcpt.to_string(),
        };

        // townies cannot
        let _ = app.execute(rcpt.clone(), msg.clone().into()).unwrap_err();

        // Gov'ner can
        app.execute(owner, msg.into()).unwrap();

        // we got tokens!
        let end = app
            .wrap()
            .query_balance(rcpt.as_str(), &mintable.denom)
            .unwrap();
        assert_eq!(end, mintable);
    }

    // TODO: Delegate / Undelegate tests
}
