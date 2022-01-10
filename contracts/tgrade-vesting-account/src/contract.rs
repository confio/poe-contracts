#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, Addr, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    AccountInfoResponse, CanExecuteResponse, ExecuteMsg, InstantiateMsg, IsHandedOverResponse,
    QueryMsg, TokenInfoResponse,
};
use crate::state::{VestingAccount, VestingPlan, VESTING_ACCOUNT};
use tg_bindings::TgradeMsg;

pub type Response = cosmwasm_std::Response<TgradeMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<TgradeMsg>;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vesting-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    create_vesting_account(deps, info, msg)?;
    Ok(Response::default())
}

fn create_vesting_account(
    deps: DepsMut,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<(), ContractError> {
    let initial_tokens = cw_utils::must_pay(&info, msg.denom.as_str())?;
    let account = VestingAccount {
        denom: msg.denom,
        recipient: msg.recipient,
        operator: msg.operator,
        oversight: msg.oversight,
        vesting_plan: msg.vesting_plan,
        frozen_tokens: Uint128::zero(),
        paid_tokens: Uint128::zero(),
        initial_tokens,
        handed_over: false,
    };
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Execute { msgs } => execute_msg(deps, info.sender, msgs),
        ExecuteMsg::ReleaseTokens { amount } => release_tokens(deps, env, info.sender, amount),
        ExecuteMsg::FreezeTokens { amount } => freeze_tokens(deps, info.sender, amount),
        ExecuteMsg::UnfreezeTokens { amount } => unfreeze_tokens(deps, info.sender, amount),
        ExecuteMsg::ChangeOperator { address } => change_operator(deps, info.sender, address),
        ExecuteMsg::HandOver {} => hand_over(deps, env, info.sender),
        _ => Err(ContractError::NotImplemented),
    }
}

fn require_operator(sender: &Addr, account: &VestingAccount) -> Result<(), ContractError> {
    if ![&account.operator, &account.oversight].contains(&sender) {
        return Err(ContractError::RequireOperator);
    };
    Ok(())
}

fn require_oversight(sender: &Addr, account: &VestingAccount) -> Result<(), ContractError> {
    if *sender != account.oversight {
        return Err(ContractError::RequireOversight);
    };
    Ok(())
}

fn require_recipient(sender: &Addr, account: &VestingAccount) -> Result<(), ContractError> {
    if *sender != account.recipient {
        return Err(ContractError::RequireRecipient);
    };
    Ok(())
}

/// Some actions are not available if hand over procedure has been completed
fn hand_over_completed(account: &VestingAccount) -> Result<(), ContractError> {
    if account.handed_over {
        return Err(ContractError::HandOverCompleted);
    }
    Ok(())
}

/// Returns information about amount of tokens that is allowed to be released
fn allowed_release(deps: Deps, env: &Env, plan: &VestingPlan) -> Result<Uint128, ContractError> {
    let token_info = token_info(deps, env)?;

    // In order to allow releasing any extra tokens sent to the account AFTER vesting
    // account has been initialized, correct amount is calculated by doing query of
    // contract's balance.

    match plan {
        VestingPlan::Discrete {
            release_at: release,
        } => {
            if release.is_expired(&env.block) {
                let current = deps
                    .querier
                    .query_balance(&env.contract.address, token_info.denom)?;
                // If end_at timestamp is already met, release all available tokens
                Ok(current.amount - token_info.frozen)
            } else {
                Ok(Uint128::zero())
            }
        }
        VestingPlan::Continuous { start_at, end_at } => {
            if !start_at.is_expired(&env.block) {
                // If start_at timestamp is not met, release nothing
                Ok(Uint128::zero())
            } else if end_at.is_expired(&env.block) {
                let current = deps
                    .querier
                    .query_balance(&env.contract.address, token_info.denom)?;
                // If end_at timestamp is already met, release all available tokens
                Ok(current.amount - token_info.frozen)
            } else {
                // If current timestamp is in between start_at and end_at, relase
                // tokens by linear ratio: tokens * ((current_time - start_time) / (end_time - start_time))
                // and subtract already released or frozen tokens
                Ok((token_info.initial
                    * Decimal::from_ratio(
                        env.block.time.seconds() - start_at.time().seconds(),
                        end_at.time().seconds() - start_at.time().seconds(),
                    ))
                .saturating_sub(token_info.released)
                .saturating_sub(token_info.frozen))
            }
        }
    }
}

