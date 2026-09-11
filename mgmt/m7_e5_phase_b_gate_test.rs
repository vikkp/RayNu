use super::{
    phase_b_surface_present, run_m7_phase_b_spa_wire_gate, PHASE_B_RESIDUAL_NOTE,
    M7_PHASE_B_SPA_WIRE_GATE_MARKER,
};

#[test]
fn m7_phase_b_spa_wire_gate_passes() {
    assert_eq!(
        M7_PHASE_B_SPA_WIRE_GATE_MARKER,
        "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK"
    );
    assert!(PHASE_B_RESIDUAL_NOTE.contains("not claimed"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("iso=0"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("skips OVMF"));
    assert!(PHASE_B_RESIDUAL_NOTE.contains("ISO-INSTALL-OK"));
    assert!(phase_b_surface_present());
    assert!(
        run_m7_phase_b_spa_wire_gate(),
        "Phase B SPA→RayNu-F host wire must hold (not iron, not ISO-INSTALL-OK)"
    );
    println!("{M7_PHASE_B_SPA_WIRE_GATE_MARKER}");
}
