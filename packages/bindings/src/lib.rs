mod gov;
mod hooks;
mod msg;
mod query;
mod sudo;
mod validator;

pub use gov::{GovProposal, ParamChange, ProtoAny};
pub use hooks::{request_privileges, Privilege, PrivilegeMsg};
pub use msg::{BlockParams, ConsensusParams, EvidenceParams, TgradeMsg};
pub use query::{ListPrivilegedResponse, TgradeQuery, ValidatorVoteResponse};
pub use sudo::{Evidence, EvidenceType, PrivilegeChangeMsg, TgradeSudoMsg, ValidatorDiff};
pub use validator::{
    Ed25519Pubkey, Ed25519PubkeyConversionError, Pubkey, ToAddress, Validator, ValidatorUpdate,
    ValidatorVote,
};

// This is a signal, such that any contract that imports these helpers will only run on the tgrade
// blockchain
#[no_mangle]
extern "C" fn requires_tgrade() {}
