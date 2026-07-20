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
    assert!(boot_ring_verify_for_test());
}

#[test]
fn ring_rejects_when_full() {
    let mut ring = AuditRing::new();
    assert_eq!(ring.capacity(), AUDIT_RING_CAP);
    for i in 0..AUDIT_RING_CAP {
        ring.append(AuditEvent::FrameAllocated { frame: i as u64 })
            .expect("append within cap");
    }
    assert!(ring
        .append(AuditEvent::FrameAllocated {
            frame: AUDIT_RING_CAP as u64,
        })
        .is_err());
    assert!(ring.verify_chain());
}
