use super::{from_mock_topology, prop_mock_numa_runtime, M5_NUMA_OK_MARKER, NUMA_L3_GAP_NOTE};

#[test]
fn mock_numa_runtime_well_formed() {
    assert!(prop_mock_numa_runtime());
    let t = from_mock_topology().expect("mock NUMA");
    assert!(t.well_formed());
    assert_eq!(t.frame_node(0), Some(0));
    assert_eq!(t.frame_node(100), Some(1));
    assert!(NUMA_L3_GAP_NOTE.contains("M6"));
    assert_eq!(M5_NUMA_OK_MARKER, "RAYNU-V-M5-NUMA-OK");
}
