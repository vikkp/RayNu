use super::{
    ept_model_has_numa_spec, ept_spec_closes_numa_todo, numa_scripts_present, run_m5_numa_gate,
};
use crate::memory::numa::{prop_mock_numa_runtime, M5_NUMA_OK_MARKER};

#[test]
fn m5_8_numa_gate_passes() {
    assert!(
        ept_model_has_numa_spec(),
        "ept_model must carry GhostNumaTopology / numa_map_enabled / marker"
    );
    assert!(
        ept_spec_closes_numa_todo(),
        "ept_spec/ept_proof must close NUMA TODO/GAP (spec side)"
    );
    assert!(numa_scripts_present(), "verus-numa-smoke.sh must be present");
    assert!(
        prop_mock_numa_runtime(),
        "iDRAC mock SRAT/SLIT must yield well-formed host NUMA"
    );
    assert!(run_m5_numa_gate());
    assert_eq!(M5_NUMA_OK_MARKER, "RAYNU-V-M5-NUMA-OK");
    println!("{M5_NUMA_OK_MARKER}");
}
