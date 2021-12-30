use crate::error::ContractError;
use crate::msg::JailingPeriod;

use super::helpers::{assert_active_validators, assert_operators, members_init};
use super::suite::SuiteBuilder;
use cw_controllers::AdminError;
use tg_utils::{Duration, Expiration, JailingDuration};

#[test]
fn only_admin_can_jail() {
    let members = vec!["member1", "member2", "member3", "member4"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .build();
    let admin = suite.admin().to_owned();

    // Admin can jail forever
    suite
        .jail(&admin, members[1], JailingDuration::Forever {})
        .unwrap();

    // Validator jailed forever is also marked as tombstoned
    let slashing = suite.list_validator_slashing(members[1]).unwrap();
    assert!(slashing.tombstoned);
    assert_eq!(slashing.jailed_until, None);

    // Admin can jail for particular duration
    suite.jail(&admin, members[2], Duration::new(3600)).unwrap();

    let slashing = suite.list_validator_slashing(members[2]).unwrap();
    assert!(!slashing.tombstoned);
    assert_eq!(
        slashing.jailed_until,
        Some(Expiration::at_timestamp(
            suite.block_info().time.plus_seconds(3600)
        ))
    );

    let jailed_until = JailingPeriod::Until(Duration::new(3600).after(&suite.app().block_info()));

    // Non-admin cannot jail forever
    let err = suite
        .jail(members[0], members[2], JailingDuration::Forever {})
        .unwrap_err();

    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );

    // Non-admin cannot jail for any duration
    let err = suite
        .jail(members[0], members[2], Duration::new(3600))
        .unwrap_err();

    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );

    let slashing = suite.list_validator_slashing(members[3]).unwrap();
    assert_eq!(slashing.jailed_until, None);

    // Just verify validators are actually jailed in the process
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0], None),
            (members[1], Some(JailingPeriod::Forever {})),
            (members[2], Some(jailed_until)),
            (members[3], None),
        ],
    )
}

#[test]
fn admin_can_unjail_almost_anyone() {
    let members = vec!["member1", "member2", "member3", "member4"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .build();
    let admin = suite.admin().to_owned();

    // Jailing some operators to have someone to unjail
    suite
        .jail(&admin, members[1], JailingDuration::Forever {})
        .unwrap();
    suite.jail(&admin, members[2], Duration::new(3600)).unwrap();

    suite.next_block().unwrap();

    // Admin can't unjail if jailing period is set to forever
    let err = suite.unjail(&admin, members[1]).unwrap_err();
    assert_eq!(
        ContractError::UnjailFromJailForeverForbidden {},
        err.downcast().unwrap()
    );

    // But can unjail if time was finite and expired
    suite.unjail(&admin, members[2]).unwrap();
    // Admin can also unjail someone who is not even jailed - it does nothing, but doesn't
    // fail
    suite.unjail(&admin, members[3]).unwrap();

    // Verify everyone is unjailed at the end
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0], None),
            (members[1], Some(JailingPeriod::Forever {})),
            (members[2], None),
            (members[3], None),
        ],
    )
}

#[test]
fn anyone_can_unjail_self_after_period() {
    let members = vec!["member1", "member2", "member3", "member4"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .build();
    let admin = suite.admin().to_owned();

    // Jail some operators to have someone to unjail in tests
    suite.jail(&admin, members[0], Duration::new(3600)).unwrap();
    suite.jail(&admin, members[1], Duration::new(3600)).unwrap();
    suite.jail(&admin, members[2], Duration::new(3600)).unwrap();

    let jailed_until = JailingPeriod::Until(Duration::new(3600).after(&suite.app().block_info()));

    // Move a little bit forward, so some time passed, but not eough for any jailing to
    // expire
    suite.next_block().unwrap();

    // I cannot unjail myself before expiration...
    let err = suite.unjail(members[0], None).unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );

    // ...even directly pointing myself
    let err = suite.unjail(members[0], members[0]).unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );

    // And I cannot unjail anyone else
    let err = suite.unjail(members[0], members[1]).unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );

    // This time go seriously into future, so jail doors become open
    suite.advance_seconds(3800).unwrap();

    // I can unjail myself without without passing operator directly
    suite.unjail(members[0], None).unwrap();

    // But I still cannot unjail my dear friend
    let err = suite.unjail(members[0], members[1]).unwrap_err();
    assert_eq!(
        ContractError::AdminError(AdminError::NotAdmin {}),
        err.downcast().unwrap(),
    );

    // However he can do it himself, also passing operator directly
    suite.unjail(members[2], members[2]).unwrap();

    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0], None),
            (members[1], Some(jailed_until)),
            (members[2], None),
            (members[3], None),
        ],
    )
}

