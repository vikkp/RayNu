use super::prop_coexist_wired;

#[test]
fn m7_8_host_nic_coexist_wired() {
    assert!(
        prop_coexist_wired(),
        "Phase F: bounded_poll beside credit scheduler (no VMXOFF-then-listen)"
    );
}
