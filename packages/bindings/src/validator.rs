use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::convert::TryFrom;
use std::convert::TryInto;

use sha2::{Digest, Sha256};

use cosmwasm_std::Binary;

/// This is returned by most queries from Tendermint
/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L336-L340
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Validator {
    // The first 20 bytes of SHA256(public key)
    pub address: Binary,
    pub power: u64,
}

/// This is used to update the validator set
/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L343-L346
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct ValidatorUpdate {
    /// This is the pubkey used in Tendermint consensus
    pub pubkey: Pubkey,
    /// This is the voting power in the consensus rounds
    pub power: u64,
}

/// This is taken from BeginBlock.LastCommitInfo
/// See https://github.com/tendermint/tendermint/blob/v0.34.8/proto/tendermint/abci/types.proto#L348-L352
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct ValidatorVote {
    // The first 20 bytes of SHA256(public key)
    pub address: Binary,
    pub power: u64,
    pub voted: bool,
}

/// A Tendermint validator pubkey.
///
/// See https://github.com/tendermint/tendermint/blob/master/proto/tendermint/crypto/keys.proto for
/// a list of available types. Sr25519 is added here as it is likely to join the party.
///
/// This type is optimized for the JSON interface. No data validation on the enum cases is performed.
/// If you don't trust the data source, you can create a `ValidatedPubkey` enum that mirrors this
/// type and uses fixed sized data fields.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, JsonSchema, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Pubkey {
    /// 32 bytes Ed25519 pubkey
    Ed25519(Binary),
    /// Must use 33 bytes 0x02/0x03 prefixed compressed pubkey format
    Secp256k1(Binary),
    /// 32 bytes Sr25519 pubkey
    Sr25519(Binary),
}

// TODO: check if we can derive this automatically when Binary implements PartialOrd.
impl PartialOrd for Pubkey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// TODO: check if we can derive this automatically when Binary implements Ord.
impl Ord for Pubkey {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Sort by case name, value
            (Pubkey::Ed25519(a), Pubkey::Ed25519(b)) => a.0.cmp(&b.0),
            (Pubkey::Ed25519(_), Pubkey::Secp256k1(_)) => Ordering::Less,
            (Pubkey::Ed25519(_), Pubkey::Sr25519(_)) => Ordering::Less,
            (Pubkey::Secp256k1(_), Pubkey::Ed25519(_)) => Ordering::Greater,
            (Pubkey::Secp256k1(a), Pubkey::Secp256k1(b)) => a.0.cmp(&b.0),
            (Pubkey::Secp256k1(_), Pubkey::Sr25519(_)) => Ordering::Less,
            (Pubkey::Sr25519(_), Pubkey::Ed25519(_)) => Ordering::Greater,
            (Pubkey::Sr25519(_), Pubkey::Secp256k1(_)) => Ordering::Greater,
            (Pubkey::Sr25519(a), Pubkey::Sr25519(b)) => a.0.cmp(&b.0),
        }
    }
}

