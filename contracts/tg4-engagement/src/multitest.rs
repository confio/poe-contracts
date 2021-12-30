mod suite;

use crate::error::ContractError;
use cosmwasm_std::{coin, coins, Decimal, Event};
use suite::{expected_members, SuiteBuilder};
use tg4::Member;
use tg_utils::{Duration, PreauthError};

/// Helper constructor for a member
fn member(addr: &str, weight: u64) -> Member {
    Member {
        addr: addr.to_owned(),
        weight,
    }
}

mod funds_distribution {
    use super::*;

    fn distribution_event(sender: &str, denom: &str, amount: u128) -> Event {
        Event::new("wasm")
            .add_attribute("sender", sender)
            .add_attribute("denom", denom)
            .add_attribute("amount", &amount.to_string())
    }

    #[test]
    fn divisible_amount_distributed() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 400)
            .build();

        let denom = suite.denom.clone();

        let resp = suite
            .distribute_funds(&members[3], None, &coins(400, &denom))
            .unwrap();

        resp.assert_event(&distribution_event(&members[3], &denom, 400));

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 400);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);

        assert_eq!(
            suite.withdrawable_funds(&members[0]).unwrap(),
            coin(50, &denom)
        );
        assert_eq!(
            suite.withdrawable_funds(&members[1]).unwrap(),
            coin(100, &denom)
        );
        assert_eq!(
            suite.withdrawable_funds(&members[2]).unwrap(),
            coin(250, &denom)
        );

        assert_eq!(suite.distributed_funds().unwrap(), coin(400, &denom));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &denom));

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 50);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn divisible_amount_distributed_twice() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1000)
            .build();

        let denom = suite.denom.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &denom))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(400, &denom));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &denom));

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 50);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 600);

        suite
            .distribute_funds(&members[3], None, &coins(600, &denom))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(1000, &denom));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &denom));

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 125);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 625);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn divisible_amount_distributed_twice_accumulated() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1000)
            .build();

        let denom = suite.denom.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &denom))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(400, &denom));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &denom));

        suite
            .distribute_funds(&members[3], None, &coins(600, &denom))
            .unwrap();

        assert_eq!(suite.distributed_funds().unwrap(), coin(1000, &denom));
        assert_eq!(suite.undistributed_funds().unwrap(), coin(0, &denom));

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 125);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 625);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn weight_changed_after_distribution() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1500)
            .build();

        let denom = suite.denom.clone();
        let owner = suite.owner.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &denom))
            .unwrap();

        // Modifying wights to:
        // member[0] => 6
        // member[1] => 0 (removed)
        // member[2] => 5
        // total_weight => 11
        suite
            .modify_members(owner.as_str(), &[(&members[0], 6)], &[&members[1]])
            .unwrap();

        // Ensure funds are withdrawn properly, considering old weights
        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 50);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 1100);

        // Distribute tokens again to ensure distribution considers new weights
        suite
            .distribute_funds(&members[3], None, &coins(1100, &denom))
            .unwrap();

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 650);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 750);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn weight_changed_after_distribution_accumulated() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1500)
            .build();

        let denom = suite.denom.clone();
        let owner = suite.owner.clone();

        suite
            .distribute_funds(&members[3], None, &coins(400, &denom))
            .unwrap();

        // Modifying wights to:
        // member[0] => 6
        // member[1] => 0 (removed)
        // member[2] => 5
        // total_weight => 11
        suite
            .modify_members(owner.as_str(), &[(&members[0], 6)], &[&members[1]])
            .unwrap();

        // Distribute tokens again to ensure distribution considers new weights
        suite
            .distribute_funds(&members[3], None, &coins(1100, &denom))
            .unwrap();

        // Withdraws sums of both distributions, so it works when they were using different weights
        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 650);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 750);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);
    }

    #[test]
    fn distribution_with_leftover() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        // Weights are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 7)
            .with_member(&members[1], 11)
            .with_member(&members[2], 13)
            .with_funds(&members[3], 3100)
            .build();

        let denom = suite.denom.clone();

        suite
            .distribute_funds(&members[3], None, &coins(100, &denom))
            .unwrap();

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 2);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 22);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 35);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 41);

        // Second distribution adding to the first one would actually make it properly divisible,
        // all shares should be properly split
        suite
            .distribute_funds(&members[3], None, &coins(3000, &denom))
            .unwrap();

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 700);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 1100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 1300);
    }

    #[test]
    fn distribution_with_leftover_accumulated() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        // Weights are set to be prime numbers, difficult to distribute over. All are mutually prime
        // with distributed amount
        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 7)
            .with_member(&members[1], 11)
            .with_member(&members[2], 13)
            .with_funds(&members[3], 3100)
            .build();

        let denom = suite.denom.clone();

        suite
            .distribute_funds(&members[3], None, &coins(100, &denom))
            .unwrap();

        // Second distribution adding to the first one would actually make it properly divisible,
        // all shares should be properly split
        suite
            .distribute_funds(&members[3], None, &coins(3000, &denom))
            .unwrap();

        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 700);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 1100);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 1300);
    }

    #[test]
    fn distribution_cross_halflife() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 1)
            .with_member(&members[1], 2)
            .with_member(&members[2], 5)
            .with_funds(&members[3], 1000)
            .with_halflife(Duration::new(100))
            .build();

        let denom = suite.denom.clone();

        // Pre-halflife split, total weights 1 + 2 + 5 = 8
        // members[0], weight 1: 400 * 1 / 8 = 50
        // members[1], weight 2: 400 * 2 / 8 = 100
        // members[2], weight 5: 400 * 5 / 8 = 250
        suite
            .distribute_funds(&members[3], None, &coins(400, &denom))
            .unwrap();

        suite.app.advance_seconds(125);
        suite.app.next_block().unwrap();

        // Post-halflife split, total weights 1 + 1 + 2 = 4
        // members[0], weight 1: 600 * 1 / 4 = 150
        // members[1], weight 1: 600 * 1 / 4 = 150
        // members[2], weight 2: 600 * 2 / 4 = 300
        suite
            .distribute_funds(&members[3], None, &coins(600, &denom))
            .unwrap();

        // Withdrawal of combined splits:
        // members[0]: 50 + 150 = 200
        // members[1]: 100 + 150 = 250
        // members[2]: 250 + 300 = 550
        suite.withdraw_funds(&members[0], None, None).unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();
        suite.withdraw_funds(&members[2], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 200);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 250);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 550);
        assert_eq!(suite.token_balance(&members[3]).unwrap(), 0);

        // Verifying halflife splits
        let mut resp = suite.members().unwrap();
        resp.sort_by_key(|member| member.addr.clone());

        let mut expected =
            expected_members(vec![(&members[0], 1), (&members[1], 1), (&members[2], 2)]);
        expected.sort_by_key(|member| member.addr.clone());

        assert_eq!(resp, expected);
    }

    #[test]
    fn redirecting_withdrawn_funds() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
            "member4".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 4)
            .with_member(&members[1], 6)
            .with_funds(&members[3], 100)
            .build();

        let denom = suite.denom.clone();

        suite
            .distribute_funds(&members[3], None, &coins(100, &denom))
            .unwrap();

        suite
            .withdraw_funds(&members[0], None, members[2].as_str())
            .unwrap();
        suite.withdraw_funds(&members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 60);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 40);
    }

    #[test]
    fn cannot_withdraw_others_funds() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 4)
            .with_member(&members[1], 6)
            .with_funds(&members[2], 100)
            .build();

        let denom = suite.denom.clone();

        suite
            .distribute_funds(&members[2], None, &coins(100, &denom))
            .unwrap();

        let err = suite
            .withdraw_funds(&members[0], members[1].as_str(), None)
            .unwrap_err();

        assert_eq!(
            ContractError::Unauthorized("Sender is neither owner or delegated".to_owned()),
            err.downcast().unwrap()
        );

        suite
            .withdraw_funds(&members[1], members[1].as_str(), None)
            .unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 40);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 60);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 0);
    }

    #[test]
    fn funds_withdrawal_delegation() {
        let members = vec![
            "member1".to_owned(),
            "member2".to_owned(),
            "member3".to_owned(),
        ];

        let mut suite = SuiteBuilder::new()
            .with_member(&members[0], 4)
            .with_member(&members[1], 6)
            .with_funds(&members[2], 100)
            .build();

        let denom = suite.denom.clone();

        assert_eq!(
            suite.delegated(&members[0]).unwrap().as_str(),
            members[0].as_str()
        );
        assert_eq!(
            suite.delegated(&members[1]).unwrap().as_str(),
            members[1].as_str()
        );

        suite
            .distribute_funds(&members[2], None, &coins(100, &denom))
            .unwrap();

        suite.delegate_withdrawal(&members[1], &members[0]).unwrap();

        suite
            .withdraw_funds(&members[0], members[1].as_str(), None)
            .unwrap();

        assert_eq!(
            suite.delegated(&members[0]).unwrap().as_str(),
            members[0].as_str()
        );
        assert_eq!(
            suite.delegated(&members[1]).unwrap().as_str(),
            members[0].as_str()
        );

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 40);
        assert_eq!(suite.token_balance(&members[0]).unwrap(), 60);
        assert_eq!(suite.token_balance(&members[1]).unwrap(), 0);
        assert_eq!(suite.token_balance(&members[2]).unwrap(), 0);
    }

    #[test]
    fn querying_unknown_address() {
        let suite = SuiteBuilder::new().with_denom("usdc").build();

        let resp = suite.withdrawable_funds("unknown").unwrap();
        assert_eq!(resp, coin(0, "usdc"))
    }
}

