use crate::error::ContractError;

use super::helpers::members_init;
use super::suite::SuiteBuilder;
use anyhow::Result as AnyResult;
use cw_controllers::AdminError;
use cw_multi_test::AppResponse;

fn assert_success(resp: AnyResult<AppResponse>) {
    assert!(resp.is_ok());
}

fn assert_unauthorized(resp: AnyResult<AppResponse>) {
    const NOT_ADMIN_ERR: ContractError = ContractError::AdminError(AdminError::NotAdmin {});

    assert_eq!(NOT_ADMIN_ERR, resp.unwrap_err().downcast().unwrap());
}

#[test]
fn admin_query_works() {
    let suite = SuiteBuilder::new().build();

    assert_eq!(
        Some(suite.admin().to_string()),
        suite.query_admin().unwrap()
    );
}

#[test]
fn admin_can_change_admin() {
    let mut suite = SuiteBuilder::new().build();

    let admin = suite.admin().to_string();
    let new_admin = "asd".to_string();

    assert_success(suite.update_admin(&admin, new_admin.clone()));
    assert_eq!(Some(new_admin), suite.query_admin().unwrap());
}

#[test]
fn admin_can_disable_admin() {
    let mut suite = SuiteBuilder::new().build();

    let admin = suite.admin().to_string();

    assert_success(suite.update_admin(&admin, None));
    assert_eq!(None, suite.query_admin().unwrap());
}

#[test]
fn non_admin_cannot_change_admin() {
    let members = vec!["member1", "member2", "member3", "member4"];

    let mut suite = SuiteBuilder::new()
        .with_operators(&members)
        .with_engagement(&members_init(&members, &[2, 3, 5, 8]))
        .build();

    assert_unauthorized(suite.update_admin("random guy", "asd".to_string()));
    assert_unauthorized(suite.update_admin(members[0], "asd".to_string()));
    assert_eq!(
        Some(suite.admin().to_string()),
        suite.query_admin().unwrap()
    );
}

#[test]
fn admin_cannot_update_admin_twice() {
    let mut suite = SuiteBuilder::new().build();

    let admin = suite.admin().to_string();
    let new_admin = "asd".to_string();

    assert_success(suite.update_admin(&admin, new_admin.clone()));
    assert_unauthorized(suite.update_admin(&admin, new_admin.clone()));
    assert_eq!(Some(new_admin), suite.query_admin().unwrap());
}
