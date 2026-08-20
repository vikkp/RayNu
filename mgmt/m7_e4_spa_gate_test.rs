use super::run_m7_e4_spa_gate;

#[test]
fn m7_e4_spa_gate_passes() {
    assert!(run_m7_e4_spa_gate(), "E4 SPA VMLAUNCH wiring must hold");
}
