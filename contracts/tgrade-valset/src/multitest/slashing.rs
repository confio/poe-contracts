use cosmwasm_std::{coin, Decimal};
use cw_controllers::AdminError;

use super::suite::SuiteBuilder;
use crate::error::ContractError;

#[test]
fn admin_can_slash() {
    let actors = vec!["member1", "member2", "member3"];

    let engagement = vec![actors[0], actors[1]];
    let members = vec![actors[0], actors[2]];

    let mut suite = SuiteBuilder::new()
        .with_engagement(&[(members[0], 20), (members[1], 10)])
        .with_operators(&members)
        .with_epoch_reward(coin(3000, "usdc"))
        .with_distribution(
            Decimal::percent(50),
            &[(engagement[0], 20), (engagement[1], 10)],
            None,
        )
        .build();

    let admin = suite.admin().to_owned();

    // Confirm there are no slashing events for actors[0]
    let slashing = suite.list_validator_slashing(actors[0]).unwrap();
    assert_eq!(slashing.addr, actors[0]);
    assert_eq!(slashing.start_height, 1);
    assert_eq!(slashing.slashing.len(), 0);
    assert!(!slashing.tombstoned);

    // Slash him
    suite
        .slash(&admin, actors[0], Decimal::percent(50))
        .unwrap();

    // Confirm slashing event
    let slashing = suite.list_validator_slashing(actors[0]).unwrap();
    assert_eq!(slashing.addr, actors[0]);
    assert_eq!(slashing.start_height, 1);
    assert_eq!(slashing.slashing.len(), 1);
    let actor0_slash = &slashing.slashing[0];
    assert_eq!(actor0_slash.slash_height, 1);
    assert_eq!(actor0_slash.portion, Decimal::percent(50));

    // First epoch. Rewards are not slashed yet, but validators and their weights should be
    // recalculated
    suite.advance_epoch().unwrap();

    suite
        .withdraw_distribution_reward(engagement[0], 0)
        .unwrap();
    suite
        .withdraw_distribution_reward(engagement[1], 0)
        .unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    assert_eq!(suite.token_balance(actors[0]).unwrap(), 2000);
    assert_eq!(suite.token_balance(actors[1]).unwrap(), 500);
    assert_eq!(suite.token_balance(actors[2]).unwrap(), 500);

    // Second epoch. Rewards are sum from previous epoch + slashed rewards from second epoch. Only
    // validation rewards are slashed here (so rewards distribution is affected), the engagement
    // contract stays unchanged
    suite.advance_epoch().unwrap();

    suite
        .withdraw_distribution_reward(engagement[0], 0)
        .unwrap();
    suite
        .withdraw_distribution_reward(engagement[1], 0)
        .unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    assert_eq!(suite.token_balance(actors[0]).unwrap(), 3750);
    assert_eq!(suite.token_balance(actors[1]).unwrap(), 1000);
    assert_eq!(suite.token_balance(actors[2]).unwrap(), 1250);
}

#[test]
fn non_admin_cant_slash() {
    let actors = vec!["member1", "member2", "member3", "member4"];

    let engagement = vec![actors[0], actors[1]];
    let members = vec![actors[0], actors[2]];

    let mut suite = SuiteBuilder::new()
        .with_engagement(&[(members[0], 20), (members[1], 10)])
        .with_operators(&members)
        .with_epoch_reward(coin(3000, "usdc"))
        .with_distribution(
            Decimal::percent(50),
            &[(engagement[0], 20), (engagement[1], 10)],
            None,
        )
        .build();

    let err = suite
        .slash(actors[3], actors[0], Decimal::percent(50))
        .unwrap_err();

    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap()
    );

    // Confirm not a slashing event
    let slashing = suite.list_validator_slashing(actors[0]).unwrap();
    assert_eq!(slashing.addr, actors[0]);
    assert_eq!(slashing.start_height, 1);
    assert_eq!(slashing.slashing.len(), 0);

    // Going two epochs to ensure validators recalculation after slashing. No distributions shall
    // be affected.
    suite.advance_epoch().unwrap();
    suite.advance_epoch().unwrap();

    suite
        .withdraw_distribution_reward(engagement[0], 0)
        .unwrap();
    suite
        .withdraw_distribution_reward(engagement[1], 0)
        .unwrap();
    suite.withdraw_validation_reward(members[0]).unwrap();
    suite.withdraw_validation_reward(members[1]).unwrap();

    assert_eq!(suite.token_balance(actors[0]).unwrap(), 4000);
    assert_eq!(suite.token_balance(actors[1]).unwrap(), 1000);
    assert_eq!(suite.token_balance(actors[2]).unwrap(), 1000);
}

#[test]
fn non_validator_query_fails() {
    let actors = vec!["member1", "member2", "member3", "member4"];

    let members = vec![actors[0], actors[2]];

    let suite = SuiteBuilder::new()
        .with_engagement(&[(members[0], 20), (members[1], 10)])
        .with_operators(&members)
        .build();

    // Confirm not a valid query for a non-validator
    let slashing = suite.list_validator_slashing(actors[1]).unwrap_err();
    assert!(slashing
        .to_string()
        .contains(&format!("Never a validator: {}", actors[1])));
}
