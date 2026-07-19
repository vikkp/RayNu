//! M3.19 host verification gate (no ISA IRQ0/IRQ4 software inject).
//!
//! Pillar: [V]
//! Proven Core: companion to `vmx/launch.rs` (boot path emits the marker).
//!
//! Checks in-tree that the Linux early loop no longer contains IRQ0/IRQ4
//! inject helpers and embeds `RAYNU-V-M3-NOIRQ-OK`. Runtime QEMU/Latitude
//! gate is `tools/qemu-boot-test.sh`.

/// Host / serial marker when the M3.19 no-IRQ-crutch gate passes.
pub const M3_NOIRQ_OK_MARKER: &str = "RAYNU-V-M3-NOIRQ-OK";

/// True when launch.rs has dropped ISA IRQ0/IRQ4 software inject.
pub fn irq_crutches_removed() -> bool {
    let s = include_str!("launch.rs");
    !s.contains("try_inject_linux_irq0")
        && !s.contains("try_inject_linux_com1_tx")
        && !s.contains("LINUX_IRQ0_VECTOR")
        && !s.contains("LINUX_IRQ4_VECTOR")
        && s.contains("note_shell_cpuid")
        && s.contains(M3_NOIRQ_OK_MARKER)
        && s.contains("try_inject_guest_apic_timer")
}

/// True when serial_pio latches SHELL from the CPUID hypercall.
pub fn shell_cpuid_latch_present() -> bool {
    let s = include_str!("../devices/serial_pio.rs");
    s.contains("fn note_shell_cpuid")
        && s.contains("SHELL_CPUID_OK")
        && s.contains("SHELL_MAGIC_OK || SHELL_CPUID_OK")
}

/// True when the QEMU gate requires the NOIRQ marker.
pub fn noirq_boot_scripts_present() -> bool {
    let smoke = include_str!("../tools/qemu-boot-test.sh");
    smoke.contains(M3_NOIRQ_OK_MARKER)
        && smoke.contains("M3.19")
        && smoke.contains("MARKER_NOIRQ")
}

/// Full M3.19 artifact gate (does not run QEMU).
pub fn run_noirq_gate() -> bool {
    irq_crutches_removed() && shell_cpuid_latch_present() && noirq_boot_scripts_present()
}

#[cfg(test)]
#[path = "noirq_gate_test.rs"]
mod noirq_gate_test;
