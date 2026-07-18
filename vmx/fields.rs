//! VMCS field encodings (Intel SDM Vol. 3D Appendix B).
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0 — encodings are architectural constants

#![allow(dead_code)]

// ── 16-bit guest state ──────────────────────────────────────────────
pub const GUEST_ES_SELECTOR: u64 = 0x0000_0800;
pub const GUEST_CS_SELECTOR: u64 = 0x0000_0802;
pub const GUEST_SS_SELECTOR: u64 = 0x0000_0804;
pub const GUEST_DS_SELECTOR: u64 = 0x0000_0806;
pub const GUEST_FS_SELECTOR: u64 = 0x0000_0808;
pub const GUEST_GS_SELECTOR: u64 = 0x0000_080A;
pub const GUEST_LDTR_SELECTOR: u64 = 0x0000_080C;
pub const GUEST_TR_SELECTOR: u64 = 0x0000_080E;

// ── 16-bit host state ───────────────────────────────────────────────
pub const HOST_ES_SELECTOR: u64 = 0x0000_0C00;
pub const HOST_CS_SELECTOR: u64 = 0x0000_0C02;
pub const HOST_SS_SELECTOR: u64 = 0x0000_0C04;
pub const HOST_DS_SELECTOR: u64 = 0x0000_0C06;
pub const HOST_FS_SELECTOR: u64 = 0x0000_0C08;
pub const HOST_GS_SELECTOR: u64 = 0x0000_0C0A;
pub const HOST_TR_SELECTOR: u64 = 0x0000_0C0C;

// ── 64-bit control ──────────────────────────────────────────────────
pub const IO_BITMAP_A: u64 = 0x0000_2000;
pub const IO_BITMAP_B: u64 = 0x0000_2002;
pub const MSR_BITMAP: u64 = 0x0000_2004;
/// EPT pointer (EPTP).
pub const EPT_POINTER: u64 = 0x0000_201A;
pub const VMCS_LINK_POINTER: u64 = 0x0000_2800;
pub const GUEST_IA32_DEBUGCTL: u64 = 0x0000_2802;
pub const GUEST_IA32_PAT: u64 = 0x0000_2804;
pub const GUEST_IA32_EFER: u64 = 0x0000_2806;

// ── 64-bit host state ───────────────────────────────────────────────
pub const HOST_IA32_PAT: u64 = 0x0000_2C00;
pub const HOST_IA32_EFER: u64 = 0x0000_2C02;

// ── 32-bit control ──────────────────────────────────────────────────
pub const PIN_BASED_VM_EXEC_CONTROL: u64 = 0x0000_4000;
pub const PRIMARY_PROC_BASED_VM_EXEC_CONTROL: u64 = 0x0000_4002;
pub const EXCEPTION_BITMAP: u64 = 0x0000_4004;
pub const PAGE_FAULT_ERROR_CODE_MASK: u64 = 0x0000_4006;
pub const PAGE_FAULT_ERROR_CODE_MATCH: u64 = 0x0000_4008;
pub const CR3_TARGET_COUNT: u64 = 0x0000_400A;
pub const VM_EXIT_CONTROLS: u64 = 0x0000_400C;
pub const VM_EXIT_MSR_STORE_COUNT: u64 = 0x0000_400E;
pub const VM_EXIT_MSR_LOAD_COUNT: u64 = 0x0000_4010;
pub const VM_ENTRY_CONTROLS: u64 = 0x0000_4012;
pub const VM_ENTRY_MSR_LOAD_COUNT: u64 = 0x0000_4014;
pub const VM_ENTRY_INTERRUPTION_INFO: u64 = 0x0000_4016;
pub const TPR_THRESHOLD: u64 = 0x0000_401C;
pub const SECONDARY_VM_EXEC_CONTROL: u64 = 0x0000_401E;

// ── 32-bit read-only data ───────────────────────────────────────────
pub const VM_INSTRUCTION_ERROR: u64 = 0x0000_4400;
pub const EXIT_REASON: u64 = 0x0000_4402;
pub const EXIT_QUALIFICATION: u64 = 0x0000_6400;

// ── 32-bit guest state ──────────────────────────────────────────────
pub const GUEST_ES_LIMIT: u64 = 0x0000_4800;
pub const GUEST_CS_LIMIT: u64 = 0x0000_4802;
pub const GUEST_SS_LIMIT: u64 = 0x0000_4804;
pub const GUEST_DS_LIMIT: u64 = 0x0000_4806;
pub const GUEST_FS_LIMIT: u64 = 0x0000_4808;
pub const GUEST_GS_LIMIT: u64 = 0x0000_480A;
pub const GUEST_LDTR_LIMIT: u64 = 0x0000_480C;
pub const GUEST_TR_LIMIT: u64 = 0x0000_480E;
pub const GUEST_GDTR_LIMIT: u64 = 0x0000_4810;
pub const GUEST_IDTR_LIMIT: u64 = 0x0000_4812;
pub const GUEST_ES_ACCESS_RIGHTS: u64 = 0x0000_4814;
pub const GUEST_CS_ACCESS_RIGHTS: u64 = 0x0000_4816;
pub const GUEST_SS_ACCESS_RIGHTS: u64 = 0x0000_4818;
pub const GUEST_DS_ACCESS_RIGHTS: u64 = 0x0000_481A;
pub const GUEST_FS_ACCESS_RIGHTS: u64 = 0x0000_481C;
pub const GUEST_GS_ACCESS_RIGHTS: u64 = 0x0000_481E;
pub const GUEST_LDTR_ACCESS_RIGHTS: u64 = 0x0000_4820;
pub const GUEST_TR_ACCESS_RIGHTS: u64 = 0x0000_4822;
pub const GUEST_INTERRUPTIBILITY_STATE: u64 = 0x0000_4824;
pub const GUEST_ACTIVITY_STATE: u64 = 0x0000_4826;
pub const GUEST_IA32_SYSENTER_CS: u64 = 0x0000_482A;

