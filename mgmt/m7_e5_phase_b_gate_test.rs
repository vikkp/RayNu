use super::{
    phase_b_surface_present, run_m7_phase_b_spa_wire_gate, M7_PHASE_B_SPA_WIRE_GATE_MARKER,
    PHASE_B_RESIDUAL_NOTE,
};

#[test]
fn m7_phase_b_spa_wire_gate_passes() {
    assert_eq!(
        M7_PHASE_B_SPA_WIRE_GATE_MARKER,
        "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK"
    );
    assert!(PHASE_B_RESIDUAL_NOTE.contains("CLOSED"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("f72b4276"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("iso=0"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("skips OVMF"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("G0 BAR/shell"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("ISO-INSTALL-OK"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("curl: (7)"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("TSC Instant"));
    assert!(phase_b_surface_present());
    assert!(
        run_m7_phase_b_spa_wire_gate(),
        "Phase B SPA→RayNu-F host wire must hold (iron closed f72b4276; not ISO-INSTALL-OK from host/CI)"
    );
    println!("{M7_PHASE_B_SPA_WIRE_GATE_MARKER}");
}
