//! Hardware VMXON / VMXOFF (Intel SDM Vol. 3C).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0 → L1 asserts at entry/exit
//!
//! Requires identity-mapped physical memory for the VMXON region (true after
//! UEFI handoff on OVMF). Nested VT-x under QEMU needs KVM (`-enable-kvm`).

use crate::arch::cpu::{
    self, enable_feature_control_vmx, FeatureControlError, IA32_VMX_BASIC,
};

/// COM1 marker when VMXON succeeds (M1.1 gate).
pub const M1_VMXON_OK_MARKER: &str = "RAYNU-V-M1-VMXON-OK";

/// COM1 marker when the CPU has no VMX (e.g. QEMU TCG without nested virt).
pub const M1_VMXON_SKIP_MARKER: &str = "RAYNU-V-M1-VMXON-SKIP";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmxHwError {
    /// CPUID.1:ECX.VMX clear.
    NotSupported,
    /// IA32_FEATURE_CONTROL locked without VMX enable.
    FeatureControl,
    /// VMXON instruction failed (RFLAGS.CF or ZF set).
    VmxonFailed,
    /// VMXOFF failed.
    VmxoffFailed,
}

impl From<FeatureControlError> for VmxHwError {
    fn from(_: FeatureControlError) -> Self {
        Self::FeatureControl
    }
}

/// Prepare the 4KiB VMXON region at `region_phys` (identity-mapped).
///
/// Writes zeros, then IA32_VMX_BASIC revision ID in the first 4 bytes.
///
/// INVARIANTS:
///   - region_phys is 4K-aligned
///   - First dword == VMCS/VMXON revision ID from IA32_VMX_BASIC[30:0]
///
/// SAFETY: `region_phys` must be a writable identity-mapped frame owned by HV.
/// KANI-TARGET: revision write only touches first 4 bytes of the page.
pub unsafe fn prepare_vmxon_region(region_phys: u64) -> Result<(), VmxHwError> {
    debug_assert_eq!(region_phys & 0xfff, 0);
    if !cpu::vmx_supported() {
        return Err(VmxHwError::NotSupported);
    }
    let basic = cpu::rdmsr(IA32_VMX_BASIC);
    let revision = (basic as u32) & 0x7fff_ffff;
    let ptr = region_phys as *mut u8;
    core::ptr::write_bytes(ptr, 0, 4096);
    core::ptr::write_volatile(ptr.cast::<u32>(), revision);
    Ok(())
}

/// Enter VMX root operation (VMXON).
///
/// Sequence (SDM Vol. 3C): feature control → CR0/CR4 fixed bits + VMXE → VMXON.
///
/// INVARIANTS:
///   - Pre: not already in VMX operation
///   - Post on Ok: CPU is in VMX root operation
///
/// SAFETY: sole owner of CR0/CR4/MSRs on this CPU; region prepared and owned.
/// KANI-TARGET: failure paths leave CR4.VMXE set only after successful prep.
pub unsafe fn vmxon(region_phys: u64) -> Result<(), VmxHwError> {
    if !cpu::vmx_supported() {
        return Err(VmxHwError::NotSupported);
    }
    cpu::cli();
    enable_feature_control_vmx()?;
    cpu::adjust_cr0_for_vmx();
    cpu::adjust_cr4_for_vmx();
    prepare_vmxon_region(region_phys)?;

    // VMXON m64: memory operand holds the physical address of the VMXON region.
    let mut region_ptr = region_phys;
    let mut failed: u8;
    // SAFETY: region_ptr is a valid stack reference; region_phys is prepared.
    core::arch::asm!(
        "vmxon [{}]",
        "setbe {}",
        in(reg) &mut region_ptr,
        lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        return Err(VmxHwError::VmxonFailed);
    }
    Ok(())
}

/// Leave VMX root operation (VMXOFF).
///
/// SAFETY: CPU must be in VMX root operation.
/// KANI-TARGET: only issued after successful vmxon in lifecycle.
pub unsafe fn vmxoff() -> Result<(), VmxHwError> {
    let mut failed: u8;
    core::arch::asm!(
        "vmxoff",
        "setbe {}",
        lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmxHwError::VmxoffFailed)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod hardware_test {
    use super::*;

    #[test]
    fn markers_stable() {
        assert_eq!(M1_VMXON_OK_MARKER, "RAYNU-V-M1-VMXON-OK");
        assert_eq!(M1_VMXON_SKIP_MARKER, "RAYNU-V-M1-VMXON-SKIP");
    }
}
