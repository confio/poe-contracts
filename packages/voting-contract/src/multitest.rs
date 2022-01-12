use suite::SuiteBuilder;

mod closing;
mod contracts;
mod early_end;
mod group_change;
mod proposing;
mod queries;
mod suite;
mod voting;

#[test]
fn simple_instantiate() {
    SuiteBuilder::new().build();
}