/// An Ed25519 public key.
///
/// This type is known to have the correct length, which serves as a minimal validation. This
/// does not mean it is a valid curve point though.
///
/// Similar types `struct Secp256k1Pubkey([u8; 33])` and `struct Sr25519Pubkey([u8; 32])`
/// should be created on demand.
///
/// ## Examples
///
/// This is generated from `Pubkey` as follows:
///
/// ```
/// # use hex_literal::hex;
/// use std::convert::TryFrom;
/// use tg_bindings::{Ed25519Pubkey, Pubkey};
///
/// let pubkey = Pubkey::Ed25519(hex!("14253d61ef42d166d02e68d540d07fdf8d65a9af0acaa46302688e788a8521e2").into());
/// let ed25519_pubkey = Ed25519Pubkey::try_from(pubkey);
/// assert!(ed25519_pubkey.is_ok());
///
/// let pubkey = Pubkey::Secp256k1(hex!("0292a066ec32d37c607519d7a86eb2107013a26b160ce3da732ee76e9b2e502492").into());
/// let ed25519_pubkey = Ed25519Pubkey::try_from(pubkey);
/// assert!(ed25519_pubkey.is_err());
/// ```
///
/// When we have an [Ed25519Pubkey] we can derive an address:
///
/// ```
/// # use hex_literal::hex;
/// use std::convert::TryFrom;
/// use tg_bindings::{Ed25519Pubkey, Pubkey, ToAddress};
///
/// let pubkey = Pubkey::Ed25519(hex!("14253d61ef42d166d02e68d540d07fdf8d65a9af0acaa46302688e788a8521e2").into());
/// let ed25519_pubkey = Ed25519Pubkey::try_from(pubkey).unwrap();
/// let address = ed25519_pubkey.to_address();
/// assert_eq!(address, hex!("0CDA3F47EF3C4906693B170EF650EB968C5F4B2C"));
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
pub struct Ed25519Pubkey([u8; 32]);

impl Ed25519Pubkey {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Returns the base64 encoded raw pubkey data.
    pub fn to_base64(&self) -> String {
        base64::encode(self.0)
    }
}

impl ToAddress for Ed25519Pubkey {
    fn to_address(&self) -> [u8; 20] {
        let hash = Sha256::digest(&self.0);
        hash[0..20].try_into().unwrap()
    }
}

pub trait ToAddress {
    fn to_address(&self) -> [u8; 20];
}

impl From<Ed25519Pubkey> for Pubkey {
    fn from(ed: Ed25519Pubkey) -> Self {
        Pubkey::Ed25519(ed.0.into())
    }
}

#[derive(Debug)]
pub enum Ed25519PubkeyConversionError {
    WrongType,
    InvalidDataLength,
}

impl<'a> TryFrom<&'a Pubkey> for Ed25519Pubkey {
    type Error = Ed25519PubkeyConversionError;

    fn try_from(pubkey: &'a Pubkey) -> Result<Self, Self::Error> {
        match pubkey {
            Pubkey::Ed25519(data) => {
                let data: [u8; 32] = data
                    .as_slice()
                    .try_into()
                    .map_err(|_| Ed25519PubkeyConversionError::InvalidDataLength)?;
                Ok(Ed25519Pubkey(data))
            }
            _ => Err(Ed25519PubkeyConversionError::WrongType),
        }
    }
}

impl TryFrom<Pubkey> for Ed25519Pubkey {
    type Error = Ed25519PubkeyConversionError;

    fn try_from(pubkey: Pubkey) -> Result<Self, Self::Error> {
        Ed25519Pubkey::try_from(&pubkey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    #[test]
    fn pubkey_implements_ord() {
        let ed_a = Pubkey::Ed25519(b"abc".into());
        let ed_b = Pubkey::Ed25519(b"bcd".into());
        let se_a = Pubkey::Secp256k1(b"abc".into());
        let se_b = Pubkey::Secp256k1(b"bcd".into());
        let sr_a = Pubkey::Sr25519(b"abc".into());
        let sr_b = Pubkey::Sr25519(b"bcd".into());
        assert!(ed_a < ed_b);
        assert!(se_a < se_b);
        assert!(sr_a < sr_b);

        assert!(ed_a < se_a);
        assert!(se_a < sr_a);
        assert!(ed_a < sr_a);
    }

    #[test]
    fn ed25519pubkey_address() {
        // Test values from https://github.com/informalsystems/tendermint-rs/blob/v0.18.1/tendermint/src/account.rs#L153-L192

        // Ed25519
        let pubkey = Ed25519Pubkey(hex!(
            "14253D61EF42D166D02E68D540D07FDF8D65A9AF0ACAA46302688E788A8521E2"
        ));
        let address = pubkey.to_address();
        assert_eq!(address, hex!("0CDA3F47EF3C4906693B170EF650EB968C5F4B2C"))
    }
}