// ── 32-bit host state ───────────────────────────────────────────────
pub const HOST_IA32_SYSENTER_CS: u64 = 0x0000_4C00;

// ── natural-width control ───────────────────────────────────────────
pub const CR0_GUEST_HOST_MASK: u64 = 0x0000_6000;
pub const CR4_GUEST_HOST_MASK: u64 = 0x0000_6002;
pub const CR0_READ_SHADOW: u64 = 0x0000_6004;
pub const CR4_READ_SHADOW: u64 = 0x0000_6006;

// ── natural-width guest state ───────────────────────────────────────
pub const GUEST_CR0: u64 = 0x0000_6800;
pub const GUEST_CR3: u64 = 0x0000_6802;
pub const GUEST_CR4: u64 = 0x0000_6804;
pub const GUEST_ES_BASE: u64 = 0x0000_6806;
pub const GUEST_CS_BASE: u64 = 0x0000_6808;
pub const GUEST_SS_BASE: u64 = 0x0000_680A;
pub const GUEST_DS_BASE: u64 = 0x0000_680C;
pub const GUEST_FS_BASE: u64 = 0x0000_680E;
pub const GUEST_GS_BASE: u64 = 0x0000_6810;
pub const GUEST_LDTR_BASE: u64 = 0x0000_6812;
pub const GUEST_TR_BASE: u64 = 0x0000_6814;
pub const GUEST_GDTR_BASE: u64 = 0x0000_6816;
pub const GUEST_IDTR_BASE: u64 = 0x0000_6818;
pub const GUEST_DR7: u64 = 0x0000_681A;
pub const GUEST_RSP: u64 = 0x0000_681C;
pub const GUEST_RIP: u64 = 0x0000_681E;
pub const GUEST_RFLAGS: u64 = 0x0000_6820;
pub const GUEST_PENDING_DBG_EXCEPTIONS: u64 = 0x0000_6822;
pub const GUEST_IA32_SYSENTER_ESP: u64 = 0x0000_6824;
pub const GUEST_IA32_SYSENTER_EIP: u64 = 0x0000_6826;

// ── natural-width host state ────────────────────────────────────────
pub const HOST_CR0: u64 = 0x0000_6C00;
pub const HOST_CR3: u64 = 0x0000_6C02;
pub const HOST_CR4: u64 = 0x0000_6C04;
pub const HOST_FS_BASE: u64 = 0x0000_6C06;
pub const HOST_GS_BASE: u64 = 0x0000_6C08;
pub const HOST_TR_BASE: u64 = 0x0000_6C0A;
pub const HOST_GDTR_BASE: u64 = 0x0000_6C0C;
pub const HOST_IDTR_BASE: u64 = 0x0000_6C0E;
pub const HOST_IA32_SYSENTER_ESP: u64 = 0x0000_6C10;
pub const HOST_IA32_SYSENTER_EIP: u64 = 0x0000_6C12;
pub const HOST_RSP: u64 = 0x0000_6C14;
pub const HOST_RIP: u64 = 0x0000_6C16;

// ── primary proc-based control bits ─────────────────────────────────
pub const CPU_BASED_HLT_EXITING: u32 = 1 << 7;
pub const CPU_BASED_USE_IO_BITMAPS: u32 = 1 << 25;
pub const CPU_BASED_USE_MSR_BITMAPS: u32 = 1 << 28;
pub const CPU_BASED_ACTIVATE_SECONDARY: u32 = 1 << 31;

/// Secondary proc-based: enable EPT.
pub const SECONDARY_ENABLE_EPT: u32 = 1 << 1;

// ── VM-exit / VM-entry control bits ─────────────────────────────────
pub const VM_EXIT_HOST_ADDR_SPACE_SIZE: u32 = 1 << 9;
pub const VM_EXIT_SAVE_IA32_EFER: u32 = 1 << 20;
pub const VM_EXIT_LOAD_IA32_EFER: u32 = 1 << 21;
pub const VM_ENTRY_IA32E_MODE: u32 = 1 << 9;
pub const VM_ENTRY_LOAD_IA32_EFER: u32 = 1 << 15;

/// Basic exit reason: HLT.
pub const EXIT_REASON_HLT: u32 = 12;

#[cfg(test)]
mod fields_test {
    use super::*;

    #[test]
    fn encodings_match_sdm_appendix_b() {
        assert_eq!(GUEST_RIP, 0x681E);
        assert_eq!(HOST_RIP, 0x6C16);
        assert_eq!(EPT_POINTER, 0x201A);
        assert_eq!(EXIT_REASON, 0x4402);
        assert_eq!(PIN_BASED_VM_EXEC_CONTROL, 0x4000);
        assert_eq!(EXIT_REASON_HLT, 12);
        assert_eq!(SECONDARY_ENABLE_EPT, 1 << 1);
    }
}
