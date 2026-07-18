//! VMX instruction wrappers (VMREAD/VMWRITE/VMCLEAR/VMPTRLD/VMLAUNCH).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L1 — failure flags checked at every call
//!
//! SAFETY: All entry points require VMX root operation and a current VMCS
//! where the SDM requires one (everything except VMCLEAR/VMPTRLD prep).
//!
//! Operand order (AT&T / Linux / syzkaller):
//!   `vmwrite %value, %field`   — primary = value, secondary = field encoding
//!   `vmread  %field, %dest`    — source = field encoding, dest = output
//!
//! A swapped VMWRITE looks like Intel error 12 (unsupported field) because the
//! CPU treats the value register as the field encoding (e.g. `!0`).

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
            (false, false) => Self::Invalid,
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
/// AT&T: `vmwrite %rax, %rdx` with RAX=value, RDX=field (Linux `vmx_asm2` /
/// syzkaller `VMSET`). Encodes as `0F 79 D0`.
///
/// SAFETY: same as [`vmwrite`].
#[inline(always)]
pub unsafe fn vmwrite_detailed(encoding: u64, value: u64) -> Result<(), VmFailKind> {
    let mut cf: u8;
    let mut zf: u8;
    core::arch::asm!(
        "vmwrite %rax, %rdx",
        "setc {cf}",
        "setz {zf}",
        in("rax") value,
        in("rdx") encoding,
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
/// AT&T: `vmread %rax, %rdx` with RAX=field, RDX=dest (Linux / SDM).
///
/// SAFETY: CPU in VMX root; VMCS current; field valid.
#[inline(always)]
pub unsafe fn vmread(encoding: u64) -> Result<u64, VmcsOpError> {
    let value: u64;
    let mut failed: u8;
    core::arch::asm!(
        "vmread %rax, %rdx",
        "setbe {failed}",
        in("rax") encoding,
        lateout("rdx") value,
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

/// VMPTRLD then VMWRITE in one asm block (no intervening exits).
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
    core::arch::asm!(
        "vmptrld ({ptr})",
        "setbe {tmp}",
        "test {tmp}, {tmp}",
        "jnz 1f",
        // AT&T: vmwrite %value, %field  (RAX=value, RDX=field).
        "vmwrite %rax, %rdx",
        "setc {cf}",
        "setz {zf}",
        "jmp 2f",
        "1:",
        "movb $1, {cf}",
        "movb $0, {zf}",
        "2:",
        ptr = in(reg) &mut ptr,
        in("rax") value,
        in("rdx") encoding,
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

/// VMRESUME — re-enter guest after a VMEXIT. Returns `Err` on failure.
///
/// On success this does not return: the next VMEXIT transfers to HOST_RIP.
///
/// SAFETY: current VMCS launched; guest/host state valid for resume; any
/// VM-entry interruption-info programmed intentionally.
#[inline]
pub unsafe fn vmresume() -> Result<(), VmcsOpError> {
    let mut failed: u8;
    core::arch::asm!(
        "vmresume",
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
