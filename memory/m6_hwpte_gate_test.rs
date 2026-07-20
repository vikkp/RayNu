use super::{
    ept_model_has_hwpte, ept_spec_closes_hwpte, hwpte_scripts_present, run_m6_hwpte_gate,
    M6_HWPTE_OK_MARKER,
};
use crate::memory::ept_hw::prop_hw_pte_identity_correspondence;

#[test]
fn m6_1_hwpte_gate_passes() {
    assert_eq!(M6_HWPTE_OK_MARKER, "RAYNU-V-M6-HWPTE-OK");
    assert!(ept_model_has_hwpte(), "ept_model must embed M6.1 HW PTE artifacts");
    assert!(
        ept_spec_closes_hwpte(),
        "ept_spec/ept_proof must close HW PTE bit-decode GAP"
    );
    assert!(hwpte_scripts_present(), "verus-hwpte-smoke.sh must be present");
    assert!(
        prop_hw_pte_identity_correspondence(),
        "live ept_hw leaf packing must match ghost encode/decode"
    );
    assert!(run_m6_hwpte_gate());
}
