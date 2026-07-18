use super::*;

#[test]
fn nop_ok_unknown_rejected() {
    assert_eq!(dispatch(0, 0).unwrap(), 0);
    assert_eq!(dispatch(99, 0), Err(HypercallError::UnknownNr));
}
