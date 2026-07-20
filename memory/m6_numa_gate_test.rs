use super::{
    ept_model_has_numa_l3, ept_spec_closes_numa_l3, numa_l3_scripts_present, run_m6_numa_gate,
    M6_NUMA_L3_GATE_MARKER,
};
use crate::memory::numa::prop_numa_affinity_l3;

#[test]
fn m6_2_numa_l3_gate_passes() {
    assert_eq!(M6_NUMA_L3_GATE_MARKER, "RAYNU-V-M6-NUMA-L3-OK");
    assert!(ept_model_has_numa_l3(), "ept_model must embed M6.2 NUMA L3 artifacts");
    assert!(
        ept_spec_closes_numa_l3(),
        "ept_spec/ept_proof must close NUMA affinity L3 GAP"
    );
    assert!(numa_l3_scripts_present(), "verus-numa-l3-smoke.sh must be present");
    assert!(
        prop_numa_affinity_l3(),
        "host mock affinity policy must hold"
    );
    assert!(run_m6_numa_gate());
    println!("RAYNU-V-M6-NUMA-L3-OK");
}
