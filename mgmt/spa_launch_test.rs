use super::{prop_spa_start_queues, M7_E4_SPA_LAUNCH_OK_MARKER};

#[test]
fn spa_start_queues_token() {
    assert!(prop_spa_start_queues());
    assert_eq!(M7_E4_SPA_LAUNCH_OK_MARKER, "RAYNU-V-M7-E4-SPA-LAUNCH-OK");
}