#[test]
fn jailed_validators_are_ignored_on_selection() {
    let members = vec!["member1", "member2", "member3", "member4"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .build();
    let admin = suite.admin().to_owned();

    // Jailing operators as test prerequirements
    suite.jail(&admin, members[0], Duration::new(3600)).unwrap();
    suite.jail(&admin, members[1], Duration::new(7200)).unwrap();

    // Move forward a bit
    suite.next_block().unwrap();

    // Only unjailed validators are selected
    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[2], 5), (members[3], 8)],
    );

    // Moving forward so jailing periods expired
    suite.advance_seconds(4000).unwrap();
    // But validators are still not selected, as they have to be unjailed
    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[2], 5), (members[3], 8)],
    );

    // Unjailed operator is taken into the account
    suite.unjail(&admin, members[0]).unwrap();
    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[0], 2), (members[2], 5), (members[3], 8)],
    );

    // Unjailed operator is taken into account even if jailing period didn't expire
    suite.unjail(&admin, members[1]).unwrap();
    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[
            (members[0], 2),
            (members[1], 3),
            (members[2], 5),
            (members[3], 8),
        ],
    );
}

#[test]
fn auto_unjail() {
    // Non-standard config: auto unjail is enabled
    let members = vec!["member1", "member2", "member3", "member4"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .with_auto_unjail()
        .build();

    let admin = suite.admin().to_owned();

    let jailed_until = JailingPeriod::Until(Duration::new(3600).after(&suite.app().block_info()));

    // Jailing some operators to begin with
    suite.jail(&admin, members[0], Duration::new(3600)).unwrap();
    suite
        .jail(&admin, members[1], JailingDuration::Forever {})
        .unwrap();

    // Move forward a little, but not enough for jailing to expire
    suite.next_block().unwrap();

    // Operators are jailed...
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0], Some(jailed_until)),
            (members[1], Some(JailingPeriod::Forever {})),
            (members[2], None),
            (members[3], None),
        ],
    );

    // ...and not taken into account on simulation
    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[2], 5), (members[3], 8)],
    );

    // Now moving forward to pass the validation expiration point
    suite.advance_seconds(4000).unwrap();

    // Jailed operator is automatically considered free...
    assert_operators(
        &suite.list_validators(None, None).unwrap(),
        &[
            (members[0], None),
            (members[1], Some(JailingPeriod::Forever {})),
            (members[2], None),
            (members[3], None),
        ],
    );

    // ...and returned in simulation
    assert_active_validators(
        &suite.simulate_active_validators().unwrap(),
        &[(members[0], 2), (members[2], 5), (members[3], 8)],
    );
}

#[test]
fn enb_block_ignores_jailed_validators() {
    let members = vec!["member1", "member2", "member3", "member4"];
    let mut suite = SuiteBuilder::new()
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .with_operators(&members)
        .build();

    let admin = suite.admin().to_owned();

    // Jailing some operators to begin with
    suite.jail(&admin, members[0], Duration::new(3600)).unwrap();
    suite
        .jail(&admin, members[1], JailingDuration::Forever {})
        .unwrap();

    suite.advance_epoch().unwrap();

    assert_active_validators(
        &suite.list_active_validators().unwrap(),
        &[(members[2], 5), (members[3], 8)],
    );
}
