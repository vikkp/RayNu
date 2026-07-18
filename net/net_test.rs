use super::*;

#[test]
fn vswitch_ports() {
    assert_eq!(VSwitch::new(8).port_count(), 8);
}
