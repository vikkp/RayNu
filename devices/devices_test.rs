use super::*;

#[test]
fn serial_listed() {
    assert!(supported_kinds().contains(&DeviceKind::Serial));
}
