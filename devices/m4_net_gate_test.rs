use super::*;
use crate::devices::virtio_net::M4_NET_OK_MARKER;

#[test]
fn marker_stable() {
    assert_eq!(M4_NET_OK_MARKER, "RAYNU-V-M4-NET-OK");
}

#[test]
fn m4_net_gate_passes() {
    assert!(run_m4_net_gate(), "M4.4 net gate failed");
}
