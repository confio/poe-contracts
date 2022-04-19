use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::multitest::helpers::addr_to_pubkey;
use crate::multitest::suite::SuiteBuilder;
use crate::state::{Config, EpochInfo, OperatorInfo, ValsetState};
use cosmwasm_std::{coin, Addr, Decimal};
use cw2::ContractVersion;
use std::convert::TryFrom;
use tg4::Tg4Contract;
use tg_bindings::Ed25519Pubkey;

#[test]
fn export_works() {
    let mut suite = SuiteBuilder::new()
        .with_max_validators(6)
        .with_min_points(3)
        .with_operators(&vec!["member1"])
        .build();

    let exp = suite.export().unwrap();
    println!("state: {:#?}", exp);

    // Contract version
    assert_eq!(
        exp.contract_version,
        ContractVersion {
            contract: CONTRACT_NAME.to_owned(),
            version: CONTRACT_VERSION.to_owned(),
        }
    );

    // Config
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
        }
    );

    // Epoch
    assert_eq!(
        exp.epoch,
        EpochInfo {
            epoch_length: 100,
            current_epoch: suite.epoch().unwrap().current_epoch,
            last_update_time: 0,
            last_update_height: 0,
        }
    );

    // One operator
    assert_eq!(exp.operators.len(), 1);
    assert_eq!(exp.operators[0].0, "member1");
    assert!(matches!(
        exp.operators[0].1,
        OperatorInfo {
            active_validator: false,
            ..
        }
    ));

    // No validators
    assert!(exp.validators.is_empty());

    // No validators start height
    assert!(exp.validators_start_height.is_empty());

    // No validators slashing height
    assert!(exp.validators_slashing.is_empty());

    // No validators jail
    assert!(exp.validators_jail.is_empty());
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
        },
        epoch: EpochInfo {
            epoch_length: 1000,
            current_epoch: 1234,
            last_update_time: 1,
            last_update_height: 2,
        },
        operators: vec![(
            "".to_owned(),
            OperatorInfo {
                pubkey: Ed25519Pubkey::try_from(addr_to_pubkey(member_addr)).unwrap(),
                metadata: Default::default(),
                active_validator: false,
            },
        )],
        validators: vec![],
        validators_start_height: vec![],
        validators_slashing: vec![],
        validators_jail: vec![],
    };

    suite.import(imp.clone()).unwrap();

    let exp = suite.export().unwrap();

    assert_eq!(imp, exp);
}
