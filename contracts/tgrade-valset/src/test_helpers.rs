#![cfg(test)]
use cosmwasm_std::{Addr, Binary};

use tg_bindings::Pubkey;

use crate::msg::{OperatorInitInfo, ValidatorMetadata};
use crate::state::ValidatorInfo;

const ED25519_PUBKEY_LENGTH: usize = 32;

// returns a list of addresses that are set in the tg4-stake contract
pub fn addrs(count: u32) -> Vec<String> {
    (1..=count).map(|x| format!("operator-{:03}", x)).collect()
}

pub fn valid_operator(seed: &str) -> OperatorInitInfo {
    OperatorInitInfo {
        operator: seed.into(),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        metadata: mock_metadata(seed),
    }
}

pub fn invalid_operator() -> OperatorInitInfo {
    OperatorInitInfo {
        operator: "foobar".into(),
        validator_pubkey: Pubkey::Ed25519(b"too-short".into()),
        metadata: mock_metadata(""),
    }
}

pub fn mock_metadata(seed: &str) -> ValidatorMetadata {
    ValidatorMetadata {
        moniker: seed.into(),
        details: Some(format!("I'm really {}", seed)),
        ..ValidatorMetadata::default()
    }
}

pub fn valid_validator(seed: &str, power: u64) -> ValidatorInfo {
    ValidatorInfo {
        operator: Addr::unchecked(seed),
        validator_pubkey: mock_pubkey(seed.as_bytes()),
        metadata: mock_metadata(seed),
        power,
    }
}

// creates a valid pubkey from a seed
pub fn mock_pubkey(base: &[u8]) -> Pubkey {
    let copies = (ED25519_PUBKEY_LENGTH / base.len()) + 1;
    let mut raw = base.repeat(copies);
    raw.truncate(ED25519_PUBKEY_LENGTH);
    Pubkey::Ed25519(Binary(raw))
}
