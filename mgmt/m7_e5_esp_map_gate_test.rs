use super::{
    esp_map_surface_present, prop_esp_map_after_launch, prop_esp_map_rejects, prop_rest_esp_map,
    run_m7_e5_esp_map_gate, E5_ESP_MAP_RESIDUAL_NOTE, M7_E5_ESP_MAP_OK_MARKER,
};

#[test]
fn m7_e5_esp_map_gate_passes() {
    assert_eq!(M7_E5_ESP_MAP_OK_MARKER, "RAYNU-V-M7-E5-ESP-MAP-OK");
    assert!(E5_ESP_MAP_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_ESP_MAP_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_ESP_MAP_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(prop_esp_map_after_launch());
    assert!(prop_esp_map_rejects());
    assert!(prop_rest_esp_map());
    assert!(esp_map_surface_present());
    assert!(
        run_m7_e5_esp_map_gate(),
        "E5 Stage 13 live ESP map must hold"
    );
}