fn execute_msg(
    deps: DepsMut,
    sender: Addr,
    msgs: Vec<CosmosMsg<TgradeMsg>>,
) -> Result<Response, ContractError> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;
    if !account.handed_over {
        return Err(ContractError::HandOverNotCompleted);
    }
    require_recipient(&sender, &account)?;

    Ok(Response::new()
        .add_messages(msgs)
        .add_attribute("action", "execute"))
}

fn release_tokens(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    requested_amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;
    require_operator(&sender, &account)?;

    let allowed_to_release = allowed_release(deps.as_ref(), &env, &account.vesting_plan)?;
    let requested_amount = requested_amount.unwrap_or(allowed_to_release);
    if requested_amount > allowed_to_release {
        return Err(ContractError::NotEnoughTokensAvailable);
    };
    helpers::release_tokens(requested_amount, sender, &mut account, deps.storage)
}

fn freeze_tokens(
    deps: DepsMut,
    sender: Addr,
    requested_amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;
    require_oversight(&sender, &account)?;

    let available_to_freeze = account.initial_tokens - account.frozen_tokens - account.paid_tokens;
    if let Some(requested_amount) = requested_amount {
        let final_frozen = std::cmp::min(requested_amount, available_to_freeze);
        helpers::freeze_tokens(final_frozen, sender, &mut account, deps.storage)
    } else {
        helpers::freeze_tokens(available_to_freeze, sender, &mut account, deps.storage)
    }
}

fn unfreeze_tokens(
    deps: DepsMut,
    sender: Addr,
    requested_amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;
    require_oversight(&sender, &account)?;

    if let Some(requested_amount) = requested_amount {
        helpers::unfreeze_tokens(requested_amount, sender, &mut account, deps.storage)
    } else {
        helpers::unfreeze_tokens(account.frozen_tokens, sender, &mut account, deps.storage)
    }
}

fn change_operator(
    deps: DepsMut,
    sender: Addr,
    new_operator: Addr,
) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;
    require_oversight(&sender, &account)?;

    account.operator = new_operator.clone();
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    Ok(Response::new()
        .add_attribute("action", "change_operator")
        .add_attribute("operator", new_operator.to_string())
        .add_attribute("sender", sender))
}

fn hand_over(deps: DepsMut, env: Env, sender: Addr) -> Result<Response, ContractError> {
    let mut account = VESTING_ACCOUNT.load(deps.storage)?;
    hand_over_completed(&account)?;
    if ![&account.recipient, &account.oversight].contains(&&sender) {
        return Err(ContractError::RequireRecipientOrOversight);
    }
    if !account.vesting_plan.is_expired(env.block.time) {
        return Err(ContractError::ContractNotExpired);
    }

    let frozen_tokens = account.frozen_tokens.u128();
    account.frozen_tokens = Uint128::zero();
    account.handed_over = true;
    account.oversight = account.recipient.clone();
    account.operator = account.recipient.clone();
    VESTING_ACCOUNT.save(deps.storage, &account)?;

    let mut response = Response::new()
        .add_attribute("action", "hand_over")
        .add_attribute("sender", sender);

    if frozen_tokens > 0 {
        let msg = BankMsg::Burn {
            amount: coins(frozen_tokens, account.denom),
        };
        response = response
            .add_attribute("burnt_tokens", frozen_tokens.to_string())
            .add_message(msg);
    }
    Ok(response)
}

