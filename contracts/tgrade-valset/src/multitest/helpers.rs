use cosmwasm_std::Binary;
use tg_bindings::Pubkey;

use crate::msg::{JailingPeriod, OperatorResponse, ValidatorMetadata, MAX_METADATA_SIZE};
use crate::state::ValidatorInfo;

// Converts address to valid public key
// Requires addr to be exactly 32 bytes long, panics otherwise
pub fn addr_to_pubkey(addr: &str) -> Pubkey {
    Pubkey::Ed25519(Binary((*addr).as_bytes().to_vec()))
}

pub fn invalid_metadata_short(seed: &str) -> ValidatorMetadata {
    ValidatorMetadata {
        moniker: seed.into(),
        identity: Some(String::new()),
        website: Some(String::new()),
        security_contact: Some(String::new()),
        details: Some(String::new()),
    }
}

pub fn invalid_metadata_long(seed: &str) -> ValidatorMetadata {
    ValidatorMetadata {
        moniker: seed.into(),
        identity: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        website: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        security_contact: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
        details: Some((0..MAX_METADATA_SIZE + 1).map(|_| "X").collect::<String>()),
    }
}

pub fn members_init<'m>(members: &[&'m str], weights: &[u64]) -> Vec<(&'m str, u64)> {
    members
        .iter()
        .zip(weights)
        .map(|(member, weight)| (*member, *weight))
        .collect()
}

/// Utility function for verifying active validators - in tests in most cases is completely ignored,
/// therefore as expected value vector of `(addr, voting_power)` are taken.
/// Also order of operators should not matter, so proper sorting is also handled.
#[track_caller]
pub fn assert_active_validators(received: &[ValidatorInfo], expected: &[(&str, u64)]) {
    let mut received: Vec<_> = received
        .iter()
        .map(|validator| (validator.operator.to_string(), validator.power))
        .collect();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|(addr, weight)| ((*addr).to_owned(), *weight))
        .collect();

    received.sort_unstable_by_key(|(addr, _)| addr.clone());
    expected.sort_unstable_by_key(|(addr, _)| addr.clone());

    assert_eq!(received, expected);
}

/// Utility function for verifying validators - in tests in most cases pubkey and metadata all
/// completely ignored, therefore as expected value vector of `(addr, jailed_until)` are taken.
/// Also order of operators should not matter, so proper sorting is also handled.
#[track_caller]
pub fn assert_operators(received: &[OperatorResponse], expected: &[(&str, Option<JailingPeriod>)]) {
    let mut received: Vec<_> = received
        .iter()
        .cloned()
        .map(|operator| (operator.operator, operator.jailed_until))
        .collect();

    let mut expected: Vec<_> = expected
        .iter()
        .cloned()
        .map(|(addr, jailing)| (addr.to_owned(), jailing))
        .collect();

    received.sort_unstable_by_key(|(addr, _)| addr.clone());
    expected.sort_unstable_by_key(|(addr, _)| addr.clone());

    assert_eq!(received, expected);
}
