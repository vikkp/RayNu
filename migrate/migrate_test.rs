use super::*;

#[test]
fn idle_by_default() {
    assert_eq!(status_stub(), MigrateStatus::Idle);
}
