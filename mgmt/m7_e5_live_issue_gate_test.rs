use super::{
    live_issue_surface_present, prop_live_issue_after_priv_vmcs, prop_live_issue_rejects,
    prop_rest_live_issue, run_m7_e5_live_issue_gate, E5_LIVE_ISSUE_RESIDUAL_NOTE,
    M7_E5_LIVE_ISSUE_OK_MARKER,
};

#[test]
fn m7_e5_live_issue_gate_passes() {
    assert_eq!(M7_E5_LIVE_ISSUE_OK_MARKER, "RAYNU-V-M7-E5-LIVE-ISSUE-OK");
    assert!(E5_LIVE_ISSUE_RESIDUAL_NOTE.contains("UnsupportedOnFirmware"));
    assert!(E5_LIVE_ISSUE_RESIDUAL_NOTE.contains("not a shipped OVMF.fd"));
    assert!(E5_LIVE_ISSUE_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_LIVE_ISSUE_RESIDUAL_NOTE.contains("live EPT is not written"));
    assert!(prop_live_issue_after_priv_vmcs());
    assert!(prop_live_issue_rejects());
    assert!(prop_rest_live_issue());
    assert!(live_issue_surface_present());
    assert!(
        run_m7_e5_live_issue_gate(),
        "E5 Stage 22 live-ESP VMLAUNCH issue path must hold"
    );
}
