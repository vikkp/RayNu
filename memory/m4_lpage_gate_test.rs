use super::*;

#[test]
fn m4_8_lpage_gate_passes() {
    assert!(
        ept_model_opts_into_verus(),
        "ept_model must set package.metadata.verus.verify = true"
    );
    assert!(
        ept_model_has_lpage_spec(),
        "ept_model must contain GhostPageSize + large_map_enabled + marker"
    );
    assert!(
        ept_spec_closes_lpage_todo(),
        "ept_spec/ept_proof must close large-page TODO/GAP (spec side)"
    );
    assert!(
        lpage_spec_scripts_present(),
        "tools/verus-lpage-spec-smoke.sh missing or incomplete"
    );
    assert!(
        prop_large_span_hpa_exclusive(),
        "live EptMap must reject HPA sharing across a multi-frame span"
    );
    assert!(
        prop_range_2m_no_overlap(),
        "EptRangeMap must reject overlapping 2MiB claims"
    );
    assert!(run_m4_lpage_gate(), "M4.8 large-page-spec gate failed");
    assert_eq!(M4_LPAGE_OK_MARKER, "RAYNU-V-M4-LPAGE-OK");
    println!("{M4_LPAGE_OK_MARKER}");
}
