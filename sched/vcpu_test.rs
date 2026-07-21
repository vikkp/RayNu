use super::*;

#[test]
fn lifecycle() {
    let mut v = Vcpu::new(0);
    v.make_runnable();
    v.enter_running();
    v.halt();
    assert_eq!(v.state(), VcpuState::Halted);
}

#[test]
fn tear_down_from_running() {
    let mut v = Vcpu::new(1);
    v.make_runnable();
    v.enter_running();
    assert!(v.tear_down().is_ok());
    assert_eq!(v.state(), VcpuState::TornDown);
    assert!(v.tear_down().is_err());
}
