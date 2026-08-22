use super::{
    prop_real_esp_after_install, prop_real_esp_rejects, prop_rest_real_esp,
    real_esp_surface_present, run_m7_e5_real_esp_gate, E5_REAL_ESP_RESIDUAL_NOTE,
    M7_E5_REAL_ESP_OK_MARKER,
};

#[test]
fn m7_e5_real_esp_gate_passes() {
    assert_eq!(M7_E5_REAL_ESP_OK_MARKER, "RAYNU-V-M7-E5-REAL-ESP-OK");
    assert!(E5_REAL_ESP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_REAL_ESP_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_REAL_ESP_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_REAL_ESP_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_real_esp_after_install());
    assert!(prop_real_esp_rejects());
    assert!(prop_rest_real_esp());
    assert!(real_esp_surface_present());
    assert!(
        run_m7_e5_real_esp_gate(),
        "E5 Stage 18 real-ESP VMLAUNCH-ready contract must hold"
    );
}