mod helpers {
    use super::*;
    use cosmwasm_std::Storage;

    pub fn release_tokens(
        amount: Uint128,
        sender: Addr,
        account: &mut VestingAccount,
        storage: &mut dyn Storage,
    ) -> Result<Response, ContractError> {
        amount_not_zero(amount)?;

        account.paid_tokens += amount;
        VESTING_ACCOUNT.save(storage, account)?;

        let msg = BankMsg::Send {
            to_address: account.recipient.to_string(),
            amount: coins(amount.u128(), account.denom.clone()),
        };
        Ok(Response::new()
            .add_attribute("action", "release_tokens")
            .add_attribute("tokens", amount.to_string())
            .add_attribute("sender", sender)
            .add_message(msg))
    }

    pub fn freeze_tokens(
        amount: Uint128,
        sender: Addr,
        account: &mut VestingAccount,
        storage: &mut dyn Storage,
    ) -> Result<Response, ContractError> {
        amount_not_zero(amount)?;

        account.frozen_tokens += amount;
        VESTING_ACCOUNT.save(storage, account)?;

        Ok(Response::new()
            .add_attribute("action", "freeze_tokens")
            .add_attribute("tokens", amount.to_string())
            .add_attribute("sender", sender))
    }

    pub fn unfreeze_tokens(
        amount: Uint128,
        sender: Addr,
        account: &mut VestingAccount,
        storage: &mut dyn Storage,
    ) -> Result<Response, ContractError> {
        amount_not_zero(amount)?;

        // Don't subtract with overflow
        account.frozen_tokens = account.frozen_tokens.saturating_sub(amount);
        VESTING_ACCOUNT.save(storage, account)?;

        Ok(Response::new()
            .add_attribute("action", "unfreeze_tokens")
            .add_attribute("tokens", amount.to_string())
            .add_attribute("sender", sender))
    }

    fn amount_not_zero(amount: Uint128) -> Result<(), ContractError> {
        if amount == Uint128::zero() {
            return Err(ContractError::ZeroTokensNotAllowed);
        };
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::AccountInfo {} => to_binary(&account_info(deps)?),
        QueryMsg::TokenInfo {} => to_binary(&token_info(deps, &env)?),
        QueryMsg::IsHandedOver {} => to_binary(&is_handed_over(deps)?),
        QueryMsg::CanExecute { sender } => to_binary(&can_execute(deps, sender)?),
    }
}

fn account_info(deps: Deps) -> StdResult<AccountInfoResponse> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;

    let info = AccountInfoResponse {
        recipient: account.recipient,
        operator: account.operator,
        oversight: account.oversight,
        vesting_plan: account.vesting_plan,
    };
    Ok(info)
}

fn token_info(deps: Deps, env: &Env) -> StdResult<TokenInfoResponse> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;
    let denom = account.denom;
    let balance = deps
        .querier
        .query_balance(&env.contract.address, denom.clone())?
        .amount;

    let info = TokenInfoResponse {
        denom,
        initial: account.initial_tokens,
        frozen: account.frozen_tokens,
        released: account.paid_tokens,
        balance,
    };
    Ok(info)
}

fn is_handed_over(deps: Deps) -> StdResult<IsHandedOverResponse> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;
    Ok(IsHandedOverResponse {
        is_handed_over: account.handed_over,
    })
}

