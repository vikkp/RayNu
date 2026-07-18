use super::*;

#[test]
fn fail_closed() {
    assert_eq!(
        check_msr(0x10, MsrAccess::Read),
        FirewallDecision::Allow
    );
    assert_eq!(
        check_msr(0x1b, MsrAccess::Write),
        FirewallDecision::Block
    );
}
