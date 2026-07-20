use super::{
    from_mock_topology, prop_mock_numa_runtime, prop_numa_affinity_l3, M5_NUMA_OK_MARKER,
    M6_NUMA_L3_OK_MARKER, NUMA_L3_GAP_CLOSED, NUMA_L3_GAP_NOTE,
};

#[test]
fn mock_numa_runtime_well_formed() {
    assert!(prop_mock_numa_runtime());
    let t = from_mock_topology().expect("mock NUMA");
    assert!(t.well_formed());
    assert_eq!(t.frame_node(0), Some(0));
    assert_eq!(t.frame_node(100), Some(1));
    assert!(NUMA_L3_GAP_NOTE.contains("M6") || NUMA_L3_GAP_CLOSED.contains("M6.2"));
    assert_eq!(M5_NUMA_OK_MARKER, "RAYNU-V-M5-NUMA-OK");
}

#[test]
fn numa_affinity_l3_prop_holds() {
    assert!(prop_numa_affinity_l3());
    assert_eq!(M6_NUMA_L3_OK_MARKER, "RAYNU-V-M6-NUMA-L3-OK");
}
