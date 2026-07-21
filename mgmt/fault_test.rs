use super::*;

#[test]
fn kill_vcpu_recover() {
    assert!(prop_kill_vcpu_recover());
}

#[test]
fn corrupt_page_fail_closed() {
    assert!(prop_corrupt_page_fail_closed());
}

#[test]
fn drop_irq_fail_closed() {
    assert!(prop_drop_irq_fail_closed());
}

#[test]
fn net_partition_recover() {
    assert!(prop_net_partition_recover());
}

#[test]
fn fault_suite() {
    assert!(prop_fault_suite());
}

#[test]
fn gap_closed_and_marker() {
    assert!(FAULT_GAP_NOTE.contains("CLOSED M6.7"));
    assert_eq!(M6_FAULT_OK_MARKER, "RAYNU-V-M6-FAULT-OK");
}
