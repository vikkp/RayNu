use super::*;
use crate::devices::virtio_blk::M4_BLK_OK_MARKER;

#[test]
fn marker_stable() {
    assert_eq!(M4_BLK_OK_MARKER, "RAYNU-V-M4-BLK-OK");
}

#[test]
fn m4_blk_gate_passes() {
    assert!(run_m4_blk_gate(), "M4.3 blk gate failed");
}
