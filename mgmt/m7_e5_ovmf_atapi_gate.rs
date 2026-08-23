//! E5 Stage 44 — firmware ATAPI READ so `sectors>0`.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: N/A
//!
//! Stage 43 closed both PCI enums then stopped the private VMCS
//! (`1b07692` n=1111 `sectors=0`). Keep running after BOTH-OK until
//! firmware issues PACKET/`READ(10)` (honest `sectors>0`) or the 32768
//! cap. Nested VT-x `5d9e346`: BOTH-OK then n=8192 `ataio=0` `unh=3`
//! `port=0xcf8` (empty-slot walk + KBC; 1s HPET per PCI I/O). Nested
//! VT-x `2674629`: n=32768 `ataio=0` `acpi=16612` `port=0` `hpet=10`
//! (`in eax,dx` BdsWait). ACPI PM 1s step. fw_cfg BootMenu=on +
//! `etc/boot-menu-wait` 0ms so FrontPage skips. Iron COM2: LIVE-BYTES-OK
//! then #UD RIP `0x109D` `pci_ide=0` `com=15515` (INVPCID/XSAVES/RDTSCP
//! missing on guest-UEFI VMCS; XSETBV must execute XCR0, not skip). Iron
//! `d5f9431` COM2: #UD gone, DXE, then n=1280..8192 `reason=0x34`
//! `rip=0x6e81ca` (pause CpuDeadLoop; no BOTH-OK). `e2af81e`
//! skipped only `pause`/`jcc rel8`/`eb f3`/`eb fe`; GCC is often
//! `eb fc` or `0F 84` rel32. Iron `891eb5b`: OSXSAVE CR4 intercept,
//! then skip of `ebecc9c3` (`leave; ret`) escaped ASSERT → `#UD` at
//! PE-header `0x109D` (stopped n=1439). Do not skip that jmp; dump
//! ASSERT return address. Iron `17449e2`: ASSERT noskip `ret=0x6e8946`
//! `rip=0x6e81ca` after host CPUID (Xeon topology+VMX). Filter guest-UEFI
//! CPUID to uniprocessor, hide VMX/x2APIC, lock FEATURE_CONTROL. Iron
//! `ad78f12`: same ASSERT after seven `RDMSR 0x1B` — xAPIC MMIO was a
//! 2MiB zero sink (`GetApicVersion()==0`). Map a 4KiB xAPIC page
//! (version 0x50014). Iron `3f417ca`: xAPIC 4K mapped, still ASSERT after
//! MTRR walk `0xFE`/`0x2FF`/`0x250`. Shadow guest MTRRs (not host). Iron
//! `408788c`: MTRR walk completed, still ASSERT after CPUID `0x1cf11b5`.
//! Nested KVM sets hypervisor CPUID bit 31 + `KVMKVMKVM`; iron did not.
//! Guest-UEFI CPUID hypervisor present + KVM signature; IA32_MISC_ENABLE
//! shadowed. Iron `8700cbb`: hypervisor CPUID still ASSERT
//! `callerrip=0x1d25193` after WRMSR then RDMSR spin. MTRR VCNT=32 +
//! PCI UC 1GiB at `0xC0000000`. fw_cfg `bootorder` trailing NUL so
//! `ConnectDevicesFromQemu` is not `INVALID_PARAMETER`. Nested
//! VT-x `8e55abf`: BOTH-OK then n=2048 `ata=0x0` `unh=0`
//! `cf8=0x80000838` — PIIX ISA `00:01.0` offset `0x38`
//! (PciBus programming, never ATA). 32768-exit cap. PIIX3 ISA PIRQ
//! `0x60-0x63` reset `0x80` so IRQ assign is not IRQ0. 8-byte command BAR + BAR-relocated ATA, secondary
//! `0x170`, EXECUTE DEVICE DIAGNOSTIC `0x90` restores `0xEB14`, BMIDE
//! BAR4 RAZ/WI. ATAPI signature after reset (`LBA mid=0x14` high=`0xEB`),
//! PACKET interrupt-reason (CDB `0x01`, data-in `0x02`, complete `0x03`),
//! and cylinder byte count. Placeholder ISO has PVD `CD001` plus a
//! minimal EFI El Torito catalog. Marker after past-SEC and a real
//! sector read. Not firmware El Torito boot. Not installer. No new
//! `*Absent` enum. No TLS.

