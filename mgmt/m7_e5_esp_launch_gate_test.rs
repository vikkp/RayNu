use super::{
    esp_launch_surface_present, prop_esp_launch_after_edk2, prop_esp_launch_rejects,
    prop_rest_esp_launch, run_m7_e5_esp_launch_gate, E5_ESP_LAUNCH_RESIDUAL_NOTE,
    M7_E5_ESP_LAUNCH_OK_MARKER,
};

#[test]
fn m7_e5_esp_launch_gate_passes() {
    assert_eq!(M7_E5_ESP_LAUNCH_OK_MARKER, "RAYNU-V-M7-E5-ESP-LAUNCH-OK");
    assert!(E5_ESP_LAUNCH_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_ESP_LAUNCH_RESIDUAL_NOTE.contains("no live OVMF.fd"));
    assert!(prop_esp_launch_after_edk2());
    assert!(prop_esp_launch_rejects());
    assert!(prop_rest_esp_launch());
    assert!(esp_launch_surface_present());
    assert!(
        run_m7_e5_esp_launch_gate(),
        "E5 Stage 12 ESP-path VMLAUNCH must hold"
    );
}
