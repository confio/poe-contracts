mod suite;

use crate::error::ContractError;
use suite::SuiteBuilder;

use cosmwasm_std::{coin, coins, BankMsg, CosmosMsg, Uint128};
use cw_multi_test::Executor;
use tg_bindings::TgradeMsg;

#[test]
fn all_initial_tokens_frozen_and_unfrozen() {
    let initial_amount = Uint128::new(100);
    let mut suite = SuiteBuilder::new()
        .with_tokens(initial_amount.u128())
        .build();

    let oversight = suite.oversight.clone();

    // passing None as amount will freeze all available tokens
    suite.freeze_tokens(&oversight, None).unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.initial, initial_amount);
    assert_eq!(token_info.frozen, initial_amount);

    // passing None as amount will unfreeze all available tokens
    suite.unfreeze_tokens(&oversight, None).unwrap();
    let token_info = suite.token_info().unwrap();
    assert_eq!(token_info.frozen, Uint128::zero());
}

mod release_tokens {
    use super::*;

    #[test]
    fn discrete() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            .with_vesting_plan_in_seconds_from_start(None, 100)
            .build();

        let operator = suite.operator.clone();

        suite.app.advance_seconds(150);
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, token_info.initial);
    }

    #[test]
    fn discrete_vesting_account_with_frozen_tokens_release() {
        let release_at_seconds = 1000u64;
        let mut suite = SuiteBuilder::new()
            .with_tokens(10000)
            .with_vesting_plan_in_seconds_from_start(None, release_at_seconds)
            .build();

        let oversight = suite.oversight.clone();
        let operator = suite.operator.clone();

        // freeze half of available tokens
        suite.freeze_tokens(&oversight, 5000).unwrap();

        // advance time to allow release
        suite.app.advance_seconds(release_at_seconds);

        // release all available tokens
        suite.release_tokens(&operator, None).unwrap();

        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, Uint128::new(5000));
        assert_eq!(token_info.released, Uint128::new(5000));

        // unfreeze and release some tokens
        suite.unfreeze_tokens(&oversight, 2500).unwrap();
        suite.release_tokens(&operator, 1000).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, Uint128::new(2500));
        assert_eq!(token_info.released, Uint128::new(6000));

        // try to release more token then available
        // 10000 initial - 2500 still frozen - 6000 released = 1500 available
        let err = suite.release_tokens(&operator, 2000).unwrap_err();
        assert_eq!(
            ContractError::NotEnoughTokensAvailable,
            err.downcast().unwrap()
        );

        // unfreeze and release all tokens
        suite.unfreeze_tokens(&oversight, None).unwrap();
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, Uint128::zero());
        assert_eq!(token_info.released, token_info.initial);
    }

    #[test]
    fn discrete_after_expiration() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            .with_vesting_plan_in_seconds_from_start(None, 100)
            .build();

        let operator = suite.operator.clone();

        // 1 second after release_at expire
        suite.app.advance_seconds(101);

        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::new(100));
    }

    #[test]
    fn continuous() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            // plan starts 100s from genesis block and ends after additional 200s
            .with_vesting_plan_in_seconds_from_start(Some(100), 300)
            .build();

        let operator = suite.operator.clone();

        // 50 seconds after start, another 150 towards end
        suite.app.advance_seconds(150);
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        // 100 * (50 / 200) = 25
        assert_eq!(token_info.released, Uint128::new(25));

        // 108 seconds after start, another 92 towards end
        suite.app.advance_seconds(58);
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        // 100 * (108 / 200) = 54
        assert_eq!(token_info.released, Uint128::new(54));

        // 199 seconds after start, 1 towards end
        suite.app.advance_seconds(91);
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        // 100 * (199 / 200) = 99.5
        assert_eq!(token_info.released, Uint128::new(99));

        // 200 seconds after start - end_at timestamp is met
        suite.app.advance_seconds(1);
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, token_info.initial);
    }

    #[test]
    fn continuous_after_expiration() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            // plan starts 100s from genesis block and ends after additional 200s
            .with_vesting_plan_in_seconds_from_start(Some(100), 300)
            .build();

        let operator = suite.operator.clone();

        // 1 second after release_at expire
        suite.app.advance_seconds(301);

        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::new(100));
    }

    #[test]
    fn continuous_vesting_account_releasing_over_year() {
        let expected_month_release = 10000;
        let month_in_seconds = 60 * 60 * 24 * 30;
        let mut suite = SuiteBuilder::new()
            .with_tokens(expected_month_release * 12)
            .with_vesting_plan_in_seconds_from_start(Some(0), month_in_seconds * 12)
            .build();

        let operator = suite.operator.clone();

        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::zero());

        // advance time a month
        suite.app.advance_seconds(month_in_seconds + 1);
        for m in 1..13 {
            // release all available tokens
            suite.release_tokens(&operator, None).unwrap();

            let token_info = suite.token_info().unwrap();
            // linear release of available tokens each month
            assert_eq!(
                token_info.released,
                Uint128::new(m * expected_month_release)
            );
            suite.app.advance_seconds(month_in_seconds);
        }

        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, token_info.initial);
    }

    // example from readme
    #[test]
    fn continuously_with_frozen_tokens() {
        let month_in_seconds = 60 * 60 * 24 * 30;
        let mut suite = SuiteBuilder::new()
            // 12 months schedule, total 400.000 tokens.
            .with_tokens(400_000)
            .with_vesting_plan_in_seconds_from_start(Some(0), month_in_seconds * 12)
            .build();

        let operator = suite.operator.clone();
        let oversight = suite.oversight.clone();

        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::zero());

        // Month 2: Accidentally send 50.000 tokens to the contract, but they don't affect schedule.
        suite.app.advance_seconds(month_in_seconds * 2);
        // mint extra 50_000 tokens
        let accidental_transfer = 50_000;
        suite.mint_tokens(accidental_transfer).unwrap();
        suite
            .app
            .execute(
                suite.owner.clone(),
                BankMsg::Send {
                    to_address: suite.contract.to_string(),
                    amount: coins(accidental_transfer, suite.denom.clone()),
                }
                .into(),
            )
            .unwrap();

        // Month 3: 100.000 are released. (all that were vested from original 400.000)
        suite.app.advance_seconds(month_in_seconds);
        let first_release = 100_000;
        suite.release_tokens(&operator, first_release).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::new(first_release));
        // Prove that no more tokens can be released
        let err = suite.release_tokens(&operator, 10).unwrap_err();
        assert_eq!(
            ContractError::NotEnoughTokensAvailable,
            err.downcast().unwrap()
        );

        // Month 5: freeze 200.000 for misbehaviour
        suite.app.advance_seconds(month_in_seconds * 2);
        suite.freeze_tokens(&oversight, 200_000).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, Uint128::new(200_000));

        // Month 6: No tokens can be released (200.000 - 100.000 - 200.000)
        suite.app.advance_seconds(month_in_seconds);
        let err = suite.release_tokens(&operator, None).unwrap_err();
        assert_eq!(ContractError::ZeroTokensNotAllowed, err.downcast().unwrap());

        // Month 10: 25.000 tokens are released (out of 333.333 - 100.000 - 200.000 = 33.333)
        suite.app.advance_seconds(month_in_seconds * 4);
        let second_release = 25_000;
        suite.release_tokens(&operator, second_release).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(
            token_info.released,
            Uint128::new(first_release + second_release)
        );

        // Month 12: All remaining tokens are released, that is Balance of 325.000 - 200.000 frozen = 125.000
        // (this is the 75.000 that finished vesting and extra 50.000 sent by accident)
        suite.app.advance_seconds(month_in_seconds * 2);
        // None releases all awailable
        suite.release_tokens(&operator, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(
            token_info.released,
            Uint128::new(400_000 - 200_000 + 50_000)
        );
    }

    #[test]
    fn continuously_with_negative_amount_results_in_zero_released() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            .with_vesting_plan_in_seconds_from_start(Some(100), 300)
            .build();

        let operator = suite.operator.clone();
        let oversight = suite.oversight.clone();

        suite.freeze_tokens(&oversight, 10).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, Uint128::new(10));

        // 5 seconds after start
        // 2 tokens are allowed to release, but we have 10 tokens frozen
        // without proper protection allowed amount could return negative value (-8)
        // In that case, zero tokens are released
        suite.app.advance_seconds(105);
        let err = suite.release_tokens(&operator, 2).unwrap_err();
        assert_eq!(
            ContractError::NotEnoughTokensAvailable,
            err.downcast().unwrap()
        );
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::zero());
    }
}