mod slashing {
    use super::*;

    #[test]
    fn slasher_slashes() {
        // Initialize two members with equal weights of 10. Slash one of members. Ensure proper
        // weights. Perform distribution and withdraw, ensure proper payouts.
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new()
            .with_member(members[0], 10)
            .with_member(members[1], 10)
            .with_funds(members[2], 600)
            .build();

        let admin = suite.owner.clone();
        let denom = suite.denom.clone();

        suite.add_slasher(admin.as_str(), members[2]).unwrap();

        assert!(!suite.is_slasher(members[1]).unwrap());
        assert!(suite.is_slasher(members[2]).unwrap());

        suite
            .slash(members[2], members[0], Decimal::percent(50))
            .unwrap();

        let mut slashed_members = suite.members().unwrap();
        slashed_members.sort_by_key(|member| member.addr.clone());

        assert_eq!(
            slashed_members,
            vec![member(members[0], 5), member(members[1], 10)]
        );

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(members[0]).unwrap(), 200);
        assert_eq!(suite.token_balance(members[1]).unwrap(), 400);
        assert_eq!(suite.token_balance(members[2]).unwrap(), 0);
    }

    #[test]
    fn admin_cant_slash() {
        // Initialize two members with equal weights of 10. Slash one of members. Ensure proper
        // weights. Perform distribution and withdraw, ensure proper payouts.
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new()
            .with_member(members[0], 10)
            .with_member(members[1], 10)
            .with_funds(members[2], 600)
            .build();

        let admin = suite.owner.clone();
        let denom = suite.denom.clone();

        let err = suite
            .slash(admin.as_str(), members[0], Decimal::percent(50))
            .unwrap_err();

        assert_eq!(
            ContractError::Unauthorized("Sender is not on slashers list".to_owned()),
            err.downcast().unwrap()
        );

        let mut slashed_members = suite.members().unwrap();
        slashed_members.sort_by_key(|member| member.addr.clone());

        assert_eq!(
            slashed_members,
            vec![member(members[0], 10), member(members[1], 10)]
        );

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(members[0]).unwrap(), 300);
        assert_eq!(suite.token_balance(members[1]).unwrap(), 300);
        assert_eq!(suite.token_balance(members[2]).unwrap(), 0);
    }

    #[test]
    fn non_slasher_cant_slash() {
        // Initialize two members with equal weights of 10. Slash one of members. Ensure proper
        // weights. Perform distribution and withdraw, ensure proper payouts.
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new()
            .with_member(members[0], 10)
            .with_member(members[1], 10)
            .with_funds(members[2], 600)
            .build();

        let denom = suite.denom.clone();

        let err = suite
            .slash(members[2], members[0], Decimal::percent(50))
            .unwrap_err();

        assert_eq!(
            ContractError::Unauthorized("Sender is not on slashers list".to_owned()),
            err.downcast().unwrap()
        );

        let mut slashed_members = suite.members().unwrap();
        slashed_members.sort_by_key(|member| member.addr.clone());

        assert_eq!(
            slashed_members,
            vec![member(members[0], 10), member(members[1], 10)]
        );

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(members[0]).unwrap(), 300);
        assert_eq!(suite.token_balance(members[1]).unwrap(), 300);
        assert_eq!(suite.token_balance(members[2]).unwrap(), 0);
    }

    #[test]
    fn remove_slasher() {
        // Add then remove slasher by admin. Then ensure that the removed slasher can't slash
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new().with_member(members[0], 10).build();

        let admin = suite.owner.clone();

        suite.add_slasher(admin.as_ref(), members[1]).unwrap();
        suite.add_slasher(admin.as_ref(), members[2]).unwrap();
        assert_eq!(
            suite.list_slashers().unwrap(),
            vec![members[1].to_owned(), members[2].to_owned()]
        );

        suite.remove_slasher(admin.as_ref(), members[1]).unwrap();
        assert_eq!(suite.list_slashers().unwrap(), vec![members[2].to_owned()]);

        suite.remove_slasher(admin.as_ref(), members[2]).unwrap();
        assert!(suite.list_slashers().unwrap().is_empty());

        let err = suite
            .slash(members[1], members[0], Decimal::percent(50))
            .unwrap_err();

        assert_eq!(
            ContractError::Unauthorized("Sender is not on slashers list".to_owned()),
            err.downcast().unwrap()
        );
    }

    #[test]
    fn slasher_removes_himself() {
        // Add then remove slasher by himself. Then ensure that the removed slasher can't slash
        let members = vec!["member1", "member2"];

        let mut suite = SuiteBuilder::new().with_member(members[0], 10).build();

        let admin = suite.owner.clone();

        suite.add_slasher(admin.as_ref(), members[1]).unwrap();
        suite.remove_slasher(members[1], members[1]).unwrap();

        let err = suite
            .slash(members[1], members[0], Decimal::percent(50))
            .unwrap_err();

        assert_eq!(
            ContractError::Unauthorized("Sender is not on slashers list".to_owned()),
            err.downcast().unwrap()
        );
    }

    #[test]
    fn non_admin_cant_add_slasher_without_preauth() {
        // Add then remove slasher by himself. Then ensure that the removed slasher can't slash
        let members = vec!["member1", "member2"];

        let mut suite = SuiteBuilder::new().with_member(members[0], 10).build();

        let err = suite.add_slasher(members[0], members[1]).unwrap_err();

        assert_eq!(
            ContractError::Preauth(PreauthError::NoPreauth {}),
            err.downcast().unwrap()
        );
    }

    #[test]
    fn add_slasher_with_preauth() {
        // Initialize two members with equal weights of 10. Slash one of members. Ensure proper
        // weights. Perform distribution and withdraw, ensure proper payouts.
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new()
            .with_member(members[0], 10)
            .with_member(members[1], 10)
            .with_funds(members[2], 600)
            .with_preaths_slashing(1)
            .build();

        let denom = suite.denom.clone();

        suite.add_slasher(members[2], members[2]).unwrap();

        suite
            .slash(members[2], members[0], Decimal::percent(50))
            .unwrap();

        let mut slashed_members = suite.members().unwrap();
        slashed_members.sort_by_key(|member| member.addr.clone());

        assert_eq!(
            slashed_members,
            vec![member(members[0], 5), member(members[1], 10)]
        );

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(members[0]).unwrap(), 200);
        assert_eq!(suite.token_balance(members[1]).unwrap(), 400);
        assert_eq!(suite.token_balance(members[2]).unwrap(), 0);
    }

    #[test]
    fn cant_remove_other_slasher() {
        // Add then remove slasher by other slasher. Then ensure that the removed slasher can
        // slash still.
        let members = vec!["member1", "member2"];

        let mut suite = SuiteBuilder::new().with_member(members[0], 10).build();

        let admin = suite.owner.clone();

        suite.add_slasher(admin.as_ref(), members[1]).unwrap();
        let err = suite.remove_slasher(members[0], members[1]).unwrap_err();

        assert_eq!(
            ContractError::Unauthorized(
                "Only slasher might remove himself or sender is not an admin".to_owned()
            ),
            err.downcast().unwrap()
        );

        suite
            .slash(members[1], members[0], Decimal::percent(50))
            .unwrap();
    }

    #[test]
    fn slashing_after_distribution() {
        // Perform full tokens distribution and withdrawal. Then slash one member. Perform another
        // full distribution and withdrawal. Ensure all funds are as expected (the second
        // distribution rewards are splitted with aligned weights.
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new()
            .with_member(members[0], 10)
            .with_member(members[1], 10)
            .with_funds(members[2], 1200)
            .build();

        let admin = suite.owner.clone();
        let denom = suite.denom.clone();

        suite.add_slasher(admin.as_str(), members[2]).unwrap();

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        suite
            .slash(members[2], members[0], Decimal::percent(50))
            .unwrap();

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(members[0]).unwrap(), 500);
        assert_eq!(suite.token_balance(members[1]).unwrap(), 700);
        assert_eq!(suite.token_balance(members[2]).unwrap(), 0);
    }

    #[test]
    fn slashing_while_withdrawal_pending() {
        // Perform tokens distribution, but don't withdraw funds. Then slash one member. Perform
        // another distribution, and withdraw all funds. Ensure funds are as expected.
        let members = vec!["member1", "member2", "member3"];

        let mut suite = SuiteBuilder::new()
            .with_member(members[0], 10)
            .with_member(members[1], 10)
            .with_funds(members[2], 1200)
            .build();

        let admin = suite.owner.clone();
        let denom = suite.denom.clone();

        suite.add_slasher(admin.as_str(), members[2]).unwrap();

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite
            .slash(members[2], members[0], Decimal::percent(50))
            .unwrap();

        suite
            .distribute_funds(members[2], None, &coins(600, &denom))
            .unwrap();

        suite.withdraw_funds(members[0], None, None).unwrap();
        suite.withdraw_funds(members[1], None, None).unwrap();

        assert_eq!(suite.token_balance(suite.contract.as_str()).unwrap(), 0);
        assert_eq!(suite.token_balance(members[0]).unwrap(), 500);
        assert_eq!(suite.token_balance(members[1]).unwrap(), 700);
        assert_eq!(suite.token_balance(members[2]).unwrap(), 0);
    }
}
