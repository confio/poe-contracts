use suite::SuiteBuilder;

mod contracts;
mod suite;

#[test]
fn simple_instantiate() {
    SuiteBuilder::new().build();
}
