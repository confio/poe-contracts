use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, export_schema_with_title, remove_schemas, schema_for};

pub use tgrade_valset::msg::{
    DistributionMsg, EpochResponse, ExecuteMsg, InstantiateMsg, ListActiveValidatorsResponse,
    ListValidatorResponse, QueryMsg, RewardsDistribution, RewardsInstantiateMsg, ValidatorResponse,
};
pub use tgrade_valset::state::Config;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(InstantiateMsg), &out_dir, "InstantiateMsg");
    export_schema_with_title(&schema_for!(ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(QueryMsg), &out_dir, "QueryMsg");

    export_schema(&schema_for!(EpochResponse), &out_dir);
    export_schema(&schema_for!(ListValidatorResponse), &out_dir);
    export_schema(&schema_for!(ValidatorResponse), &out_dir);
    export_schema(&schema_for!(ListActiveValidatorsResponse), &out_dir);

    export_schema(&schema_for!(DistributionMsg), &out_dir);
    export_schema(&schema_for!(RewardsInstantiateMsg), &out_dir);
    export_schema(&schema_for!(RewardsDistribution), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
}
