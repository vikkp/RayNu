use super::*;

#[test]
fn m3_19_noirq_gate_passes() {
    assert!(
        irq_crutches_removed(),
        "launch.rs must drop try_inject_linux_irq0 / try_inject_linux_com1_tx"
    );
    assert!(
        shell_cpuid_latch_present(),
        "serial_pio must latch SHELL from CPUID hypercall"
    );
    assert!(
        noirq_boot_scripts_present(),
        "qemu-boot-test.sh must require RAYNU-V-M3-NOIRQ-OK"
    );
    assert!(run_noirq_gate(), "M3.19 NOIRQ gate failed");
    assert_eq!(M3_NOIRQ_OK_MARKER, "RAYNU-V-M3-NOIRQ-OK");
    println!("{M3_NOIRQ_OK_MARKER}");
}