use super::guest_fw::reset_guest_fw;
use super::iso::{attach_cdrom_uefi, reset_host_cdrom, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_ovmf_both_gate::run_m7_e5_ovmf_both_gate;
use crate::devices::guest_platform::{boot_menu_wait_skips_bds, bootorder_nul_terminated};
use crate::devices::ide_cdrom;
use crate::vmx::guest_uefi::{
    atapi_read_evidence, guest_uefi_cpuid_has_hypervisor, guest_uefi_cpuid_is_kvm,
    guest_uefi_cpuid_leaf1_is_uniprocessor, guest_uefi_filter_cpuid, guest_uefi_is_misc_enable,
    guest_uefi_is_mtrr_msr, guest_uefi_misc_enable_read, guest_uefi_mtrr_read,
    guest_uefi_mtrr_reset, guest_uefi_mtrr_write, guest_uefi_mtrr_pci_uc_hole, guest_uefi_xapic_is_not_sink, hlt_should_resume,
    post_dxe_should_stop, preempt_deadloop_is_assert_epilogue, preempt_deadloop_should_skip,
    preempt_deadloop_skip_len, spin_short_jmp_should_skip, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
    GUEST_UEFI_FEATURE_CONTROL_VALUE, GUEST_UEFI_KVM_CPUID_LEAF, GUEST_UEFI_MISC_ENABLE_DEFAULT,
    GUEST_UEFI_MISC_ENABLE_MSR, GUEST_UEFI_POST_DXE_TAIL, M7_E5_OVMF_ATAPI_OK_MARKER,
};

/// Host / CI / QEMU marker when firmware read an ATAPI sector.
pub const M7_E5_OVMF_ATAPI_GATE_MARKER: &str = M7_E5_OVMF_ATAPI_OK_MARKER;

pub fn prop_atapi_signature_and_read10() -> bool {
    ide_cdrom::reset();
    if !ide_cdrom::present_placeholder() {
        return false;
    }
    // After reset/present, ATAPI signature so firmware sends PACKET (0xA0).
    let mid = ide_cdrom::ata_io(0x01F4, true, 1, 0) as u8;
    let high = ide_cdrom::ata_io(0x01F5, true, 1, 0) as u8;
    if mid != 0x14 || high != 0xEB {
        ide_cdrom::reset();
        return false;
    }
    let _ = ide_cdrom::ata_io(0x01F7, false, 1, 0xA0);
    let reason = ide_cdrom::ata_io(0x01F2, true, 1, 0) as u8;
    if reason != 0x01 {
        ide_cdrom::reset();
        return false;
    }
    let pvd = match ide_cdrom::host_read10(16) {
        Some(s) => s,
        None => {
            ide_cdrom::reset();
            return false;
        }
    };
    let br = match ide_cdrom::host_read10(17) {
        Some(s) => s,
        None => {
            ide_cdrom::reset();
            return false;
        }
    };
    let sectors = ide_cdrom::sectors_read();
    let packets = ide_cdrom::packet_commands();
    let scsi = ide_cdrom::last_scsi();
    ide_cdrom::reset();
    mid == 0x14
        && high == 0xEB
        && &pvd[1..6] == b"CD001"
        && &br[7..30] == b"EL TORITO SPECIFICATION"
        && atapi_read_evidence(sectors)
        && packets >= 2
        && scsi == 0x28
}

pub fn prop_bar_relocated_read10() -> bool {
    ide_cdrom::reset();
    if !ide_cdrom::present_placeholder() {
        return false;
    }
    ide_cdrom::pci_write_addr(ide_cdrom::pci_config_addr() | 0x10);
    ide_cdrom::pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    let probe = ide_cdrom::pci_read_data(0xCFC, 4);
    ide_cdrom::pci_write_data(0xCFC, 4, 0xC000);
    let _ = ide_cdrom::ata_io(0xC007, false, 1, 0xA0);
    let cdb = [0x28u8, 0, 0, 0, 0, 16, 0, 0, 1, 0, 0, 0];
    for chunk in cdb.chunks(2) {
        let w = u64::from(chunk[0]) | (u64::from(chunk[1]) << 8);
        let _ = ide_cdrom::ata_io(0xC000, false, 2, w);
    }
    let pvd0 = ide_cdrom::ata_io(0xC000, true, 1, 0) as u8;
    let sectors = ide_cdrom::sectors_read();
    ide_cdrom::reset();
    probe == 0xFFFF_FFF9 && pvd0 == 1 && atapi_read_evidence(sectors)
}

pub fn ovmf_atapi_surface_present() -> bool {
    reset_host_cdrom();
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let qemu = include_str!("../tools/qemu-boot-test.sh");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let ide = include_str!("../devices/ide_cdrom.rs");
    let plat = include_str!("../devices/guest_platform.rs");
    let flash = include_str!("../tools/flash-cruzer-esp.sh");
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
        && !spa.contains("Launch OVMF")
        && !spa.contains("btn-vl")
        && adr.contains("RAYNU-V-M7-E5-OVMF-ATAPI-OK")
        && qemu.contains("RAYNU-V-M7-E5-OVMF-ATAPI-OK")
        && guest.contains("maybe_print_atapi")
        && guest.contains("atapi_read_evidence")
        && guest.contains("not both-enum-alone")
        && guest.contains("ATAPI signature")
        && guest.contains("PACKET interrupt-reason")
        && guest.contains("ata cmd=")
        && guest.contains("io unhandled port=0x")
        && guest.contains("pci wr=0x")
        && guest.contains("8192-exit cap")
        && guest.contains("32768-exit cap")
        && guest.contains("hpet_tick_sink_by")
        && plat.contains("is_kbc_port")
        && plat.contains("hpet_tick_sink_by")
        && plat.contains("ACPI_PM_STEP")
        && plat.contains("0x0040_0000")
        && plat.contains("etc/boot-menu-wait")
        && plat.contains("BOOT_MENU_WAIT")
        && plat.contains("FW_CFG_BOOT_MENU")
        && plat.contains("ide@1,1/drive@0")
        && plat.contains("ide@0,1/drive@0")
        && guest.contains("ACPI PM 1s step")
        && guest.contains("handle_xsetbv")
        && guest.contains("xsetbv_masked_xcr0")
        && guest.contains("XSETBV executes XCR0")
        && guest.contains("SECONDARY_ENABLE_INVPCID")
        && guest.contains("SECONDARY_ENABLE_XSAVES")
        && guest.contains("SECONDARY_ENABLE_RDTSCP")
        && guest.contains("0x109D")
        && guest.contains("0x6e81ca")
        && guest.contains("preempt_deadloop_should_skip")
        && guest.contains("preempt_deadloop_skip_len")
        && guest.contains("preempt_deadloop_is_assert_epilogue")
        && guest.contains("891eb5b")
        && guest.contains("leave; ret")
        && guest.contains("ebecc9c3")
        && guest.contains("ebf3c9c3")
        && guest.contains("guest_uefi_filter_cpuid")
        && guest.contains("GUEST_UEFI_FEATURE_CONTROL_VALUE")
        && guest.contains("ad78f12")
        && guest.contains("xAPIC 4K")
        && guest.contains("GetApicVersion")
        && plat.contains("is_xapic_2m_gpa")
        && guest.contains("guest_uefi_is_mtrr_msr")
        && guest.contains("3f417ca")
        && guest.contains("MTRR shadow")
        && guest.contains("408788c")
        && guest.contains("KVMKVMKVM")
        && guest.contains("callerrip")
        && guest.contains("8700cbb")
        && guest.contains("VCNT=32")
        && guest.contains("bootorder NUL")
        && guest.contains("0xC0000000")
        && plat.contains("bootorder_nul_terminated")
        && guest.contains("17449e2")
        && guest.contains("uniprocessor")
        && guest.contains("pause CpuDeadLoop")
        && guest.contains("preempt noskip")
        && guest.contains("eb fc")
        && guest.contains("tick n=")
        && guest.contains("PIIX3 ISA PIRQ")
        && guest.contains("ataio=")
        && guest.contains("0x80000838")
        && ide.contains("ATAPI_INT_CD")
        && ide.contains("ATAPI_SIG_LBA")
        && ide.contains("EL TORITO SPECIFICATION")
        && ide.contains("SCSI_REQUEST_SENSE")
        && ide.contains("ATA_CMD_DIAGNOSTIC")
        && ide.contains("0xFFFF_FFF8")
        && ide.contains("0xFFFF_FFF0")
        && ide.contains("ata_io_accesses")
        && plat.contains("fill_isa_cfg")
        && plat.contains("PIRQA")
        && flash.contains("EFI/RayNu/OVMF.fd")
        && flash.contains("ovmf_has_fvh")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 44 package. Host gate + QEMU marker — not iron, not Everest E5.
pub fn run_m7_e5_ovmf_atapi_gate() -> bool {
    reset_guest_fw();
    reset_host_cdrom();
    ide_cdrom::reset();
    guest_uefi_mtrr_reset();
    let ok = ovmf_atapi_surface_present()
        && prop_atapi_signature_and_read10()
        && prop_bar_relocated_read10()
        && run_m7_e5_ovmf_both_gate()
        && !atapi_read_evidence(0)
        && atapi_read_evidence(1)
        && !post_dxe_should_stop(false, 2000, 0, 1)
        && !post_dxe_should_stop(true, 115, 115, 0)
        && post_dxe_should_stop(true, 115, 115, 1)
        && post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL, 115, 0)
        && !post_dxe_should_stop(true, 115 + GUEST_UEFI_POST_DXE_TAIL - 1, 115, 0)
        && hlt_should_resume()
        && spin_short_jmp_should_skip(0xEB, 0xF3)
        && preempt_deadloop_should_skip(0xF3, 0x90)
        && preempt_deadloop_should_skip(0x74, 0xEC)
        && preempt_deadloop_should_skip(0xEB, 0xFC)
        && preempt_deadloop_should_skip(0xEB, 0xEC)
        && preempt_deadloop_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3]) == 0
        && preempt_deadloop_skip_len(&[0xEB, 0xF3, 0xC9, 0xC3]) == 2
        && preempt_deadloop_is_assert_epilogue(&[0xEB, 0xEC, 0xC9, 0xC3])
        && !preempt_deadloop_is_assert_epilogue(&[0xEB, 0xF3, 0xC9, 0xC3])
        && !preempt_deadloop_is_assert_epilogue(&[0xEB, 0xFC, 0x90, 0x90])
        && !spin_short_jmp_should_skip(0xEB, 0xFC)
        && !spin_short_jmp_should_skip(0xEB, 0xEC)
        && !preempt_deadloop_should_skip(0x74, 0x02)
        && preempt_deadloop_skip_len(&[0xF3, 0x90]) == 2
        && preempt_deadloop_skip_len(&[0x0F, 0x84, 0xE8, 0xFF, 0xFF, 0xFF]) == 6
        && preempt_deadloop_skip_len(&[0x0F, 0x84, 0x10, 0, 0, 0]) == 0
        && boot_menu_wait_skips_bds()
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not both-enum-alone")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ATAPI signature")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PACKET interrupt-reason")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8-byte IDE command BAR")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EXECUTE DEVICE DIAGNOSTIC")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8192-exit cap")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("32768-exit cap")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5d9e346")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("2674629")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ACPI PM 1s step")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("INVPCID/RDTSCP/XSAVES")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("XSETBV executes XCR0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("etc/boot-menu-wait")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("drive@0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("insn=ebec")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x109D")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x6e81ca")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pause CpuDeadLoop")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preempt pause/jcc skip")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preempt eb/jcc32 skip")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preempt noskip dump")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("891eb5b")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("leave; ret")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("17449e2")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ebf3c9c3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("uniprocessor")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("FEATURE_CONTROL")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ad78f12")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("xAPIC 4K")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("3f417ca")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR shadow")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("408788c")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("KVMKVMKVM")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("callerrip")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8700cbb")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("VCNT=32")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("bootorder NUL")
        && guest_uefi_xapic_is_not_sink()
        && guest_uefi_is_mtrr_msr(0x250)
        && guest_uefi_mtrr_read(0xFE)
            == Some(crate::vmx::guest_uefi::GUEST_UEFI_MTRRCAP)
        && guest_uefi_mtrr_pci_uc_hole()
        && bootorder_nul_terminated()
        && guest_uefi_mtrr_write(0x200, 6)
        && guest_uefi_mtrr_read(0x200) == Some(6)
        && guest_uefi_is_misc_enable(GUEST_UEFI_MISC_ENABLE_MSR)
        && guest_uefi_misc_enable_read(GUEST_UEFI_MISC_ENABLE_MSR)
            == Some(GUEST_UEFI_MISC_ENABLE_DEFAULT)
        && GUEST_UEFI_FEATURE_CONTROL_VALUE == 1
        && {
            let r = guest_uefi_filter_cpuid(1, 0);
            guest_uefi_cpuid_leaf1_is_uniprocessor(r.ebx, r.edx)
                && (r.ecx & crate::arch::cpu::CPUID_ECX_VMX) == 0
                && guest_uefi_cpuid_has_hypervisor(r.ecx)
        }
        && {
            let k = guest_uefi_filter_cpuid(GUEST_UEFI_KVM_CPUID_LEAF, 0);
            guest_uefi_cpuid_is_kvm(k.ebx, k.ecx, k.edx)
        }
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8042 KBC")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8e55abf")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX3 ISA PIRQ")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80000838")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not firmware El Torito boot")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK")
        && M7_E5_OVMF_ATAPI_GATE_MARKER == "RAYNU-V-M7-E5-OVMF-ATAPI-OK";
    ide_cdrom::reset();
    reset_host_cdrom();
    reset_guest_fw();
    guest_uefi_mtrr_reset();
    ok
}

#[cfg(test)]
#[path = "m7_e5_ovmf_atapi_gate_test.rs"]
mod m7_e5_ovmf_atapi_gate_test;
