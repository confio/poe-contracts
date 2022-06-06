use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::msg::OperatorResponse;
use crate::multitest::helpers::addr_to_pubkey;
use crate::multitest::suite::{Suite, SuiteBuilder};
use crate::state::{
    Config, EpochInfo, SlashingResponse, StartHeightResponse, ValidatorInfo, ValidatorSlashing,
    ValsetState,
};
use cosmwasm_std::{coin, Addr, Decimal};
use cw2::ContractVersion;
use tg4::Tg4Contract;
use tg_utils::Duration;

#[test]
fn export_contains_all_state() {
    let mut suite: Suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .with_operators(&["member1"])
        .build();

    // state snapshot
    let orig_state = suite.dump_raw_valset_state();

    // export and import into new contract
    let exp = suite.export().unwrap();
    let mut suite = SuiteBuilder::new().build();
    suite.import(exp).unwrap();

    // state snapshot
    let new_state = suite.dump_raw_valset_state();

    // compare two snapshots
    assert_eq!(orig_state, new_state);
}

#[test]
fn export_works() {
    let mut suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .with_operators(&["member1"])
        .build();

    let exp = suite.export().unwrap();

    assert_eq!(
        exp.contract_version,
        ContractVersion {
            contract: CONTRACT_NAME.to_owned(),
            version: CONTRACT_VERSION.to_owned(),
        }
    );

    assert_eq!(exp.admin, Some(Addr::unchecked("admin")));

    assert_eq!(
        exp.config,
        Config {
            membership: Tg4Contract(suite.membership.clone()),
            min_points: 3,
            max_validators: 6,
            scaling: None,
            epoch_reward: coin(100, "usdc"),
            fee_percentage: Default::default(),
            auto_unjail: false,
            double_sign_slash_ratio: Decimal::percent(50),
            distribution_contracts: vec![],
            validator_group: suite.validator_group.clone(),
            verify_validators: false,
            offline_jail_duration: Duration::new(0)
        }
    );

    assert_eq!(
        exp.epoch,
        EpochInfo {
            epoch_length: 100,
            current_epoch: suite.epoch().unwrap().current_epoch,
            last_update_time: 0,
            last_update_height: 0,
        }
    );

    assert_eq!(exp.operators.len(), 1);
    assert_eq!(exp.operators[0].operator, "member1");
    assert!(!exp.operators[0].active_validator);

    assert!(exp.validators.is_empty());

    assert!(exp.validators_start_height.is_empty());

    assert!(exp.validators_slashing.is_empty());
}

#[test]
fn import_works() {
    let member_addr = "reallylongaddresstofit32charact1";
    let mut suite = SuiteBuilder::new().build();

    let imp = ValsetState {
        contract_version: ContractVersion {
            contract: "contract_name".to_owned(),
            version: "version".to_owned(),
        },
        admin: Some(Addr::unchecked("imported_admin")),
        config: Config {
            membership: Tg4Contract(Addr::unchecked("membership")),
            min_points: 30,
            max_validators: 60,
            scaling: None,
            epoch_reward: coin(200, "usdc"),
            fee_percentage: Default::default(),
            auto_unjail: true,
            double_sign_slash_ratio: Decimal::percent(100),
            distribution_contracts: vec![],
            validator_group: Addr::unchecked("validator_group"),
            verify_validators: true,
            offline_jail_duration: Duration::new(86400),
        },
        epoch: EpochInfo {
            epoch_length: 1000,
            current_epoch: 1234,
            last_update_time: 1,
            last_update_height: 2,
        },
        operators: vec![OperatorResponse {
            operator: member_addr.to_owned(),
            pubkey: addr_to_pubkey(member_addr),
            metadata: Default::default(),
            active_validator: false,
            jailed_until: None,
        }],
        validators: vec![ValidatorInfo {
            validator_pubkey: addr_to_pubkey(member_addr),
            operator: Addr::unchecked(member_addr),
            power: 10,
        }],
        validators_start_height: vec![StartHeightResponse {
            validator: member_addr.to_owned(),
            height: 1234,
        }],
        validators_slashing: vec![SlashingResponse {
            validator: member_addr.to_owned(),
            slashing: vec![ValidatorSlashing {
                slash_height: 1234,
                portion: Decimal::percent(25),
            }],
        }],
    };

    suite.import(imp.clone()).unwrap();

    let exp = suite.export().unwrap();

    assert_eq!(imp, exp);
}

#[test]
fn import_deletes_existing_entries() {
    let member_addr_ori = "reallylongaddresstofit32charact1";
    let member_addr_new = "reallylongaddresstofit32charact2";

    let mut suite = SuiteBuilder::new()
        .with_operators(&[member_addr_ori])
        .build();

    let imp = ValsetState {
        contract_version: ContractVersion {
            contract: "contract_name".to_owned(),
            version: "version".to_owned(),
        },
        admin: None,
        config: Config {
            membership: Tg4Contract(Addr::unchecked("membership")),
            min_points: 30,
            max_validators: 60,
            scaling: None,
            epoch_reward: coin(200, "usdc"),
            fee_percentage: Default::default(),
            auto_unjail: true,
            double_sign_slash_ratio: Decimal::percent(100),
            distribution_contracts: vec![],
            validator_group: Addr::unchecked("validator_group"),
            verify_validators: true,
            offline_jail_duration: Duration::new(86400),
        },
        epoch: EpochInfo {
            epoch_length: 1000,
            current_epoch: 1234,
            last_update_time: 1,
            last_update_height: 2,
        },
        operators: vec![OperatorResponse {
            operator: member_addr_new.to_owned(),
            pubkey: addr_to_pubkey(member_addr_new),
            metadata: Default::default(),
            active_validator: false,
            jailed_until: None,
        }],
        validators: vec![],
        validators_start_height: vec![],
        validators_slashing: vec![],
    };

    suite.import(imp.clone()).unwrap();

    let exp = suite.export().unwrap();

    assert_eq!(imp, exp);
}
