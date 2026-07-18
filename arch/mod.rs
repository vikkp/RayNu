//! x86 / Dell R640-specific helpers.
//!
//! Pillar: [D]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

/// Stub for future CPUID / SMBIOS vendor logging (Tier 1, ADR-005).
pub fn log_cpu_vendor_stub() {
    // Intentionally empty at M0 scaffold — no CPUID yet.
}

/// Placeholder NUMA / socket count until ACPI MADT parsing lands.
pub fn socket_count_stub() -> u32 {
    1
}

#[cfg(test)]
#[path = "arch_test.rs"]
mod arch_test;
