use super::*;

#[test]
fn confines_cross_guest() {
    assert!(validate_ipi_target(1, 1, 0).is_ok());
    assert_eq!(
        validate_ipi_target(1, 2, 0),
        Err(IpiError::TargetNotOwned)
    );
}
