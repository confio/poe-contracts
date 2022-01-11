use crate::{error::ContractError, msg::*, state::*};

use cosmwasm_std::{coin, Addr, CosmosMsg, Timestamp, Uint128};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, CosmosRouter, Executor};
use tg_bindings::TgradeMsg;
use tg_bindings_test::TgradeApp;
use tg_utils::Expiration;

use anyhow::Result as AnyResult;
use derivative::Derivative;

pub fn vesting_contract() -> Box<dyn Contract<TgradeMsg>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

pub struct SuiteBuilder {
    recipient: String,
    operator: String,
    oversight: String,
    denom: String,
    vesting_plan: VestingPlan,
    initial_tokens: u128,
    owner: String,
    app: TgradeApp,
}

impl SuiteBuilder {
    pub fn new() -> SuiteBuilder {
        let default_owner = "owner";
        let mut app = TgradeApp::new(default_owner);
        app.back_to_genesis();
        SuiteBuilder {
            recipient: "RECIPIENT".to_owned(),
            operator: "OPERATOR".to_owned(),
            oversight: "OVERSIGHT".to_owned(),
            denom: "DENOM".to_owned(),
            // create any vesting plan, just to decrease boilerplate code
            // in a lot of cases it's not needed
            vesting_plan: VestingPlan::Discrete {
                release_at: Expiration::at_timestamp(Timestamp::from_seconds(1)),
            },
            initial_tokens: 0u128,
            owner: default_owner.to_owned(),
            app,
        }
    }

    pub fn with_tokens(mut self, amount: u128) -> Self {
        self.initial_tokens = amount;
        self
    }

    pub fn with_vesting_plan_in_seconds_from_start(
        mut self,
        start_at: Option<u64>,
        end_at: u64,
    ) -> Self {
        // processing initial block in build() later adds 5 seconds to current block time,
        // so add them extra during initialization to even the calculations
        let initial_time = 5;
        let block_info = self.app.block_info();
        self.vesting_plan = match start_at {
            Some(start_at) => {
                let start_at =
                    Expiration::at_timestamp(block_info.time.plus_seconds(start_at + initial_time));
                let end_at =
                    Expiration::at_timestamp(block_info.time.plus_seconds(end_at + initial_time));
                VestingPlan::Continuous { start_at, end_at }
            }
            None => {
                let release_at =
                    Expiration::at_timestamp(block_info.time.plus_seconds(end_at + initial_time));
                VestingPlan::Discrete { release_at }
            }
        };
        self
    }

    #[track_caller]
    pub fn build(mut self) -> Suite {
        let owner = Addr::unchecked(self.owner.clone());

        let block_info = self.app.block_info();
        let denom = self.denom.clone();
        let amount = Uint128::new(self.initial_tokens);

        self.app
            .init_modules(|router, api, storage| -> AnyResult<()> {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    owner.clone(),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom: denom.clone(),
                        amount,
                        recipient: owner.to_string(),
                    })
                    .into(),
                )?;
                Ok(())
            })
            .unwrap();

        let contract_id = self.app.store_code(vesting_contract());
        let recipient = Addr::unchecked(self.recipient);
        let operator = Addr::unchecked(self.operator);
        let oversight = Addr::unchecked(self.oversight);
        let contract = self
            .app
            .instantiate_contract(
                contract_id,
                Addr::unchecked(owner.clone()),
                &InstantiateMsg {
                    denom: denom.clone(),
                    recipient: recipient.clone(),
                    operator: operator.clone(),
                    oversight: oversight.clone(),
                    vesting_plan: self.vesting_plan,
                },
                &[coin(self.initial_tokens, denom.clone())],
                "vesting",
                Some(owner.to_string()),
            )
            .unwrap();

        // process initial genesis block
        self.app.next_block().unwrap();

        Suite {
            owner,
            app: self.app,
            contract,
            recipient,
            operator,
            oversight,
            denom,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    #[derivative(Debug = "ignore")]
    pub app: TgradeApp,
    pub owner: Addr,
    /// Vesting contract address,
    pub contract: Addr,
    pub recipient: Addr,
    pub operator: Addr,
    pub oversight: Addr,
    pub denom: String,
}

impl Suite {
    pub fn mint_tokens(&mut self, amount: u128) -> AnyResult<()> {
        let block_info = self.app.block_info();
        let denom = self.denom.clone();
        let owner = self.owner.to_string();

        self.app
            .init_modules(|router, api, storage| -> AnyResult<()> {
                router.execute(
                    api,
                    storage,
                    &block_info,
                    Addr::unchecked(owner.clone()),
                    CosmosMsg::Custom(TgradeMsg::MintTokens {
                        denom,
                        amount: Uint128::new(amount),
                        recipient: owner,
                    })
                    .into(),
                )?;
                Ok(())
            })
    }

    pub fn release_tokens(
        &mut self,
        sender: &Addr,
        amount: impl Into<Option<u128>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.contract.clone(),
            &ExecuteMsg::ReleaseTokens {
                amount: amount.into().map(Uint128::new),
            },
            &[],
        )
    }

    pub fn freeze_tokens(
        &mut self,
        sender: &Addr,
        amount: impl Into<Option<u128>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.contract.clone(),
            &ExecuteMsg::FreezeTokens {
                amount: amount.into().map(Uint128::new),
            },
            &[],
        )
    }

    pub fn unfreeze_tokens(
        &mut self,
        sender: &Addr,
        amount: impl Into<Option<u128>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.contract.clone(),
            &ExecuteMsg::UnfreezeTokens {
                amount: amount.into().map(Uint128::new),
            },
            &[],
        )
    }

    pub fn handover(&mut self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.contract.clone(),
            &ExecuteMsg::HandOver {},
            &[],
        )
    }

    pub fn execute(&mut self, sender: &Addr, msg: CosmosMsg<TgradeMsg>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.contract.clone(),
            &ExecuteMsg::Execute { msgs: vec![msg] },
            &[],
        )
    }

    pub fn token_info(&self) -> Result<TokenInfoResponse, ContractError> {
        let resp: TokenInfoResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::TokenInfo {})?;
        Ok(resp)
    }

    fn is_handed_over(&self) -> Result<IsHandedOverResponse, ContractError> {
        let resp: IsHandedOverResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.contract.clone(), &QueryMsg::IsHandedOver {})?;
        Ok(resp)
    }

    pub fn assert_is_handed_over(&self, is_handed_over: bool) {
        assert_eq!(
            self.is_handed_over().unwrap(),
            IsHandedOverResponse { is_handed_over }
        );
    }
}