fn can_execute(deps: Deps, sender: String) -> StdResult<CanExecuteResponse> {
    let account = VESTING_ACCOUNT.load(deps.storage)?;
    if !account.handed_over {
        return Ok(CanExecuteResponse { can_execute: false });
    }
    match require_recipient(&Addr::unchecked(sender), &account) {
        Ok(_) => Ok(CanExecuteResponse { can_execute: true }),
        Err(_) => Ok(CanExecuteResponse { can_execute: false }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_matches::assert_matches;

    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{from_binary, Coin, MessageInfo, OwnedDeps, Timestamp};
    use tg_utils::Expiration;

    const OWNER: &str = "owner";
    const RECIPIENT: &str = "recipient";
    const OPERATOR: &str = "operator";
    const OVERSIGHT: &str = "oversight";

    /// Default timestamp from mock_env() in seconds with 100 seconds added
    const DEFAULT_RELEASE: u64 = 1571797419 + 100;

    const VESTING_DENOM: &str = "vesting";

    struct SuiteBuilder {
        recipient: Addr,
        operator: Addr,
        oversight: Addr,
        vesting_plan: VestingPlan,
        coins: Vec<Coin>,
    }

    impl Default for SuiteBuilder {
        fn default() -> Self {
            Self {
                recipient: Addr::unchecked(RECIPIENT),
                operator: Addr::unchecked(OPERATOR),
                oversight: Addr::unchecked(OVERSIGHT),
                vesting_plan: VestingPlan::Discrete {
                    release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                },
                coins: vec![Coin::new(100, VESTING_DENOM)],
            }
        }
    }

    impl SuiteBuilder {
        fn with_continuous_vesting_plan(mut self, start_at: u64, end_at: u64) -> Self {
            self.vesting_plan = VestingPlan::Continuous {
                start_at: Expiration::at_timestamp(Timestamp::from_seconds(start_at)),
                end_at: Expiration::at_timestamp(Timestamp::from_seconds(end_at)),
            };
            self
        }

        fn build(self) -> Suite {
            let mut deps = mock_dependencies();
            let owner = mock_info(self.recipient.as_str(), &self.coins);

            let instantiate_message = InstantiateMsg {
                denom: self.coins[0].denom.clone(),
                recipient: self.recipient,
                operator: self.operator,
                oversight: self.oversight,
                vesting_plan: self.vesting_plan,
            };

            instantiate(
                deps.as_mut().branch(),
                mock_env(),
                owner,
                instantiate_message,
            )
            .unwrap();

            Suite {
                deps,
                env: mock_env(),
            }
        }
    }

    struct Suite {
        deps: OwnedDeps<MockStorage, MockApi, MockQuerier>,
        env: Env,
    }

    impl Suite {
        fn freeze_tokens(
            &mut self,
            sender: &str,
            amount: Option<u128>,
        ) -> Result<Response, ContractError> {
            execute(
                self.deps.as_mut(),
                self.env.clone(),
                MessageInfo {
                    sender: Addr::unchecked(sender),
                    funds: vec![],
                },
                ExecuteMsg::FreezeTokens {
                    amount: amount.map(Uint128::new),
                },
            )
        }

        fn unfreeze_tokens(
            &mut self,
            sender: &str,
            amount: Option<u128>,
        ) -> Result<Response, ContractError> {
            execute(
                self.deps.as_mut(),
                self.env.clone(),
                MessageInfo {
                    sender: Addr::unchecked(sender),
                    funds: vec![],
                },
                ExecuteMsg::UnfreezeTokens {
                    amount: amount.map(Uint128::new),
                },
            )
        }

        fn release_tokens(
            &mut self,
            sender: &str,
            amount: Option<u128>,
        ) -> Result<Response, ContractError> {
            execute(
                self.deps.as_mut(),
                self.env.clone(),
                MessageInfo {
                    sender: Addr::unchecked(sender),
                    funds: vec![],
                },
                ExecuteMsg::ReleaseTokens {
                    amount: amount.map(Uint128::new),
                },
            )
        }

        fn change_operator(
            &mut self,
            sender: &str,
            new_operator: &str,
        ) -> Result<Response, ContractError> {
            execute(
                self.deps.as_mut(),
                self.env.clone(),
                MessageInfo {
                    sender: Addr::unchecked(sender),
                    funds: vec![],
                },
                ExecuteMsg::ChangeOperator {
                    address: Addr::unchecked(new_operator),
                },
            )
        }

        fn hand_over(&mut self, sender: &str) -> Result<Response, ContractError> {
            execute(
                self.deps.as_mut(),
                self.env.clone(),
                MessageInfo {
                    sender: Addr::unchecked(sender),
                    funds: vec![],
                },
                ExecuteMsg::HandOver {},
            )
        }

        fn query_token_info(&self) -> StdResult<TokenInfoResponse> {
            from_binary(
                &query(self.deps.as_ref(), self.env.clone(), QueryMsg::TokenInfo {}).unwrap(),
            )
        }
    }

    mod query {
        use super::*;

        #[test]
        fn execute() {
            let mut suite = SuiteBuilder::default().build();

            // can't execute before hand over
            assert_eq!(
                can_execute(suite.deps.as_ref(), OVERSIGHT.to_string()),
                Ok(CanExecuteResponse { can_execute: false })
            );

            // hand over has been completed
            suite.env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE);
            // recipient becomes an oversight after hand over
            assert_matches!(suite.hand_over(RECIPIENT), Ok(_));

            assert_eq!(
                // previous oversight from before hand over
                can_execute(suite.deps.as_ref(), OVERSIGHT.to_string()),
                Ok(CanExecuteResponse { can_execute: false })
            );

            assert_eq!(
                can_execute(suite.deps.as_ref(), OPERATOR.to_string()),
                Ok(CanExecuteResponse { can_execute: false })
            );

            assert_eq!(
                can_execute(suite.deps.as_ref(), RECIPIENT.to_string()),
                Ok(CanExecuteResponse { can_execute: true })
            );
        }
    }

    mod unauthorized {
        use super::*;

        #[test]
        fn freeze() {
            let mut suite = SuiteBuilder::default().build();

            assert_matches!(
                suite.freeze_tokens(RECIPIENT, None),
                Err(ContractError::RequireOversight)
            );

            assert_matches!(
                suite.freeze_tokens(OPERATOR, None),
                Err(ContractError::RequireOversight)
            );
        }

        #[test]
        fn unfreeze() {
            let mut suite = SuiteBuilder::default().build();

            assert_matches!(
                suite.unfreeze_tokens(RECIPIENT, Some(50)),
                Err(ContractError::RequireOversight)
            );

            assert_matches!(
                suite.unfreeze_tokens(OPERATOR, Some(50)),
                Err(ContractError::RequireOversight)
            );
        }

        #[test]
        fn change_account_operator() {
            let mut suite = SuiteBuilder::default().build();

            assert_matches!(
                suite.change_operator(RECIPIENT, RECIPIENT),
                Err(ContractError::RequireOversight)
            );

            assert_matches!(
                suite.change_operator(OPERATOR, RECIPIENT),
                Err(ContractError::RequireOversight)
            );
        }

        #[test]
        fn release() {
            let mut suite = SuiteBuilder::default().build();

            assert_matches!(
                suite.release_tokens(RECIPIENT, Some(50)),
                Err(ContractError::RequireOperator)
            );
        }

        #[test]
        fn hand_over() {
            let mut suite = SuiteBuilder::default().build();

            assert_matches!(
                suite.hand_over(OPERATOR),
                Err(ContractError::RequireRecipientOrOversight)
            );
        }
    }

    mod allowed_release {
        use super::*;

        #[test]
        fn discrete_before_expiration() {
            let suite = SuiteBuilder::default().build();

            let account = account_info(suite.deps.as_ref()).unwrap();
            assert_eq!(
                allowed_release(suite.deps.as_ref(), &mock_env(), &account.vesting_plan),
                Ok(Uint128::zero())
            );
        }

        #[test]
        fn continuous_before_expiration() {
            let suite = SuiteBuilder::default()
                .with_continuous_vesting_plan(DEFAULT_RELEASE, DEFAULT_RELEASE + 200)
                .build();

            let account = account_info(suite.deps.as_ref()).unwrap();

            assert_eq!(
                allowed_release(suite.deps.as_ref(), &suite.env, &account.vesting_plan),
                Ok(Uint128::zero())
            );
        }
    }

    mod release_tokens {
        use super::*;

        #[test]
        fn discrete_before_expiration() {
            let mut suite = SuiteBuilder::default().build();

            assert_eq!(
                suite.release_tokens(OPERATOR, Some(25)),
                Err(ContractError::NotEnoughTokensAvailable)
            );
            assert_matches!(
                suite.query_token_info(),
                Ok(TokenInfoResponse {
                    released,
                    ..
                }) if released == Uint128::zero()
            );
        }

        #[test]
        fn continuously_more_then_allowed() {
            let mut suite = SuiteBuilder::default()
                .with_continuous_vesting_plan(DEFAULT_RELEASE, DEFAULT_RELEASE + 200)
                .build();

            // 50 seconds after start, another 150 towards end
            // 25 tokens are allowed to release, but we try to get 30 tokens
            suite.env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE).plus_seconds(50);
            let amount_to_release = 30;
            assert_eq!(
                suite.release_tokens(OPERATOR, Some(amount_to_release)),
                Err(ContractError::NotEnoughTokensAvailable)
            );
            assert_matches!(
                suite.query_token_info(),
                Ok(TokenInfoResponse {
                    released,
                    ..
                }) if released == Uint128::zero()
            );
        }
    }

    #[test]
    fn instantiate_without_tokens() {
        let mut deps = mock_dependencies();
        let owner = mock_info(OWNER, &[]);

        let instantiate_message = InstantiateMsg {
            denom: VESTING_DENOM.to_string(),
            recipient: Addr::unchecked(RECIPIENT),
            operator: Addr::unchecked(OPERATOR),
            oversight: Addr::unchecked(OVERSIGHT),
            vesting_plan: VestingPlan::Discrete {
                release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
            },
        };

        assert_matches!(
            instantiate(
                deps.as_mut().branch(),
                mock_env(),
                owner,
                instantiate_message
            ),
            Err(ContractError::PaymentError(_))
        );
    }

    #[test]
    fn get_account_info() {
        let suite = SuiteBuilder::default().build();

        assert_eq!(
            account_info(suite.deps.as_ref()),
            Ok(AccountInfoResponse {
                recipient: Addr::unchecked(RECIPIENT),
                operator: Addr::unchecked(OPERATOR),
                oversight: Addr::unchecked(OVERSIGHT),
                vesting_plan: VestingPlan::Discrete {
                    release_at: Expiration::at_timestamp(Timestamp::from_seconds(DEFAULT_RELEASE)),
                }
            })
        );
    }

    #[test]
    fn get_token_info() {
        let suite = SuiteBuilder::default().build();

        assert_eq!(
            suite.query_token_info(),
            Ok(TokenInfoResponse {
                denom: VESTING_DENOM.to_string(),
                initial: Uint128::new(100),
                frozen: Uint128::zero(),
                released: Uint128::zero(),
                // because no tokens were actually sent in UT
                balance: Uint128::zero(),
            })
        );
    }

    #[test]
    fn freeze_tokens_success() {
        let mut suite = SuiteBuilder::default().build();

        assert_eq!(
            suite.freeze_tokens(OVERSIGHT, None),
            Ok(Response::new()
                .add_attribute("action", "freeze_tokens")
                .add_attribute("tokens", "100".to_string())
                .add_attribute("sender", Addr::unchecked(OVERSIGHT)))
        );
        let info = suite.query_token_info().unwrap();
        assert_eq!(info.frozen, Uint128::new(100));
    }

    #[test]
    fn freeze_too_many_tokens() {
        let mut suite = SuiteBuilder::default().build();

        assert_eq!(
            // 10 tokens more then instantiated by default
            suite.freeze_tokens(OVERSIGHT, Some(110)),
            Ok(Response::new()
                .add_attribute("action", "freeze_tokens")
                .add_attribute("tokens", "100".to_string())
                .add_attribute("sender", Addr::unchecked(OVERSIGHT)))
        );
        let info = suite.query_token_info().unwrap();
        assert_eq!(info.frozen, Uint128::new(100));
    }

    #[test]
    fn unfreeze_tokens_success() {
        let mut suite = SuiteBuilder::default().build();

        suite.freeze_tokens(OVERSIGHT, Some(50)).unwrap();
        let info = suite.query_token_info().unwrap();
        assert_eq!(info.frozen, Uint128::new(50));
        assert_eq!(
            // passing None will unfreeze all available previously frozen tokens
            suite.unfreeze_tokens(OVERSIGHT, None),
            Ok(Response::new()
                .add_attribute("action", "unfreeze_tokens")
                .add_attribute("tokens", "50".to_string())
                .add_attribute("sender", Addr::unchecked(OVERSIGHT)))
        );
        let info = suite.query_token_info().unwrap();
        assert_eq!(info.frozen, Uint128::zero());
    }

    #[test]
    fn change_account_operator_success() {
        let mut suite = SuiteBuilder::default().build();

        assert_eq!(
            suite.change_operator(OVERSIGHT, RECIPIENT),
            Ok(Response::new()
                .add_attribute("action", "change_operator")
                .add_attribute("operator", RECIPIENT.to_string())
                .add_attribute("sender", OVERSIGHT.to_string()))
        );
        assert_matches!(
            account_info(suite.deps.as_ref()),
            Ok(AccountInfoResponse {
                operator,
                ..
            }) if operator == Addr::unchecked(RECIPIENT)
        );
    }

    mod handover {
        use super::*;

        #[test]
        fn account_is_handed_over() {
            let mut suite = SuiteBuilder::default().build();

            assert_eq!(
                is_handed_over(suite.deps.as_ref()),
                Ok(IsHandedOverResponse {
                    is_handed_over: false
                })
            );

            let tokens_to_burn = 50;
            suite
                .freeze_tokens(OVERSIGHT, Some(tokens_to_burn))
                .unwrap();

            suite.env.block.time = Timestamp::from_seconds(DEFAULT_RELEASE);

            assert_eq!(
                suite.hand_over(OVERSIGHT),
                Ok(Response::new()
                    .add_attribute("action", "hand_over")
                    .add_attribute("sender", OVERSIGHT.to_string())
                    .add_attribute("burnt_tokens", tokens_to_burn.to_string())
                    .add_message(BankMsg::Burn {
                        amount: coins(tokens_to_burn, VESTING_DENOM)
                    }))
            );
            assert_eq!(
                is_handed_over(suite.deps.as_ref()),
                Ok(IsHandedOverResponse {
                    is_handed_over: true
                })
            );
            assert_matches!(
                suite.query_token_info(),
                Ok(TokenInfoResponse {
                    frozen,
                    ..
                }) if frozen == Uint128::zero()
            );
            assert_matches!(
                account_info(suite.deps.as_ref()),
                Ok(AccountInfoResponse {
                    oversight,
                    ..
                }) if oversight == RECIPIENT
            );
        }

        #[test]
        fn before_expire() {
            let mut suite = SuiteBuilder::default().build();

            assert_eq!(
                suite.hand_over(OVERSIGHT),
                Err(ContractError::ContractNotExpired)
            );
        }
    }

    #[test]
    fn zero_tokens_operations_not_allowed() {
        let mut suite = SuiteBuilder::default().build();

        assert_eq!(
            suite.freeze_tokens(OVERSIGHT, Some(0)),
            Err(ContractError::ZeroTokensNotAllowed)
        );
        assert_eq!(
            suite.unfreeze_tokens(OVERSIGHT, Some(0)),
            Err(ContractError::ZeroTokensNotAllowed)
        );
        assert_eq!(
            suite.release_tokens(OVERSIGHT, Some(0)),
            Err(ContractError::ZeroTokensNotAllowed)
        );
    }
}
