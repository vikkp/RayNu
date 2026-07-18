//! x86 / Dell R640-specific helpers.
//!
//! Pillar: [D] [V]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

pub mod apic;
pub mod cpu;

/// Stub for future SMBIOS vendor logging (Tier 1, ADR-005).
pub fn log_cpu_vendor_stub() {
    // CPUID vendor string logging can land with richer arch bring-up.
}

/// Placeholder NUMA / socket count until ACPI MADT parsing lands.
pub fn socket_count_stub() -> u32 {
    1
}

#[cfg(test)]
#[path = "arch_test.rs"]
mod arch_test;
