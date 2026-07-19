//! M3.19 host verification gate (no IRQ4; IRQ0 only until SHELL).
//!
//! Pillar: [V]
//! Proven Core: companion to `vmx/launch.rs` (boot path emits the marker).
//!
//! Latitude showed that dropping **both** ISA software injects stalls Linux
//! (APIC calibrate needs IRQ0 jiffies; `console=ttyS0` needs IRQ4 TX). M3.19:
//! - removes IRQ4 / COM1 TX inject;
//! - keeps IRQ0 inject **only until** `guest_shell_ok()`;
//! - omits `console=ttyS0` from `REAL_LINUX_CMDLINE` (earlyprintk only).
//!
//! Runtime QEMU/Latitude gate is `tools/qemu-boot-test.sh`.

/// Host / serial marker when the M3.19 NOIRQ policy gate passes.
pub const M3_NOIRQ_OK_MARKER: &str = "RAYNU-V-M3-NOIRQ-OK";

/// True when launch.rs matches the M3.19 IRQ policy.
pub fn irq_crutches_removed() -> bool {
    let s = include_str!("launch.rs");
    !s.contains("try_inject_linux_com1_tx")
        && !s.contains("LINUX_IRQ4_VECTOR")
        && s.contains("try_inject_linux_irq0")
        && s.contains("LINUX_IRQ0_VECTOR")
        && s.contains("guest_shell_ok()")
        && s.contains("note_shell_cpuid")
        && s.contains(M3_NOIRQ_OK_MARKER)
        && s.contains("try_inject_guest_apic_timer")
}

/// True when `REAL_LINUX_CMDLINE` keeps earlyprintk and omits `console=ttyS0`.
pub fn real_cmdline_earlyprintk_only() -> bool {
    let s = include_str!("../guest/linux_boot.rs");
    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("pub const REAL_LINUX_CMDLINE") && t.contains("rdinit=/init") {
            return t.contains("earlyprintk=serial,ttyS0") && !t.contains("console=ttyS0");
        }
    }
    false
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
    irq_crutches_removed()
        && real_cmdline_earlyprintk_only()
        && shell_cpuid_latch_present()
        && noirq_boot_scripts_present()
}

#[cfg(test)]
#[path = "noirq_gate_test.rs"]
mod noirq_gate_test;
