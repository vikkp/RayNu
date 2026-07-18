use super::*;

#[test]
fn lifecycle() {
    let mut v = Vcpu::new(0);
    v.make_runnable();
    v.enter_running();
    v.halt();
    assert_eq!(v.state(), VcpuState::Halted);
}