mod handover {
    use super::*;

    #[test]
    fn with_tokens_burned() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            .with_vesting_plan_in_seconds_from_start(None, 100)
            .build();

        let oversight = suite.oversight.clone();
        let recipient = suite.recipient.clone();

        suite.freeze_tokens(&oversight, None).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, token_info.initial);
        assert_eq!(token_info.balance, Uint128::new(100));
        suite.assert_is_handed_over(false);

        suite.app.advance_seconds(101);
        suite.handover(&recipient).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.frozen, Uint128::zero());
        assert_eq!(token_info.balance, Uint128::zero());
        suite.assert_is_handed_over(true);
    }

    #[test]
    fn with_message_executed() {
        let mut suite = SuiteBuilder::new()
            .with_tokens(100)
            .with_vesting_plan_in_seconds_from_start(None, 100)
            .build();

        let oversight = suite.oversight.clone();
        let recipient = suite.recipient.clone();

        // send bank transfer from recipient to oversight
        // using vesting account as a proxy account
        let transfer_msg = CosmosMsg::<TgradeMsg>::Bank(BankMsg::Send {
            to_address: oversight.to_string(),
            amount: vec![coin(50, suite.denom.clone())],
        });

        // falstart execute will fail
        let err = suite.execute(&recipient, transfer_msg.clone()).unwrap_err();
        assert_eq!(ContractError::HandOverNotCompleted, err.downcast().unwrap());

        suite.app.advance_seconds(101);
        // release tokens after completing hand over
        suite.handover(&recipient).unwrap();
        // use recipient instead of oversight as a sender,
        // because after hand over he's becoming a new oversight
        suite.release_tokens(&recipient, 25).unwrap();
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.released, Uint128::new(25));
        assert_eq!(token_info.balance, Uint128::new(75));
        suite.assert_is_handed_over(true);

        suite.execute(&recipient, transfer_msg).unwrap();
        // after transfer, balance should be 75 - 50 = 25
        let token_info = suite.token_info().unwrap();
        assert_eq!(token_info.balance, Uint128::new(25));

        // querry oversight's account balance to confirm he received tokens
        let balance = suite
            .app
            .wrap()
            .query_balance(oversight.as_str(), token_info.denom)
            .unwrap();
        assert_eq!(balance.amount, Uint128::new(50));
    }
}
