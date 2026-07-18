use super::*;

#[test]
fn starts_defined() {
    assert_eq!(initial_lifecycle(), VmLifecycle::Defined);
}
