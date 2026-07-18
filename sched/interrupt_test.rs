use super::*;

#[test]
fn rejects_out_of_range() {
    assert_eq!(validate_vector(256), Err(InjectError::InvalidVector));
    assert_eq!(validate_vector(0x80).unwrap(), 0x80);
}

#[test]
fn pack_external_inject() {
    let info = prepare_external_inject(M2_IRQ_VECTOR).unwrap();
    assert_eq!(info & 0xff, M2_IRQ_VECTOR);
    assert_eq!((info >> 8) & 7, INTR_TYPE_EXTERNAL);
    assert_ne!(info & INTR_INFO_VALID, 0);
    assert_eq!(M2_IRQ_OK_MARKER, "RAYNU-V-M2-IRQ-OK");
    assert_eq!(M2_TIMER_OK_MARKER, "RAYNU-V-M2-TIMER-OK");
}

#[test]
fn prepare_rejects_bad_vector() {
    assert_eq!(
        prepare_external_inject(300),
        Err(InjectError::InvalidVector)
    );
}
