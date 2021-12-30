mod hackatom;
mod proposals;
mod suite;

use crate::error::ContractError;
use suite::{get_proposal_id, SuiteBuilder};

use cosmwasm_std::Decimal;
use cw3::Status;
use tg_test_utils::RulesBuilder;

#[test]
fn migrate_contract() {
    let members = vec!["owner", "voter1"];

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 2)
        .with_group_member(members[1], 1)
        .with_voting_rules(rules)
        .build();

    let validator_contract = suite.contract.clone();
    let owner = suite.owner.clone();

    let hack1 = suite.app.store_code(hackatom::contract());
    let hack2 = suite.app.store_code(hackatom::contract());

    let beneficiary = "beneficiary";
    let new_beneficiary = "new_beneficiary";

    // Instantiate hackatom contract with Validator Contract as an admin
    let hackatom_contract =
        suite.instantiate_hackatom_contract(validator_contract, hack1, beneficiary);

    let res = suite
        .query_contract_code_id(hackatom_contract.clone())
        .unwrap();
    assert_eq!(res, hack1);
    let res = suite.query_beneficiary(hackatom_contract.clone()).unwrap();
    assert_eq!(res, beneficiary.to_owned());

    // Propose hackatom migration; "owner" is a sender of message with voting power 2 (66%)
    let proposal = suite
        .propose_migrate_hackatom(
            owner.clone(),
            hackatom_contract.clone(),
            new_beneficiary,
            hack2,
        )
        .unwrap();
    let proposal_id: u64 = get_proposal_id(&proposal).unwrap();

    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Passed);

    suite.execute(owner.as_str(), proposal_id).unwrap();
    let proposal_status = suite.query_proposal_status(proposal_id).unwrap();
    assert_eq!(proposal_status, Status::Executed);

    // Confirm contract changed from hack1 to hack2
    let res = suite
        .query_contract_code_id(hackatom_contract.clone())
        .unwrap();
    assert_eq!(res, hack2);
    let res = suite.query_beneficiary(hackatom_contract).unwrap();
    assert_eq!(res, new_beneficiary.to_owned());
}

#[test]
fn propose_migration_to_not_properly_owned_contract() {
    let members = vec!["owner", "voter1"];

    let rules = RulesBuilder::new()
        .with_threshold(Decimal::percent(50))
        .build();

    let mut suite = SuiteBuilder::new()
        .with_group_member(members[0], 2)
        .with_group_member(members[1], 1)
        .with_voting_rules(rules)
        .build();

    let owner = suite.owner.clone();

    let hack = suite.app.store_code(hackatom::contract());

    // Instantiate hackatom contract with "owner" as an admin
    let hackatom_contract = suite.instantiate_hackatom_contract(owner.clone(), hack, "beneficiary");

    let err = suite
        .propose_migrate_hackatom(owner, hackatom_contract, "new_beneficiary", 666)
        .unwrap_err();
    assert_eq!(
        ContractError::Unauthorized(
            "Validator Proposal contract is not an admin of contract proposed to migrate"
                .to_owned()
        ),
        err.downcast().unwrap()
    );
}
