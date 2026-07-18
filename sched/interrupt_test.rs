use super::*;

#[test]
fn rejects_out_of_range() {
    assert_eq!(validate_vector(256), Err(InjectError::InvalidVector));
    assert_eq!(validate_vector(0x80).unwrap(), 0x80);
}
