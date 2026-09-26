//! M8.7 host gate. Prints the host marker only.

use super::{
    perc_surface_present, run_m8_perc_host_gate, M8_PERC_GATE_MARKER, M8_PERC_LUN_OK_MARKER,
};

#[test]
fn m8_perc_host_gate_passes() {
    assert!(perc_surface_present());
    assert!(run_m8_perc_host_gate());
    assert_eq!(M8_PERC_GATE_MARKER, "RAYNU-V-M8-PERC-HOST-OK");
    assert_ne!(M8_PERC_GATE_MARKER, M8_PERC_LUN_OK_MARKER);
    println!("{M8_PERC_GATE_MARKER}");
}
