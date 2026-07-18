use super::*;

#[test]
fn chain_verifies_after_append() {
    let mut ring = AuditRing::new();
    ring.append(AuditEvent::BootStarted {
        milestone: Milestone::M0,
    })
    .unwrap();
    ring.append(AuditEvent::FrameAllocated { frame: 3 })
        .unwrap();
    assert!(ring.verify_chain());
    assert_eq!(ring.len(), 2);
}

#[test]
fn audit_log_macro_records() {
    let before = boot_ring_len_for_test();
    record_event(AuditEvent::BootStarted {
        milestone: Milestone::M0,
    });
    assert!(boot_ring_len_for_test() > before);
}
