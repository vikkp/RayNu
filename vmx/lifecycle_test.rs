//! Unit tests + future Kani targets for VMX lifecycle.

use super::*;

#[test]
fn enable_then_disable() {
    let mut vmx = VmxLifecycle::new();
    assert_eq!(vmx.state(), VmxState::Off);
    vmx.enable().unwrap();
    assert_eq!(vmx.state(), VmxState::Root);
    vmx.disable().unwrap();
    assert_eq!(vmx.state(), VmxState::Off);
}

#[test]
fn double_enable_rejected() {
    let mut vmx = VmxLifecycle::new();
    vmx.enable().unwrap();
    assert_eq!(vmx.enable(), Err(VmxError::InvalidState));
}

// KANI-TARGET: bounded checks for lifecycle transitions once Kani is in CI.
