use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};

use tg3::{
    Tg3ExecuteMsg, Tg3QueryMsg, VoteListResponse, VoteResponse, VoterDetail, VoterListResponse,
    VoterResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(Tg3ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(Tg3QueryMsg), &out_dir, "QueryMsg");
    export_schema(&schema_for!(VoteResponse), &out_dir);
    export_schema(&schema_for!(VoteListResponse), &out_dir);
    export_schema(&schema_for!(VoterResponse), &out_dir);
    export_schema(&schema_for!(VoterDetail), &out_dir);
    export_schema(&schema_for!(VoterListResponse), &out_dir);
}
