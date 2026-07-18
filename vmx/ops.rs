//! VMX instruction wrappers (VMREAD/VMWRITE/VMCLEAR/VMPTRLD/VMLAUNCH).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L1 — failure flags checked at every call
//!
//! SAFETY: All entry points require VMX root operation and a current VMCS
//! where the SDM requires one (everything except VMCLEAR/VMPTRLD prep).
//!
//! VMWRITE/VMREAD use AT&T operand order matching Linux/KVM.

/// VMX instruction failed (RFLAGS.CF and/or ZF set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmcsOpError;

/// Distinguish VMfailInvalid (CF) from VMfailValid (ZF) for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmFailKind {
    /// CF=1 — typically no current VMCS / invalid VMCS pointer.
    Invalid,
    /// ZF=1 — current VMCS valid but instruction error (see VM_INSTRUCTION_ERROR).
    Valid,
    Both,
}

impl VmFailKind {
    pub fn from_flags(cf: u8, zf: u8) -> Self {
        match (cf != 0, zf != 0) {
            (true, true) => Self::Both,
            (true, false) => Self::Invalid,
            (false, true) => Self::Valid,
            (false, false) => Self::Invalid, // unreachable if caller checked failure
        }
    }
}

/// Write `value` to VMCS field `encoding`.
///
/// SAFETY: CPU in VMX root; VMCS current (VMPTRLD); field valid for this CPU.
#[inline(always)]
pub unsafe fn vmwrite(encoding: u64, value: u64) -> Result<(), VmcsOpError> {
    match vmwrite_detailed(encoding, value) {
        Ok(()) => Ok(()),
        Err(_) => Err(VmcsOpError),
    }
}

/// VMWRITE with CF/ZF breakdown (for bring-up diagnostics).
///
/// SAFETY: same as [`vmwrite`].
#[inline(always)]
pub unsafe fn vmwrite_detailed(encoding: u64, value: u64) -> Result<(), VmFailKind> {
    let mut cf: u8;
    let mut zf: u8;
    // AT&T / Linux: vmwrite %field, %value → SDM reg=value, r/m=field.
    core::arch::asm!(
        "vmwrite {field}, {value}",
        "setc {cf}",
        "setz {zf}",
        field = in(reg) encoding,
        value = in(reg) value,
        cf = lateout(reg_byte) cf,
        zf = lateout(reg_byte) zf,
        options(nostack, att_syntax),
    );
    if cf == 0 && zf == 0 {
        Ok(())
    } else {
        Err(VmFailKind::from_flags(cf, zf))
    }
}

/// Read VMCS field `encoding`.
///
/// SAFETY: CPU in VMX root; VMCS current; field valid.
#[inline(always)]
pub unsafe fn vmread(encoding: u64) -> Result<u64, VmcsOpError> {
    let value: u64;
    let mut failed: u8;
    core::arch::asm!(
        "vmread {field}, {value}",
        "setbe {failed}",
        field = in(reg) encoding,
        value = lateout(reg) value,
        failed = lateout(reg_byte) failed,
        options(nostack, att_syntax),
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

/// VMPTRST — read current-VMCS pointer (`u64::MAX` if none).
///
/// SAFETY: CPU in VMX root operation.
#[inline]
pub unsafe fn vmptrst() -> Result<u64, VmcsOpError> {
    let mut ptr = 0u64;
    let mut failed: u8;
    core::arch::asm!(
        "vmptrst [{}]",
        "setbe {}",
        in(reg) &mut ptr,
        lateout(reg_byte) failed,
        options(nostack),
    );
    if failed != 0 {
        Err(VmcsOpError)
    } else {
        Ok(ptr)
    }
}

/// VMPTRLD then immediately VMWRITE in one asm block (no intervening exits).
///
/// Nested VT-x on some hosts has been observed to drop the current-VMCS
/// pointer across intervening calls/I/O; keeping both ops in one block avoids that.
///
/// SAFETY: `region_phys` is a cleared, owned VMCS frame; CPU in VMX root.
#[inline(always)]
pub unsafe fn vmptrld_and_vmwrite(
    region_phys: u64,
    encoding: u64,
    value: u64,
) -> Result<(), VmFailKind> {
    let mut ptr = region_phys;
    let mut cf: u8;
    let mut zf: u8;
    // AT&T throughout: vmptrld (mem), vmwrite %field, %value.
    core::arch::asm!(
        "vmptrld ({ptr})",
        "setbe {tmp}",
        "test {tmp}, {tmp}",
        "jnz 1f",
        "vmwrite {field}, {value}",
        "setc {cf}",
        "setz {zf}",
        "jmp 2f",
        "1:",
        // VMPTRLD failed — report as Invalid (no current VMCS).
        "movb $1, {cf}",
        "movb $0, {zf}",
        "2:",
        ptr = in(reg) &mut ptr,
        field = in(reg) encoding,
        value = in(reg) value,
        tmp = out(reg_byte) _,
        cf = out(reg_byte) cf,
        zf = out(reg_byte) zf,
        options(nostack, att_syntax),
    );
    if cf == 0 && zf == 0 {
        Ok(())
    } else {
        Err(VmFailKind::from_flags(cf, zf))
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
        Ok(())
    }
}

#[cfg(test)]
mod ops_test {
    #[test]
    fn error_type_exists() {
        let _ = super::VmcsOpError;
        let _ = super::VmFailKind::Invalid;
    }
}
