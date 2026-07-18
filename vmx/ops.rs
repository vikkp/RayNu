//! VMX instruction wrappers (VMREAD/VMWRITE/VMCLEAR/VMPTRLD/VMLAUNCH).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L1 — failure flags checked at every call
//!
//! SAFETY: All entry points require VMX root operation and a current VMCS
//! where the SDM requires one (everything except VMCLEAR/VMPTRLD prep).

/// VMWRITE failed (RFLAGS.CF or ZF set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmcsOpError;

/// Write `value` to VMCS field `encoding`.
///
/// SAFETY: CPU in VMX root; VMCS current (VMPTRLD); field valid for this CPU.
#[inline]
pub unsafe fn vmwrite(encoding: u64, value: u64) -> Result<(), VmcsOpError> {
    let mut failed: u8;
    core::arch::asm!(
        "vmwrite {value}, {encoding}",
        "setbe {failed}",
        encoding = in(reg) encoding,
        value = in(reg) value,
        failed = lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmcsOpError)
    } else {
        Ok(())
    }
}

/// Read VMCS field `encoding`.
///
/// SAFETY: CPU in VMX root; VMCS current; field valid.
#[inline]
pub unsafe fn vmread(encoding: u64) -> Result<u64, VmcsOpError> {
    let value: u64;
    let mut failed: u8;
    core::arch::asm!(
        "vmread {value}, {encoding}",
        "setbe {failed}",
        encoding = in(reg) encoding,
        value = lateout(reg) value,
        failed = lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmcsOpError)
    } else {
        Ok(value)
    }
}

/// VMCLEAR — initialize / put VMCS in clear state.
///
/// SAFETY: `region_phys` is a 4K-aligned identity-mapped VMCS region owned by HV.
#[inline]
pub unsafe fn vmclear(region_phys: u64) -> Result<(), VmcsOpError> {
    let mut ptr = region_phys;
    let mut failed: u8;
    core::arch::asm!(
        "vmclear [{}]",
        "setbe {}",
        in(reg) &mut ptr,
        lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmcsOpError)
    } else {
        Ok(())
    }
}

/// VMPTRLD — make VMCS current.
///
/// SAFETY: region previously VMCLEAR'd; identity-mapped; owned by HV.
#[inline]
pub unsafe fn vmptrld(region_phys: u64) -> Result<(), VmcsOpError> {
    let mut ptr = region_phys;
    let mut failed: u8;
    core::arch::asm!(
        "vmptrld [{}]",
        "setbe {}",
        in(reg) &mut ptr,
        lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmcsOpError)
    } else {
        Ok(())
    }
}

/// VMLAUNCH — enter guest. Returns `Err` if the instruction fails (no entry).
///
/// On success this does not return: a VMEXIT transfers control to HOST_RIP.
///
/// SAFETY: VMCS fully programmed; HOST_RIP/HOST_RSP valid; interrupts masked
/// as required by the control configuration.
#[inline]
pub unsafe fn vmlaunch() -> Result<(), VmcsOpError> {
    let mut failed: u8;
    core::arch::asm!(
        "vmlaunch",
        "setbe {}",
        lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmcsOpError)
    } else {
        // Successful VMLAUNCH never falls through.
        Ok(())
    }
}

#[cfg(test)]
mod ops_test {
    #[test]
    fn error_type_exists() {
        let _ = super::VmcsOpError;
    }
}
