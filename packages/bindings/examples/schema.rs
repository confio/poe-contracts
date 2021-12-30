use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use tg_bindings::{
    ListPrivilegedResponse, Privilege, TgradeMsg, TgradeQuery, TgradeSudoMsg, ValidatorDiff,
    ValidatorVoteResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(TgradeMsg), &out_dir);
    export_schema(&schema_for!(TgradeQuery), &out_dir);
    export_schema(&schema_for!(TgradeSudoMsg), &out_dir);
    export_schema(&schema_for!(ListPrivilegedResponse), &out_dir);
    export_schema(&schema_for!(Privilege), &out_dir);
    export_schema(&schema_for!(ValidatorVoteResponse), &out_dir);
    export_schema(&schema_for!(ValidatorDiff), &out_dir);
}
