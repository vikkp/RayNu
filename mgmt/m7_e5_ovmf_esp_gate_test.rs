use super::{
    ovmf_esp_surface_present, prop_ovmf_esp_after_probe, prop_ovmf_esp_rejects,
    prop_rest_ovmf_esp, run_m7_e5_ovmf_esp_gate, E5_OVMF_ESP_RESIDUAL_NOTE,
    M7_E5_OVMF_ESP_OK_MARKER,
};

#[test]
fn m7_e5_ovmf_esp_gate_passes() {
    assert_eq!(M7_E5_OVMF_ESP_OK_MARKER, "RAYNU-V-M7-E5-OVMF-ESP-OK");
    assert!(E5_OVMF_ESP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(prop_ovmf_esp_after_probe());
    assert!(prop_ovmf_esp_rejects());
    assert!(prop_rest_ovmf_esp());
    assert!(ovmf_esp_surface_present());
    assert!(
        run_m7_e5_ovmf_esp_gate(),
        "E5 Stage 6 ESP OVMF load must hold"
    );
}
