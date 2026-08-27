//! Private guest-UEFI VMCS + EPT + VMLAUNCH of retained ESP `OVMF.fd`.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014 guest firmware path)
//! VERIFICATION: L1 (runtime assert + host tests; QEMU is the launch gate)
//!
//! After Stage 36 retain, this module allocates a **private** VMCS and EPT,
//! maps the retained bytes at the firmware-alias window, and issues a real
//! `VMLAUNCH` at reset vector `0xFFFF_FFF0`. It does not write the E4 SHELL
//! VMCS or EPT. Fixtures are refused. Host `cargo test` never executes the
//! instruction.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use crate::arch::cpu::{CPUID_ECX_X2APIC, CPUID_ECX_HYPERVISOR};
use crate::boot::ovmf_esp::{self, MIN_REAL_OVMF_BYTES};
use crate::memory::frame_allocator::{FrameAllocator, PhysFrame};
use crate::sched::msr_firewall::{self, CpuidRegs};
use crate::vmx::launch::{
    alias_ept_covers_reset, GuestUefiLaunchError, GUEST_UEFI_FIRMWARE_TOP_GPA,
    GUEST_UEFI_PRIVATE_VMCS_ID,
};

#[cfg(target_os = "uefi")]
use crate::arch::cpu::{
    self, adjust_vmx_controls, true_ctl_msrs_supported, IA32_EFER, IA32_FS_BASE,
    IA32_GS_BASE, IA32_PAT, IA32_SYSENTER_CS, IA32_SYSENTER_EIP, IA32_SYSENTER_ESP, IA32_VMX_CR0_FIXED0,
    IA32_VMX_CR0_FIXED1, IA32_VMX_CR4_FIXED0, IA32_VMX_CR4_FIXED1, IA32_VMX_ENTRY_CTLS,
    IA32_VMX_EXIT_CTLS, IA32_VMX_PINBASED_CTLS, IA32_VMX_PROCBASED_CTLS, IA32_VMX_PROCBASED_CTLS2,
    IA32_VMX_TRUE_ENTRY_CTLS, IA32_VMX_TRUE_EXIT_CTLS, IA32_VMX_TRUE_PINBASED_CTLS,
    IA32_VMX_TRUE_PROCBASED_CTLS,
};
#[cfg(target_os = "uefi")]
use crate::audit::AuditEvent;
#[cfg(target_os = "uefi")]
use crate::audit_log;
#[cfg(target_os = "uefi")]
use crate::boot::serial;
use crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES;
#[cfg(target_os = "uefi")]
use crate::memory::ept_hw::{self, frames_required_firmware_alias};
#[cfg(target_os = "uefi")]
use crate::vmx::fields::*;
#[cfg(target_os = "uefi")]
use crate::vmx::launch::{
    install_host_tss, prepare_vmcs_region, LaunchError, GUEST_UEFI_RESET_CS, GUEST_UEFI_RESET_RIP,
    GUEST_UEFI_RESET_VECTOR_GPA, GUEST_UEFI_UNRESTRICTED_GUEST,
};
#[cfg(target_os = "uefi")]
use crate::vmx::ops;

/// QEMU / serial marker when VMLAUNCH entered retained OVMF (not VM-entry fail).
pub const M7_E5_OVMF_VMLAUNCH_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK";

/// Honest residual. First guest-UEFI entry is not Everest E5.
pub const E5_OVMF_VMLAUNCH_RESIDUAL_NOTE: &str =
    "residual: private guest-UEFI VMCS + EPT VMLAUNCH of retained ESP OVMF.fd; CR4.VMXE host-owned + CR4.OSXSAVE host-owned so OVMF SEC mov cr4,0x640 does not #GP and CpuDxe mov cr4,0x668 does not clear OSXSAVE; COM1/COM2 forwarded; past-SEC when linear leaves last 64KiB and PEI PCI or firmware serial or HLT; attach_cdrom_uefi after FirmwareArmed is GuestVisible (PCI IDE/ATAPI; IDE at 00:00.1); unarmed stays UnsupportedOnFirmware; CMOS/fw_cfg/i440fx platform; i440FX host at 00:08.0; PEI 00:00.0 DID is i440FX 0x1237 (PlatformMemMapInitialization VGA IoMemory HOB at 0xA0000-1MiB, stock QEMU map, not merged [0, LowMemory)); DXE latches virtio 0x1042 on other-BDF CF8 (PciBus/BOTH-OK); virtio Header Type is multifunction so a walk finds IDE fn1; PIIX 00:01.1 is the same CD; PIIX4 PM at 00:01.3; remap i440FX DID in guest-private OVMF copy (cmp bx, not LZMA 37 12); CF8|CFC byte offset matches QEMU pci_host_data_read; EPT sink-resume for high MMIO; 4MiB flash window (VARS gap at 0xFFC00000); empty VARS _FVH; live HPET; HPET 1s step; stop RIP insn dump; spin jmp skip; past-PEI/DXE or CD boot attempt; empty virtio-blk at 00:00.0; fw_cfg bootorder CD then disk (PIIX ide@1,1 then virtio-fn1 ide@0,1, master drive@0, not slave drive@1; scsi-first skipped IDE Start); ACPI PM timer (port 0 dword + PIIX 0x408) so AcpiTimerLib Delay can end when DID is 0x1042; post-DXE spends the 32768-exit cap until ATAPI sectors>0 (not virtio-alone; not both-enum-alone; 1b07692 n=1111 BOTH then stopped with sectors=0; 8e55abf n=2048 ata=0 unh=0 still PciBus cf8=0x80000838 ISA 00:01.0 offset 0x38; 5d9e346 n=8192 ataio=0 unh=3 port=0xcf8 empty-slot walk + KBC; 8192-exit cap ended on CF8; 2674629 n=32768 ataio=0 acpi=16612 port=0 in eax,dx); PIIX3 ISA PIRQ 0x60-0x63 default 0x80; HPET 1s on preemption/HLT not PCI I/O; 8042 KBC 0x60/0x64; KeyboardWaitForValue; nested c19b91f BOTH-OK then n=32768 ataio=0 acpi=14903 port=0x64 (OBF never set after 0xAA); self-test 0x55 plus command ACK; ACPI PM 1s step; iron COM2 #UD RIP 0x109D pci_ide=0; iron 0ca02e6 skipped eb ec then #UD RIP 0x109D CR4=0x668 DebugLib dumped COM1 until cap; #UD intercept XSAVE retry/UD2 skip; iron d5f9431 #UD gone then n=1280..8192 reason=0x34 rip=0x6e81ca (pause CpuDeadLoop, no BOTH-OK); preempt pause/jcc skip; e2af81e missed GCC eb fc / 0F 84 rel32 (iron COM2 insn=ebec jmp -20); preempt eb/jcc32 skip; iron 891eb5b OSXSAVE CR4 intercept then skipped ebecc9c3 leave; ret then #UD 0x109D DAA PE header; do not skip jmp whose fallthrough is leave; ret; dump ASSERT retaddr; iron 17449e2 ASSERT noskip ret=0x6e8946 rip=0x6e81ca after host CPUID (Xeon topology+VMX); guest-UEFI CPUID uniprocessor hide VMX/x2APIC; FEATURE_CONTROL lock no VMX; iron ad78f12 CPUID uniprocessor then ASSERT ret=0x6e8946 after seven RDMSR 0x1B and CPUID 0x1cf11b5; xAPIC 2MiB was sink zeros (version 0); xAPIC 4K version 0x50014 not sink; iron 3f417ca xAPIC 4K mapped still ASSERT after MTRR walk 0xFE/0x2FF/0x250 (host MTRR passthrough + fixed reads 0); MTRR shadow VCNT=32 FIX WB VGA UC plus PCI hole UC 1GB at 0xC0000000; iron 408788c MTRR walk completed then still ASSERT ret=0x6e8946 after CPUID 0x1cf11b5 (not GetAllMtrrs); nested KVM sets hypervisor CPUID bit 31 plus KVMKVMKVM leaves, iron passthrough did not; guest-UEFI CPUID hypervisor present plus KVM signature; IA32_MISC_ENABLE shadowed not host; ASSERT dump callerrip plus home slots; unique RDMSR val=; iron 8700cbb hypervisor CPUID still ASSERT callerrip=0x1d25193 after WRMSR then RDMSR spin (MtrrLib WorkingRangeCount vs VCNT=8); fw_cfg bootorder NUL so ConnectDevicesFromQemu is not INVALID_PARAMETER; unique WRMSR; iron 0b7d647 VCNT=32 0xfe=0x520 PCI UC hole then firmware zeroed 0x200 still ASSERT callerrip=0x1d25193 lastmsr=EFER file=@B is pointer bytes; QEMU BOTH-OK skipped ebf3c9c3 (not ASSERT gone); EFER.LMA equals LME and CR0.PG plus IA-32e entry matches LMA; iron b4b4847 efer=0xd00 pg=1 csl=1 still ASSERT callerrip=0x1d25193 r8 is gPcdDataBaseSignatureGuid; debugcon 0x402 tee; unique CPUID; iron c40f4a8 pcdsig=1 after 32-pair MTRR walk still ASSERT; iron aee545f DXE assert skip caller=0x1d25193 then #UD linear=0x109d stop n=5364 sectors=0; revert iron ebec skip; MTRR power-on E=0 VCNT=8 no UC hole (firmware programs); iron 10cb881 VCNT=8 power-on still ASSERT callerrip=0x1d25193 mtrrdef=0xc06 mtrr0=0x80000000 noskip flood; VCNT=32 power-on no hole plus mtrr1/mtrrv dump; iron a9ffaa5 VCNT=32 power-on mtrr0=0x80000000 lastmsr=EFER still ASSERT; DxeCore CoreStartImage call EntryPoint (c6401801ff5020) loc2=ldri CpuDxe; MTRR GetAllMtrrs then paging refresh IsExecuteDisableEnabled; hide NX/1G/TME and strip EFER.NXE so CpuDxe does not ASSERT_EFI_ERROR SetMemorySpaceAttributes XP; dump ldri ImageBase; 80000008 subleaf 0; iron 5f59c86 efer=0x500 lastmsr=0x23f imgentry CpuDxe NXE-off still ASSERT (MtrrGetMemoryAttributes not XP); MAXPHYADDR [36,48] not clip-36; QEMU CI 17449e2 stuck ebf3c9c3 (jmp -13 leave;ret) — keep that skip (nested BOTH-OK); unguarded ebec skip was 891eb5b #UD; preempt noskip dump; guest-UEFI INVPCID/RDTSCP/XSAVES; XSETBV executes XCR0 (not skip_insn); fw_cfg etc/boot-menu-wait 0ms skip BdsWait; HLT skip so DXE can walk PCI; CR-access resume; firmware-simultaneous PCI enum; 8259 PIC RAZ/WI; fw_cfg etc/e820 32MiB; exception insn dump; ATAPI signature + PACKET interrupt-reason so firmware can READ(10); 8-byte IDE command BAR and BAR-relocated ATA; EXECUTE DEVICE DIAGNOSTIC 0x90 restores 0xEB14; BMIDE BAR4 RAZ/WI; first unhandled I/O traced; not firmware El Torito boot; not installer; not ISO-INSTALL-OK; no guest UEFI distro; iron d5fceb1 MAXPHYADDR unclip past CpuDxe then #PF err=0 CR2 0x80B000 MEMFD mov al,[disp32] (linear dump was RIP not CR2); identity_map_not_present NP 2M/4K in guest PT via ram_hpa; iron 3311ff3 #PF cr3=0x0 fail=alloc; build_identity_4g SEC PML4 0x800000; iron 7ea62ea fail=present SEC already mapped CR2 still VMWRITE CR3; iron 13e8bd2 CR3 identity then same #PF fail=present (walker present, CPU NP); rebuild SEC 4G identity once; hide LA57; iron COM2 after CR3 load #PF err=0x9 cr2=0xa027c8 (P+RSVD; NX-in-PTE with NXE=0); rebuild 4G on reserved-bit #PF; iron 101b8ec 4G n=1 n=2 then fail=present cr2=0x1ae7078 pde=0x30646870 (MEMFD heap clobber); HV identity PML4 at 0x200000 not 0x800000; e820 reserved 36KiB; always rebuild 4G; iron cc7d78a HV PML4 4G n=1 cr3=0x200000 then EPT violation gpa=0xc01df1b7 reason=0x30 (PCI hole; 4G identity present, EPT sink stopped at 1GiB); sink-resume PCI hole 0xC0000000; iron fdf07ba maps=4 EPT sink worked then #PF err=0x2 cr2=0x1e9000 pde=0xc0000083 4G n=2 then ASSERT callerrip=0x1d25193 lastmsr=0x23f mtrr0=0x80000000 (4G WB identity vs MTRR UC 2-4GiB); RAM-only identity leaves plus UC 2MiB sink #PF; nested 5db28e3 #PF cr2=0xffc00000 after RAM-only (flash NP) stop n=1007 BOTH missing; identity also maps flash 0xFFC00000 plus xAPIC 0xFEE00000; iron eb4b27d flash+xAPIC identity then #PF cr2=0x80000008 err=0xb pde=0xc0400083 (RSVD 1GiB PDPTE in MTRR UC hole); iron 73576cc bulk UC 2MiB identity then #PF cr2=0x1e9000 4G n=2 ASSERT callerrip=0x1d25193 lastmsr=0x23f (PAT UC- vs MTRR UC); iron a428202 on-demand mmio then #PF cr2=0x80000008 err=0xb pde=0xc0400083 identity MMIO fail (1GiB PDPTE after retargeted PDPT); split RSVD 1GiB into SEC PD even when PML4[0] is not pml4+0x1000; rebuild 4G then retry one PAT-UC 2MiB; hole stays NP at 4G rebuild; iron 124c1a8 identity MMIO n=2 then #PF cr2=0xffffffff96808086 err=0x2 pde=0 rip=0x300000 insn=afafafaf (sign-extended 32-bit 0x96808086 walks PML4[511] not low 4G); map high-half 2MiB to zero-extended GPA; e820 reserved 44KiB; iron b25d75b identity MMIO n=3 then #PF cr2=0x80000008 rip=0x30108e #UD linear=0x301093 insn=82bf (firmware PT stores at 0x80000008 hit shared HPET EPT sink); dedicated 2MiB UC scratch HPA for GPA 0x80000000 not zero sink; iron 577c9eb scratch 0x80000000 then EPT sink gpa=0xc0200000 then #PF cr2=0x9896808086 err=0x2 pde=0 rip=0x300001 insn=afafafaf (leftover-high 32-bit hole; PT stores at 0xC0200000 hit shared zero sink); scratch pool for hole PT pages except live HPET 2MiB; leftover-high CR2 overflow PML4[1]; poison-fill RIP not resume; iron 471391f pool=8 maps=2 then #PF cr2=0x1e9000 err=0x2 pde=0xc0000083 4G n=2 ASSERT callerrip=0x1d25193 lastmsr=0x23f; split 1GiB RAM PDPTE do not rebuild 4G; pre-scratch 0xC0000000+0xFCE00000; iron d757a0a SPLIT n=2 cr2=0x1e9000 then #PF err=0x9 pde=0xafafafafafafafaf cr2=0x1d1e6cb (firmware 0xAF-filled SEC PD after 1GiB); identity_refill_low4g_pd; stop n=1172 err=0x3 pde=0x1c000e7 rip=0x1de592 then E4 R640-BOOT-OK not Stage 44; iron 0bad45d refill then #PF 0x80000008 MMIO n=2 EPT scratch 0x80000000 plus 0xC0200000..0xC0A00000 then EPT sink gpa=0xc0c00000 leftover CR2 0x9896808086 rip=0xd00001 firmware-serial #DE RIP 0xCFFF9E DIV RCX=0 ASSERT ebec noskip; scratch pool 32 plus pre-scratch 0xC0000000..0xC0E00000 and 0x80000000; iron 5837243 pool=32 then EPT scratch walk 0xC1000000..0xC3A00000 then scratch cap gpa=0xc3c00000 sink RIP 0x3d00001 pci_ide=0; guest_uefi_ept_scratch_on_qual write/fetch only; EPT hole ro R+X sink for hole reads so a later store can upgrade; pre-scratch only 0x80000000; iron da2c9c4 pool=32 then EPT scratch 0xC0000000 plus 0x80000000 plus 0xC0200000..0xC3C00000 then scratch cap gpa=0xc3e00000 qual=0x184 RIP 0x3dfffff pci_ide=0; SDM bit 8 walk bits 2:0 are original access; guest_uefi_ept_scratch_on_qual is data-write only (not fetch); EPT hole ro R+X sink for hole reads so a later store can upgrade; iron f93caee write-only scratch then EPT hole ro gpa=0xc0000000 qual=0x184 plus scratch 0x80000000 plus hole ro 0xc0200000 plus scratch 0xc0000000 qual=0x1ab then #PF cr2=0x9896808086 rip=0x300001 insn=afafafaf poison fill (hole RO mapped live HPET SINK_HPA as PTEs); dedicated zero 2MiB for hole RO not SINK_HPA; HPET stays on SINK_HPA at 0xFEC00000/0xFED00000 only; not bulk 2-4GiB (73576cc ASSERT); not WB RAM (fdf07ba ASSERT); iron 06b011a hole-zero then #PF err=0x3 cr2=0x1d1abb8 pde=0x1c000e7 rip=0x1de592 (CR0.WP stack push in 2MiB identity; not leftover-high 0x9896808086); identity SPLIT4K 2MiB to 4K RW; nested Intel 06b011a BOTH-OK ataio=236 packet=0 (skip_insn after one word of rep insw); string/REP PIO so IDENTIFY lands; nested Intel 1e0f4a7 io string fw_cfg 0x511 then #PF cr2=0x205f18 4G n=2 cr2=-1 stop rip=0x28f402 BOTH missing; iron COM2 1e0f4a7 io string 0x511 n=4 then identity 4G n=1 cr2=0x80b000 ticks rip=0x3d2be4 ASSERT noskip callerrip=0x1f21193 lastmsr=0x23f mtrr0=0x80000000 imgentry=0x1dd97d3 pci_ide=0 (never SPLIT n=2); string RAM fill is ATA-only; iron COM2 54a8708 no 0x511 then identity SPLIT n=2 then SPLIT4K n=3 cr2=0x1d1abb8 pde=0x219067 then AlreadyPresent loop to identity cap n=256 stop n=1421 rip=0x1de592 pci_ide=0; SPLIT4K MOV CR3 after split; do not resume already-RW; iron COM2 19b0c11 hole-zero then identity MMIO n=2 cr2=0x80000008 then tick reason=0x34 rip=0x27e22d5 insn empty (RIP left 32MiB); hole RO was R+X so fetch executed dedicated zeros; leftover CR2 0x9896808086 rip=0x3ed00001 identity MMIO n=4..256 2MiB walk identity cap stop n=5687 pci_ide=0 then E4 R640-BOOT-OK not Stage 44; hole RO is R only (no X); do not identity-map [32MiB, 0x80000000); split PDPT[0] 1GiB on EPT, MOV CR3, and preemption while RIP is in 32MiB; iron COM2 89c3731 SPLIT PDPT0 then identity 4G n=1 hole ro then SPLIT4K n=2 cr2=0x1d1abb8 pde=0x219027 pte=0x1d1a067 already RW stop n=1168 rip=0x1de592 pci_ide=0 (RIP stayed in 32MiB; not 0x27e22d5); CR0.WP ANDs R/W through PML4/PDPT so OR walk R/W not only the 4K leaf; iron COM2 7413554 SPLIT4K n=2 resumed pml4e=0x5a6d (RO) pdpte=0x202067 then tick rip=0x1df1b5; then #PF cr2=0xfee00020 err=0x9 pdpte=0xc0600083 pml4e=0x5a6f stop n=1395 rip=0x1d84c7 pci_ide=0 (firmware 1GiB RSVD over xAPIC; not already-RW; not 0x27e22d5); map_mmio xAPIC RSVD 1GiB; iron COM2 32ee302 identity MMIO n=3 cr2=0xfee00020 then tick rip=0x1d6be4 then ASSERT noskip callerrip=0x1d25193 lastmsr=0x23f mtrrdef=0xc06 mtrr0=0x80000000 mtrr1=0x3fff80000800 imgentry=0x1bdd7d3 pci_ide=0 (WB xAPIC/flash 2MiB in MTRR UC 2-4GiB; not already-RW); PAT-UC PCD+PWT on flash+xAPIC identity; nested Intel 48c598a BOTH-OK ataio=1308 packet=0 insn=ef then edc9c3 (SET FEATURES 0xEF ABRT then IN EAX,DX poll; never PACKET); SET FEATURES succeeds DRDY not ABRT; iron COM2 855ba1c/48c598a PAT-UC then identity MMIO n=3 cr2=0xfee00020 ASSERT noskip callerrip=0x1d25193 lastmsr=0x23f mtrr1=0x3fff80000800 pci_ide=0 (PDPT[3] RSVD split; PDPT[2] 1GiB WB over 2-3GiB MTRR UC, no #PF); split sibling 1GiB in the UC hole; dump pdpte2; nested Intel 73ed589 BOTH-OK ATAPI-OK sectors=1 packet=9 scsi=0x28 ata=0xa0 ataio=982 then E4 Linux #DF vec=8 after BZIMAGE (OVMF XSETBV left host XCR0; E4 copies host CR4.OSXSAVE); restore host XCR0 and CR4.OSXSAVE after guest-UEFI before E4; iron COM2 pdpte2=0xc0400083 then MMIO n=4 pde=0xfee000ff ASSERT callerrip=0x1d25193 pci_ide=0 (CpuDxe software-walks 1GiB WB PDPT[2]; RAM SPLIT n=2 pdpt_i=0 never split the hole); identity_split_mtrr_uc_hole PDPT[2]+[3] on every identity map including 0x1e9000; dump pdpte2 after MMIO; iron COM2 8df2793 SPLIT PDPT0 then 4G n=1 then EPT hole ro gpa=0xc0000000 then SPLIT4K n=2 pdpte2=0x204067 (PD not 1GiB WB) no xAPIC #PF then ASSERT callerrip=0x1d25193 lastmsr=0x23f mtrr0=0x80000000 pci_ide=0 ataio=0 (CpuDxe software-walks NP 2-4GiB vs MTRR UC); PAT-UC 2-4GiB hole PCD+PWT at 4G rebuild not 73576cc UC-; dump pde8000 after 4G; iron COM2 d7bfb23 4G pde8000=0x800000ff SPLIT4K pml4e=0x5a6d pdpte2=0x204067 no xAPIC #PF still ASSERT callerrip=0x1d25193 (firmware PDPT 0x5000; PDPT[3] can stay 1GiB WB); identity_sync_live_mtrr_uc_hole live PDPT on SPLIT4K/4G/MMIO (not GPA 0x5000 until PML4[0] points there; not tick); dump pdpte3; iron COM2 1de9389 pdpte3=0x205067 PS clear pde8000=0x800000ff still ASSERT callerrip=0x1d25193 lastmsr=0x23f (1GiB PDPT[3] disproved); dump pml4e/pde8000/pdefee/pdeffc/pat at ASSERT; iron COM2 44c56db pde8000=0x800000ff pdpte3=0x205067 pdefee=0xfee000ff pdeffc=0xffc000ff pat=0x0 still ASSERT callerrip=0x1d25193 lastmsr=0x23f (VMCLEAR GUEST_IA32_PAT=0; Xeon VM-entry LOAD_PAT; PA0=UC vs MTRR WB RAM); init GUEST_IA32_PAT SDM reset 0x0007040600070406 plus HOST_IA32_PAT like E4 launch.rs; dump entry=; nested Intel 1a93cb8 ATAPI-OK sectors=1 then E4 #DF vec=8 cr4=0x2060 (startup_64 cr4&=0x1060 cleared OSFXSR); host-own OSFXSR+OSXMMEXCPT like VMXE; nested Intel ab25682 ATAPI-OK then ERROR unexpected CR-access rip=0x8400276 qual=0x4 cr4=0x2668 (startup_64 mov cr4 intercepted); emulate MOV CR4 keep VMXE+OSFXSR; iron COM2 1a93cb8 IA32_PAT guest=0x7010600070406 host=0x7010600070406 entry=0xd1fb then ASSERT pat=0x7010600070406 entry=0xd3fb pde8000=0x800000ff pdpte3=0x205067 lastmsr=0x23f mtrrdef=0xc06 mtrr0=0x80000000 (PAT WB proved; NP [32MiB, 2GiB) vs MTRR WB); guest PT WB [32MiB, 2GiB); do not EPT-map that window (89c3731); dump pde20; iron COM2 28f42d2 pde20=0x20000e7 pde8000=0x800000ff pat=0x7010600070406 still ASSERT callerrip=0x1d25193 lastmsr=0x23f (PDPT[0] mid-gap WB proved; live firmware PDPT[1] 1-2GiB NP vs MTRR WB); identity_ensure_pdpt_2m PDPT[1]; dump pde4000 pdpte1; iron COM2 be1b028 pde20=0x20000e7 pde4000=0x400000e7 pdpte1=0x203067 pde8000=0x800000ff pat=0x7010600070406 still ASSERT callerrip=0x1d25193 lastmsr=0x23f maxpa=46 mtrrdef=0xc06 pml4e=0x5a6f (0-4GiB guest PT matches MTRR WB+UC; NP [4GiB, 2^46) vs default WB; PML4E PWT); cap iron MAXPHYADDR 32 so GCD equals 4GiB identity (clip-36 left 4-64GiB NP); nested 36/40 stays; identity_clear_table_pwt_pcd live PML4E; iron COM2 162809f maxpa=32 mtrr1=0x80000800 pml4e=0x1a02023 (PWT clear) pde20=0x2000083 pde4000=0x400000e7 still ASSERT callerrip=0x1d25193 lastmsr=0x23f imgentry=0x6e87d3 no 4G n=1 (firmware PDPT 0x1a02000 sparse PDPT[0] NP vs MTRR WB); identity_refill_low4g_pd_keep_4k PDPT[0]; dump pde40 pdpte0 cr3; nested Intel 1b587dd BOTH-OK ataio=0 (ensure_pdpt_2m(0) on 1GiB retargeted SEC PD); keep_4k NP-only, do not split PDPT[0] 1GiB on sync; iron COM2 1b587dd/55d4dc6 keep_4k pde20=0x20000e7 pde40=0x40000e7 pde4000=0x400000e7 pde8000=0x800000ff maxpa=32 pml4e=0x1a02023 (no PWT) mtrr1=0x80000800 still ASSERT callerrip=0x1d25193 lastmsr=0x23f imgentry=0x6e87d3 pci_ide=0 ataio=0 (0-4GiB PT matches MTRR WB+UC; 2MiB at GPA 0 spans 1MiB fixed-MTRR); identity_split_gpa0_fixed_mtrr 4K at 0-2MiB; identity_clear_table_pwt_pcd also TABLE_FLAGS USER; dump pde0 pte0 pdpte2; iron COM2 659e7de SPLIT PDPT0 flood tick n=256 rip=0xfffcd6d6 (identity_map_mmio_2m 0x1E9000 smashed GPA0 4K every preempt); mmio 2m keeps 4K tables; nested Intel 61f84c6 GPA0 SPLIT4K then BOTH-OK pci_ide=1 ataio=0 3/3 (ATAPI-OK missing); guest_uefi_gpa0_fixed_mtrr_split iron maxpa=32 only; nested 36/40 keeps 2MiB at GPA 0; iron COM2 84171aa SPLIT4K GPA0 pde0=0x20b027 pte0=0x67 pde20=0x2000083 pde40=0x4000083 still ASSERT callerrip=0x1d25193 (GPA0 4K plus table USER proved; firmware 2MiB no USER); keep_4k OR LARGE_2M_FLAGS onto WB 2MiB (0x83 to 0xE7); dump pde6e; nested Intel 5811368 SPLIT4K GPA0 then BOTH-OK pci_ide=1 ataio=0 3/3 (host 46+ capped to 32); guest_uefi_gpa0_split_now skips GPA0 when host CPUID hypervisor bit is set; iron COM2 489d118 GPA0 4K pte0=0x67 pte1m=0x100067 pml4e1=0x0 pde20=0x20000e7 still ASSERT callerrip=0x1d25193 lastmsr=0x23f mtrr0=0x80000000 mtrr1=0x80000800 (0-4GiB PT matches MTRR; leftover-high NP; GCD untested spans PEI Uc32Base WB+UC); fw_cfg etc/e820 reserved PCI UC [2GiB,4GiB) so PlatformAddHobCB splits GCD at MTRR UC; iron COM2 38481d9 e820 type-2 reserved PCI UC still ASSERT pde8000=0x800000ff callerrip=0x1d25193 (GCD untested [32MiB,4GiB) mixed mid-gap WB + 4G PAT-UC; this OVMF.fd ignores type-2 below 4GiB); identity_set_pat_uc_hole WB 2-4GiB until firmware UC MTRR live then PAT-UC (not fdf07ba WB-while-UC-live; not 8df2793 NP); iron COM2 f07a597 PAT-UC+MTRR match still ASSERT pde8000=0x800000ff mtrr0=0x80000000 callerrip=0x1d25193 (guest PT family exhausted; GCD mixed range); hold valid UC variable MTRRs so CpuDxe RefreshGcd sees default WB (MTRR UC held (GCD)); guest_uefi_mtrr_set_admit_uc; iron COM2 22e0cb2 MTRR UC held mtrrv=0 pde8000=E7 still ASSERT callerrip=0x1d25193 (mixed MTRR disproved); e820 type-2 reserved [32MiB, 2GiB) mid-gap so GCD splits before Uc32Base (P3; 38481d9 PCI-hole type-2 ignored); iron COM2 f9a08c9 mid-gap reserved still ASSERT callerrip=0x1d25193 mtrrv=0 pde8000=E7 (e820 ignored); CMOS+fw_cfg LowMemory 2GiB so PEI HOB ends at Uc32Base (not EPT-map [32MiB, 2GiB)); iron COM2 fad19b2 CMOS 2GiB then EPT unbacked report-RAM gpa=0x7bddd000 reason=0x30 stop n=600 (firmware heap at top of LowMemory; ASSERT 0x1d25193 gone); lazy 2MiB WB EPT report-RAM pool (not identity 2GiB; not 89c3731); iron COM2 32e7d46 report-RAM pool=32 mapped gpa=0x7bddd000 then high heap; tick rip=0x7f8e21ca reason=0x34 same=376 lastmsr=0x23f insn empty (32MiB peek); peek report-RAM HPA for skip/ASSERT dump (do not skip ebecc9c3); iron COM2 957e0ad insn=ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f cr3=0x7fa01000 pml4e=0 pci_ide=0 (CpuDxe ASSERT relocated into report-RAM; 32MiB PT walk missed high CR3); P2 MTRR UC admitted (hold left mtrrv=0 vs GCD UC at 2GiB); dump-walk CR3 via report-RAM peek; E4 hide LA57 (nested Intel ATAPI-OK then #DF trampoline 0x9e036); iron COM2 c70768b MTRR UC admitted mtrrv=1 mtrr0=0x80000000 pde8000=0x80000083 cr3=0x7fa01000 pml4e=0x7fa02023 insn=ebecc9c3 callerrip=0x7fd25193 pci_ide=0 (live report-RAM CR3 WB 2MiB vs admitted UC; 32MiB identity_sync missed pml4>=ram_len); guest_uefi_pt_paint_live_uc_hole peek/poke PAT-UC on high CR3 (CMOS 2GiB GCD split; not f07a597 low-CR3 paint; not skip ebecc9c3); iron COM2 4ae87de painted n=1029 pde8000=0x800000ff mtrrv=1 then ASSERT insn=ebecc9c3 pde0=0xe3 pte0=0 (PAT-UC+MTRR match on live CR3; 2MiB GPA0 spans 1MiB fixed-MTRR; identity_split_gpa0 TableOutOfRam); guest_uefi_pt_split_gpa0 peek/poke HV PT 0x20B000 on live PD[0]; iron COM2 7e5d70f GPA0 4K live CR3 n=513 pde0=0x20b027 pte0=0x67 pte1m=0x100067 pde8000=0x800000ff still ASSERT insn=ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f pci_ide=0 (PT matches MTRR on live high CR3; stop PT peek/poke; CpuDxe RefreshGcd GCD/HOB); do not lower CMOS 2GiB (32MiB LowMemory already ASSERTed); do not retry P3 mid-gap type-2; iron c1476d3 hypervisor etc/e820 VGA hole logged but PEI never opened the file (CMOS size to HOBs to GCD, not ScanE820); PEI 00:00.0 DID i440FX 0x1237 so PlatformMemMapInitialization adds IoMemory 0xA0000-1MiB; DXE latches virtio 0x1042 on other-BDF CF8; do not remap cmp bx 0x1237 while PEI captures HostBridgeDevId; dump e820= fwdir= pei_did=; iron f7620f6 PEI pci cfg=0x80000002 val=0x1237 pei_did=1 DXE virtio DID latch 00:01.03 then virtio 0x1042 VIRTIO-OK DXE-OK sectors=0 plat=1 e820=0 fwdir=0 remap n=0 still ASSERT ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f pde0=0x20b027 pte0=0x67 pte1m=0x100067 (DID fork closed); iron d6b012a pte_a0000=0xa0067 pte_c0000=0xc0067 (GPA0 identity WB; firmware FIX 0x250-0x26f are 0x06 WB; not GCD VGA punch; do not PAT-UC VGA); filehex test r9d jnz wbinvd mov rax EFI_UNSUPPORTED is CpuFlush FlushType!=0 (P1 hold 22e0cb2 already disproved mixed MTRR); nop jnz in live report-RAM so every FlushType WBINVD; dump r9=; VMLAUNCH insn issued only when presence is true";

/// QEMU / serial marker when OVMF ran past the first triple-fault.
pub const M7_E5_OVMF_ALIVE_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ALIVE-OK";

/// QEMU / serial marker when OVMF left the SEC tail (not full DXE / not installer).
pub const M7_E5_OVMF_PAST_SEC_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-PAST-SEC-OK";

/// QEMU / serial marker when the guest-UEFI VMCS can see CD media.
pub const M7_E5_OVMF_CDROM_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-CDROM-OK";

/// QEMU / serial marker when PEI/DXE progressed or the guest attempted CD boot.
pub const M7_E5_OVMF_DXE_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-DXE-OK";

/// QEMU / serial marker when guest-UEFI sees empty virtio-blk + CD→disk order.
pub const M7_E5_OVMF_VIRTIO_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-VIRTIO-OK";

/// QEMU / serial marker when firmware enumerated virtio `00:00.0` and IDE `00:00.1`
/// on the same boot. Not ATAPI sectors. Not installer.
pub const M7_E5_OVMF_BOTH_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-BOTH-OK";

/// QEMU / serial marker when firmware issued ATAPI READ and `sectors>0`.
/// Not a completed El Torito CD boot. Not installer.
pub const M7_E5_OVMF_ATAPI_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ATAPI-OK";

/// Last 64 KiB of the 4 GiB space. OVMF 4M SEC / VTF lives here
/// (reset vector `0xFFFF_FFF0`; Stage 38 first exits at `0xFFFF_Fxxx`).
pub const GUEST_UEFI_SEC_TAIL_GPA: u64 = 0xFFFF_0000;

/// Resume cap after Stage 40's 256-exit window — PEI/DXE + PciBus + ATAPI.
/// Nested VT-x `5d9e346`: BOTH-OK then n=8192 `ataio=0` `unh=3`
/// `port=0xcf8` (empty-slot walk + KBC). Nested VT-x `2674629`:
/// 32768 still `ataio=0` `acpi=16612` `port=0` (BdsWait).
pub const GUEST_UEFI_RESUME_CAP: u32 = 32768;

/// After DXE evidence, spend the rest of [`GUEST_UEFI_RESUME_CAP`] unless firmware
/// actually read an ATAPI sector. Nested VT-x `1b07692`: BOTH-OK at n=1111
/// then the private VMCS stopped with `sectors=0` — PciBus never reached PACKET.
pub const GUEST_UEFI_POST_DXE_TAIL: u32 = GUEST_UEFI_RESUME_CAP;

/// Pin-based VMX-preemption timer (SDM 24.6.1 bit 6). Lets a HPET Delay
/// that never does I/O still VMEXIT so [`hpet_tick_sink`] can move time.
#[cfg(target_os = "uefi")]
const PIN_BASED_VMX_PREEMPTION_TIMER: u32 = 1 << 6;
#[cfg(target_os = "uefi")]
const VMX_PREEMPTION_TIMER_VALUE: u64 = 0x482E;
#[cfg(target_os = "uefi")]
const VMX_PREEMPTION_TIMER_TICKS: u64 = 0x0010_0000;
#[cfg(target_os = "uefi")]
const EXIT_REASON_PREEMPTION_TIMER: u32 = 52;

/// Guest-UEFI HLT must skip/resume. Stopping on HLT aborts the post-DXE
/// PciBus walk of IDE `00:00.1`. Not a timer inject. Not ATAPI.
pub fn hlt_should_resume() -> bool {
    true
}

/// DebugLib `CpuDeadLoop` is `jmp rel8` −13 (`eb f3`) or `jmp $` (`eb fe`).
/// Nested VT-x `707a849`: 1s HPET left `rip=0x6e812d insn=ebf3…` `pci_ide=0`.
/// Do **not** skip every backward `jmp rel8` on I/O exits — iron COM2
/// skipped a retry then #UD-dumped COM1 until the cap (`pci_ide=0`).
/// Delay `jcc` on I/O stays.
pub fn spin_short_jmp_should_skip(b0: u8, b1: u8) -> bool {
    b0 == 0xEB && (b1 == 0xF3 || b1 == 0xFE)
}

/// Preemption-only CpuDeadLoop (iron `d5f9431` n=1280..8192 `reason=0x34`
/// `rip=0x6e81ca`, one tick `0x6e81b8`). Ubuntu OVMF `CpuDeadLoop` is
/// `for (;;) CpuPause()`. GCC `-O2` is often `pause` + `jmp rel8` −4
/// (`eb fc`); MSVC/near is `0F 84` rel32. `e2af81e` skipped only
/// `pause` / `jcc rel8` / `eb f3`/`eb fe`, so `eb fc` stayed stuck.
/// Not used on I/O exits (Delay `jcc` stays; never skip every
/// backward `jmp rel8` on I/O — that #UD-dumped COM1).
/// Two-byte match does **not** see `leave; ret` fallthrough — use
/// [`preempt_deadloop_skip_len`].
pub fn preempt_deadloop_should_skip(b0: u8, b1: u8) -> bool {
    if b0 == 0xF3 && b1 == 0x90 {
        return true;
    }
    if b0 == 0xEB || (0x70..=0x7F).contains(&b0) {
        let off = b1 as i8;
        if off <= -2 && off >= -32 {
            return true;
        }
    }
    false
}

/// Skipping this insn would land on `leave; ret` (`c9 c3`).
pub fn insn_fallthrough_is_leave_ret(bytes: &[u8], insn_len: usize) -> bool {
    bytes.len() >= insn_len + 2 && bytes[insn_len] == 0xC9 && bytes[insn_len + 1] == 0xC3
}

/// Iron `891eb5b`/`17449e2`: `jmp rel8` −20 (`eb ec`) then `leave; ret`.
/// QEMU CI `17449e2` failed BOTH/ATAPI: Ubuntu OVMF is `jmp rel8` −13
/// (`eb f3`) then `leave; ret` (`insn=ebf3c9c3` `rip=0x6e812d`). Nested
/// `1b07692` BOTH-OK needed that `eb f3` skip so PciBus could walk fn1.
/// Only the iron −20 encoding is the PE-header `#UD` ASSERT; keep `eb f3`.
pub fn preempt_deadloop_is_assert_epilogue(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0xEB && bytes[1] == 0xEC && bytes[2] == 0xC9 && bytes[3] == 0xC3
}

/// First GPA treated as DXE RAM (above real-mode IVT / BIOS data).
/// Iron `c40f4a8` ASSERT RIP `0x6e81ca` and caller `0x1d25193` are in range;
/// PE-header `#UD` at `0x109D` is not.
pub const GUEST_UEFI_DXE_RAM_FLOOR: u64 = 0x10_0000;

/// True when `addr` is firmware DXE RAM, not flash and not the PE MZ stub.
///
/// INVARIANTS:
/// - `0x109D` (iron `891eb5b` `#UD`) is false
/// - Iron `c40f4a8` `callerrip=0x1d25193` is true
pub fn guest_uefi_assert_caller_is_dxe_ram(addr: u64) -> bool {
    addr >= GUEST_UEFI_DXE_RAM_FLOOR && addr < GUEST_UEFI_LOW_RAM_BYTES
}

/// Guarded skip of iron `eb ec` + `leave; ret`.
///
/// Iron `aee545f`: RIP/caller were in DXE RAM (`0x6e81ca` / `0x1d25193`);
/// skip then `#UD` at `0x109d` (same as unguarded `891eb5b`). Never skip
/// iron `ebecc9c3`. QEMU `eb f3` stays on [`preempt_deadloop_skip_len`].
pub fn preempt_deadloop_guarded_assert_skip_len(bytes: &[u8], rip: u64, caller: u64) -> u8 {
    let _ = (rip, caller);
    let _ = preempt_deadloop_is_assert_epilogue(bytes);
    0
}

/// Bytes to advance guest RIP on a preemption CpuDeadLoop match.
/// 2: `pause` / backward `jmp rel8` / backward `jcc rel8` (including QEMU
///    `eb f3` + `leave; ret`).
/// 6: near `jcc` (`0F 8x` rel32) with a small backward displacement.
/// 0: unknown, or iron `eb ec` + `leave; ret` without the DXE-RAM guard
///    ([`preempt_deadloop_guarded_assert_skip_len`]).
pub fn preempt_deadloop_skip_len(bytes: &[u8]) -> u8 {
    if bytes.len() < 2 {
        return 0;
    }
    if preempt_deadloop_is_assert_epilogue(bytes) {
        return 0;
    }
    if preempt_deadloop_should_skip(bytes[0], bytes[1]) {
        return 2;
    }
    if bytes.len() >= 6 && bytes[0] == 0x0F && (0x80..=0x8F).contains(&bytes[1]) {
        let disp = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        if disp <= -2 && disp >= -64 {
            return 6;
        }
    }
    0
}

/// XSETBV only accepts XCR0. Other XCRs would #GP; we skip those.
pub fn xsetbv_accepts_xcr(xcr: u32) -> bool {
    xcr == 0
}

/// Mask guest XSETBV XCR0 to host CPUID.0D:0. Bit 0 (x87) stays set.
/// AVX (bit 2) requires SSE (bit 1). Same rule as E4 `handle_xsetbv_and_resume`.
pub fn xsetbv_masked_xcr0(value: u64, host_mask: u64) -> u64 {
    let mut v = (value & host_mask) | 1;
    if v & 0x6 == 0x4 {
        v |= 0x2;
    }
    v
}

/// Host XCR0 to write after guest-UEFI. Uncaptured → x87 only (reset).
/// Nested Intel `73ed589`: ATAPI-OK then E4 Linux `#DF` vec=8 because
/// `handle_xsetbv` wrote host XCR0 and E4 copied host CR4.OSXSAVE.
pub fn e4_restore_xcr0_value(saved: u64, captured: bool, host_mask: u64) -> u64 {
    let v = if captured { saved } else { 1 };
    xsetbv_masked_xcr0(v, host_mask)
}

/// Host CR4 after guest-UEFI: OSXSAVE matches the pre-OVMF capture.
/// Clear only after XCR0 is restored (SDM: OSXSAVE off with AVX XCR0 is #GP).
pub fn e4_restore_cr4_osxsave(host_cr4: u64, had_osxsave: bool) -> u64 {
    if had_osxsave {
        host_cr4 | crate::arch::cpu::CR4_OSXSAVE
    } else {
        host_cr4 & !crate::arch::cpu::CR4_OSXSAVE
    }
}

/// IA32_FEATURE_CONTROL for guest-UEFI: locked, VMX off.
/// Raw `rdmsr` was 0 (unlocked); OVMF CpuDxe then ASSERTs. Nested QEMU
/// already presents a locked MSR. Do not `#GP` firmware.
pub const GUEST_UEFI_FEATURE_CONTROL_VALUE: u64 = 1;

/// CPUID.1:EDX bit 28 — HTT / multi-thread package.
pub const CPUID_EDX_HTT: u32 = 1 << 28;

/// KVM hypervisor CPUID leaf. Nested `-cpu host` exposes this; iron does not.
pub const GUEST_UEFI_KVM_CPUID_LEAF: u32 = 0x4000_0000;
/// `KVMK` in little-endian register bytes.
pub const GUEST_UEFI_KVM_EBX: u32 = 0x4B4D_564B;
/// `VMKV`.
pub const GUEST_UEFI_KVM_ECX: u32 = 0x564B_4D56;
/// `M\0\0\0`.
pub const GUEST_UEFI_KVM_EDX: u32 = 0x0000_004D;

/// IA32_MISC_ENABLE. Fast-Strings + TM1 + PerfMon + MONITOR (not Limit CPUID).
pub const GUEST_UEFI_MISC_ENABLE_MSR: u32 = 0x1A0;
pub const GUEST_UEFI_MISC_ENABLE_DEFAULT: u64 = 0x4_0089;

/// Filter guest-UEFI CPUID to a uniprocessor without VMX/x2APIC.
///
/// Iron `17449e2`: DXE CPUID at `rip=0x1cf11b5` then ASSERT `CpuDeadLoop`
/// `ret=0x6e8946` `rip=0x6e81ca`. Host passthrough advertised Xeon Silver
/// 4110 topology + VMX (16 threads). Nested QEMU is `-smp 1` without VMX
/// for L2, so BOTH-OK never hit this ASSERT.
/// Iron `408788c`: MTRR walk finished, then the same ASSERT after CPUID
/// `0x1cf11b5`. Nested KVM sets ECX.hypervisor (bit 31) and
/// `CPUID.40000000` = `KVMKVMKVM`; bare-metal iron did not.
/// Iron `8700cbb`: hypervisor CPUID + `KVMKVMKVM` still ASSERT
/// `callerrip=0x1d25193` after WRMSR then RDMSR spin. Not the KVM path.
/// Iron `0b7d647`: VCNT=32 (`0xfe=0x520`) and PCI UC hole present; firmware
/// then WRMSR-zeroed `0x200`/`0x201`. Same ASSERT `callerrip=0x1d25193`
/// `lastmsr=0xc0000080`. QEMU BOTH-OK skipped `eb f3` (not a real fix).
/// Iron `b4b4847`: EFER.LMA applied (`efer=0xd00` LME+LMA+NXE, `pg=1`
/// `csl=1`) — still the same ASSERT. Not EFER.LMA. `r8` bytes are
/// `gPcdDataBaseSignatureGuid`. Do not skip iron `eb ec`+`leave;ret`.
/// Iron `a9ffaa5`: MTRRs programmed (VCNT=32 power-on, firmware UC at
/// 2 GiB). `callerrip=0x1d25193` is DxeCore `CoreStartImage` after
/// `Image->EntryPoint` (`mov [img+0x18],1; call [img+0x20]`), `loc2s=ldri`.
/// `lastmsr=0xc0000080` is CpuDxe paging refresh reading EFER.NXE.
/// Iron `5f59c86`: NXE already off (`efer=0x500`) `imgentry=0x6e87d3`
/// CpuDxe still ASSERT `lastmsr=0x23f` (MtrrLib GetAll / GetMemoryAttributes,
/// not XP). Clip-36 left `[4GiB, 64GiB)` NP vs default WB — not a mask
/// bug vs `a9ffaa5`. Iron `be1b028` proved 0–4GiB guest PT (`pde20`
/// `pde4000` `pde8000`) then still ASSERT `maxpa=46` `mtrrdef=0xc06`
/// `pml4e=0x5a6f`. Cap iron width at 32 so GCD equals the 4GiB map;
/// nested 36/40 stays. Iron COM2 `84171aa`: GPA0 4K `pde0=0x20b027`
/// `pte0=0x67` `maxpa=32` still ASSERT `callerrip=0x1d25193` with
/// firmware 2 MiB `pde20=0x2000083` `pde40=0x4000083` (no USER).
/// keep_4k ORs `LARGE_2M_FLAGS` so those leaves become `0xE7`.
/// Iron COM2 `489d118`: GPA0 4K `pte0=0x67` `pte1m=0x100067` `pml4e1=0x0`
/// plus firmware 2 MiB `0xE7` still ASSERT — 0–4 GiB PT matches MTRR;
/// GCD untested `[32MiB, 4GiB)` spans PEI `Uc32Base` UC. `etc/e820`
/// reserved PCI UC `[2GiB, 4GiB)` did not split that descriptor
/// (iron `38481d9` same `pde8000=0x800000ff`). Iron `22e0cb2`: hold UC
/// (`MTRR UC held` `mtrrv=0` `pde8000=E7`) still ASSERT — mixed MTRR
/// disproved. Iron `f9a08c9` e820 type-2 mid-gap ignored (same dump).
/// CMOS/fw_cfg LowMemory 2 GiB (P5; not identity-map the gap).
/// Iron `fad19b2`: PEI used CMOS 2 GiB then EPT-stopped at `0x7bddd000`.
/// P6 lazy-maps 2 MiB WB HPA (not 2 GiB identity; not `89c3731`).
/// 4G paints 2–4GiB WB until firmware programs the UC pair
/// (`guest_uefi_mtrr_uc_hole_live`), then [`identity_set_pat_uc_hole`].
/// Iron `c70768b`: admit UC (`mtrrv=1` `mtrr0=0x80000000`) then ASSERT
/// `pde8000=0x80000083` `cr3=0x7fa01000` — live tables are report-RAM;
/// 32 MiB `identity_sync_live_mtrr_uc_hole` missed them. Peek/poke
/// [`guest_uefi_pt_paint_live_uc_hole`]. Do not skip `ebecc9c3`.
/// Keep NX/1G/TME hidden and NXE stripped;
/// 80000008 subleaf 0 with EAX[31:16] clear.
pub fn guest_uefi_filter_cpuid(leaf: u32, subleaf: u32) -> CpuidRegs {
    if leaf == GUEST_UEFI_KVM_CPUID_LEAF {
        return CpuidRegs {
            eax: GUEST_UEFI_KVM_CPUID_LEAF + 1,
            ebx: GUEST_UEFI_KVM_EBX,
            ecx: GUEST_UEFI_KVM_ECX,
            edx: GUEST_UEFI_KVM_EDX,
        };
    }
    if leaf == GUEST_UEFI_KVM_CPUID_LEAF + 1 {
        return CpuidRegs {
            eax: 0,
            ebx: 0,
            ecx: 0,
            edx: 0,
        };
    }
    let mut r = msr_firewall::filter_cpuid(leaf, subleaf);
    r.ecx &= !CPUID_ECX_X2APIC;
    match leaf {
        1 => {
            r.ebx = (r.ebx & 0xFFFF) | (1 << 16);
            r.edx &= !CPUID_EDX_HTT;
            r.ecx |= CPUID_ECX_HYPERVISOR;
        }
        4 => r.eax &= !(0x3F << 26),
        7 if subleaf == 0 => {
            r.ebx &= !((1 << 2) | (1 << 12) | (1 << 15));
            r.ecx &= !(CPUID_LEAF7_ECX_TME_EN | CPUID_LEAF7_ECX_LA57);
        }
        0x8000_0001 => {
            r.edx &= !(CPUID_80000001_EDX_NX | CPUID_80000001_EDX_PAGE1GB);
        }
        0xB | 0x1F => {
            r.eax = 0;
            r.ebx = 0;
            r.ecx = subleaf & 0xFF;
            r.edx = 0;
        }
        0x8000_0008 => {
            // Firmware often leaves ECX=0x80000008 from the previous leaf.
            let r8 = msr_firewall::filter_cpuid(0x8000_0008, 0);
            r.eax = guest_uefi_cpuid_80000008_eax(r8.eax);
            r.ebx = r8.ebx;
            r.ecx = r8.ecx;
            r.edx = r8.edx;
        }
        _ => {}
    }
    r
}

/// IA32_EFER LME / LMA. Guest-UEFI launch is unrestricted real mode
/// (`GUEST_IA32_EFER=0`, IA-32e entry off). SEC WRMSR `val=0x100` sets
/// LME only. CR0.PG writes do not exit (mask 0). Architectural LMA is
/// LME && CR0.PG — not CS.L (compatibility mode has CS.L=0 with LMA=1).
/// Iron `0b7d647` last MSR before ASSERT was EFER (`0xc0000080`).
/// Iron `a9ffaa5`: MTRR hole programmed, then `lastmsr=EFER` again —
/// CpuDxe `RefreshGcdMemoryAttributesFromPaging` / `IsExecuteDisableEnabled`.
pub const GUEST_UEFI_EFER_LME: u64 = 1 << 8;
pub const GUEST_UEFI_EFER_LMA: u64 = 1 << 10;
/// IA32_EFER.NXE (SDM 2.2.1 bit 11). CpuDxe paging refresh ORs
/// `EFI_MEMORY_XP` into GCD when NXE=1, then `ASSERT_EFI_ERROR` on
/// `SetMemorySpaceAttributes`. Guest-UEFI does not need NX.
pub const GUEST_UEFI_EFER_NXE: u64 = 1 << 11;
/// CPUID.80000001 EDX.NX (bit 20) and 1G pages (bit 26).
pub const CPUID_80000001_EDX_NX: u32 = 1 << 20;
pub const CPUID_80000001_EDX_PAGE1GB: u32 = 1 << 26;
/// CPUID.7.0 ECX.TME_EN (bit 13). MtrrLib subtracts KeyID bits from
/// MAXPHYADDR if this is set and `IA32_TME_ACTIVATE` enable is 1.
pub const CPUID_LEAF7_ECX_TME_EN: u32 = 1 << 13;
/// CPUID.7.0 ECX.LA57 (bit 16). Guest-UEFI is 4-level only; 5-level walks
/// would disagree with [`crate::vmx::guest_pt::identity_map_not_present`].
pub const CPUID_LEAF7_ECX_LA57: u32 = 1 << 16;
/// i440FX / nested QEMU floor. Clip-36 (`5f59c86`) still left
/// `[4GiB, 64GiB)` NP vs MTRR default WB.
pub const GUEST_UEFI_PHYS_BITS_MIN: u32 = 36;
/// 4-level paging cap (5-level LA57 stays hidden). Nested 36/40 stays here.
pub const GUEST_UEFI_PHYS_BITS_MAX: u32 = 48;
/// Iron Xeon 4110 is 46. Identity is PDPT[0..3] (PAGE1GB hidden). Cap so
/// MtrrLib does not walk NP `[4GiB, 2^46)` vs default WB (`be1b028`).
pub const GUEST_UEFI_PHYS_BITS_IRON_CAP: u32 = 32;

/// MAXPHYADDR for guest-UEFI CPUID.80000008 / MTRR masks.
///
/// INVARIANTS:
/// - Nested 36/40 stays in [`GUEST_UEFI_PHYS_BITS_MIN`, [`GUEST_UEFI_PHYS_BITS_MAX`]]
/// - Host 46+ (iron) is [`GUEST_UEFI_PHYS_BITS_IRON_CAP`] (4GiB identity)
/// - Not clip-36: that still left `[4GiB, 64GiB)` unmapped
pub fn guest_uefi_phys_bits(host_eax: u32) -> u32 {
    let host = host_eax & 0xFF;
    if host >= 46 {
        GUEST_UEFI_PHYS_BITS_IRON_CAP
    } else {
        host.clamp(GUEST_UEFI_PHYS_BITS_MIN, GUEST_UEFI_PHYS_BITS_MAX)
    }
}

/// Iron-only: split the 2 MiB at GPA 0 so paging does not span the 1 MiB
/// fixed-MTRR boundary. Nested Intel `61f84c6`: GPA0 SPLIT4K then BOTH-OK
/// `pci_ide=1` `ataio=0` 3/3 (ATAPI-OK missing). Nested Intel `5811368`:
/// host MAXPHYADDR 46+ was capped to 32, GPA0 ran anyway, BOTH-OK
/// `ataio=0` again. Width `<= 32` is not enough — skip when the host
/// CPUID.1 ECX.hypervisor bit is set (KVM nested). Iron R640 is bare
/// metal (bit clear) and still splits. Iron COM2 `84171aa`: GPA0 4K ran
/// (`pde0=0x20b027` `pte0=0x67`) and still ASSERT — not sufficient alone.
pub fn guest_uefi_gpa0_fixed_mtrr_split(width: u32) -> bool {
    width <= GUEST_UEFI_PHYS_BITS_IRON_CAP
}

/// GPA0 4K only on bare-metal iron (`maxpa=32`). Nested hypervisor skips.
pub fn guest_uefi_gpa0_split_now(width: u32, host_hypervisor: bool) -> bool {
    guest_uefi_gpa0_fixed_mtrr_split(width) && !host_hypervisor
}

/// Host CPUID.1 ECX.hypervisor — set under nested KVM, clear on iron.
pub fn guest_uefi_host_hypervisor_present() -> bool {
    guest_uefi_cpuid_has_hypervisor(msr_firewall::filter_cpuid(1, 0).ecx)
}

/// CPUID.80000008 EAX: phys + virt, reserved [31:16] clear.
pub fn guest_uefi_cpuid_80000008_eax(host_eax: u32) -> u32 {
    let virt = (host_eax >> 8) & 0xFF;
    guest_uefi_phys_bits(host_eax) | (virt << 8)
}

/// Variable MTRR mask: Valid (bit 11) plus address bits below MAXPHYADDR.
pub fn guest_uefi_mtrr_var_mask_sanitize(value: u64, phys_bits: u32) -> u64 {
    let addr = if (12..64).contains(&phys_bits) {
        ((1u64 << phys_bits) - 1) & !0xFFFu64
    } else {
        0x000F_FFFF_FFFF_F000
    };
    (value & (1 << 11)) | (value & addr)
}

/// OVMF 4M `[FD.MEMFD]` base (`0x800000`). Iron `d5fceb1` #PF operand sits here.
pub const GUEST_UEFI_MEMFD_BASE: u64 = 0x800000;
/// Hypervisor 4 GiB identity PML4. Below MEMFD so CpuDxe heap cannot clobber
/// CR3 (iron `101b8ec` `pde=0x30646870` at `0x800000`).
pub const GUEST_UEFI_HV_PML4: u64 = 0x200000;
/// Iron `d5fceb1`: `mov al,[0x80B000]` after CpuDxe. Dump `linear=` was RIP.
pub const GUEST_UEFI_IRON_PF_CR2: u64 = 0x80B000;
/// Iron COM2 after `13e8bd2` CR3 load: `#PF` `err=0x9` (P+RSVD) at MEMFD+0x2027c8.
/// NX-in-PTE with `EFER.NXE=0` (or other reserved PTE bits). Not `0x80B000`.
pub const GUEST_UEFI_IRON_PF_RSVD_CR2: u64 = 0xA027C8;
/// Iron `101b8ec`: after two 4G rebuilds at MEMFD, `#PF` `err=0` `cr2=0x1ae7078`
/// `pde=0x30646870` (heap overwrote SEC tables). `fail=present`.
pub const GUEST_UEFI_IRON_PF_HEAP_CR2: u64 = 0x1AE7078;
/// Iron `cc7d78a`: HV PML4 4G n=1 `cr3=0x200000`, then EPT violation
/// `gpa=0xC01DF1B7` `reason=0x30` (PCI hole; RIP was `0x1DF1B7`).
pub const GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA: u64 = 0xC01D_F1B7;
/// Iron `fdf07ba`: after identity 4G n=1, heap write `#PF` `err=0x2` (NP+W)
/// `cr2=0x1e9000` `pde=0xc0000083` then ASSERT `callerrip=0x1d25193`.
pub const GUEST_UEFI_IRON_PF_HEAP_WR_CR2: u64 = 0x1E9000;
/// Iron `d757a0a`: after SPLIT n=2, `#PF` `err=0x9` at the heap-write RIP.
pub const GUEST_UEFI_IRON_PF_POISON_CR2: u64 = 0x1D1_E6CB;
/// `pde=` dump when the SEC PD was 0xAF-filled.
pub const GUEST_UEFI_IRON_PF_POISON_PDE: u64 = 0xAFAF_AFAF_AFAF_AFAF;
/// Iron `eb4b27d`: flash+xAPIC identity then `#PF` `err=0xb` (P+W+RSVD)
/// `cr2=0x80000008` `pde=0xc0400083` (1GiB PDPTE, reserved bits 29:13).
pub const GUEST_UEFI_IRON_PF_MTRR_UC_CR2: u64 = 0x8000_0008;
/// Iron `124c1a8`: identity MMIO n=2 then `#PF` `err=0x2` `pde=0`
/// `cr2=0xffffffff96808086` (sign-extended 32-bit `0x96808086`; PML4[511]).
pub const GUEST_UEFI_IRON_PF_SIGNEXT_CR2: u64 = 0xFFFF_FFFF_9680_8086;
/// Iron `b25d75b`: identity MMIO n=3 then `#PF` `cr2=0x80000008` `rip=0x30108e`
/// and `#UD` `linear=0x301093` (`insn=82bf…`). Firmware stored page tables
/// at `0x80000008`; the shared EPT sink discarded those writes. Not RAM
/// (ADR-004 / `fdf07ba`). Dedicated 2 MiB scratch HPA, PAT-UC + EPT UC.
pub const GUEST_UEFI_IRON_MMIO_SCRATCH_GPA: u64 = 0x8000_0000;
/// Iron `577c9eb`: after scratch `0x80000000`, EPT sink `gpa=0xc0200000`
/// then `#PF` `cr2=0x9896808086` `rip=0x300001` `insn=afafafaf`.
pub const GUEST_UEFI_IRON_SINK_PT_GPA: u64 = 0xC020_0000;
/// Leftover high dword `0x98` plus 32-bit hole GPA `0x96808086`.
pub const GUEST_UEFI_IRON_PF_TRUNC32_CR2: u64 = 0x0000_0098_9680_8086;
/// Dedicated UC 2 MiB frames for hole PT **stores** (not the HPET zero page).
/// Iron `0bad45d`: pool=8 then `EPT sink gpa=0xc0c00000` after
/// `0x80000000`+`0xC0000000..0xC0A00000`; leftover CR2 then `#DE` RIP
/// `0xCFFF9E` `DIV RCX` with RCX=0 (sunk zeros).
/// Iron `5837243`: pool=32 then a sequential hole **read** walk
/// `0xC1000000..0xC3A00000` filled the pool; `EPT scratch cap`
/// `gpa=0xc3c00000` then sink; leftover CR2; RIP `0x3d00001`; stop n=1343
/// `reason=0x0` `pci_ide=0`. Scratch only on EPT write/fetch; hole reads
/// get an R+X sink so a later store can upgrade (do not bulk 2–4 GiB).
pub const GUEST_UEFI_MMIO_SCRATCH_SLOTS: usize = 32;
/// Lazy 2 MiB WB frames for CMOS/fw_cfg 2 GiB LowMemory that EPT does not
/// identity-map. Iron `fad19b2`: PEI used the 2 GiB lie, then EPT-stopped
/// at `gpa=0x7bddd000` (`reason=0x30` `n=600`; ASSERT `0x1d25193` gone).
/// 32 slots = 64 MiB. Do **not** identity-map `[32MiB, 2GiB)` (`89c3731`).
pub const GUEST_UEFI_REPORT_RAM_SLOTS: usize = 32;
pub const GUEST_UEFI_REPORT_RAM_PAGE: u64 = 0x20_0000;
/// Iron `fad19b2` first unbacked report-RAM GPA (top of 2 GiB LowMemory).
pub const GUEST_UEFI_IRON_REPORT_RAM_GPA: u64 = 0x7BDD_D000;
/// Iron `32e7d46`: after lazy WB map, CpuDeadLoop at top of LowMemory
/// (`reason=0x34` `same=376` `lastmsr=0x23f` `insn=` empty — 32 MiB peek).
pub const GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP: u64 = 0x7F8E_21CA;
/// Iron `957e0ad`: peek showed `insn=ebecc9c3` (noskip) and the same
/// CpuDxe ASSERT offset as `0x1d25193`, relocated into report-RAM.
pub const GUEST_UEFI_IRON_ASSERT_CALLER_RIP: u64 = 0x7FD2_5193;
/// Firmware CR3 in report-RAM (`gpa=0x7fa00000` + 4 KiB). 32 MiB
/// `identity_walk_*` printed `pml4e=0`.
pub const GUEST_UEFI_IRON_HIGH_CR3: u64 = 0x7FA0_1000;
/// Bits 51:12 of a paging-structure pointer (SDM; NX is bit 63).
pub const GUEST_UEFI_PT_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
pub const GUEST_UEFI_PT_PRESENT: u64 = 1;
pub const GUEST_UEFI_PT_RW: u64 = 1 << 1;
pub const GUEST_UEFI_PT_USER: u64 = 1 << 2;
pub const GUEST_UEFI_PT_PWT: u64 = 1 << 3;
pub const GUEST_UEFI_PT_PCD: u64 = 1 << 4;
pub const GUEST_UEFI_PT_ACCESSED: u64 = 1 << 5;
pub const GUEST_UEFI_PT_DIRTY: u64 = 1 << 6;
pub const GUEST_UEFI_PT_LARGE: u64 = 1 << 7;
/// P|RW|US|PWT|PCD|A|D|PS — PAT index 3 UC (`0xFF`).
pub const GUEST_UEFI_PT_LARGE_2M_UC: u64 = GUEST_UEFI_PT_PRESENT
    | GUEST_UEFI_PT_RW
    | GUEST_UEFI_PT_USER
    | GUEST_UEFI_PT_PWT
    | GUEST_UEFI_PT_PCD
    | GUEST_UEFI_PT_ACCESSED
    | GUEST_UEFI_PT_DIRTY
    | GUEST_UEFI_PT_LARGE;
/// Iron `c70768b`: live report-RAM CR3 2 MiB WB at 2 GiB (`pde8000=`).
pub const GUEST_UEFI_IRON_PDE8000_WB: u64 = 0x8000_0083;
/// Iron `4ae87de`: after PAT-UC paint `pde8000=0x800000ff` still ASSERT;
/// live CR3 `pde0=0xe3` (2 MiB spanning 1 MiB fixed-MTRR).
pub const GUEST_UEFI_IRON_PDE0_2M: u64 = 0xE3;
/// Non-leaf PDE pointing at a 4 K PT (P|RW|US|A).
pub const GUEST_UEFI_PT_TABLE: u64 =
    GUEST_UEFI_PT_PRESENT | GUEST_UEFI_PT_RW | GUEST_UEFI_PT_USER | GUEST_UEFI_PT_ACCESSED;
/// 4 K identity leaf (P|RW|US|A|D).
pub const GUEST_UEFI_PT_LEAF_4K: u64 = GUEST_UEFI_PT_TABLE | GUEST_UEFI_PT_DIRTY;
/// Iron `d6b012a` `filehex` at `rcx=0x7ee68fa0`: CpuFlush only WBINVD when
/// `FlushType==0`, else `mov rax, EFI_UNSUPPORTED` (`0x8000000000000003`).
/// `test r9d; jnz +4; wbinvd; jmp; mov rax, UNSUPPORTED`. P1 hold already
/// disproved mixed MTRR (`22e0cb2`). Do not skip `ebecc9c3`.
pub const GUEST_UEFI_CPU_FLUSH_UNSUPPORTED: &[u8] = &[
    0x45, 0x85, 0xC9, 0x75, 0x04, 0x0F, 0x09, 0xEB, 0x12, 0x48, 0xB8, 0x03, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
];
/// Offset of `jnz +4` (`75 04`) inside [`GUEST_UEFI_CPU_FLUSH_UNSUPPORTED`].
pub const GUEST_UEFI_CPU_FLUSH_JNZ_OFF: usize = 3;
/// Iron `d6b012a` decompressed CpuFlush in report-RAM.
pub const GUEST_UEFI_IRON_CPU_FLUSH_GPA: u64 = 0x7EE6_8FA0;

/// Nop `jnz` so CpuFlush WBINVD for every FlushType (EFI_UNSUPPORTED → SUCCESS).
///
/// Iron `d6b012a`: `pte_a0000=0xa0067` is GPA0 identity WB, firmware FIX
/// MTRRs are `0x06` WB — not a GCD VGA punch. `filehex` is this stub.
/// Pattern is LZMA-compressed in `OVMF.fd`; patch live report-RAM.
pub fn guest_uefi_patch_cpu_flush_unsupported(buf: &mut [u8]) -> u32 {
    let pat = GUEST_UEFI_CPU_FLUSH_UNSUPPORTED;
    let jnz = GUEST_UEFI_CPU_FLUSH_JNZ_OFF;
    if pat.len() < jnz.saturating_add(2) {
        return 0;
    }
    let mut n = 0u32;
    let mut i = 0usize;
    while i.saturating_add(pat.len()) <= buf.len() {
        if &buf[i..i + pat.len()] == pat {
            buf[i + jnz] = 0x90;
            buf[i + jnz + 1] = 0x90;
            n = n.saturating_add(1);
            i = i.saturating_add(pat.len());
        } else {
            i = i.saturating_add(1);
        }
    }
    n
}

/// EPT leaf memory type WB. Scratch/sink stay UC (`ept_leaf_large(..., 0)`).
pub const GUEST_UEFI_EPT_MT_WB: u64 = 6;
/// Iron `0bad45d`: first hole GPA that hit the shared zero sink.
pub const GUEST_UEFI_IRON_SCRATCH_CAP_GPA: u64 = 0xC0C0_0000;
/// Iron `5837243`: first hole GPA that exhausted pool=32 (read walk).
pub const GUEST_UEFI_IRON_SCRATCH_WALK_GPA: u64 = 0xC3C0_0000;
/// Iron `da2c9c4`: fetch+walk (qual bit 8) exhausted pool=32.
pub const GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA: u64 = 0xC3E0_0000;
/// SDM 28.2.1: bit 2 fetch + bit 7 GLA valid + bit 8 paging-structure.
pub const GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK: u64 = 0x184;
/// Iron `f93caee`: A/D RMW walk (bits 0+1+3+5+7+8 = `0x1ab`). Scratch (bit 1).
pub const GUEST_UEFI_IRON_EPT_QUAL_AD_WALK: u64 = 0x1AB;
/// Iron `f93caee`: hole RO mapped live HPET `SINK_HPA`; leftover CR2 then this RIP.
pub const GUEST_UEFI_IRON_HOLE_RO_HPET_RIP: u64 = 0x300001;
/// Iron `06b011a` / `d757a0a`: present+write `#PF` after RAM 1GiB SPLIT.
pub const GUEST_UEFI_IRON_PF_WP_CR2: u64 = 0x1D1_ABB8;
pub const GUEST_UEFI_IRON_PF_WP_RIP: u64 = 0x1DE_592;
pub const GUEST_UEFI_IRON_PF_WP_ERR: u64 = 0x3;
pub const GUEST_UEFI_IRON_PF_WP_PDE: u64 = 0x1C0_00E7;
/// Iron `54a8708`: first SPLIT4K PDE (PT `0x219000` | table flags, no PS).
pub const GUEST_UEFI_IRON_PF_WP_SPLIT_PDE: u64 = 0x219067;
/// Iron `7413554`: stack `#PF` PML4E before walk R/W (`PDPT` at `0x5000`, R/W=0).
pub const GUEST_UEFI_IRON_PF_WP_PML4E_RO: u64 = 0x5A6D;
/// Iron `7413554`: `GetApicVersion` `MOV EAX,[RCX]` at xAPIC ID (`0xFEE00020`).
pub const GUEST_UEFI_IRON_PF_XAPIC_CR2: u64 = 0xFEE0_0020;
pub const GUEST_UEFI_IRON_PF_XAPIC_ERR: u64 = 0x9;
pub const GUEST_UEFI_IRON_PF_XAPIC_PDPTE: u64 = 0xC060_0083;
pub const GUEST_UEFI_IRON_PF_XAPIC_RIP: u64 = 0x1D8_4C7;

/// Iron `54a8708`: after SPLIT4K the 4K PTE is already RW; resuming
/// `AlreadyPresent` looped to the identity cap (`n=256`). Do not resume.
pub fn guest_uefi_pf_split4k_resume_already_rw() -> bool {
    false
}
/// Iron `19b0c11`: after MMIO n=2, preemption RIP `0x27e22d5` (empty insn).
/// Hole RO was R+X so fetch executed dedicated zeros.
pub const GUEST_UEFI_IRON_HOLE_X_RIP: u64 = 0x27E_22D5;
/// Iron `19b0c11`: leftover CR2 then this RIP; identity MMIO n=4..256.
pub const GUEST_UEFI_IRON_ZERO_FILL_RIP: u64 = 0x3ED0_0001;
/// SDM 28.2.1 I/O qualification bit 4 = string, bit 5 = REP.
pub const GUEST_UEFI_IO_QUAL_STRING: u64 = 1 << 4;
pub const GUEST_UEFI_IO_QUAL_REP: u64 = 1 << 5;
/// Cap one VMEXIT of `rep insw` (IDENTIFY is 256 words; CD sector is 1024).
pub const GUEST_UEFI_IO_STRING_CAP: u64 = 4096;
/// Nested Intel `06b011a`: `rep insw` IDENTIFY from ATA data `0x1F0`.
pub const GUEST_UEFI_IO_QUAL_REP_INSW_1F0: u64 =
    (0x1F0 << 16) | GUEST_UEFI_IO_QUAL_REP | GUEST_UEFI_IO_QUAL_STRING | (1 << 3) | 1;

/// Hole PT pages persist on scratch HPA. Live HPET/IOAPIC 2 MiB stays the
/// shared sink (`0xFEC00000`). Not bulk 2–4 GiB (`73576cc` ASSERT).
pub fn guest_uefi_mmio_needs_scratch(gpa: u64) -> bool {
    let g = guest_uefi_pf_gpa32(gpa);
    crate::devices::guest_platform::is_platform_sink_gpa(g)
        && (g & !0x1F_FFFF) != crate::devices::guest_platform::HPET_SINK_PAGE
}

/// GPA in the 2 GiB LowMemory lie that launch does not identity-map.
///
/// INVARIANTS:
/// - Iron `fad19b2` `0x7bddd000` is true
/// - `[0, 32MiB)` and `[2GiB, …)` are false
pub fn guest_uefi_report_ram_should_map(gpa: u64) -> bool {
    crate::devices::guest_platform::is_unbacked_report_ram_gpa(gpa)
}

/// 2 MiB-align a report-RAM GPA. Iron `fad19b2`: `0x7bddd000` → `0x7BC00000`.
pub fn guest_uefi_report_ram_gpa_2m(gpa: u64) -> u64 {
    gpa & !(GUEST_UEFI_REPORT_RAM_PAGE - 1)
}

/// Offset within a 2 MiB report-RAM leaf. Iron `32e7d46`: `0x7f8e21ca` → `0xE21CA`.
pub fn guest_uefi_report_ram_page_off(gpa: u64) -> u64 {
    gpa & (GUEST_UEFI_REPORT_RAM_PAGE - 1)
}

/// GPA of the PML4E that maps `gva`. Iron `957e0ad`: CR3 `0x7fa01000`.
pub fn guest_uefi_pt_pml4e_gpa(cr3: u64, gva: u64) -> u64 {
    (cr3 & GUEST_UEFI_PT_ADDR_MASK).wrapping_add(((gva >> 39) & 0x1ff) * 8)
}

/// Read a PML4E via `peek` (32 MiB identity or mapped report-RAM).
pub fn guest_uefi_pt_walk_pml4e<F: Fn(u64) -> u64>(peek: F, cr3: u64, gva: u64) -> u64 {
    peek(guest_uefi_pt_pml4e_gpa(cr3, gva))
}

/// Read a PDPTE (or the 1 GiB leaf) via `peek`.
pub fn guest_uefi_pt_walk_pdpte<F: Fn(u64) -> u64>(peek: F, cr3: u64, gva: u64) -> u64 {
    let e4 = guest_uefi_pt_walk_pml4e(&peek, cr3, gva);
    if (e4 & GUEST_UEFI_PT_PRESENT) == 0 {
        return 0;
    }
    peek((e4 & GUEST_UEFI_PT_ADDR_MASK).wrapping_add(((gva >> 30) & 0x1ff) * 8))
}

/// Read a PDE (or the 2 MiB / 1 GiB leaf) via `peek`.
pub fn guest_uefi_pt_walk_pde<F: Fn(u64) -> u64>(peek: F, cr3: u64, gva: u64) -> u64 {
    let e3 = guest_uefi_pt_walk_pdpte(&peek, cr3, gva);
    if (e3 & GUEST_UEFI_PT_PRESENT) == 0 || (e3 & GUEST_UEFI_PT_LARGE) != 0 {
        return e3;
    }
    peek((e3 & GUEST_UEFI_PT_ADDR_MASK).wrapping_add(((gva >> 21) & 0x1ff) * 8))
}

/// Read a 4 KiB PTE via `peek`, or 0 if the walk is still a large leaf.
pub fn guest_uefi_pt_walk_pte<F: Fn(u64) -> u64>(peek: F, cr3: u64, gva: u64) -> u64 {
    let e2 = guest_uefi_pt_walk_pde(&peek, cr3, gva);
    if (e2 & GUEST_UEFI_PT_PRESENT) == 0 || (e2 & GUEST_UEFI_PT_LARGE) != 0 {
        return 0;
    }
    peek((e2 & GUEST_UEFI_PT_ADDR_MASK).wrapping_add(((gva >> 12) & 0x1ff) * 8))
}

/// 2 MiB leaf in `[2GiB, 4GiB)` that is not PAT-UC (iron `c70768b` `0x80000083`).
pub fn guest_uefi_pt_pde_is_wb_hole(e: u64) -> bool {
    (e & GUEST_UEFI_PT_PRESENT) != 0
        && (e & GUEST_UEFI_PT_LARGE) != 0
        && (e & (GUEST_UEFI_PT_PCD | GUEST_UEFI_PT_PWT))
            != (GUEST_UEFI_PT_PCD | GUEST_UEFI_PT_PWT)
}

/// PAT-UC 2 MiB leaf for a hole GPA (PCD+PWT, not 73576cc UC-).
pub fn guest_uefi_pt_pde_pat_uc(gpa: u64) -> u64 {
    (gpa & !0x1F_FFFF) | GUEST_UEFI_PT_LARGE_2M_UC
}

/// Clear PWT/PCD on a table entry and set USER (PageTableLib ANDs U/S).
pub fn guest_uefi_pt_table_user(e: u64) -> u64 {
    if (e & GUEST_UEFI_PT_PRESENT) == 0 || (e & GUEST_UEFI_PT_LARGE) != 0 {
        e
    } else {
        (e & !(GUEST_UEFI_PT_PWT | GUEST_UEFI_PT_PCD)) | GUEST_UEFI_PT_USER
    }
}

/// Paint live CR3 `[2GiB, 4GiB)` 2 MiB leaves to PAT-UC via peek/poke.
///
/// Iron `c70768b`: P2 admit UC (`mtrrv=1` `mtrr0=0x80000000`) then ASSERT
/// `insn=ebecc9c3` `pde8000=0x80000083` `cr3=0x7fa01000`. 32 MiB
/// `identity_sync_live_mtrr_uc_hole` returns 0 (`pml4 >= ram_len`).
/// f07a597 PAT-UC matched MTRR on **low** CR3 and still ASSERTed because
/// GCD mixed `[32MiB, 4GiB)`. CMOS 2 GiB split GCD; this paints the high
/// CR3. Do **not** skip `ebecc9c3`. Do **not** tick-sync 1GiB (`1de9389`).
pub fn guest_uefi_pt_paint_live_uc_hole<Peek, Poke>(peek: Peek, poke: Poke, cr3: u64) -> u32
where
    Peek: Fn(u64) -> u64,
    Poke: Fn(u64, u64) -> bool,
{
    if cr3 == 0 {
        return 0;
    }
    let mut n = 0u32;
    let pml4 = cr3 & GUEST_UEFI_PT_ADDR_MASK;
    let e4 = peek(pml4);
    if (e4 & GUEST_UEFI_PT_PRESENT) == 0 || (e4 & GUEST_UEFI_PT_LARGE) != 0 {
        return 0;
    }
    let e4c = guest_uefi_pt_table_user(e4);
    if e4c != e4 && poke(pml4, e4c) {
        n = n.saturating_add(1);
    }
    let pdpt = e4c & GUEST_UEFI_PT_ADDR_MASK;
    if pdpt == 0 {
        return n;
    }
    for i in 0..4u64 {
        let slot = pdpt.wrapping_add(i * 8);
        let e = peek(slot);
        let c = guest_uefi_pt_table_user(e);
        if c != e && poke(slot, c) {
            n = n.saturating_add(1);
        }
    }
    for pdpt_i in 2..=3u64 {
        let e3 = peek(pdpt.wrapping_add(pdpt_i * 8));
        let e3c = guest_uefi_pt_table_user(e3);
        if (e3c & GUEST_UEFI_PT_PRESENT) == 0 || (e3c & GUEST_UEFI_PT_LARGE) != 0 {
            continue;
        }
        let pd = e3c & GUEST_UEFI_PT_ADDR_MASK;
        if pd == 0 {
            continue;
        }
        for i in 0..512u64 {
            let gpa = (pdpt_i * 512 + i) * GUEST_UEFI_REPORT_RAM_PAGE;
            let slot = pd.wrapping_add(i * 8);
            let e = peek(slot);
            if e != 0 && !guest_uefi_pt_pde_is_wb_hole(e) {
                continue;
            }
            let want = guest_uefi_pt_pde_pat_uc(gpa);
            if e != want && poke(slot, want) {
                n = n.saturating_add(1);
            }
        }
    }
    n
}

/// HV SPLIT4K PT for GPA 0 (`0x20B000`). Live high CR3 points PD[0] here.
pub fn guest_uefi_gpa0_split_pt_gpa() -> u64 {
    crate::vmx::guest_pt::identity_split_pt_gpa(GUEST_UEFI_HV_PML4, 0)
}

/// True when PD[0] is a 2 MiB identity leaf (iron `4ae87de` `pde0=0xe3`).
/// Phys must be 0 — a 2 MiB leaf at another frame is not GPA 0 identity.
pub fn guest_uefi_pt_pde0_is_2m(e: u64) -> bool {
    (e & GUEST_UEFI_PT_PRESENT) != 0
        && (e & GUEST_UEFI_PT_LARGE) != 0
        && (e & GUEST_UEFI_PT_ADDR_MASK) == 0
}

/// Split live CR3 GPA 0 from 2 MiB to 4 K so no leaf spans 1 MiB fixed-MTRR.
///
/// Iron `4ae87de`: `MTRR UC live PT painted n=1029` `pde8000=0x800000ff`
/// `mtrr0=0x80000000` then ASSERT `insn=ebecc9c3` `pde0=0xe3` `pte0=0`.
/// PAT-UC+MTRR match on high CR3 (and CMOS 2 GiB GCD) still ASSERTs.
/// `identity_split_gpa0_fixed_mtrr` returns TableOutOfRam for
/// `cr3=0x7fa01000`. Fill the HV PT at [`guest_uefi_gpa0_split_pt_gpa`]
/// and poke live PD[0]. Do **not** skip `ebecc9c3`. Nested skips via
/// [`guest_uefi_gpa0_split_now`].
pub fn guest_uefi_pt_split_gpa0<Peek, Poke>(
    peek: Peek,
    poke: Poke,
    cr3: u64,
    pt_gpa: u64,
) -> u32
where
    Peek: Fn(u64) -> u64,
    Poke: Fn(u64, u64) -> bool,
{
    if cr3 == 0 || pt_gpa == 0 || (pt_gpa & 0xFFF) != 0 {
        return 0;
    }
    let e4 = guest_uefi_pt_walk_pml4e(&peek, cr3, 0);
    if (e4 & GUEST_UEFI_PT_PRESENT) == 0 || (e4 & GUEST_UEFI_PT_LARGE) != 0 {
        return 0;
    }
    let e3 = guest_uefi_pt_walk_pdpte(&peek, cr3, 0);
    if (e3 & GUEST_UEFI_PT_PRESENT) == 0 || (e3 & GUEST_UEFI_PT_LARGE) != 0 {
        return 0;
    }
    let pd = e3 & GUEST_UEFI_PT_ADDR_MASK;
    if pd == 0 {
        return 0;
    }
    let e2 = peek(pd);
    if !guest_uefi_pt_pde0_is_2m(e2) {
        return 0;
    }
    let mut n = 0u32;
    for i in 0..512u64 {
        let leaf = (i * 4096) | GUEST_UEFI_PT_LEAF_4K;
        if poke(pt_gpa.wrapping_add(i * 8), leaf) {
            n = n.saturating_add(1);
        }
    }
    if poke(pd, pt_gpa | GUEST_UEFI_PT_TABLE) {
        n = n.saturating_add(1);
    }
    n
}

/// SDM 28.2.1 EPT-violation qualification bit 1 is a data write.
pub fn guest_uefi_ept_qual_is_write(qual: u64) -> bool {
    (qual & (1 << 1)) != 0
}

/// SDM 28.2.1 EPT-violation qualification bit 2 is an instruction fetch.
pub fn guest_uefi_ept_qual_is_fetch(qual: u64) -> bool {
    (qual & (1 << 2)) != 0
}

/// SDM 28.2.1 bit 8: GPA is a guest paging-structure entry. Bits 2:0 are
/// the **original** access (read/write/fetch), not a store to that GPA.
pub fn guest_uefi_ept_qual_is_walk(qual: u64) -> bool {
    (qual & (1 << 8)) != 0
}

/// Scratch HPA is for **data writes** to the hole (firmware PT stores).
/// Iron `da2c9c4`: `guest_uefi_ept_scratch_on_qual` also treated fetch as
/// scratch; sequential `0xC0000000..0xC3C00000` then cap `gpa=0xc3e00000`
/// `qual=0x184` (fetch + GLA valid + bit 8 walk) RIP `0x3dfffff`. Bit 8
/// walks **read** the PTE; bits 2:0 are the original access. Do not
/// scratch fetch or paging-structure reads (do not bulk 2–4 GiB).
pub fn guest_uefi_ept_scratch_on_qual(qual: u64) -> bool {
    guest_uefi_ept_qual_is_write(qual)
}

/// Hole RO is a data-read of zeros (PTE walks). Instruction fetch of that
/// GPA must not execute the dedicated zero page.
///
/// Iron `19b0c11`: `qual=0x184` walks stay; fetch-without-walk at
/// `rip=0x27e22d5` executed zeros then MMIO n=4..256.
pub fn guest_uefi_ept_hole_ro_on_qual(qual: u64) -> bool {
    !(guest_uefi_ept_qual_is_fetch(qual) && !guest_uefi_ept_qual_is_walk(qual))
}

/// Dedicated-zero hole RO is read-only, never executable.
///
/// INVARIANTS:
/// - Iron `19b0c11` R+X fetch of `0x27e22d5` is false
///
/// VERIFICATION: L1 (host tests)
pub fn guest_uefi_ept_hole_ro_allows_execute() -> bool {
    false
}

/// RIP has left the 32 MiB guest-UEFI slab into the identity gap.
///
/// Iron `19b0c11`: `rip=0x27e22d5` then `0x3ed00001` then 2 MiB MMIO walk.
pub fn guest_uefi_rip_is_hole_execute(rip: u64) -> bool {
    rip >= GUEST_UEFI_LOW_RAM_BYTES
        && rip < crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR
}

/// Hole RO identity must be a dedicated zero frame, never the live HPET sink.
///
/// INVARIANTS:
/// - Iron `f93caee` `EPT hole ro` onto `SINK_HPA` (HPET as PTEs) is false
/// - Distinct non-zero HPAs is true
///
/// VERIFICATION: L1 (host tests)
pub fn guest_uefi_hole_ro_uses_dedicated_zero(hole_zero_hpa: u64, sink_hpa: u64) -> bool {
    hole_zero_hpa != 0 && sink_hpa != 0 && hole_zero_hpa != sink_hpa
}

/// SDM 27.2.1 #PF bits 0+1 set, reserved (bit 3) clear: present+write.
pub fn guest_uefi_pf_error_is_present_write(err: u64) -> bool {
    (err & 1) != 0 && (err & 2) != 0 && (err & GUEST_UEFI_PF_ERR_RSVD) == 0
}

/// Iron `06b011a`: `err=0x3` stack write in the 32 MiB slab. Not MMIO.
/// Do not identity-map every present+write `#PF` (protection faults stay).
pub fn guest_uefi_pf_should_fix_ram_wp(err: u64, cr2: u64) -> bool {
    guest_uefi_pf_error_is_present_write(err) && cr2 < GUEST_UEFI_LOW_RAM_BYTES
}

/// SDM 28.2.1 I/O qualification bit 4.
pub fn guest_uefi_io_qual_is_string(qual: u64) -> bool {
    (qual & GUEST_UEFI_IO_QUAL_STRING) != 0
}

/// SDM 28.2.1 I/O qualification bit 5.
pub fn guest_uefi_io_qual_is_rep(qual: u64) -> bool {
    (qual & GUEST_UEFI_IO_QUAL_REP) != 0
}

/// How many I/O iterations this VMEXIT should emulate.
///
/// Nested Intel `06b011a`: `ataio=236` then `packet=0` because `skip_insn`
/// advanced past the whole `rep insw` after one 16-bit `ata_io`.
pub fn guest_uefi_io_string_count(qual: u64, rcx: u64) -> u64 {
    if !guest_uefi_io_qual_is_string(qual) {
        return 1;
    }
    if !guest_uefi_io_qual_is_rep(qual) {
        return 1;
    }
    if rcx == 0 {
        return 0;
    }
    rcx.min(GUEST_UEFI_IO_STRING_CAP)
}

/// Nested Intel `1e0f4a7`: `rep insw` on fw_cfg `0x511` wrote guest RAM
/// and then `#PF` `cr2=0x205f18` (inside HV identity tables) `4G n=2`,
/// `cr2=-1`, stop `rip=0x28f402` `BOTH` missing. Iron COM2 same commit:
/// `io string 0x511` then CpuDxe ASSERT `callerrip=0x1f21193`
/// `lastmsr=0x23f` `mtrr0=0x80000000` `imgentry=0x1dd97d3` `rip=0x3d2be4`
/// `pci_ide=0` (never `identity SPLIT n=2`). Only ATA command-block
/// FIFOs (`IoReadFifo16` IDENTIFY / PACKET) fill RAM. Other string I/O
/// stays one-shot + `skip_insn` (same as `06b011a`).
pub fn guest_uefi_io_string_fills_ram(port: u16) -> bool {
    (0x01F0..=0x01F7).contains(&port) || (0x0170..=0x0177).contains(&port)
}

/// Compatibility/protected mode uses EDI/ESI/ECX, not the 64-bit regs.
pub fn guest_uefi_io_addr_reg(reg: u64, long_mode: bool) -> u64 {
    if long_mode {
        reg
    } else {
        reg as u32 as u64
    }
}

/// RDI/RSI step after one INS/OUTS. DF is RFLAGS bit 10.
pub fn guest_uefi_io_string_advance(addr: u64, size: u8, df: bool) -> u64 {
    let step = u64::from(size);
    if df {
        addr.wrapping_sub(step)
    } else {
        addr.wrapping_add(step)
    }
}

/// Iron `b25d75b` / `577c9eb`: unused 32 MiB slab is `0xAF` fill. RIP in
/// that fill is wild control flow. Do not resume.
pub fn guest_uefi_insn_is_poison_fill(b0: u8, b1: u8, b2: u8, b3: u8) -> bool {
    b0 == 0xAF && b1 == 0xAF && b2 == 0xAF && b3 == 0xAF
}
/// SDM 27.2.1 #PF error bit 3 is RSVD.
pub const GUEST_UEFI_PF_ERR_RSVD: u64 = 1 << 3;
/// Cap identity-map #PF fixups in the 32 MiB slab (not a skip of `eb ec`).
pub const GUEST_UEFI_PF_IDENTITY_CAP: u32 = 256;

/// SDM 27.2.1 #PF error bit 0 is P (0 = not-present).
pub fn guest_uefi_pf_error_is_not_present(err: u64) -> bool {
    (err & 1) == 0
}

/// SDM 27.2.1 #PF error bit 3 is RSVD (reserved PTE bit, including NX when NXE=0).
pub fn guest_uefi_pf_error_is_reserved(err: u64) -> bool {
    (err & GUEST_UEFI_PF_ERR_RSVD) != 0
}

/// Identity-map a not-present or reserved-bit #PF in guest-UEFI low RAM.
///
/// INVARIANTS:
/// - Iron `d5fceb1` `err=0` `CR2=0x80B000` is true
/// - Iron COM2 `err=0x9` `CR2=0xA027C8` is true (NX/reserved in SEC tables)
/// - Present-bit protection faults (`err==1`) are false
/// - Flash alias / SEC tail (`0xFFFF_0000`) is true (nested `5db28e3`)
/// - Iron `eb4b27d` `err=0xb` `CR2=0x80000008` is false (mmio 2MiB, not 4G)
pub fn guest_uefi_pf_should_identity_map(err: u64, cr2: u64) -> bool {
    let in_ram = cr2 < GUEST_UEFI_LOW_RAM_BYTES;
    let in_flash = cr2 >= GUEST_UEFI_FLASH_BASE && cr2 < 0x1_0000_0000;
    (in_ram || in_flash)
        && (guest_uefi_pf_error_is_not_present(err) || guest_uefi_pf_error_is_reserved(err))
}

/// Present 1 GiB PDPTE / 2 MiB PDE (bit 0 + PS).
pub fn guest_uefi_pde_is_large(pde: u64) -> bool {
    (pde & 1) != 0 && (pde & (1 << 7)) != 0
}

/// Iron `471391f`: after 4G n=1, `#PF` `cr2=0x1e9000` `err=0x2`
/// `pde=0xc0000083` (1GiB at `0xC0000000` covering low RAM). Rebuilding 4G
/// n=2 reopened `fdf07ba` ASSERT `callerrip=0x1d25193`. Split the 1GiB
/// back to the SEC PD instead.
/// Iron `d757a0a`: SPLIT n=2 then `#PF` `err=0x9` `pde=0xafafafafafafafaf`
/// `cr2=0x1d1e6cb` (RIP). Firmware 0xAF-filled the PD; refill all RAM 2MiB.
pub fn guest_uefi_pf_should_split_ram_1g(err: u64, cr2: u64, pde: u64) -> bool {
    guest_uefi_pf_should_identity_map(err, cr2) && guest_uefi_pde_is_large(pde)
}

/// Unused slab / OVMF debug-fill. Present + reserved → `err=0x9`.
pub fn guest_uefi_pde_is_poison(pde: u64) -> bool {
    crate::vmx::guest_pt::identity_pde_is_poison(pde)
}

/// Iron `fdf07ba`: after EPT sink, 4G WB identity then ASSERT `lastmsr=0x23f`.
/// Iron `73576cc`: bulk **PCD-only** (PAT UC-) still ASSERTed.
/// Iron `8df2793`: hole PD `pdpte2=0x204067` then ASSERT — NP vs MTRR UC.
/// `[2GiB, 4GiB)` PAT-UC PCD+PWT at 4G; `[ram_len, 2GiB)` guest-PT WB
/// (iron `1a93cb8` NP vs MTRR WB). Iron `28f42d2`: PDPT[0] `pde20` WB
/// still ASSERT — fill live PDPT[1]. Do not EPT-map that window.
/// NP or RSVD `#PF` in a sink GPA gets one PAT-UC 2 MiB leaf; EPT still
/// sink-resumes.
/// Iron `eb4b27d`: `err=0xb` is present+RSVD — NP-only missed the 1GiB PDPTE.
/// Iron `124c1a8`: sign-extended 32-bit CR2 is not a 64-bit GPA; canonicalize
/// before the sink check, then map the high-half walk.
/// Iron `577c9eb`: leftover-high `0x9896808086` is the same 32-bit hole GPA.
pub fn guest_uefi_pf_gpa32(cr2: u64) -> u64 {
    crate::vmx::guest_pt::identity_hole32_gpa(cr2).unwrap_or(cr2)
}

pub fn guest_uefi_pf_should_map_mmio(err: u64, cr2: u64) -> bool {
    if !(guest_uefi_pf_error_is_not_present(err) || guest_uefi_pf_error_is_reserved(err)) {
        return false;
    }
    let gpa = guest_uefi_pf_gpa32(cr2);
    // Iron 19b0c11: [32MiB, 0x80000000) identity MMIO n=4..256 executed
    // zeros (RIP 0x3ed00001, 0x3ee00000, …). Only MTRR UC / PCI hole.
    if gpa < crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR {
        return false;
    }
    if crate::devices::guest_platform::is_platform_sink_gpa(gpa) {
        return true;
    }
    // Iron 7413554: live xAPIC is not a sink. Firmware PDPT[3]=0xc0600083
    // (1GiB + RSVD bits 21:22) covers GetApicVersion at 0xFEE00020.
    if crate::devices::guest_platform::is_xapic_2m_gpa(gpa) {
        return true;
    }
    crate::vmx::guest_pt::identity_hole32_gpa(cr2).is_some() && gpa < 0x1_0000_0000
}

/// Hypervisor 4 GiB identity PML4. Not OVMF MEMFD (`0x800000`).
pub fn guest_uefi_pf_sec_cr3() -> u64 {
    GUEST_UEFI_HV_PML4
}

/// Iron `7ea62ea`: VMCS CR3 is 0 while CpuDxe #PF'd NP. Do not walk GPA 0.
pub fn guest_uefi_pf_should_load_sec_cr3(cr3: u64) -> bool {
    cr3 == 0
}

/// Load/rebuild HV identity CR3. MEMFD (`0x800000`) is firmware heap after
/// CpuDxe (iron `101b8ec`); still match it so an old CR3 is switched off.
pub fn guest_uefi_pf_should_rebuild_sec_cr3(cr3: u64) -> bool {
    cr3 == GUEST_UEFI_HV_PML4 || cr3 == GUEST_UEFI_MEMFD_BASE
}

/// `LOADED_IMAGE_PRIVATE_DATA` signature `'ldri'`. Iron ASSERT `loc2s`.
pub const GUEST_UEFI_LDRI_SIG: [u8; 4] = *b"ldri";
/// `Info.ImageBase` in `LOADED_IMAGE_PRIVATE_DATA` (x64).
pub const GUEST_UEFI_LDRI_IMAGEBASE_OFF: u64 = 0x68;
/// `Info.ImageSize`.
pub const GUEST_UEFI_LDRI_IMAGESIZE_OFF: u64 = 0x70;
/// `Type` (PE subsystem).
pub const GUEST_UEFI_LDRI_TYPE_OFF: u64 = 0x10;
/// `EntryPoint`.
pub const GUEST_UEFI_LDRI_ENTRY_OFF: u64 = 0x20;
/// CR0.PG (SDM 2.5 bit 31).
pub const GUEST_UEFI_CR0_PG: u64 = 1 << 31;
/// VM-entry “IA-32e mode guest” (SDM Table 24-13 bit 9). Must match LMA.
pub const GUEST_UEFI_VM_ENTRY_IA32E: u64 = 1 << 9;
/// QEMU/OVMF `PcdDebugIoPort` (PlatformDebugLibIoPort). Not COM1.
pub const GUEST_UEFI_DEBUGCON_PORT: u16 = 0x402;
/// `gPcdDataBaseSignatureGuid` (MdeModulePkg). Iron `b4b4847` ASSERT `r8`.
pub const GUEST_UEFI_PCD_DATABASE_SIG: [u8; 16] = [
    0x3c, 0x19, 0x7d, 0x3c, 0x2c, 0x68, 0x14, 0x4c, 0xa6, 0x8f, 0x55, 0x2d, 0xea, 0x4f, 0x43, 0x7e,
];

pub fn guest_uefi_is_pcd_database_sig(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && bytes[..16] == GUEST_UEFI_PCD_DATABASE_SIG
}

pub fn guest_uefi_is_ldri_sig(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == GUEST_UEFI_LDRI_SIG
}

/// CS access-rights long-mode bit (SDM Table 24-2 bit 13).
pub fn guest_uefi_cs_ar_is_long(ar: u64) -> bool {
    (ar & (1 << 13)) != 0
}

pub fn guest_uefi_cr0_is_paging(cr0: u64) -> bool {
    (cr0 & GUEST_UEFI_CR0_PG) != 0
}

/// Keep EFER.LMA consistent with LME && CR0.PG (SDM 9.8.5).
/// Strip NXE so CpuDxe paging refresh does not advertise `EFI_MEMORY_XP`.
pub fn guest_uefi_efer_with_lma(efer: u64, paging: bool) -> u64 {
    let efer = efer & !GUEST_UEFI_EFER_NXE;
    if (efer & GUEST_UEFI_EFER_LME) != 0 && paging {
        efer | GUEST_UEFI_EFER_LMA
    } else {
        efer & !GUEST_UEFI_EFER_LMA
    }
}

/// VM-entry IA-32e control must equal EFER.LMA or the next VMRESUME fails.
pub fn guest_uefi_ia32e_entry_ctls(entry_ctls: u64, lma: bool) -> u64 {
    if lma {
        entry_ctls | GUEST_UEFI_VM_ENTRY_IA32E
    } else {
        entry_ctls & !GUEST_UEFI_VM_ENTRY_IA32E
    }
}

pub fn is_debugcon_port(port: u16) -> bool {
    port == GUEST_UEFI_DEBUGCON_PORT
}

/// Leaf 1 reports one logical processor and APIC ID 0.
pub fn guest_uefi_cpuid_leaf1_is_uniprocessor(ebx: u32, edx: u32) -> bool {
    ((ebx >> 16) & 0xFF) == 1 && (ebx >> 24) == 0 && (edx & CPUID_EDX_HTT) == 0
}

/// Leaf 1 ECX hypervisor-present bit (KVM nested vs iron).
pub fn guest_uefi_cpuid_has_hypervisor(ecx: u32) -> bool {
    (ecx & CPUID_ECX_HYPERVISOR) != 0
}

/// `CPUID.40000000` vendor string is `KVMKVMKVM`.
pub fn guest_uefi_cpuid_is_kvm(ebx: u32, ecx: u32, edx: u32) -> bool {
    ebx == GUEST_UEFI_KVM_EBX && ecx == GUEST_UEFI_KVM_ECX && edx == GUEST_UEFI_KVM_EDX
}

/// Stop the private VMCS after DXE once firmware read an ATAPI sector, or the tail is spent.
///
/// INVARIANTS:
/// - `false` until DXE printed (PEI still needs the full resume cap)
/// - `true` as soon as DXE printed **and** `sectors > 0` (honest PACKET READ)
/// - `true` after `GUEST_UEFI_POST_DXE_TAIL` exits past the DXE print (the 32768 cap)
/// - both PCI enums alone do **not** stop (Stage 43 `1b07692` n=1111 BOTH then
///   stopped with `sectors=0`; firmware never issued PACKET)
///
/// Nested VT-x: PEI only `inw` DID of `00:00.0`. IDE is virtio fn1 `00:00.1`.
pub fn post_dxe_should_stop(dxe_printed: bool, exit_n: u32, dxe_at: u32, sectors: u32) -> bool {
    if !dxe_printed {
        return false;
    }
    atapi_read_evidence(sectors) || exit_n.saturating_sub(dxe_at) >= GUEST_UEFI_POST_DXE_TAIL
}

/// Honest ATAPI evidence: firmware (or a host PACKET path) read a CD sector.
/// Do not fake. PCI enum is not a sector read.
pub fn atapi_read_evidence(sectors: u32) -> bool {
    sectors > 0
}

/// Firmware-simultaneous CD + disk: both PCI functions enumerated on one boot.
/// Do not fake. GuestVisible is not `ide_enum`.
pub fn both_pci_evidence(virtio_enum: bool, ide_enum: bool) -> bool {
    virtio_enum && ide_enum
}

/// Bitmask slot for bus 0 `dev.fun` (devs 0–31). Used to log a CF8 select once.
pub fn pci_bdf_bit(dev: u8, fun: u8) -> Option<(usize, u64)> {
    if fun > 7 || dev > 31 {
        return None;
    }
    let idx = u32::from(dev) * 8 + u32::from(fun);
    Some(((idx / 64) as usize, 1u64 << (idx % 64)))
}

/// OVMF SEC on 4M CODE does `mov eax,0x640; mov cr4,eax` (clears VMXE).
/// Same #GP as Linux `startup_64` without the E4 CR4.VMXE mask.
pub const E5_OVMF_SEC_CR4_VALUE: u64 = 0x640;

/// CR4.VMXE — host-owned so SEC `mov cr4,0x640` does not #GP.
pub const GUEST_UEFI_CR4_VMXE: u64 = 1 << 13;
/// CR4.OSXSAVE — host-owned so CpuDxe `mov cr4,0x668` cannot clear it.
/// Iron `0ca02e6`: dump CR4=0x668 (no bit 18), then XSAVE `#UD` RIP `0x109D`.
pub const GUEST_UEFI_CR4_OSXSAVE: u64 = 1 << 18;
/// Guest-UEFI CR4 bits the host keeps set across MOV CR4.
pub const GUEST_UEFI_CR4_HOST_OWNED: u64 = GUEST_UEFI_CR4_VMXE | GUEST_UEFI_CR4_OSXSAVE;

/// Apply a guest MOV-to-CR4: keep VMXE+OSXSAVE set (SDM 28.2.1 mask).
pub fn apply_guest_cr4_write(requested: u64) -> u64 {
    (requested & !GUEST_UEFI_CR4_HOST_OWNED) | GUEST_UEFI_CR4_HOST_OWNED
}

/// CR4 read shadow: guest must not see VMXE; guest must see OSXSAVE.
pub fn guest_cr4_read_shadow(guest_cr4: u64) -> u64 {
    guest_cr4 & !GUEST_UEFI_CR4_VMXE
}

/// Strip 66/67/F0/F2/F3 and REX so host tests can match XSAVE/UD2 encodings.
pub fn skip_x86_legacy_prefixes(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 | 0x40..=0x4F => i += 1,
            _ => break,
        }
    }
    &bytes[i..]
}

/// `#UD` that is XSAVE/XRSTOR/XSAVEOPT/XSAVEC/XSAVES/XRSTORS (needs OSXSAVE).
/// Not FXSAVE (`0F AE /0`) — that uses OSFXSR.
pub fn ud_xsave_family(bytes: &[u8]) -> bool {
    let b = skip_x86_legacy_prefixes(bytes);
    if b.len() < 3 || b[0] != 0x0F {
        return false;
    }
    let reg = (b[2] >> 3) & 7;
    match b[1] {
        0xAE => reg == 4 || reg == 5 || reg == 6,
        0xC7 => reg == 3 || reg == 4 || reg == 5,
        _ => false,
    }
}

/// ASSERT / DebugLib `ud2` (`0F 0B`).
pub fn ud_is_ud2(bytes: &[u8]) -> bool {
    let b = skip_x86_legacy_prefixes(bytes);
    b.len() >= 2 && b[0] == 0x0F && b[1] == 0x0B
}

/// I/O exit qualification → port (SDM 28.2.1 bits 31:16).
pub fn io_port_from_qual(qual: u64) -> u16 {
    ((qual >> 16) & 0xffff) as u16
}

/// Linear RIP has left the last 64 KiB (typical OVMF 4M SEC/VTF window).
pub fn linear_left_sec_tail(linear: u64) -> bool {
    linear < GUEST_UEFI_SEC_TAIL_GPA
}

pub fn is_pci_config_port(port: u16) -> bool {
    port == 0xCF8 || port == 0xCFC
}

pub fn is_com_uart_port(port: u16) -> bool {
    (0x03F8..=0x03FF).contains(&port) || (0x02F8..=0x02FF).contains(&port)
}

/// Honest past-SEC: left the SEC tail and saw PEI PCI, firmware COM, or HLT.
pub fn past_sec_evidence(
    left_sec: bool,
    pci_config: bool,
    com_bytes: u32,
    guest_hlt: bool,
) -> bool {
    left_sec && (pci_config || com_bytes > 0 || guest_hlt)
}

/// Linear RIP is executing from guest-UEFI low RAM (PEI relocated / DXE).
pub fn exec_from_low_ram(linear: u64) -> bool {
    linear < GUEST_UEFI_LOW_RAM_BYTES
}

/// Honest past-PEI/DXE or a guest CD boot attempt (ATAPI sector read).
///
/// Platform CMOS/fw_cfg alone is not enough. Need a CD READ or firmware
/// executing from the low-RAM window after past-SEC.
pub fn dxe_or_cd_boot_evidence(
    past_sec: bool,
    sectors: u32,
    platform_mem: bool,
    exec_ram: bool,
) -> bool {
    past_sec && (sectors > 0 || (platform_mem && exec_ram))
}

/// Copy up to `out.len()` bytes of guest-UEFI low RAM at identity `linear`.
///
/// Used to dump the `#GP` instruction (nested VT-x `5b2739a` `rip=0x80201a`)
/// and the stop RIP (nested VT-x `105ffbe` `rip=0x6e812d` HPET poll).
/// PEI runs with paging off / identity, so GPA = linear.
pub fn copy_low_ram_at(ram: &[u8], linear: u64, out: &mut [u8]) -> usize {
    let start = linear as usize;
    if out.is_empty() || start >= ram.len() {
        return 0;
    }
    let n = out.len().min(ram.len() - start);
    out[..n].copy_from_slice(&ram[start..start + n]);
    n
}

/// Copy bytes from one 2 MiB report-RAM window. Iron `32e7d46` RIP `0x7f8e21ca`
/// lives here, not in the 32 MiB identity slab (`insn=` was empty).
pub fn copy_report_ram_at(page: &[u8], gpa: u64, out: &mut [u8]) -> usize {
    let start = guest_uefi_report_ram_page_off(gpa) as usize;
    if out.is_empty() || start >= page.len() {
        return 0;
    }
    let n = out.len().min(page.len() - start);
    out[..n].copy_from_slice(&page[start..start + n]);
    n
}

/// Store a little-endian PTE into a 2 MiB report-RAM window (8 bytes).
pub fn store_report_ram_u64(page: &mut [u8], gpa: u64, val: u64) -> bool {
    let start = guest_uefi_report_ram_page_off(gpa) as usize;
    if start.saturating_add(8) > page.len() {
        return false;
    }
    let bytes = val.to_le_bytes();
    page[start..start + 8].copy_from_slice(&bytes);
    true
}

/// Store a little-endian PTE into guest-UEFI low RAM (8 bytes).
pub fn store_low_ram_u64(ram: &mut [u8], linear: u64, val: u64) -> bool {
    let start = linear as usize;
    if start.saturating_add(8) > ram.len() {
        return false;
    }
    let bytes = val.to_le_bytes();
    ram[start..start + 8].copy_from_slice(&bytes);
    true
}

/// Store a little-endian I/O value into a 2 MiB report-RAM window.
pub fn store_report_ram_at(page: &mut [u8], gpa: u64, val: u64, size: u8) -> usize {
    let n = size as usize;
    if n == 0 || n > 4 {
        return 0;
    }
    let start = guest_uefi_report_ram_page_off(gpa) as usize;
    if start.saturating_add(n) > page.len() {
        return 0;
    }
    for i in 0..n {
        page[start + i] = (val >> (8 * i)) as u8;
    }
    n
}

/// Load a little-endian I/O value from a 2 MiB report-RAM window.
pub fn load_report_ram_at(page: &[u8], gpa: u64, size: u8) -> Option<u64> {
    let n = size as usize;
    if n == 0 || n > 4 {
        return None;
    }
    let start = guest_uefi_report_ram_page_off(gpa) as usize;
    if start.saturating_add(n) > page.len() {
        return None;
    }
    let mut v = 0u64;
    for i in 0..n {
        v |= u64::from(page[start + i]) << (8 * i);
    }
    Some(v)
}

/// Store a little-endian I/O value into guest-UEFI low RAM (`rep insw`).
pub fn store_low_ram_at(ram: &mut [u8], linear: u64, val: u64, size: u8) -> usize {
    let n = size as usize;
    if n == 0 || n > 4 {
        return 0;
    }
    let start = linear as usize;
    if start.saturating_add(n) > ram.len() {
        return 0;
    }
    for i in 0..n {
        ram[start + i] = (val >> (8 * i)) as u8;
    }
    n
}

/// Load a little-endian I/O value from guest-UEFI low RAM (`rep outsw`).
pub fn load_low_ram_at(ram: &[u8], linear: u64, size: u8) -> Option<u64> {
    let n = size as usize;
    if n == 0 || n > 4 {
        return None;
    }
    let start = linear as usize;
    if start.saturating_add(n) > ram.len() {
        return None;
    }
    let mut v = 0u64;
    for i in 0..n {
        v |= u64::from(ram[start + i]) << (8 * i);
    }
    Some(v)
}

static LAUNCH_ENTERED: AtomicBool = AtomicBool::new(false);
static MARKER_PRINTED: AtomicBool = AtomicBool::new(false);
static LAST_EXIT_REASON: AtomicU32 = AtomicU32::new(0);
static LAST_GUEST_RIP: AtomicU64 = AtomicU64::new(0);
static LAST_LINEAR: AtomicU64 = AtomicU64::new(0);
static LAST_GUEST_PHYS: AtomicU64 = AtomicU64::new(0);
static LAST_INSN_ERROR: AtomicU32 = AtomicU32::new(0);
static EXIT_COUNT: AtomicU32 = AtomicU32::new(0);
static NON_TF_EXITS: AtomicU32 = AtomicU32::new(0);
static ALIVE_PRINTED: AtomicBool = AtomicBool::new(false);
static PAST_SEC_PRINTED: AtomicBool = AtomicBool::new(false);
static LEFT_SEC: AtomicBool = AtomicBool::new(false);
static PCI_CONFIG_SEEN: AtomicBool = AtomicBool::new(false);
static COM_BYTES: AtomicU32 = AtomicU32::new(0);
static COM_BANNER: AtomicBool = AtomicBool::new(false);
static UART_LCR_COM1: AtomicU8 = AtomicU8::new(0);
static UART_LCR_COM2: AtomicU8 = AtomicU8::new(0);
static CONTINUE_GUEST: AtomicBool = AtomicBool::new(false);
static DXE_PRINTED: AtomicBool = AtomicBool::new(false);
static BOTH_PRINTED: AtomicBool = AtomicBool::new(false);
static ATAPI_PRINTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static GPA0_SPLIT_PRINTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static PDPT0_SPLIT_PRINTED: AtomicBool = AtomicBool::new(false);
static DXE_AT_N: AtomicU32 = AtomicU32::new(0);
static EPT_PML4: AtomicU64 = AtomicU64::new(0);
static SINK_HPA: AtomicU64 = AtomicU64::new(0);
static HOLE_ZERO_HPA: AtomicU64 = AtomicU64::new(0);
static MMIO_SCRATCH_HPA: [AtomicU64; GUEST_UEFI_MMIO_SCRATCH_SLOTS] =
    [const { AtomicU64::new(0) }; GUEST_UEFI_MMIO_SCRATCH_SLOTS];
static MMIO_SCRATCH_GPA: [AtomicU64; GUEST_UEFI_MMIO_SCRATCH_SLOTS] =
    [const { AtomicU64::new(u64::MAX) }; GUEST_UEFI_MMIO_SCRATCH_SLOTS];
static REPORT_RAM_HPA: [AtomicU64; GUEST_UEFI_REPORT_RAM_SLOTS] =
    [const { AtomicU64::new(0) }; GUEST_UEFI_REPORT_RAM_SLOTS];
static REPORT_RAM_GPA: [AtomicU64; GUEST_UEFI_REPORT_RAM_SLOTS] =
    [const { AtomicU64::new(u64::MAX) }; GUEST_UEFI_REPORT_RAM_SLOTS];
static REPORT_RAM_MAPS: AtomicU32 = AtomicU32::new(0);
static CPU_FLUSH_PATCHED: AtomicU32 = AtomicU32::new(0);
static LIVE_UC_PT_PAINTED: AtomicU32 = AtomicU32::new(0);
static LIVE_GPA0_SPLIT: AtomicU32 = AtomicU32::new(0);
static SINK_MAPS: AtomicU32 = AtomicU32::new(0);
static PCI_DID_TRACE: AtomicU32 = AtomicU32::new(0);
static PCI_HT_TRACE: AtomicU32 = AtomicU32::new(0);
static PCI_BAR_TRACE: AtomicU32 = AtomicU32::new(0);
static HLT_SKIPS: AtomicU32 = AtomicU32::new(0);
static SPIN_JMP_SKIPS: AtomicU32 = AtomicU32::new(0);
static LAST_PREEMPT_RIP: AtomicU64 = AtomicU64::new(u64::MAX);
static PREEMPT_SAME_RIP: AtomicU32 = AtomicU32::new(0);
static CR_ACCESSES: AtomicU32 = AtomicU32::new(0);
static PCI_BDF_SEEN0: AtomicU64 = AtomicU64::new(0);
static PCI_BDF_SEEN1: AtomicU64 = AtomicU64::new(0);
static PCI_BDF_SEEN2: AtomicU64 = AtomicU64::new(0);
static PCI_BDF_SEEN3: AtomicU64 = AtomicU64::new(0);
static LAST_IO_PORT: AtomicU32 = AtomicU32::new(0);
static LAST_CF8: AtomicU32 = AtomicU32::new(0);
static RAM_HPA: AtomicU64 = AtomicU64::new(0);
static RAM_REMAP_N: AtomicU32 = AtomicU32::new(0);
static RAM_REMAP_TRIES: AtomicU32 = AtomicU32::new(0);
static HPET_TICKS: AtomicU32 = AtomicU32::new(0);
static PREEMPT_RELOAD: AtomicU32 = AtomicU32::new(0);
static IO_UNHANDLED_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static IO_STRING_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static KBC_WR_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static XSETBV_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static UD_XSAVE_RETRY: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static UD2_SKIPS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static ASSERT_DEADLOOP_DUMP: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static PF_FIXUPS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static SEC_IDENTITY_REBUILT: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static MSR_SEEN: [AtomicU32; 24] = [const { AtomicU32::new(0xFFFF_FFFF) }; 24];
#[cfg(target_os = "uefi")]
static MSR_SEEN_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static WRMSR_SEEN: [AtomicU32; 16] = [const { AtomicU32::new(0xFFFF_FFFF) }; 16];
#[cfg(target_os = "uefi")]
static WRMSR_SEEN_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static LAST_GUEST_MSR: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static LAST_EFER: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "uefi")]
static CPUID_SEEN: [AtomicU64; 16] = [const { AtomicU64::new(u64::MAX) }; 16];
#[cfg(target_os = "uefi")]
static CPUID_SEEN_N: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static DBG_LEN: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static DBG_LINES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static DBG_BUF: [AtomicU8; 80] = [const { AtomicU8::new(0) }; 80];

#[cfg(target_os = "uefi")]
static mut SAVED_RAX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RBX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RCX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RDX: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RSI: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RDI: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_RBP: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R8: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R9: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R10: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R11: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R12: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R13: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R14: u64 = 0;
#[cfg(target_os = "uefi")]
static mut SAVED_R15: u64 = 0;

static mut SAVED_VMCS: u64 = 0;
static mut E4_ALLOC: *mut FrameAllocator = core::ptr::null_mut();
static mut E4_LIFE: *mut crate::vmx::lifecycle::VmxLifecycle = core::ptr::null_mut();
static mut E4_RSP: u64 = 0;
static mut E4_RESUME: u64 = 0;
#[cfg(target_os = "uefi")]
static HOST_XCR0_SAVED: AtomicU64 = AtomicU64::new(1);
#[cfg(target_os = "uefi")]
static HOST_OSXSAVE_SAVED: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static HOST_XSAVE_CAPTURED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static HOST_XSAVE_RESTORED: AtomicBool = AtomicBool::new(false);

/// True after a real VMLAUNCH entered the guest (HOST_RIP reached).
pub fn guest_uefi_vmlaunch_entered() -> bool {
    LAUNCH_ENTERED.load(Ordering::Acquire)
}

/// Last recorded basic exit reason (0 if none).
pub fn last_exit_reason() -> u32 {
    LAST_EXIT_REASON.load(Ordering::Acquire)
}

/// QEMU/OVMF 4 MiB pflash base (`OVMF.fd` VARS at `0xFFC00000`, CODE on top).
pub const GUEST_UEFI_FLASH_BASE: u64 = 0xFFC0_0000;
/// Full flash window. Nested VT-x `1991a27` EPT-faulted at `gpa=0xffc00000`
/// because a CODE-only image was top-aligned at `0xFFC84000` (VARS gap).
pub const GUEST_UEFI_FLASH_WINDOW: u64 = 4 * 1024 * 1024;

/// Map any 1–4 MiB retained image into a 4 MiB window at [`GUEST_UEFI_FLASH_BASE`].
/// Pad is leading erased flash (`0xFF`); [`stamp_empty_ovmf_vars`] writes a
/// VARS `_FVH` when the pad is Debian 4M sized. Reset vector stays at `0xFFFF_FFF0`.
pub fn flash_window_gpa_and_pad(image_len: u64) -> Option<(u64, u64)> {
    const MIN: u64 = MIN_REAL_OVMF_BYTES as u64;
    if image_len < MIN || image_len > GUEST_UEFI_FLASH_WINDOW {
        return None;
    }
    Some((GUEST_UEFI_FLASH_BASE, GUEST_UEFI_FLASH_WINDOW - image_len))
}

/// Debian/QEMU 4M VARS firmware-volume size (`OVMF_VARS_4M.fd`).
pub const OVMF_VARS_FV_BYTES: usize = 0x84000;

/// Empty NV storage prefix: `EFI_FIRMWARE_VOLUME_HEADER` + authenticated
/// `VARIABLE_STORE_HEADER`. Byte-identical to the first 0x64 bytes of
/// Debian `OVMF_VARS_4M.fd`. Remainder of the FV is erased NOR (`0xFF`).
///
/// FileSystemGuid is `EFI_SYSTEM_NV_DATA_FV_GUID`. Store signature is
/// `gEfiAuthenticatedVariableGuid`. Format `0x5A` / State `0xFE` (healthy).
pub const OVMF_VARS_EMPTY_PREFIX: [u8; 0x64] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x8d, 0x2b, 0xf1, 0xff, 0x96, 0x76, 0x8b, 0x4c, 0xa9, 0x85, 0x27, 0x47, 0x07, 0x5b, 0x4f, 0x50,
    0x00, 0x40, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x5f, 0x46, 0x56, 0x48, 0xff, 0xfe, 0x04, 0x00,
    0x48, 0x00, 0xaf, 0xb8, 0x00, 0x00, 0x00, 0x02, 0x84, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x2c, 0xf3, 0xaa, 0x7b, 0x94, 0x9a, 0x43,
    0xa1, 0x80, 0x2e, 0x14, 0x4e, 0xc3, 0x77, 0x92, 0xb8, 0xff, 0x03, 0x00, 0x5a, 0xfe, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

/// Stamp an empty VARS firmware volume into a CODE-only 4 MiB pad.
///
/// INVARIANTS:
/// - Writes only when `pad.len() == OVMF_VARS_FV_BYTES` (Debian 4M CODE-only)
/// - Prefix is `_FVH` + empty authenticated variable store
/// - Does not touch CODE (caller copies CODE after this)
/// - Does not fake PCI enum
///
/// VERIFICATION: L1 (host tests)
pub fn stamp_empty_ovmf_vars(pad: &mut [u8]) -> bool {
    if pad.len() != OVMF_VARS_FV_BYTES {
        return false;
    }
    let n = OVMF_VARS_EMPTY_PREFIX.len();
    pad[..n].copy_from_slice(&OVMF_VARS_EMPTY_PREFIX);
    true
}

/// Live alias window for a retained 1–4 MiB image: `4 GiB - len`.
///
/// Stage 15 [`crate::vmx::launch::firmware_alias_gpa`] stays 4 MiB-only
/// (bookkeeping contract). This helper is the real retain path.
pub fn live_firmware_alias_gpa(bytes_len: u64) -> Option<u64> {
    const MIN: u64 = MIN_REAL_OVMF_BYTES as u64;
    const MAX: u64 = 4 * 1024 * 1024;
    if bytes_len < MIN || bytes_len > MAX || (bytes_len & 0xfff) != 0 {
        return None;
    }
    let gpa = GUEST_UEFI_FIRMWARE_TOP_GPA - bytes_len;
    if alias_ept_covers_reset(gpa, bytes_len) {
        Some(gpa)
    } else {
        None
    }
}

/// xAPIC 2 MiB window is a live 4 KiB page (version 0x50014), not sink zeros.
/// Iron `ad78f12`: CPUID uniprocessor still ASSERT `ret=0x6e8946` after
/// seven `RDMSR 0x1B` — GetApicVersion() of a zero sink.
pub fn guest_uefi_xapic_is_not_sink() -> bool {
    crate::devices::guest_platform::is_xapic_2m_gpa(0xFEE0_0000)
        && !crate::devices::guest_platform::is_platform_sink_gpa(0xFEE0_0000)
        && crate::devices::lapic_virt::XAPIC_VERSION == 0x0005_0014
}

/// Iron `cc7d78a`: PCI-hole GPA is a 2 MiB sink, not a stop. Flash and xAPIC stay real.
pub fn guest_uefi_pci_hole_is_sink() -> bool {
    crate::devices::guest_platform::is_platform_sink_gpa(GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA)
        && crate::devices::guest_platform::is_platform_sink_gpa(0xC000_0000)
        && crate::devices::guest_platform::is_platform_sink_gpa(0x8000_0000)
        && !crate::devices::guest_platform::is_platform_sink_gpa(0xFFC0_0000)
        && !crate::devices::guest_platform::is_platform_sink_gpa(0xFEE0_0000)
}

/// IA32_MTRRCAP: VCNT=32 (MtrrLib max), FIX, WC. Never passthrough the host.
/// Iron `10cb881`: VCNT=8 power-on still ASSERT `callerrip=0x1d25193`
/// `mtrrdef=0xc06` `mtrr0=0x80000000` (firmware UC at 2 GiB). Combined
/// power-on (`E=0`, no hole) with VCNT=32 so WorkingRangeCount can fit.
/// Iron `aee545f`: VCNT=32 + pre-enabled 1 GiB UC hole then skip `#UD` `0x109d`.
pub const GUEST_UEFI_MTRR_VCNT: u64 = 32;
pub const GUEST_UEFI_MTRRCAP: u64 = GUEST_UEFI_MTRR_VCNT | (1 << 8) | (1 << 10);
/// SDM reset: E=0, FE=0, default type UC.
pub const GUEST_UEFI_MTRR_DEF_DEFAULT: u64 = 0;
/// Eight packed WB types (Intel MTRR type 6). Firmware WRMSR, not reset.
pub const GUEST_UEFI_MTRR_WB_PACKED: u64 = 0x0606_0606_0606_0606;
/// 32 variable pairs (`0x200`–`0x23F`).
pub const GUEST_UEFI_MTRR_VAR_MSRS: usize = 64;
/// SDM Vol. 3 Table 11-11: IA32_PAT RESET (PA0=WB, PA1=WT, PA2=UC-, PA3=UC).
/// Iron `44c56db`: VMCLEAR left `GUEST_IA32_PAT=0`; Xeon VM-entry LOAD_PAT
/// then PA0=UC vs MTRR WB and CpuDxe ASSERTs `callerrip=0x1d25193`.
pub const IA32_PAT_RESET: u64 = 0x0007_0406_0007_0406;

/// PAT memory type at `pa_index` 0..7 (3 bits each, 8-bit stride).
pub fn ia32_pat_memory_type(pat: u64, pa_index: u32) -> u64 {
    (pat >> (pa_index.saturating_mul(8))) & 0x7
}

/// Historical pre-enabled UC hole (`aee545f`). Not programmed at reset.
pub const GUEST_UEFI_MTRR_PCI_UC_BASE: u64 = 0xC000_0000;
/// 36-bit physmask for that 1 GiB region plus Valid (bit 11).
pub const GUEST_UEFI_MTRR_PCI_UC_MASK: u64 = 0xF_C000_0800;

static GUEST_MTRR_DEF: AtomicU64 = AtomicU64::new(GUEST_UEFI_MTRR_DEF_DEFAULT);
static GUEST_MTRR_VAR: [AtomicU64; GUEST_UEFI_MTRR_VAR_MSRS] =
    [const { AtomicU64::new(0) }; GUEST_UEFI_MTRR_VAR_MSRS];
static GUEST_MTRR_FIXED: [AtomicU64; 11] =
    [const { AtomicU64::new(0) }; 11];
static GUEST_MISC_ENABLE: AtomicU64 = AtomicU64::new(GUEST_UEFI_MISC_ENABLE_DEFAULT);
/// Iron `f07a597`: PAT-UC+MTRR match still ASSERT. Hold valid UC variable
/// pairs so CpuDxe RefreshGcd sees default WB in one GCD range.
static MTRR_ADMIT_UC: AtomicBool = AtomicBool::new(false);
static MTRR_UC_HELD: AtomicU32 = AtomicU32::new(0);

/// True for IA32 MTRR MSRs (not PAT `0x277`).
pub fn guest_uefi_is_mtrr_msr(msr: u32) -> bool {
    matches!(msr, 0x00FE | 0x02FF | 0x0250 | 0x0258 | 0x0259)
        || (0x0200..=0x023F).contains(&msr)
        || (0x0268..=0x026F).contains(&msr)
}

fn mtrr_fixed_index(msr: u32) -> Option<usize> {
    match msr {
        0x250 => Some(0),
        0x258 => Some(1),
        0x259 => Some(2),
        0x268..=0x26F => Some(3 + (msr as usize - 0x268)),
        _ => None,
    }
}

/// SDM power-on MTRRs: disabled (`E=0`), no variable ranges, fixed 0.
pub fn guest_uefi_mtrr_reset() {
    GUEST_MTRR_DEF.store(GUEST_UEFI_MTRR_DEF_DEFAULT, Ordering::Release);
    for slot in GUEST_MTRR_VAR.iter() {
        slot.store(0, Ordering::Release);
    }
    for slot in GUEST_MTRR_FIXED.iter() {
        slot.store(0, Ordering::Release);
    }
    GUEST_MISC_ENABLE.store(GUEST_UEFI_MISC_ENABLE_DEFAULT, Ordering::Release);
    crate::vmx::guest_pt::identity_set_pat_uc_hole(false);
    MTRR_ADMIT_UC.store(false, Ordering::Release);
    MTRR_UC_HELD.store(0, Ordering::Release);
}

/// True when MTRRs are SDM power-on (disabled, no pre-cooked UC hole).
pub fn guest_uefi_mtrr_poweron_disabled() -> bool {
    guest_uefi_mtrr_read(0x2FF) == Some(0)
        && guest_uefi_mtrr_read(0x200) == Some(0)
        && guest_uefi_mtrr_read(0x201) == Some(0)
        && guest_uefi_mtrr_read(0x250) == Some(0)
        && (GUEST_UEFI_MTRRCAP & 0xFF) == GUEST_UEFI_MTRR_VCNT
}

/// Count variable pairs with Valid (mask bit 11). Power-on is 0.
pub fn guest_uefi_mtrr_valid_var_pairs() -> u32 {
    let vcnt = (GUEST_UEFI_MTRRCAP & 0xFF) as u32;
    let mut n = 0u32;
    let mut i = 0u32;
    while i < vcnt {
        let mask = guest_uefi_mtrr_read(0x201 + i * 2).unwrap_or(0);
        if (mask & (1 << 11)) != 0 {
            n = n.saturating_add(1);
        }
        i = i.saturating_add(1);
    }
    n
}

/// Historical: pre-enabled 1 GiB UC PCI hole. False at power-on reset.
pub fn guest_uefi_mtrr_pci_uc_hole() -> bool {
    guest_uefi_mtrr_read(0x200) == Some(GUEST_UEFI_MTRR_PCI_UC_BASE)
        && guest_uefi_mtrr_read(0x201) == Some(GUEST_UEFI_MTRR_PCI_UC_MASK)
}

/// Tests / P1 hold: `false` drops valid UC variable MTRRs. P2 live path
/// admits UC after CMOS 2 GiB so firmware can match GCD at `Uc32Base`.
pub fn guest_uefi_mtrr_set_admit_uc(on: bool) {
    MTRR_ADMIT_UC.store(on, Ordering::Release);
}

/// How many UC variable WRMSRs were dropped (iron `f07a597` GCD experiment).
pub fn guest_uefi_mtrr_uc_held() -> u32 {
    MTRR_UC_HELD.load(Ordering::Acquire)
}

/// True when any valid variable MTRR is type UC (0).
pub fn guest_uefi_mtrr_any_valid_uc() -> bool {
    let vcnt = (GUEST_UEFI_MTRRCAP & 0xFF) as u32;
    let mut i = 0u32;
    while i < vcnt {
        let base = guest_uefi_mtrr_read(0x200 + i * 2).unwrap_or(0);
        let mask = guest_uefi_mtrr_read(0x201 + i * 2).unwrap_or(0);
        if (mask & (1 << 11)) != 0 && (base & 0xFF) == 0 {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// True when a valid variable MTRR is UC at 2 GiB (iron `mtrr0=0x80000000`)
/// or the historical 3 GiB PCI hole. Arms [`identity_set_pat_uc_hole`].
pub fn guest_uefi_mtrr_uc_hole_live() -> bool {
    let vcnt = (GUEST_UEFI_MTRRCAP & 0xFF) as u32;
    let mut i = 0u32;
    while i < vcnt {
        let base = guest_uefi_mtrr_read(0x200 + i * 2).unwrap_or(0);
        let mask = guest_uefi_mtrr_read(0x201 + i * 2).unwrap_or(0);
        if (mask & (1 << 11)) != 0 && (base & 0xFF) == 0 {
            let b = base & !0xFFFu64;
            if b == crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR || b == GUEST_UEFI_MTRR_PCI_UC_BASE
            {
                return true;
            }
        }
        i = i.saturating_add(1);
    }
    false
}

fn guest_uefi_maybe_arm_pat_uc_hole() {
    if !guest_uefi_mtrr_uc_hole_live() {
        return;
    }
    crate::vmx::guest_pt::identity_set_pat_uc_hole(true);
    #[cfg(target_os = "uefi")]
    {
        let ram_hpa = RAM_HPA.load(Ordering::Acquire);
        // SAFETY: guest-UEFI VMLAUNCH path; current VMCS is loaded.
        // identity_sync writes only the exclusive guest-UEFI slab.
        let cr3 = unsafe { ops::vmread(GUEST_CR3) }.unwrap_or(0);
        if ram_hpa != 0 && cr3 != 0 {
            let _ = unsafe {
                crate::vmx::guest_pt::identity_sync_live_mtrr_uc_hole(
                    ram_hpa,
                    GUEST_UEFI_LOW_RAM_BYTES,
                    cr3,
                )
            };
        }
        unsafe {
            guest_uefi_paint_live_uc_hole_now();
        }
    }
}

/// Host CPUID.80000008 phys width clamped for guest-UEFI MTRR masks.
pub fn guest_uefi_phys_width() -> u32 {
    guest_uefi_phys_bits(msr_firewall::filter_cpuid(0x8000_0008, 0).eax)
}

/// Shadowed MTRR read. `None` if `msr` is not an MTRR.
pub fn guest_uefi_mtrr_read(msr: u32) -> Option<u64> {
    if msr == 0x00FE {
        return Some(GUEST_UEFI_MTRRCAP);
    }
    if msr == 0x02FF {
        return Some(GUEST_MTRR_DEF.load(Ordering::Acquire));
    }
    if (0x0200..=0x023F).contains(&msr) {
        let raw = GUEST_MTRR_VAR[(msr - 0x0200) as usize].load(Ordering::Acquire);
        if (msr & 1) == 1 {
            return Some(guest_uefi_mtrr_var_mask_sanitize(raw, guest_uefi_phys_width()));
        }
        return Some(raw);
    }
    let i = mtrr_fixed_index(msr)?;
    Some(GUEST_MTRR_FIXED[i].load(Ordering::Acquire))
}

/// Shadowed MTRR write (CAP is read-only). `false` if not an MTRR.
pub fn guest_uefi_mtrr_write(msr: u32, value: u64) -> bool {
    if !guest_uefi_is_mtrr_msr(msr) {
        return false;
    }
    if msr == 0x00FE {
        return true;
    }
    if msr == 0x02FF {
        GUEST_MTRR_DEF.store(value, Ordering::Release);
        return true;
    }
    if (0x0200..=0x023F).contains(&msr) {
        let v = if (msr & 1) == 1 {
            guest_uefi_mtrr_var_mask_sanitize(value, guest_uefi_phys_width())
        } else {
            value
        };
        let idx = (msr - 0x0200) as usize;
        let prev = GUEST_MTRR_VAR[idx].load(Ordering::Acquire);
        GUEST_MTRR_VAR[idx].store(v, Ordering::Release);
        if guest_uefi_mtrr_any_valid_uc() && !MTRR_ADMIT_UC.load(Ordering::Acquire) {
            GUEST_MTRR_VAR[idx].store(prev, Ordering::Release);
            let n = MTRR_UC_HELD.fetch_add(1, Ordering::AcqRel);
            #[cfg(target_os = "uefi")]
            if n == 0 {
                serial::write_str("boot: guest-UEFI MTRR UC held (GCD)\n");
            }
            return true;
        }
        guest_uefi_maybe_arm_pat_uc_hole();
        return true;
    }
    if let Some(i) = mtrr_fixed_index(msr) {
        GUEST_MTRR_FIXED[i].store(value, Ordering::Release);
        return true;
    }
    false
}

/// True for IA32_MISC_ENABLE (`0x1A0`). Host passthrough on iron exposes
/// Xeon Limit-CPUID / XD / SpeedStep bits that nested KVM does not.
pub fn guest_uefi_is_misc_enable(msr: u32) -> bool {
    msr == GUEST_UEFI_MISC_ENABLE_MSR
}

/// Shadowed MISC_ENABLE. `None` if `msr` is not `0x1A0`.
pub fn guest_uefi_misc_enable_read(msr: u32) -> Option<u64> {
    if msr == GUEST_UEFI_MISC_ENABLE_MSR {
        Some(GUEST_MISC_ENABLE.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Shadowed MISC_ENABLE write. `false` if not `0x1A0`.
pub fn guest_uefi_misc_enable_write(msr: u32, value: u64) -> bool {
    if msr != GUEST_UEFI_MISC_ENABLE_MSR {
        return false;
    }
    GUEST_MISC_ENABLE.store(value, Ordering::Release);
    true
}

/// Host-test / reset helper. Does not VMCLEAR a live VMCS.
pub fn reset_guest_uefi_launch() {
    LAUNCH_ENTERED.store(false, Ordering::Release);
    MARKER_PRINTED.store(false, Ordering::Release);
    LAST_EXIT_REASON.store(0, Ordering::Release);
    LAST_GUEST_RIP.store(0, Ordering::Release);
    LAST_LINEAR.store(0, Ordering::Release);
    LAST_GUEST_PHYS.store(0, Ordering::Release);
    LAST_INSN_ERROR.store(0, Ordering::Release);
    EXIT_COUNT.store(0, Ordering::Release);
    NON_TF_EXITS.store(0, Ordering::Release);
    ALIVE_PRINTED.store(false, Ordering::Release);
    PAST_SEC_PRINTED.store(false, Ordering::Release);
    LEFT_SEC.store(false, Ordering::Release);
    PCI_CONFIG_SEEN.store(false, Ordering::Release);
    COM_BYTES.store(0, Ordering::Release);
    COM_BANNER.store(false, Ordering::Release);
    UART_LCR_COM1.store(0, Ordering::Release);
    UART_LCR_COM2.store(0, Ordering::Release);
    CONTINUE_GUEST.store(false, Ordering::Release);
    DXE_PRINTED.store(false, Ordering::Release);
    BOTH_PRINTED.store(false, Ordering::Release);
    ATAPI_PRINTED.store(false, Ordering::Release);
    DXE_AT_N.store(0, Ordering::Release);
    EPT_PML4.store(0, Ordering::Release);
    SINK_HPA.store(0, Ordering::Release);
    HOLE_ZERO_HPA.store(0, Ordering::Release);
    for i in 0..GUEST_UEFI_MMIO_SCRATCH_SLOTS {
        MMIO_SCRATCH_HPA[i].store(0, Ordering::Release);
        MMIO_SCRATCH_GPA[i].store(u64::MAX, Ordering::Release);
    }
    for i in 0..GUEST_UEFI_REPORT_RAM_SLOTS {
        REPORT_RAM_HPA[i].store(0, Ordering::Release);
        REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
    }
    REPORT_RAM_MAPS.store(0, Ordering::Release);
    CPU_FLUSH_PATCHED.store(0, Ordering::Release);
    SINK_MAPS.store(0, Ordering::Release);
    PCI_DID_TRACE.store(0, Ordering::Release);
    PCI_HT_TRACE.store(0, Ordering::Release);
    PCI_BAR_TRACE.store(0, Ordering::Release);
    HLT_SKIPS.store(0, Ordering::Release);
    SPIN_JMP_SKIPS.store(0, Ordering::Release);
    LAST_PREEMPT_RIP.store(u64::MAX, Ordering::Release);
    PREEMPT_SAME_RIP.store(0, Ordering::Release);
    CR_ACCESSES.store(0, Ordering::Release);
    PCI_BDF_SEEN0.store(0, Ordering::Release);
    PCI_BDF_SEEN1.store(0, Ordering::Release);
    PCI_BDF_SEEN2.store(0, Ordering::Release);
    PCI_BDF_SEEN3.store(0, Ordering::Release);
    LAST_IO_PORT.store(0, Ordering::Release);
    LAST_CF8.store(0, Ordering::Release);
    RAM_HPA.store(0, Ordering::Release);
    RAM_REMAP_N.store(0, Ordering::Release);
    RAM_REMAP_TRIES.store(0, Ordering::Release);
    HPET_TICKS.store(0, Ordering::Release);
    PREEMPT_RELOAD.store(0, Ordering::Release);
    IO_UNHANDLED_N.store(0, Ordering::Release);
    guest_uefi_mtrr_reset();
    crate::devices::guest_platform::reset();
    crate::devices::guest_virtio_blk::reset();
}

/// Exits after a successful entry that were not triple-fault / VM-entry fail.
pub fn guest_uefi_non_tf_exits() -> u32 {
    NON_TF_EXITS.load(Ordering::Acquire)
}

pub fn guest_uefi_alive() -> bool {
    ALIVE_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_past_sec() -> bool {
    PAST_SEC_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_com_bytes() -> u32 {
    COM_BYTES.load(Ordering::Acquire)
}

pub fn guest_uefi_dxe() -> bool {
    DXE_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_both() -> bool {
    BOTH_PRINTED.load(Ordering::Acquire)
}

pub fn guest_uefi_atapi() -> bool {
    ATAPI_PRINTED.load(Ordering::Acquire)
}

#[cfg(target_os = "uefi")]
fn alloc_phys(alloc: &mut FrameAllocator) -> Option<u64> {
    alloc.allocate_frame().map(|f| {
        crate::audit::integrity::record_event(AuditEvent::FrameAllocated { frame: f.0 });
        f.to_phys()
    })
}

/// Launch retained ESP `OVMF.fd` on a private VMCS + EPT.
///
/// INVARIANTS:
/// - Refuses when [`ovmf_esp::bytes_present`] is false (fixtures stay out)
/// - Does not write the E4 SHELL VMCS or EPT
/// - Host `cargo test` never executes `ops::vmlaunch`
/// - On VMLAUNCH instruction failure, logs and returns (host stays alive)
///
/// On successful entry, HOST_RIP logs the first VMEXIT, prints the marker
/// when the exit is not a VM-entry failure, VMCLEARs the private VMCS, and
/// resumes the E4 SHELL path on the original stack.
pub unsafe fn run_retained_ovmf_vmlaunch(
    alloc: &mut FrameAllocator,
) -> Result<(), GuestUefiLaunchError> {
    if !ovmf_esp::bytes_present() {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    let Some(bytes) = ovmf_esp::retained_bytes() else {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    };
    if bytes.len() < MIN_REAL_OVMF_BYTES || !ovmf_esp::accept_real_ovmf_bytes(bytes) {
        return Err(GuestUefiLaunchError::MissingEspFirmware);
    }
    let Some((gpa, _pad)) = flash_window_gpa_and_pad(bytes.len() as u64) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    if !alias_ept_covers_reset(gpa, GUEST_UEFI_FLASH_WINDOW) {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }
    let map_len = GUEST_UEFI_FLASH_WINDOW;

    #[cfg(not(target_os = "uefi"))]
    {
        let _ = (
            alloc,
            GUEST_UEFI_PRIVATE_VMCS_ID,
            E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
        );
        return Err(GuestUefiLaunchError::PrivateVmcsNotLaunched);
    }

    #[cfg(target_os = "uefi")]
    {
        match launch_uefi(alloc, bytes, gpa, map_len) {
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn launch_uefi(
    alloc: &mut FrameAllocator,
    bytes: &[u8],
    gpa: u64,
    fw_len: u64,
) -> Result<(), GuestUefiLaunchError> {
    capture_host_xsave_before_guest_uefi();
    guest_uefi_mtrr_reset();
    // Iron `957e0ad`: hold left `mtrrv=0` while GCD/HOB treat [2GiB,4GiB)
    // as UC (`callerrip=0x7fd25193` same CpuDxe ASSERT). Admit UC.
    guest_uefi_mtrr_set_admit_uc(true);
    serial::write_line("boot: guest-UEFI MTRR UC admitted (GCD)");
    let pages = (fw_len + 4095) / 4096;
    let Some(fw_frame) = alloc.allocate_contiguous(pages) else {
        serial::write_line("boot: guest-UEFI no frames for retained OVMF copy");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let fw_hpa = fw_frame.to_phys();
    let pad = (fw_len as usize).saturating_sub(bytes.len());
    // SAFETY: exclusive 4 MiB guest-private flash copy; pad is in-range.
    // KANI-TARGET: pad CODE-only OVMF into 4MiB window (outside Proven Core).
    unsafe {
        core::ptr::write_bytes(fw_hpa as *mut u8, 0xFF, (pages * 4096) as usize);
    }
    // Empty VARS `_FVH` at 0xFFC00000 so PEI does not parse erased NOR.
    // Matches Debian OVMF_VARS_4M.fd prefix; remainder stays 0xFF.
    let vars_n = if pad > 0 {
        // SAFETY: pad bytes are the leading range of the exclusive 4 MiB copy.
        // KANI-TARGET: stamp empty VARS into CODE-only pad (outside Proven Core).
        let pad_slice = unsafe { core::slice::from_raw_parts_mut(fw_hpa as *mut u8, pad) };
        u32::from(stamp_empty_ovmf_vars(pad_slice))
    } else {
        0
    };
    // SAFETY: CODE image fits after `pad` in the exclusive 4 MiB copy.
    // KANI-TARGET: copy CODE-only OVMF after VARS pad (outside Proven Core).
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), (fw_hpa as *mut u8).add(pad), bytes.len());
    }
    // SAFETY: exclusive guest-private firmware copy; the retain buffer is
    // a different range. Do **not** remap `cmp bx, 0x1237` — PEI captures
    // HostBridgeDevId from i440FX DID `0x1237` at `00:00.0` (stock QEMU
    // PlatformMemMapInitialization). `remap_i440fx_did_imm` stays in-tree
    // for the Stage 43 source gate; maybe_remap_guest_ram is not called.
    // KANI-TARGET: guest-private OVMF copy (outside Proven Core).
    let remap_n = 0u32;
    let _ = crate::boot::ovmf_esp::remap_i440fx_did_imm;
    serial::write_str("boot: guest-UEFI 4MiB flash pad=0x");
    write_hex(pad as u64);
    serial::write_byte(b'\n');
    serial::write_str("boot: guest-UEFI empty VARS _FVH n=");
    write_dec(vars_n as u64);
    serial::write_byte(b'\n');
    serial::write_str("boot: guest-UEFI ovmf remap i440FX DID->virtio n=");
    write_dec(remap_n as u64);
    serial::write_byte(b'\n');

    let ram_pages = GUEST_UEFI_LOW_RAM_BYTES / 4096;
    let Some(ram_frame) = alloc.allocate_contiguous_aligned(ram_pages, 512) else {
        serial::write_line("boot: guest-UEFI no 32MiB RAM slab");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let ram_hpa = ram_frame.to_phys();
    core::ptr::write_bytes(ram_hpa as *mut u8, 0, GUEST_UEFI_LOW_RAM_BYTES as usize);
    RAM_HPA.store(ram_hpa, Ordering::Release);

    let ept_need = frames_required_firmware_alias(gpa, fw_len);
    if ept_need > 8 {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }
    let mut ept_frames = [0u64; 8];
    for slot in ept_frames.iter_mut().take(ept_need) {
        let Some(f) = alloc_phys(alloc) else {
            serial::write_line("boot: guest-UEFI no EPT table frames");
            return Err(GuestUefiLaunchError::LaunchSetupFailed);
        };
        *slot = f;
    }
    let eptp = match ept_hw::build_firmware_alias_ept(
        gpa,
        fw_hpa,
        fw_len,
        ram_hpa,
        &mut ept_frames[..ept_need],
    ) {
        Ok(v) => v,
        Err(_) => {
            serial::write_line("boot: guest-UEFI alias EPT build failed");
            return Err(GuestUefiLaunchError::LaunchSetupFailed);
        }
    };
    let pml4 = ept_frames[0];
    if !ept_hw::gpa_is_mapped(pml4, GUEST_UEFI_RESET_VECTOR_GPA)
        || !ept_hw::gpa_is_mapped(pml4, gpa)
        || !ept_hw::gpa_is_mapped(pml4, 0)
        || !ept_hw::gpa_is_mapped(pml4, GUEST_UEFI_LOW_RAM_BYTES - 4096)
    {
        serial::write_line("boot: guest-UEFI alias EPT walk failed");
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }
    crate::devices::guest_platform::reset();
    EPT_PML4.store(pml4, Ordering::Release);
    if let Some(sink_frame) = alloc.allocate_contiguous_aligned(512, 512) {
        let sink_hpa = sink_frame.to_phys();
        core::ptr::write_bytes(sink_hpa as *mut u8, 0, 2 * 1024 * 1024);
        // SAFETY: exclusive 2 MiB sink; HPET sits at 0xFED00000 in this page.
        // KANI-TARGET: live HPET in guest-UEFI sink (outside Proven Core).
        let hpet_ok = crate::devices::guest_platform::hpet_init_sink(unsafe {
            core::slice::from_raw_parts_mut(sink_hpa as *mut u8, 2 * 1024 * 1024)
        });
        SINK_HPA.store(sink_hpa, Ordering::Release);
        // Iron f93caee: hole RO used SINK_HPA (live HPET). Firmware read
        // HPET as PTEs then leftover CR2 0x9896808086 RIP 0x300001
        // poison fill. Dedicated zero 2MiB; never hpet_init_sink.
        if let Some(zero_frame) = alloc.allocate_contiguous_aligned(512, 512) {
            let hole_zero = zero_frame.to_phys();
            core::ptr::write_bytes(hole_zero as *mut u8, 0, 2 * 1024 * 1024);
            debug_assert!(hole_zero != sink_hpa);
            HOLE_ZERO_HPA.store(hole_zero, Ordering::Release);
            serial::write_str("boot: guest-UEFI hole-zero hpa=0x");
            write_hex(hole_zero);
            serial::write_line(" (not HPET sink)");
        } else {
            serial::write_line("boot: guest-UEFI WARN — no hole-zero frame (will not RO-sink onto HPET)");
        }
        let mut scratch_n = 0u64;
        let mut scratch0 = 0u64;
        for i in 0..GUEST_UEFI_MMIO_SCRATCH_SLOTS {
            if let Some(scratch_frame) = alloc.allocate_contiguous_aligned(512, 512) {
                let scratch_hpa = scratch_frame.to_phys();
                core::ptr::write_bytes(scratch_hpa as *mut u8, 0, 2 * 1024 * 1024);
                MMIO_SCRATCH_HPA[i].store(scratch_hpa, Ordering::Release);
                MMIO_SCRATCH_GPA[i].store(u64::MAX, Ordering::Release);
                if i == 0 {
                    scratch0 = scratch_hpa;
                }
                scratch_n += 1;
            } else {
                break;
            }
        }
        if scratch0 != 0 {
            serial::write_str("boot: guest-UEFI mmio scratch_hpa=0x");
            write_hex(scratch0);
            serial::write_str(" pool=");
            write_dec(scratch_n);
            serial::write_byte(b'\n');
        }
        // Iron fad19b2: CMOS 2GiB then EPT unbacked report-RAM gpa=0x7bddd000.
        // Preallocate 2MiB WB frames; map on EPT. GPA need not equal HPA
        // (ADR-004). Separate from UC scratch. Do not identity-map 2GiB.
        let mut report_n = 0u64;
        for i in 0..GUEST_UEFI_REPORT_RAM_SLOTS {
            if let Some(report_frame) = alloc.allocate_contiguous_aligned(512, 512) {
                let report_hpa = report_frame.to_phys();
                core::ptr::write_bytes(report_hpa as *mut u8, 0, 2 * 1024 * 1024);
                REPORT_RAM_HPA[i].store(report_hpa, Ordering::Release);
                REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
                report_n += 1;
            } else {
                break;
            }
        }
        if report_n != 0 {
            serial::write_str("boot: guest-UEFI report-RAM pool=");
            write_dec(report_n);
            serial::write_byte(b'\n');
        }
        // Do not 2MiB-sink 0xFEE00000: OVMF GetApicVersion() reads 0 and
        // DebugAssert (iron ad78f12, seven RDMSR 0x1B, then ret=0x6e8946).
        // Iron cc7d78a: 4G identity then EPT violation gpa=0xC01DF1B7
        // (PCI hole). PDPT[3] already exists for flash; link empty PDs for
        // 1–3GiB so later 4G walks can 2MiB-scratch. Do not pre-sink
        // 0xC0000000 (iron 577c9eb PT stores must persist).
        for i in 1u64..=2 {
            if let Some(pd) = alloc_phys(alloc) {
                let _ = ept_link_empty_pd(i as usize, pd);
            }
        }
        // Present EPT leaves so early hole walks do not EPT-fault.
        // Iron 5837243: pre-scratch of 0xC0000000..0xC0E00000 plus a
        // read-walk of 0xC1000000..0xC3A00000 filled pool=32; cap at
        // 0xC3C00000 then sink; RIP 0x3d00001. Only pre-scratch the
        // known PT-store GPA. Hole reads are R-only dedicated zero (not HPET
        // SINK_HPA; iron f93caee). Do not bulk 2–4GiB (73576cc).
        let _ = ept_map_2m_scratch(0x8000_0000);
        for &mm in &[0xFEC0_0000u64, 0xFED0_0000] {
            if ept_map_2m_sink(mm) {
                SINK_MAPS.fetch_add(1, Ordering::AcqRel);
            }
        }
        serial::write_str("boot: guest-UEFI platform sink_hpa=0x");
        write_hex(sink_hpa);
        serial::write_str(" maps=");
        write_dec(SINK_MAPS.load(Ordering::Acquire) as u64);
        serial::write_str(" live HPET=");
        write_dec(hpet_ok as u64);
        serial::write_byte(b'\n');
        if let (Some(pt), Some(lapic)) = (alloc_phys(alloc), alloc_phys(alloc)) {
            core::ptr::write_bytes(lapic as *mut u8, 0, 4096);
            // SAFETY: exclusive 4KiB xAPIC image; not host LAPIC.
            // KANI-TARGET: guest-UEFI xAPIC 4K fill (outside Proven Core).
            crate::devices::lapic_virt::fill_xapic_page(unsafe {
                core::slice::from_raw_parts_mut(lapic as *mut u8, 4096)
            });
            if ept_install_xapic_4k(pt, lapic) {
                serial::write_str("boot: guest-UEFI xAPIC 4K hpa=0x");
                write_hex(lapic);
                serial::write_str(" ver=0x");
                write_hex(u64::from(crate::devices::lapic_virt::XAPIC_VERSION));
                serial::write_byte(b'\n');
            } else {
                serial::write_line("boot: guest-UEFI WARN — xAPIC 4K EPT map failed");
            }
        } else {
            serial::write_line("boot: guest-UEFI WARN — no xAPIC 4K frames");
        }
    } else {
        serial::write_line("boot: guest-UEFI no 2MiB platform sink — CMOS/fw_cfg still live");
    }

    let Some(vmcs) = alloc_phys(alloc) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let Some(stack_frame) = alloc.allocate_contiguous(4) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let host_stack = stack_frame.to_phys();
    core::ptr::write_bytes(host_stack as *mut u8, 0, 4 * 4096);
    let host_rsp = (host_stack + 4 * 4096) & !0xFu64;
    let Some(gdt) = alloc_phys(alloc) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let Some(tss) = alloc_phys(alloc) else {
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    };
    let io_a = alloc_phys(alloc);
    let io_b = alloc_phys(alloc);
    let msr_bmp = alloc_phys(alloc);

    SAVED_VMCS = vmcs;
    let _ = GUEST_UEFI_PRIVATE_VMCS_ID;

    serial::write_str("boot: guest-UEFI private VMCS=0x");
    write_hex(vmcs);
    serial::write_str(" EPTP=0x");
    write_hex(eptp);
    serial::write_str(" alias_gpa=0x");
    write_hex(gpa);
    serial::write_str(" fw_hpa=0x");
    write_hex(fw_hpa);
    serial::write_str(" bytes=");
    write_dec(fw_len);
    serial::write_byte(b'\n');

    if crate::devices::ide_cdrom::present_placeholder_if_idle() {
        serial::write_str("boot: guest-UEFI CD GuestVisible iso=");
        write_dec(crate::devices::ide_cdrom::retained_iso_id());
        serial::write_str(" bytes=");
        write_dec(crate::devices::ide_cdrom::retained_len() as u64);
        serial::write_byte(b'\n');
    }
    if crate::devices::guest_virtio_blk::present() {
        serial::write_line("boot: guest-UEFI virtio-blk empty CD→disk order");
    }
    if crate::devices::guest_platform::e820_splits_gcd_mid_gap() {
        serial::write_line("boot: guest-UEFI e820 mid-gap reserved (GCD)");
    }
    if crate::devices::guest_platform::e820_splits_vga_below_1m() {
        serial::write_line(
            "boot: guest-UEFI fw_cfg etc/e820 offered (PEI FindFile or CMOS HOBs)",
        );
    }
    serial::write_line(
        "boot: guest-UEFI PEI 00:00.0 DID i440FX 0x1237 (MemMap VGA HOB)",
    );
    if crate::devices::guest_platform::platform_reports_2g_lowmem() {
        serial::write_line("boot: guest-UEFI CMOS LowMemory 2GiB (GCD)");
    }

    if let Err(e) = setup_guest_uefi_vmcs(vmcs, host_rsp, gdt, tss, eptp, io_a, io_b, msr_bmp) {
        serial::write_str("boot: guest-UEFI VMCS setup failed: ");
        serial::write_line(match e {
            LaunchError::EptUnsupported => "EptUnsupported",
            LaunchError::PrepareFailed => "PrepareFailed",
            LaunchError::ClearFailed => "ClearFailed",
            LaunchError::PtrldFailed => "PtrldFailed",
            LaunchError::VmwriteFailed { .. } => "VmwriteFailed",
            LaunchError::LaunchFailed { .. } => "LaunchFailed",
            LaunchError::CpuidExitingUnsupported => "CpuidExitingUnsupported",
        });
        return Err(GuestUefiLaunchError::LaunchSetupFailed);
    }

    serial::write_line("boot: guest-UEFI VMLAUNCH → reset vector 0xFFFF_FFF0");
    match ops::vmlaunch() {
        Ok(()) => {
            serial::write_line("boot: ERROR — guest-UEFI VMLAUNCH returned Ok");
            Err(GuestUefiLaunchError::LaunchSetupFailed)
        }
        Err(_) => {
            let ierr = ops::vmread(VM_INSTRUCTION_ERROR).unwrap_or(0xFFFF) as u32;
            LAST_INSN_ERROR.store(ierr, Ordering::Release);
            serial::write_str("boot: guest-UEFI VMLAUNCH failed insn_error=0x");
            write_hex_u32(ierr);
            serial::write_byte(b'\n');
            if ierr == 7 {
                serial::write_line("boot: hint: error 7 = invalid VMX control field(s)");
            } else if ierr == 8 {
                serial::write_line("boot: hint: error 8 = invalid host-state");
            }
            let _ = ops::vmclear(vmcs);
            Err(GuestUefiLaunchError::LaunchSetupFailed)
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn setup_guest_uefi_vmcs(
    vmcs: u64,
    host_rsp: u64,
    gdt: u64,
    tss: u64,
    eptp: u64,
    io_a: Option<u64>,
    io_b: Option<u64>,
    msr_bmp: Option<u64>,
) -> Result<(), LaunchError> {
    let (gdt_base, gdt_limit, tr, tr_base) = install_host_tss(gdt, tss)?;
    let use_true = true_ctl_msrs_supported();
    let pin_msr = if use_true {
        IA32_VMX_TRUE_PINBASED_CTLS
    } else {
        IA32_VMX_PINBASED_CTLS
    };
    let proc_msr = if use_true {
        IA32_VMX_TRUE_PROCBASED_CTLS
    } else {
        IA32_VMX_PROCBASED_CTLS
    };
    let exit_msr = if use_true {
        IA32_VMX_TRUE_EXIT_CTLS
    } else {
        IA32_VMX_EXIT_CTLS
    };
    let entry_msr = if use_true {
        IA32_VMX_TRUE_ENTRY_CTLS
    } else {
        IA32_VMX_ENTRY_CTLS
    };

    let pin = adjust_vmx_controls(
        PIN_BASED_EXTERNAL_INTERRUPT_EXITING | PIN_BASED_VMX_PREEMPTION_TIMER,
        pin_msr,
    );
    // Same wanted bits as E4, then drop unconditional I/O if bitmaps won
    // (SDM: the two I/O-exit controls must not both be 1).
    let mut primary = adjust_vmx_controls(
        CPU_BASED_HLT_EXITING
            | CPU_BASED_USE_IO_BITMAPS
            | CPU_BASED_UNCONDITIONAL_IO
            | CPU_BASED_USE_MSR_BITMAPS
            | CPU_BASED_ACTIVATE_SECONDARY,
        proc_msr,
    );
    if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        primary &= !CPU_BASED_UNCONDITIONAL_IO;
    }
    if primary & CPU_BASED_USE_TPR_SHADOW != 0 {
        serial::write_line("boot: guest-UEFI WARN — TPR shadow forced (no virt-APIC)");
    }
    if primary & CPU_BASED_ACTIVATE_SECONDARY == 0 {
        serial::write_line("boot: guest-UEFI secondary controls not allowed");
        return Err(LaunchError::EptUnsupported);
    }
    let exit_wanted = VM_EXIT_HOST_ADDR_SPACE_SIZE
        | VM_EXIT_ACK_INTERRUPT_ON_EXIT
        | VM_EXIT_SAVE_IA32_EFER
        | VM_EXIT_LOAD_IA32_EFER
        | VM_EXIT_SAVE_IA32_PAT
        | VM_EXIT_LOAD_IA32_PAT;
    let entry_wanted = VM_ENTRY_LOAD_IA32_EFER | VM_ENTRY_LOAD_IA32_PAT; // no IA-32e — real mode
    let exit_ctls = adjust_vmx_controls(exit_wanted, exit_msr);
    let entry_ctls = adjust_vmx_controls(entry_wanted, entry_msr);
    if entry_ctls & VM_ENTRY_IA32E_MODE != 0 {
        serial::write_line(
            "boot: guest-UEFI ERROR — IA-32e entry forced (need unrestricted real mode)",
        );
        return Err(LaunchError::EptUnsupported);
    }
    // Iron COM2: host CPUID advertises INVPCID/XSAVES/RDTSCP (Xeon Silver
    // 4110); OVMF then executes them. Without these bits the insn #UD at
    // RIP 0x109D, DebugLib dumps COM1, CpuDeadLoop, pci_ide=0. Nested
    // QEMU CPUID often lacks them, so BOTH-OK never saw this. Same bits
    // as the E4 SHELL VMCS (launch.rs).
    let secondary = adjust_vmx_controls(
        SECONDARY_ENABLE_EPT
            | GUEST_UEFI_UNRESTRICTED_GUEST
            | SECONDARY_ENABLE_RDTSCP
            | SECONDARY_ENABLE_INVPCID
            | SECONDARY_ENABLE_XSAVES,
        IA32_VMX_PROCBASED_CTLS2,
    );
    if secondary & SECONDARY_ENABLE_EPT == 0 {
        serial::write_line("boot: guest-UEFI EPT not allowed");
        return Err(LaunchError::EptUnsupported);
    }
    if secondary & GUEST_UEFI_UNRESTRICTED_GUEST == 0 {
        serial::write_line("boot: guest-UEFI unrestricted guest not allowed");
        return Err(LaunchError::EptUnsupported);
    }
    if secondary & SECONDARY_ENABLE_RDTSCP == 0 {
        serial::write_line("boot: guest-UEFI WARN — RDTSCP not allowed; OVMF may #UD");
    }
    if secondary & SECONDARY_ENABLE_INVPCID == 0 {
        serial::write_line("boot: guest-UEFI WARN — INVPCID not allowed; OVMF may #UD");
    }
    if secondary & SECONDARY_ENABLE_XSAVES == 0 {
        serial::write_line("boot: guest-UEFI WARN — XSAVES not allowed; OVMF may #UD");
    }

    if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        let (Some(a), Some(b)) = (io_a, io_b) else {
            return Err(LaunchError::PrepareFailed);
        };
        core::ptr::write_bytes(a as *mut u8, 0xFF, 4096);
        core::ptr::write_bytes(b as *mut u8, 0xFF, 4096);
    }
    if primary & CPU_BASED_USE_MSR_BITMAPS != 0 {
        let Some(bmp) = msr_bmp else {
            return Err(LaunchError::PrepareFailed);
        };
        core::ptr::write_bytes(bmp as *mut u8, 0xFF, 4096);
    }

    let host_cr0 = cpu::read_cr0();
    let host_cr3 = cpu::read_cr3();
    let host_cr4 = cpu::read_cr4();
    let efer = cpu::rdmsr(IA32_EFER);
    let host_pat = cpu::rdmsr(IA32_PAT);
    let guest_pat = if host_pat != 0 {
        host_pat
    } else {
        IA32_PAT_RESET
    };
    let idtr = cpu::sidt();
    let cs = cpu::read_cs();
    let ss = cpu::read_ss();
    let ds = cpu::read_ds();
    let es = cpu::read_es();
    let fs = cpu::read_fs();
    let gs = cpu::read_gs();
    let fs_base = cpu::rdmsr(IA32_FS_BASE);
    let gs_base = cpu::rdmsr(IA32_GS_BASE);
    let sysenter_cs = cpu::rdmsr(IA32_SYSENTER_CS) as u32;
    let sysenter_esp = cpu::rdmsr(IA32_SYSENTER_ESP);
    let sysenter_eip = cpu::rdmsr(IA32_SYSENTER_EIP);

    let guest_cr0 = guest_cr0_real();
    let guest_cr4 = apply_guest_cr4_write(guest_cr4_real());

    prepare_vmcs_region(vmcs)?;
    ops::vmclear(vmcs).map_err(|_| LaunchError::ClearFailed)?;
    prepare_vmcs_region(vmcs)?;

    let host_rip = guest_uefi_vmexit_landing as *const () as u64;

    match ops::vmptrld_and_vmwrite(vmcs, VMCS_LINK_POINTER, !0u64) {
        Ok(()) => {}
        Err(_) => {
            return Err(LaunchError::VmwriteFailed {
                field: VMCS_LINK_POINTER,
            })
        }
    }

    vw(PIN_BASED_VM_EXEC_CONTROL, pin as u64)?;
    if pin & PIN_BASED_VMX_PREEMPTION_TIMER != 0 {
        vw(VMX_PREEMPTION_TIMER_VALUE, VMX_PREEMPTION_TIMER_TICKS)?;
        PREEMPT_RELOAD.store(VMX_PREEMPTION_TIMER_TICKS as u32, Ordering::Release);
        serial::write_line("boot: guest-UEFI VMX preemption timer for live HPET");
    }
    vw(PRIMARY_PROC_BASED_VM_EXEC_CONTROL, primary as u64)?;
    vw(VM_EXIT_CONTROLS, exit_ctls as u64)?;
    vw(VM_ENTRY_CONTROLS, entry_ctls as u64)?;
    // Catch #UD/#DF/#GP/#PF. #UD: iron 0ca02e6 XSAVE without OSXSAVE
    // dumped COM1 in the guest; intercept so we can set OSXSAVE and retry.
    // #PF: iron d5fceb1 err=0 CR2 0x80B000 MEMFD after CpuDxe; identity-map NP.
    const UEFI_EXC_BITMAP: u32 = (1 << 6) | (1 << 8) | (1 << 13) | (1 << 14);
    vw(EXCEPTION_BITMAP, UEFI_EXC_BITMAP as u64)?;
    vw(PAGE_FAULT_ERROR_CODE_MASK, 0)?;
    vw(PAGE_FAULT_ERROR_CODE_MATCH, 0)?;
    vw(CR3_TARGET_COUNT, 0)?;
    vw(VM_EXIT_MSR_STORE_COUNT, 0)?;
    vw(VM_EXIT_MSR_LOAD_COUNT, 0)?;
    vw(VM_ENTRY_MSR_LOAD_COUNT, 0)?;
    vw(VM_ENTRY_INTERRUPTION_INFO, 0)?;
    vw(CR0_GUEST_HOST_MASK, 0)?;
    // Host-own CR4.VMXE (E4 Linux path) and CR4.OSXSAVE (iron 0ca02e6).
    // OVMF SEC `mov cr4, 0x640` clears VMXE; CpuDxe `mov cr4, 0x668` clears OSXSAVE.
    vw(CR4_GUEST_HOST_MASK, GUEST_UEFI_CR4_HOST_OWNED)?;
    vw(CR0_READ_SHADOW, 0)?;
    vw(CR4_READ_SHADOW, guest_cr4_read_shadow(guest_cr4))?;
    vw(SECONDARY_VM_EXEC_CONTROL, secondary as u64)?;
    vw(EPT_POINTER, eptp)?;
    if primary & CPU_BASED_USE_MSR_BITMAPS != 0 {
        if let Some(bmp) = msr_bmp {
            vw(MSR_BITMAP, bmp)?;
        }
    }
    if primary & CPU_BASED_USE_IO_BITMAPS != 0 {
        if let (Some(a), Some(b)) = (io_a, io_b) {
            vw(IO_BITMAP_A, a)?;
            vw(IO_BITMAP_B, b)?;
        }
    }

    // Real-mode reset (SDM 9.1.4) + unrestricted guest.
    const AR_CODE16: u64 = 0x009B;
    const AR_DATA16: u64 = 0x0093;
    const AR_TR16: u64 = 0x008B;
    const AR_LDTR_UNUSABLE: u64 = 1 << 16;

    vw(GUEST_CS_SELECTOR, GUEST_UEFI_RESET_CS as u64)?;
    vw(GUEST_SS_SELECTOR, 0)?;
    vw(GUEST_DS_SELECTOR, 0)?;
    vw(GUEST_ES_SELECTOR, 0)?;
    vw(GUEST_FS_SELECTOR, 0)?;
    vw(GUEST_GS_SELECTOR, 0)?;
    vw(GUEST_LDTR_SELECTOR, 0)?;
    vw(GUEST_TR_SELECTOR, 0)?;

    vw(GUEST_CS_BASE, 0xFFFF_0000)?;
    vw(GUEST_SS_BASE, 0)?;
    vw(GUEST_DS_BASE, 0)?;
    vw(GUEST_ES_BASE, 0)?;
    vw(GUEST_FS_BASE, 0)?;
    vw(GUEST_GS_BASE, 0)?;
    vw(GUEST_LDTR_BASE, 0)?;
    vw(GUEST_TR_BASE, 0)?;
    vw(GUEST_GDTR_BASE, 0)?;
    vw(GUEST_IDTR_BASE, 0)?;

    vw(GUEST_CS_LIMIT, 0xFFFF)?;
    vw(GUEST_SS_LIMIT, 0xFFFF)?;
    vw(GUEST_DS_LIMIT, 0xFFFF)?;
    vw(GUEST_ES_LIMIT, 0xFFFF)?;
    vw(GUEST_FS_LIMIT, 0xFFFF)?;
    vw(GUEST_GS_LIMIT, 0xFFFF)?;
    vw(GUEST_LDTR_LIMIT, 0xFFFF)?;
    vw(GUEST_TR_LIMIT, 0xFFFF)?;
    vw(GUEST_GDTR_LIMIT, 0xFFFF)?;
    vw(GUEST_IDTR_LIMIT, 0xFFFF)?;

    vw(GUEST_CS_ACCESS_RIGHTS, AR_CODE16)?;
    vw(GUEST_SS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_DS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_ES_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_FS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_GS_ACCESS_RIGHTS, AR_DATA16)?;
    vw(GUEST_LDTR_ACCESS_RIGHTS, AR_LDTR_UNUSABLE)?;
    vw(GUEST_TR_ACCESS_RIGHTS, AR_TR16)?;

    vw(GUEST_CR0, guest_cr0)?;
    vw(GUEST_CR3, 0)?;
    vw(GUEST_CR4, guest_cr4)?;
    vw(GUEST_DR7, 0x400)?;
    vw(GUEST_IA32_EFER, 0)?;
    // VMCLEAR leaves GUEST_IA32_PAT=0. Iron 44c56db dumped pat=0x0 after
    // GetAllMtrrs (lastmsr=0x23f): PA0=UC vs MTRR WB RAM. Iron COM2
    // 1a93cb8 proved PAT WB (`pat=0x7010600070406`); remaining ASSERT
    // is NP mid-gap vs MTRR WB. E4 launch.rs writes PAT when SAVE/LOAD
    // PAT is set; guest-UEFI RDMSR 0x277 is also VmcsPat, so the field
    // must be the SDM reset even if LOAD_PAT is not forced.
    vw(GUEST_IA32_PAT, guest_pat)?;
    vw(GUEST_RSP, 0)?;
    vw(GUEST_RIP, GUEST_UEFI_RESET_RIP)?;
    vw(GUEST_RFLAGS, 0x2)?;
    vw(GUEST_ACTIVITY_STATE, 0)?;
    vw(GUEST_INTERRUPTIBILITY_STATE, 0)?;
    vw(GUEST_PENDING_DBG_EXCEPTIONS, 0)?;
    vw(GUEST_IA32_SYSENTER_CS, 0)?;
    vw(GUEST_IA32_SYSENTER_ESP, 0)?;
    vw(GUEST_IA32_SYSENTER_EIP, 0)?;

    vw(HOST_ES_SELECTOR, (es & 0xF8) as u64)?;
    vw(HOST_CS_SELECTOR, (cs & 0xF8) as u64)?;
    vw(HOST_SS_SELECTOR, (ss & 0xF8) as u64)?;
    vw(HOST_DS_SELECTOR, (ds & 0xF8) as u64)?;
    vw(HOST_FS_SELECTOR, (fs & 0xF8) as u64)?;
    vw(HOST_GS_SELECTOR, (gs & 0xF8) as u64)?;
    vw(HOST_TR_SELECTOR, (tr & 0xF8) as u64)?;
    vw(HOST_CR0, host_cr0)?;
    vw(HOST_CR3, host_cr3)?;
    vw(HOST_CR4, host_cr4)?;
    vw(HOST_FS_BASE, fs_base)?;
    vw(HOST_GS_BASE, gs_base)?;
    vw(HOST_TR_BASE, tr_base)?;
    vw(HOST_GDTR_BASE, gdt_base)?;
    vw(HOST_IDTR_BASE, idtr.base)?;
    vw(HOST_IA32_SYSENTER_CS, sysenter_cs as u64)?;
    vw(HOST_IA32_SYSENTER_ESP, sysenter_esp)?;
    vw(HOST_IA32_SYSENTER_EIP, sysenter_eip)?;
    vw(HOST_IA32_EFER, efer)?;
    if exit_ctls & VM_EXIT_LOAD_IA32_PAT != 0 {
        vw(HOST_IA32_PAT, guest_pat)?;
    }
    serial::write_str("boot: guest-UEFI IA32_PAT guest=0x");
    write_hex(guest_pat);
    serial::write_str(" host=0x");
    write_hex(host_pat);
    serial::write_str(" entry=0x");
    write_hex(u64::from(entry_ctls));
    serial::write_byte(b'\n');
    vw(HOST_RSP, host_rsp)?;
    vw(HOST_RIP, host_rip)?;

    let _ = gdt_limit;
    Ok(())
}

#[cfg(target_os = "uefi")]
unsafe fn guest_cr0_real() -> u64 {
    let fixed0 = cpu::rdmsr(IA32_VMX_CR0_FIXED0);
    let fixed1 = cpu::rdmsr(IA32_VMX_CR0_FIXED1);
    // Unrestricted guest: PE and PG in FIXED0 are not enforced.
    let mut cr0 = 0x6000_0010u64; // CD | NW | ET
    cr0 |= fixed0 & !0x8000_0001;
    cr0 &= fixed1;
    cr0
}

#[cfg(target_os = "uefi")]
unsafe fn guest_cr4_real() -> u64 {
    let fixed0 = cpu::rdmsr(IA32_VMX_CR4_FIXED0);
    let fixed1 = cpu::rdmsr(IA32_VMX_CR4_FIXED1);
    fixed0 & fixed1
}

#[cfg(target_os = "uefi")]
unsafe fn vw(field: u64, value: u64) -> Result<(), LaunchError> {
    ops::vmwrite(field, value).map_err(|_| LaunchError::VmwriteFailed { field })
}

/// HOST_RIP trampoline — save guest GPRs before Rust clobbers them.
#[cfg(target_os = "uefi")]
#[unsafe(naked)]
pub unsafe extern "C" fn guest_uefi_vmexit_landing() -> ! {
    core::arch::naked_asm!(
        "mov [rip + {slot_rax}], rax",
        "mov [rip + {slot_rbx}], rbx",
        "mov [rip + {slot_rcx}], rcx",
        "mov [rip + {slot_rdx}], rdx",
        "mov [rip + {slot_rsi}], rsi",
        "mov [rip + {slot_rdi}], rdi",
        "mov [rip + {slot_rbp}], rbp",
        "mov [rip + {slot_r8}], r8",
        "mov [rip + {slot_r9}], r9",
        "mov [rip + {slot_r10}], r10",
        "mov [rip + {slot_r11}], r11",
        "mov [rip + {slot_r12}], r12",
        "mov [rip + {slot_r13}], r13",
        "mov [rip + {slot_r14}], r14",
        "mov [rip + {slot_r15}], r15",
        "jmp {cont}",
        slot_rax = sym SAVED_RAX,
        slot_rbx = sym SAVED_RBX,
        slot_rcx = sym SAVED_RCX,
        slot_rdx = sym SAVED_RDX,
        slot_rsi = sym SAVED_RSI,
        slot_rdi = sym SAVED_RDI,
        slot_rbp = sym SAVED_RBP,
        slot_r8 = sym SAVED_R8,
        slot_r9 = sym SAVED_R9,
        slot_r10 = sym SAVED_R10,
        slot_r11 = sym SAVED_R11,
        slot_r12 = sym SAVED_R12,
        slot_r13 = sym SAVED_R13,
        slot_r14 = sym SAVED_R14,
        slot_r15 = sym SAVED_R15,
        cont = sym guest_uefi_vmexit,
    );
}

#[cfg(target_os = "uefi")]
#[unsafe(naked)]
unsafe extern "C" fn guest_uefi_vmresume() -> ! {
    core::arch::naked_asm!(
        "mov rax, [rip + {slot_rax}]",
        "mov rbx, [rip + {slot_rbx}]",
        "mov rcx, [rip + {slot_rcx}]",
        "mov rdx, [rip + {slot_rdx}]",
        "mov rsi, [rip + {slot_rsi}]",
        "mov rdi, [rip + {slot_rdi}]",
        "mov rbp, [rip + {slot_rbp}]",
        "mov r8, [rip + {slot_r8}]",
        "mov r9, [rip + {slot_r9}]",
        "mov r10, [rip + {slot_r10}]",
        "mov r11, [rip + {slot_r11}]",
        "mov r12, [rip + {slot_r12}]",
        "mov r13, [rip + {slot_r13}]",
        "mov r14, [rip + {slot_r14}]",
        "mov r15, [rip + {slot_r15}]",
        "vmresume",
        "jmp {fail}",
        slot_rax = sym SAVED_RAX,
        slot_rbx = sym SAVED_RBX,
        slot_rcx = sym SAVED_RCX,
        slot_rdx = sym SAVED_RDX,
        slot_rsi = sym SAVED_RSI,
        slot_rdi = sym SAVED_RDI,
        slot_rbp = sym SAVED_RBP,
        slot_r8 = sym SAVED_R8,
        slot_r9 = sym SAVED_R9,
        slot_r10 = sym SAVED_R10,
        slot_r11 = sym SAVED_R11,
        slot_r12 = sym SAVED_R12,
        slot_r13 = sym SAVED_R13,
        slot_r14 = sym SAVED_R14,
        slot_r15 = sym SAVED_R15,
        fail = sym guest_uefi_resume_failed,
    );
}

#[cfg(target_os = "uefi")]
unsafe extern "C" fn guest_uefi_resume_failed() -> ! {
    serial::write_line("boot: guest-UEFI VMRESUME failed — continuing E4 SHELL");
    leave_to_e4();
}

#[cfg(target_os = "uefi")]
fn tick_hpet_on_exit(basic: u32, gpa: u64, qual: u64) {
    let sink = SINK_HPA.load(Ordering::Acquire);
    if sink == 0 {
        return;
    }
    let acpi_io = basic == EXIT_REASON_IO_INSTRUCTION && {
        let port = io_port_from_qual(qual);
        let size = ((qual & 7) + 1) as u8;
        crate::devices::guest_platform::is_acpi_pm_timer_io(port, size)
    };
    let step = if basic == EXIT_REASON_PREEMPTION_TIMER
        || basic == EXIT_REASON_HLT
        || acpi_io
        || (basic == EXIT_REASON_EPT_VIOLATION && crate::devices::guest_platform::is_hpet_gpa(gpa))
    {
        crate::devices::guest_platform::HPET_MAIN_STEP
    } else {
        0
    };
    // SAFETY: 2 MiB exclusive sink allocated at launch.
    // KANI-TARGET: HPET tick in guest-UEFI sink (outside Proven Core).
    let v = crate::devices::guest_platform::hpet_tick_sink_by(
        unsafe { core::slice::from_raw_parts_mut(sink as *mut u8, 2 * 1024 * 1024) },
        step,
    );
    if step != 0 && v != 0 {
        HPET_TICKS.fetch_add(1, Ordering::AcqRel);
    }
}

/// HOST_RIP continuation for the private guest-UEFI VMCS. Not the E4 SHELL landing.
#[cfg(target_os = "uefi")]
pub unsafe extern "C" fn guest_uefi_vmexit() -> ! {
    LAUNCH_ENTERED.store(true, Ordering::Release);
    let reason = ops::vmread(EXIT_REASON).unwrap_or(0xFFFF) as u32;
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let cs_base = ops::vmread(GUEST_CS_BASE).unwrap_or(0);
    let gpa = ops::vmread(GUEST_PHYSICAL_ADDRESS).unwrap_or(0);
    let intr = ops::vmread(VM_EXIT_INTR_INFO).unwrap_or(0);
    tick_hpet_on_exit(reason & 0xFFFF, gpa, qual);
    LAST_EXIT_REASON.store(reason, Ordering::Release);
    LAST_GUEST_RIP.store(rip, Ordering::Release);
    LAST_GUEST_PHYS.store(gpa, Ordering::Release);

    let n = EXIT_COUNT.fetch_add(1, Ordering::AcqRel) + 1;
    let basic = reason & 0xFFFF;
    let entry_fail = (reason & 0x8000_0000) != 0
        || basic == EXIT_REASON_VMENTRY_GUEST_STATE
        || basic == EXIT_REASON_VMENTRY_MSR_LOAD;
    let tf = basic == EXIT_REASON_TRIPLE_FAULT;
    let fetch_fail = basic == EXIT_REASON_EPT_VIOLATION && gpa == GUEST_UEFI_RESET_VECTOR_GPA;
    let linear = cs_base.wrapping_add(rip);
    LAST_LINEAR.store(linear, Ordering::Release);
    sync_guest_efer_lma();
    if linear_left_sec_tail(linear) {
        LEFT_SEC.store(true, Ordering::Release);
    }
    if basic == EXIT_REASON_PREEMPTION_TIMER {
        let prev = LAST_PREEMPT_RIP.swap(rip, Ordering::AcqRel);
        if prev == rip {
            PREEMPT_SAME_RIP.fetch_add(1, Ordering::AcqRel);
        } else {
            PREEMPT_SAME_RIP.store(1, Ordering::Release);
        }
    }

    if n <= 16 {
        serial::write_str("boot: guest-UEFI VMEXIT n=");
        write_dec(n as u64);
        serial::write_str(" reason=0x");
        write_hex_u32(reason);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_str(" cs_base=0x");
        write_hex(cs_base);
        serial::write_str(" qual=0x");
        write_hex(qual);
        serial::write_str(" gpa=0x");
        write_hex(gpa);
        if basic == EXIT_REASON_EXCEPTION_NMI {
            serial::write_str(" intr=0x");
            write_hex(intr);
        }
        serial::write_byte(b'\n');
    } else if n % 256 == 0 {
        serial::write_str("boot: guest-UEFI tick n=");
        write_dec(n as u64);
        serial::write_str(" reason=0x");
        write_hex_u32(reason);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_str(" hpet=");
        write_dec(HPET_TICKS.load(Ordering::Acquire) as u64);
        serial::write_str(" pci_ide=");
        write_dec(crate::devices::ide_cdrom::pci_enumerated() as u64);
        serial::write_str(" ataio=");
        write_dec(crate::devices::ide_cdrom::ata_io_accesses() as u64);
        serial::write_str(" spin=");
        write_dec(SPIN_JMP_SKIPS.load(Ordering::Acquire) as u64);
        serial::write_str(" same=");
        write_dec(PREEMPT_SAME_RIP.load(Ordering::Acquire) as u64);
        serial::write_str(" msr=0x");
        write_hex(u64::from(LAST_GUEST_MSR.load(Ordering::Acquire)));
        serial::write_str(" insn=");
        dump_low_ram_insn(linear);
        serial::write_byte(b'\n');
        guest_uefi_patch_cpu_flush_all_mapped();
    }

    if !entry_fail && !fetch_fail {
        if !MARKER_PRINTED.swap(true, Ordering::AcqRel) {
            serial::write_line(M7_E5_OVMF_VMLAUNCH_OK_MARKER);
            serial::write_str("boot: guest-UEFI linear=0x");
            write_hex(linear);
            serial::write_byte(b'\n');
        }
        if n == 1 {
            audit_log!(AuditEvent::OvmfGuestUefiVmlaunched {
                exit_reason: reason as u64,
                guest_rip: rip,
            });
        }
        if !tf {
            let nt = NON_TF_EXITS.fetch_add(1, Ordering::AcqRel) + 1;
            if nt >= 2 || basic == EXIT_REASON_HLT {
                maybe_print_alive(basic);
            }
            maybe_print_past_sec(basic == EXIT_REASON_HLT);
        }
    } else {
        serial::write_line("boot: guest-UEFI VM-entry/fetch failed — marker not claimed");
    }

    let mut resume = false;
    if !entry_fail && !tf && !fetch_fail && n < GUEST_UEFI_RESUME_CAP {
        resume = match basic {
            EXIT_REASON_IO_INSTRUCTION => handle_io(qual),
            EXIT_REASON_CPUID => handle_cpuid(),
            EXIT_REASON_MSR_READ => handle_rdmsr(),
            EXIT_REASON_MSR_WRITE => handle_wrmsr(),
            EXIT_REASON_HLT => {
                maybe_print_alive(basic);
                maybe_print_past_sec(true);
                // Skip, do not stop: a firmware HLT would otherwise cut the
                // post-DXE tail before PciBus walks IDE `00:00.1`.
                if hlt_should_resume() {
                    let k = HLT_SKIPS.fetch_add(1, Ordering::AcqRel);
                    if k < 4 {
                        serial::write_line("boot: guest-UEFI HLT skip");
                    }
                    skip_insn()
                } else {
                    false
                }
            }
            EXIT_REASON_EPT_VIOLATION => handle_ept(gpa, qual),
            EXIT_REASON_CR_ACCESS => handle_cr(qual),
            EXIT_REASON_EXCEPTION_NMI => handle_exception_nmi(intr, rip, linear, qual),
            EXIT_REASON_EXTERNAL_INTERRUPT => true,
            EXIT_REASON_PREEMPTION_TIMER => {
                // Iron 19b0c11: between tick 1024 (RIP still 0x1f6ba35 in
                // 32MiB) and 1280 (RIP 0x27e22d5 execute-from-zero) firmware
                // installed a 1GiB PDPTE. Split while RIP is still in RAM.
                let ram_hpa = RAM_HPA.load(Ordering::Acquire);
                let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
                if ram_hpa != 0 && cr3 != 0 && !guest_uefi_rip_is_hole_execute(rip) {
                    guest_uefi_split_low_ram_1g(cr3, ram_hpa);
                }
                true
            }
            EXIT_REASON_XSETBV => handle_xsetbv(),
            // INVD / INVLPG / RDTSC / PAUSE / WBINVD — skip, keep PEI moving.
            13 | 14 | 16 | 40 | 54 => skip_insn(),
            _ => false,
        };
        if resume {
            // Preemption (and any other resume) is not always an instruction
            // exit. CpuDeadLoop `pause` / `jmp $` never does I/O; skip those
            // so firmware can fall through. Delay `jcc` on I/O stays.
            // Iron `eb ec` + `leave; ret`: dump only. `aee545f` DXE-RAM
            // skip then #UD at 0x109d (same as 891eb5b). QEMU keeps eb f3.
            // Iron d5f9431: pause+jcc deadloop at 0x6e81ca only on
            // preemption (HPET +256 per 256 exits, no PCI/ATA).
            let skipped = if basic == EXIT_REASON_EXCEPTION_NMI {
                false
            } else if basic == EXIT_REASON_PREEMPTION_TIMER {
                skip_preempt_deadloop(linear, rip)
            } else {
                skip_spin_short_jmp(linear, rip)
            };
            if skipped {
                let k = SPIN_JMP_SKIPS.fetch_add(1, Ordering::AcqRel);
                if k < 8 {
                    serial::write_str("boot: guest-UEFI spin jmp skip insn=");
                    dump_low_ram_insn(linear);
                    serial::write_byte(b'\n');
                }
            } else if basic == EXIT_REASON_PREEMPTION_TIMER {
                let same = PREEMPT_SAME_RIP.load(Ordering::Acquire);
                if ASSERT_DEADLOOP_DUMP.load(Ordering::Acquire) == 0 && (same == 8 || same == 64)
                {
                    serial::write_str("boot: guest-UEFI preempt noskip same=");
                    write_dec(same as u64);
                    serial::write_str(" insn=");
                    dump_low_ram_insn(linear);
                    serial::write_byte(b'\n');
                }
            }
        }
        if resume
            && post_dxe_should_stop(
                DXE_PRINTED.load(Ordering::Acquire),
                n,
                DXE_AT_N.load(Ordering::Acquire),
                crate::devices::ide_cdrom::sectors_read(),
            )
        {
            resume = false;
        }
    }

    if resume {
        let reload = PREEMPT_RELOAD.load(Ordering::Acquire);
        if reload != 0 {
            let _ = ops::vmwrite(VMX_PREEMPTION_TIMER_VALUE, u64::from(reload));
        }
        CONTINUE_GUEST.store(true, Ordering::Release);
        guest_uefi_vmresume();
    }
    serial::write_str("boot: guest-UEFI stop n=");
    write_dec(n as u64);
    serial::write_str(" reason=0x");
    write_hex_u32(reason);
    serial::write_str(" rip=0x");
    write_hex(rip);
    serial::write_str(" left_sec=");
    write_dec(LEFT_SEC.load(Ordering::Acquire) as u64);
    serial::write_str(" pci=");
    write_dec(PCI_CONFIG_SEEN.load(Ordering::Acquire) as u64);
    serial::write_str(" com=");
    write_dec(COM_BYTES.load(Ordering::Acquire) as u64);
    serial::write_str(" past_sec=");
    write_dec(PAST_SEC_PRINTED.load(Ordering::Acquire) as u64);
    serial::write_str(" cd=");
    write_dec(crate::devices::ide_cdrom::is_visible() as u64);
    serial::write_str(" pci_ide=");
    write_dec(crate::devices::ide_cdrom::pci_enumerated() as u64);
    serial::write_str(" virtio=");
    write_dec(crate::devices::guest_virtio_blk::pci_enumerated() as u64);
    serial::write_str(" sectors=");
    write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
    serial::write_str(" packet=");
    write_dec(crate::devices::ide_cdrom::packet_commands() as u64);
    serial::write_str(" scsi=0x");
    write_hex_u32(u32::from(crate::devices::ide_cdrom::last_scsi()));
    serial::write_str(" ata=0x");
    write_hex_u32(u32::from(crate::devices::ide_cdrom::last_ata_cmd()));
    serial::write_str(" ataio=");
    write_dec(crate::devices::ide_cdrom::ata_io_accesses() as u64);
    serial::write_str(" unh=");
    write_dec(IO_UNHANDLED_N.load(Ordering::Acquire) as u64);
    serial::write_str(" plat=");
    write_dec(crate::devices::guest_platform::platform_memory_served() as u64);
    serial::write_str(" dxe=");
    write_dec(DXE_PRINTED.load(Ordering::Acquire) as u64);
    serial::write_str(" cf8=0x");
    write_hex_u32(LAST_CF8.load(Ordering::Acquire));
    serial::write_str(" port=0x");
    write_hex_u32(LAST_IO_PORT.load(Ordering::Acquire));
    serial::write_str(" bdfs=");
    write_dec(
        u64::from(PCI_BDF_SEEN0.load(Ordering::Acquire).count_ones())
            + u64::from(PCI_BDF_SEEN1.load(Ordering::Acquire).count_ones())
            + u64::from(PCI_BDF_SEEN2.load(Ordering::Acquire).count_ones())
            + u64::from(PCI_BDF_SEEN3.load(Ordering::Acquire).count_ones()),
    );
    serial::write_str(" hlt=");
    write_dec(HLT_SKIPS.load(Ordering::Acquire) as u64);
    serial::write_str(" spin=");
    write_dec(SPIN_JMP_SKIPS.load(Ordering::Acquire) as u64);
    serial::write_str(" cr=");
    write_dec(CR_ACCESSES.load(Ordering::Acquire) as u64);
    serial::write_str(" acpi=");
    write_dec(crate::devices::guest_platform::acpi_pm_timer_reads() as u64);
    serial::write_str(" ramr=");
    write_dec(RAM_REMAP_N.load(Ordering::Acquire) as u64);
    serial::write_str(" cmos=0x");
    write_hex_u32(u32::from(crate::devices::guest_platform::last_cmos_index()));
    serial::write_str(" hpet=");
    write_dec(HPET_TICKS.load(Ordering::Acquire) as u64);
    serial::write_str(" pre=");
    dump_low_ram_insn(linear.saturating_sub(16));
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    leave_to_e4();
}

#[cfg(target_os = "uefi")]
fn maybe_print_alive(basic: u32) {
    if MARKER_PRINTED.load(Ordering::Acquire)
        && !ALIVE_PRINTED.swap(true, Ordering::AcqRel)
        && (NON_TF_EXITS.load(Ordering::Acquire) >= 2 || basic == EXIT_REASON_HLT)
    {
        serial::write_line(M7_E5_OVMF_ALIVE_OK_MARKER);
        serial::write_str("boot: guest-UEFI non-tf exits=");
        write_dec(NON_TF_EXITS.load(Ordering::Acquire) as u64);
        serial::write_byte(b'\n');
        audit_log!(AuditEvent::OvmfGuestUefiAlive {
            exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
            last_reason: LAST_EXIT_REASON.load(Ordering::Acquire) as u64,
        });
    }
}

#[cfg(target_os = "uefi")]
fn maybe_print_past_sec(guest_hlt: bool) {
    if !MARKER_PRINTED.load(Ordering::Acquire) || !ALIVE_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if !past_sec_evidence(
        LEFT_SEC.load(Ordering::Acquire),
        PCI_CONFIG_SEEN.load(Ordering::Acquire),
        COM_BYTES.load(Ordering::Acquire),
        guest_hlt,
    ) {
        return;
    }
    if PAST_SEC_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_PAST_SEC_OK_MARKER);
    serial::write_str("boot: guest-UEFI past-SEC linear left 0xFFFF_0000 pci=");
    write_dec(PCI_CONFIG_SEEN.load(Ordering::Acquire) as u64);
    serial::write_str(" com=");
    write_dec(COM_BYTES.load(Ordering::Acquire) as u64);
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiPastSec {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        linear: LAST_GUEST_RIP.load(Ordering::Acquire),
        com_bytes: COM_BYTES.load(Ordering::Acquire) as u64,
    });
    maybe_print_cdrom();
    maybe_print_virtio();
    maybe_print_both();
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_cdrom() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if crate::devices::ide_cdrom::take_marker() {
        serial::write_line(M7_E5_OVMF_CDROM_OK_MARKER);
        serial::write_str("boot: guest-UEFI CD visible pci_ide=");
        write_dec(crate::devices::ide_cdrom::pci_enumerated() as u64);
        serial::write_str(" sectors=");
        write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
        serial::write_byte(b'\n');
        audit_log!(AuditEvent::OvmfGuestUefiCdrom {
            exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
            pci_enum: crate::devices::ide_cdrom::pci_enumerated() as u64,
            sectors: crate::devices::ide_cdrom::sectors_read() as u64,
        });
    }
    maybe_print_virtio();
    maybe_print_both();
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_virtio() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if crate::devices::guest_virtio_blk::take_marker() {
        serial::write_line(M7_E5_OVMF_VIRTIO_OK_MARKER);
        serial::write_str("boot: guest-UEFI virtio-blk pci=");
        write_dec(crate::devices::guest_virtio_blk::pci_enumerated() as u64);
        serial::write_str(" boot=CD,disk");
        serial::write_byte(b'\n');
        audit_log!(AuditEvent::OvmfGuestUefiVirtio {
            exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
            pci_enum: crate::devices::guest_virtio_blk::pci_enumerated() as u64,
        });
    }
    maybe_print_both();
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_both() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if !both_pci_evidence(
        crate::devices::guest_virtio_blk::pci_enumerated(),
        crate::devices::ide_cdrom::pci_enumerated(),
    ) {
        return;
    }
    if BOTH_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_BOTH_OK_MARKER);
    serial::write_str("boot: guest-UEFI both pci virtio=1 ide=1");
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiBoth {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        virtio: 1,
        ide: 1,
    });
    maybe_print_atapi();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_atapi() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    let sectors = crate::devices::ide_cdrom::sectors_read();
    if !atapi_read_evidence(sectors) {
        return;
    }
    if ATAPI_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_ATAPI_OK_MARKER);
    serial::write_str("boot: guest-UEFI atapi sectors=");
    write_dec(sectors as u64);
    serial::write_str(" packet=");
    write_dec(crate::devices::ide_cdrom::packet_commands() as u64);
    serial::write_str(" scsi=0x");
    write_hex_u32(u32::from(crate::devices::ide_cdrom::last_scsi()));
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiAtapi {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        sectors: sectors as u64,
    });
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_dxe() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    let linear = LAST_LINEAR.load(Ordering::Acquire);
    let cs_ok = exec_from_low_ram(linear);
    if !dxe_or_cd_boot_evidence(
        true,
        crate::devices::ide_cdrom::sectors_read(),
        crate::devices::guest_platform::platform_memory_served(),
        cs_ok,
    ) {
        return;
    }
    if DXE_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    DXE_AT_N.store(EXIT_COUNT.load(Ordering::Acquire), Ordering::Release);
    serial::write_line(M7_E5_OVMF_DXE_OK_MARKER);
    serial::write_str("boot: guest-UEFI past-PEI/DXE or CD boot attempt sectors=");
    write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
    serial::write_str(" plat=");
    write_dec(crate::devices::guest_platform::platform_memory_served() as u64);
    serial::write_str(" ram_rip=");
    write_dec(cs_ok as u64);
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiDxe {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        sectors: crate::devices::ide_cdrom::sectors_read() as u64,
        platform: crate::devices::guest_platform::platform_memory_served() as u64,
    });
}

#[cfg(target_os = "uefi")]
fn note_pci_cf8(addr: u32) {
    if (addr & 0x8000_0000) == 0 {
        return;
    }
    let (bus, dev, fun, _) = crate::devices::guest_platform::pci_bdf(addr);
    if bus != 0 {
        return;
    }
    if dev != 0 || fun != 0 {
        if crate::devices::guest_virtio_blk::latch_dxe_virtio_did() {
            serial::write_str("boot: guest-UEFI DXE virtio DID latch 00:");
            write_hex_u8(dev);
            serial::write_byte(b'.');
            write_hex_u8(fun);
            serial::write_byte(b'\n');
        }
    }
    let Some((word, bit)) = pci_bdf_bit(dev, fun) else {
        return;
    };
    let slot = match word {
        0 => &PCI_BDF_SEEN0,
        1 => &PCI_BDF_SEEN1,
        2 => &PCI_BDF_SEEN2,
        _ => &PCI_BDF_SEEN3,
    };
    let prev = slot.fetch_or(bit, Ordering::AcqRel);
    if prev & bit != 0 {
        return;
    }
    serial::write_str("boot: guest-UEFI pci select 00:");
    write_hex_u8(dev);
    serial::write_byte(b'.');
    write_hex_u8(fun);
    serial::write_byte(b'\n');
}

/// First DID / Header Type / class+BAR config cycles (Stage 44: PciBus vs ATA).
#[cfg(target_os = "uefi")]
fn trace_pci_cfg(cfg: u32, val: u32, size: u8, write: bool) {
    let aligned = (cfg as u8) & 0xFC;
    let (ctr, cap) = if aligned == 0 {
        (&PCI_DID_TRACE, 16u32)
    } else if aligned == 0x0C {
        (&PCI_HT_TRACE, 8u32)
    } else if aligned == 0x08 || (0x10..=0x20).contains(&aligned) {
        (&PCI_BAR_TRACE, 24u32)
    } else {
        return;
    };
    let n = ctr.fetch_add(1, Ordering::AcqRel);
    if n >= cap {
        return;
    }
    serial::write_str(if write {
        "boot: guest-UEFI pci wr=0x"
    } else {
        "boot: guest-UEFI pci cfg=0x"
    });
    write_hex_u32(cfg);
    serial::write_str(" val=0x");
    write_hex_u32(val);
    serial::write_str(" size=");
    write_dec(u64::from(size));
    if aligned == 0 && !write {
        serial::write_str(" pei_did=");
        write_dec(crate::devices::guest_virtio_blk::pei_host_bridge_did() as u64);
    }
    serial::write_byte(b'\n');
}

#[cfg(target_os = "uefi")]
fn write_hex_u8(v: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    serial::write_byte(HEX[(v >> 4) as usize]);
    serial::write_byte(HEX[(v & 0xf) as usize]);
}

/// Keep guest+host CR4.OSXSAVE set (XSAVE / XSETBV). Host-owned bit 18.
#[cfg(target_os = "uefi")]
unsafe fn force_guest_cr4_osxsave() {
    let cur = ops::vmread(GUEST_CR4).unwrap_or(0);
    let val = apply_guest_cr4_write(cur);
    let _ = ops::vmwrite(GUEST_CR4, val);
    let _ = ops::vmwrite(CR4_READ_SHADOW, guest_cr4_read_shadow(val));
    // SAFETY: VMX root. OSXSAVE is not CR4-fixed0-forbidden on this CPU.
    // KANI-TARGET: guest-UEFI host CR4.OSXSAVE (outside Proven Core).
    let host = cpu::read_cr4();
    if host & cpu::CR4_OSXSAVE == 0 {
        cpu::write_cr4(host | cpu::CR4_OSXSAVE);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn copy_guest_identity_bytes(linear: u64, buf: &mut [u8]) -> usize {
    if linear < GUEST_UEFI_LOW_RAM_BYTES {
        let hpa = RAM_HPA.load(Ordering::Acquire);
        if hpa == 0 {
            return 0;
        }
        // SAFETY: exclusive guest-UEFI 32 MiB slab; firmware is VMX-halted.
        // KANI-TARGET: identity peek low RAM (outside Proven Core).
        let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
        return copy_low_ram_at(ram, linear, buf);
    }
    if !guest_uefi_report_ram_should_map(linear) {
        return 0;
    }
    let hpa = report_ram_hpa_lookup(linear);
    if hpa == 0 {
        return 0;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already mapped for this GPA.
    // KANI-TARGET: identity peek report-RAM (outside Proven Core).
    let page = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
    copy_report_ram_at(page, linear, buf)
}

#[cfg(target_os = "uefi")]
unsafe fn read_low_ram_insn(linear: u64, buf: &mut [u8; 16]) -> usize {
    copy_guest_identity_bytes(linear, buf)
}

/// Iron `0ca02e6`: `#UD` RIP `0x109D` CR4=0x668 (no OSXSAVE). Guest DebugLib
/// dumped COM1; preempt skip of `eb ec` re-entered the dump until n=32768.
/// Iron `891eb5b`: OSXSAVE host-own intercepted SEC CR4; `#UD` intercept
/// stopped at n=1439 (no dump loop) after skip of `ebecc9c3` executed
/// `leave; ret` into PE-header DAA at `0x109D`. Retry XSAVE after setting
/// OSXSAVE. Skip `ud2` so ASSERT does not dump-loop. Unknown `#UD` stops.
#[cfg(target_os = "uefi")]
unsafe fn handle_ud(rip: u64, linear: u64) -> bool {
    let mut buf = [0u8; 16];
    let n = read_low_ram_insn(linear, &mut buf);
    force_guest_cr4_osxsave();
    if ud_xsave_family(&buf[..n]) {
        let k = UD_XSAVE_RETRY.fetch_add(1, Ordering::AcqRel);
        if k < 4 {
            serial::write_str("boot: guest-UEFI #UD XSAVE retry OSXSAVE insn=");
            dump_low_ram_insn(linear);
            serial::write_byte(b'\n');
        }
        return true;
    }
    if ud_is_ud2(&buf[..n]) {
        let k = UD2_SKIPS.fetch_add(1, Ordering::AcqRel);
        if k < 4 {
            serial::write_str("boot: guest-UEFI #UD2 skip insn=");
            dump_low_ram_insn(linear);
            serial::write_byte(b'\n');
        }
        if k >= 32 {
            return false;
        }
        return ops::vmwrite(GUEST_RIP, rip.wrapping_add(2)).is_ok();
    }
    serial::write_str("boot: guest-UEFI #UD unknown linear=0x");
    write_hex(linear);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    false
}

#[cfg(target_os = "uefi")]
unsafe fn handle_exception_nmi(intr: u64, rip: u64, linear: u64, qual: u64) -> bool {
    let valid = (intr & (1u64 << 31)) != 0;
    let vec = (intr & 0xff) as u8;
    if valid && vec == 6 {
        return handle_ud(rip, linear);
    }
    if valid && vec == 14 {
        return handle_pf(rip, linear, qual);
    }
    let err = ops::vmread(VM_EXIT_INTR_ERROR_CODE).unwrap_or(0);
    let cs = ops::vmread(GUEST_CS_SELECTOR).unwrap_or(0);
    let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
    serial::write_str("boot: guest-UEFI exception intr=0x");
    write_hex(intr);
    serial::write_str(" err=0x");
    write_hex(err);
    serial::write_str(" cs=0x");
    write_hex(cs);
    serial::write_str(" cr0=0x");
    write_hex(cr0);
    serial::write_str(" linear=0x");
    write_hex(linear);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    false
}

/// Iron `101b8ec`: two 4G rebuilds at MEMFD then `fail=present` `pde=0x30646870`.
/// Build at [`GUEST_UEFI_HV_PML4`] (`0x200000`), always (`force`).
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_rebuild_sec_identity(
    ram_hpa: u64,
    force: bool,
) -> Result<crate::vmx::guest_pt::IdentityMapKind, crate::vmx::guest_pt::IdentityMapError> {
    if !force && SEC_IDENTITY_REBUILT.swap(true, Ordering::AcqRel) {
        return Err(crate::vmx::guest_pt::IdentityMapError::AlreadyPresent);
    }
    SEC_IDENTITY_REBUILT.store(true, Ordering::Release);
    crate::vmx::guest_pt::build_identity_4g(
        ram_hpa,
        GUEST_UEFI_LOW_RAM_BYTES,
        guest_uefi_pf_sec_cr3(),
    )?;
    Ok(crate::vmx::guest_pt::IdentityMapKind::Rebuild4G)
}

/// Split PDPT[0] 1GiB back to RAM-only 2MiB (iron `19b0c11` RIP `0x27e22d5`).
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_split_gpa0_1m_mtrr(walk: u64, ram_hpa: u64) -> bool {
    if ram_hpa == 0
        || !guest_uefi_gpa0_split_now(
            guest_uefi_phys_width(),
            guest_uefi_host_hypervisor_present(),
        )
    {
        return false;
    }
    let r = crate::vmx::guest_pt::identity_split_gpa0_fixed_mtrr(
        walk,
        ram_hpa,
        GUEST_UEFI_LOW_RAM_BYTES,
    );
    matches!(r, Ok(crate::vmx::guest_pt::IdentityMapKind::Split4K))
}

/// Split PDPT[0] 1GiB back to RAM-only 2MiB (iron `19b0c11` RIP `0x27e22d5`).
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_split_low_ram_1g(walk: u64, ram_hpa: u64) {
    if ram_hpa == 0 {
        return;
    }
    let r = crate::vmx::guest_pt::identity_map_mmio_2m(
        walk,
        GUEST_UEFI_IRON_PF_HEAP_WR_CR2,
        ram_hpa,
        GUEST_UEFI_LOW_RAM_BYTES,
    );
    let g0 = guest_uefi_split_gpa0_1m_mtrr(walk, ram_hpa);
    if matches!(r, Ok(crate::vmx::guest_pt::IdentityMapKind::Mmio2M)) {
        let _ = ops::vmwrite(GUEST_CR3, walk);
        if !PDPT0_SPLIT_PRINTED.swap(true, Ordering::AcqRel) {
            serial::write_line("boot: guest-UEFI #PF identity SPLIT PDPT0");
        }
    }
    if g0 && !GPA0_SPLIT_PRINTED.swap(true, Ordering::AcqRel) {
        let _ = ops::vmwrite(GUEST_CR3, walk);
        serial::write_line("boot: guest-UEFI identity SPLIT4K GPA0 1MiB MTRR");
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_pf(rip: u64, linear: u64, cr2: u64) -> bool {
    let err = ops::vmread(VM_EXIT_INTR_ERROR_CODE).unwrap_or(0);
    let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
    let ram_hpa = RAM_HPA.load(Ordering::Acquire);
    let walk = if guest_uefi_pf_should_load_sec_cr3(cr3) {
        guest_uefi_pf_sec_cr3()
    } else {
        cr3
    };
    let pde = if ram_hpa != 0 {
        crate::vmx::guest_pt::identity_walk_pde(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        )
    } else {
        0
    };
    serial::write_str("boot: guest-UEFI #PF cr2=0x");
    write_hex(cr2);
    serial::write_str(" err=0x");
    write_hex(err);
    serial::write_str(" cr3=0x");
    write_hex(cr3);
    serial::write_str(" cr0=0x");
    write_hex(ops::vmread(GUEST_CR0).unwrap_or(0));
    serial::write_str(" cr4=0x");
    write_hex(ops::vmread(GUEST_CR4).unwrap_or(0));
    serial::write_str(" efer=0x");
    write_hex(ops::vmread(GUEST_IA32_EFER).unwrap_or(0));
    serial::write_str(" pde=0x");
    write_hex(pde);
    serial::write_str(" pte=0x");
    write_hex(if ram_hpa != 0 {
        crate::vmx::guest_pt::identity_walk_pte(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        )
    } else {
        0
    });
    serial::write_str(" pml4e=0x");
    write_hex(if ram_hpa != 0 {
        crate::vmx::guest_pt::identity_walk_pml4e(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        )
    } else {
        0
    });
    serial::write_str(" pdpte=0x");
    write_hex(if ram_hpa != 0 {
        crate::vmx::guest_pt::identity_walk_pdpte(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        )
    } else {
        0
    });
    serial::write_str(" pdpte2=0x");
    write_hex(if ram_hpa != 0 {
        crate::vmx::guest_pt::identity_walk_pdpte(
            walk,
            crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        )
    } else {
        0
    });
    serial::write_str(" pdpte3=0x");
    write_hex(if ram_hpa != 0 {
        crate::vmx::guest_pt::identity_walk_pdpte(
            walk,
            crate::vmx::guest_pt::IDENTITY_MTRR_UC_3G,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        )
    } else {
        0
    });
    serial::write_str(" rip=0x");
    write_hex(rip);
    serial::write_str(" linear=0x");
    write_hex(linear);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_byte(b'\n');
    if ram_hpa == 0 {
        return false;
    }
    if guest_uefi_pf_should_fix_ram_wp(err, cr2) {
        let k = PF_FIXUPS.fetch_add(1, Ordering::AcqRel);
        if k >= GUEST_UEFI_PF_IDENTITY_CAP {
            serial::write_line("boot: guest-UEFI #PF identity cap");
            return false;
        }
        let r = crate::vmx::guest_pt::identity_fix_ram_wp(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        );
        match r {
            Ok(crate::vmx::guest_pt::IdentityMapKind::Split4K) => {
                // Same TLB shootdown as RAM 1GiB SPLIT n=2. VPID is off, but
                // iron 54a8708 still #PF'd the RW 4K leaf until the cap.
                let _ = ops::vmwrite(GUEST_CR3, walk);
                serial::write_str("boot: guest-UEFI #PF identity SPLIT4K n=");
                write_dec(u64::from(k) + 1);
                serial::write_str(" cr2=0x");
                write_hex(cr2);
                serial::write_byte(b'\n');
                return true;
            }
            Err(crate::vmx::guest_pt::IdentityMapError::AlreadyPresent) => {
                // Iron 54a8708: looping this hit the identity cap. Iron
                // 89c3731: leaf was already RW; identity_fix_ram_wp now
                // ORs R/W on PML4/PDPT first so this path is the true
                // residual (walker and CPU still disagree).
                serial::write_line(
                    "boot: guest-UEFI #PF identity SPLIT4K already RW — not looping",
                );
                return false;
            }
            _ => {
                serial::write_line("boot: guest-UEFI #PF identity SPLIT4K fail");
                return false;
            }
        }
    }
    if !guest_uefi_pf_should_identity_map(err, cr2) {
        if !guest_uefi_pf_should_map_mmio(err, cr2) {
            return false;
        }
        if guest_uefi_rip_is_hole_execute(rip) {
            serial::write_line("boot: guest-UEFI #PF MMIO skip — RIP left 32MiB RAM");
            return false;
        }
        if guest_uefi_rip_is_poison_fill(rip) {
            serial::write_line("boot: guest-UEFI #PF poison fill — not resume");
            return false;
        }
        let k = PF_FIXUPS.fetch_add(1, Ordering::AcqRel);
        if k >= GUEST_UEFI_PF_IDENTITY_CAP {
            serial::write_line("boot: guest-UEFI #PF identity cap");
            return false;
        }
        let mut r = crate::vmx::guest_pt::identity_map_mmio_2m(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        );
        if !matches!(
            r,
            Ok(crate::vmx::guest_pt::IdentityMapKind::Mmio2M)
                | Err(crate::vmx::guest_pt::IdentityMapError::AlreadyPresent)
        ) {
            // Iron a428202: 1GiB PDPTE after firmware retargeted PDPT.
            let _ = guest_uefi_rebuild_sec_identity(ram_hpa, true);
            let sec = guest_uefi_pf_sec_cr3();
            let _ = ops::vmwrite(GUEST_CR3, sec);
            r = crate::vmx::guest_pt::identity_map_mmio_2m(
                sec,
                cr2,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            );
        } else {
            let _ = ops::vmwrite(GUEST_CR3, walk);
        }
        match r {
            Ok(crate::vmx::guest_pt::IdentityMapKind::Mmio2M)
            | Err(crate::vmx::guest_pt::IdentityMapError::AlreadyPresent) => {
                serial::write_str("boot: guest-UEFI #PF identity MMIO n=");
                write_dec(u64::from(k) + 1);
                serial::write_str(" cr2=0x");
                write_hex(cr2);
                serial::write_str(" pde=0x");
                write_hex(crate::vmx::guest_pt::identity_walk_pde(
                    walk,
                    cr2,
                    ram_hpa,
                    GUEST_UEFI_LOW_RAM_BYTES,
                ));
                serial::write_str(" pdpte2=0x");
                write_hex(crate::vmx::guest_pt::identity_walk_pdpte(
                    walk,
                    crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR,
                    ram_hpa,
                    GUEST_UEFI_LOW_RAM_BYTES,
                ));
                serial::write_str(" pdpte3=0x");
                write_hex(crate::vmx::guest_pt::identity_walk_pdpte(
                    walk,
                    crate::vmx::guest_pt::IDENTITY_MTRR_UC_3G,
                    ram_hpa,
                    GUEST_UEFI_LOW_RAM_BYTES,
                ));
                serial::write_byte(b'\n');
                let gpa32 = guest_uefi_pf_gpa32(cr2);
                if guest_uefi_mmio_needs_scratch(gpa32) {
                    let q = if (err & 2) != 0 {
                        2
                    } else if (err & (1 << 4)) != 0 {
                        4
                    } else {
                        1
                    };
                    if guest_uefi_ept_scratch_on_qual(q) && ept_map_2m_scratch(gpa32) {
                        serial::write_str("boot: guest-UEFI EPT scratch gpa=0x");
                        write_hex(gpa32 & !0x1F_FFFF);
                        serial::write_str(" pf-qual=0x");
                        write_hex(q);
                        serial::write_byte(b'\n');
                    } else if !guest_uefi_ept_scratch_on_qual(q) {
                        let _ = ept_map_2m_hole_ro_sink(gpa32);
                    }
                }
                guest_uefi_split_low_ram_1g(walk, ram_hpa);
                true
            }
            Ok(_) => true,
            Err(e) => {
                serial::write_str("boot: guest-UEFI #PF identity MMIO fail=");
                match e {
                    crate::vmx::guest_pt::IdentityMapError::OutOfRam => {
                        serial::write_str("oor");
                    }
                    crate::vmx::guest_pt::IdentityMapError::TableOutOfRam => {
                        serial::write_str("tbl");
                    }
                    crate::vmx::guest_pt::IdentityMapError::NeedAlloc => {
                        serial::write_str("alloc");
                    }
                    crate::vmx::guest_pt::IdentityMapError::AlreadyPresent => {
                        serial::write_str("present");
                    }
                }
                serial::write_byte(b'\n');
                false
            }
        }
    } else {
    let k = PF_FIXUPS.fetch_add(1, Ordering::AcqRel);
    if k >= GUEST_UEFI_PF_IDENTITY_CAP {
        serial::write_line("boot: guest-UEFI #PF identity cap");
        return false;
    }
    // Iron 471391f: CR3 already HV PML4, 1GiB PDPTE 0xc0000083 over RAM.
    // Split back to the SEC PD. Do not rebuild 4G (ASSERT lastmsr=0x23f).
    // Iron d757a0a: refill the PD (firmware 0xAF-filled it after 1GiB).
    if !guest_uefi_pf_should_load_sec_cr3(cr3) {
        let r = crate::vmx::guest_pt::identity_map_mmio_2m(
            walk,
            cr2,
            ram_hpa,
            GUEST_UEFI_LOW_RAM_BYTES,
        );
        if matches!(
            r,
            Ok(crate::vmx::guest_pt::IdentityMapKind::Mmio2M)
                | Err(crate::vmx::guest_pt::IdentityMapError::AlreadyPresent)
        ) {
            let _ = ops::vmwrite(GUEST_CR3, walk);
            serial::write_str("boot: guest-UEFI #PF identity SPLIT n=");
            write_dec(u64::from(k) + 1);
            serial::write_str(" cr2=0x");
            write_hex(cr2);
            serial::write_byte(b'\n');
            return true;
        }
    }
    let sec = guest_uefi_pf_sec_cr3();
    let used = sec;
    let r = guest_uefi_rebuild_sec_identity(ram_hpa, true);
    let rebuilt = matches!(r, Ok(crate::vmx::guest_pt::IdentityMapKind::Rebuild4G));
    if used != cr3 || rebuilt {
        let _ = ops::vmwrite(GUEST_CR3, used);
    }
    match r {
        Ok(kind) => {
            serial::write_str("boot: guest-UEFI #PF identity ");
            match kind {
                crate::vmx::guest_pt::IdentityMapKind::Large2M => {
                    serial::write_str("2M");
                }
                crate::vmx::guest_pt::IdentityMapKind::Page4K => {
                    serial::write_str("4K");
                }
                crate::vmx::guest_pt::IdentityMapKind::Cr3Sec => {
                    serial::write_str("CR3");
                }
                crate::vmx::guest_pt::IdentityMapKind::Rebuild4G => {
                    serial::write_str("4G");
                }
                crate::vmx::guest_pt::IdentityMapKind::Mmio2M => {
                    serial::write_str("MMIO");
                }
                crate::vmx::guest_pt::IdentityMapKind::Split4K => {
                    serial::write_str("SPLIT4K");
                }
            }
            serial::write_str(" n=");
            write_dec(u64::from(k) + 1);
            if used != cr3 || rebuilt {
                serial::write_str(" cr3=0x");
                write_hex(used);
            }
            let _ = crate::vmx::guest_pt::identity_sync_live_mtrr_uc_hole(
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
                used,
            );
            let _ = guest_uefi_split_gpa0_1m_mtrr(used, ram_hpa);
            serial::write_str(" pde0=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pde(
                used,
                0,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pde20=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pde(
                used,
                GUEST_UEFI_LOW_RAM_BYTES,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pde40=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pde(
                used,
                crate::vmx::guest_pt::IDENTITY_WB_64M,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pde6e=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pde(
                used,
                crate::vmx::guest_pt::IDENTITY_CPU_DXE_IMG,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pde4000=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pde(
                used,
                crate::vmx::guest_pt::IDENTITY_WB_1G,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pdpte1=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pdpte(
                used,
                crate::vmx::guest_pt::IDENTITY_WB_1G,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pde8000=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pde(
                used,
                crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_str(" pdpte3=0x");
            write_hex(crate::vmx::guest_pt::identity_walk_pdpte(
                used,
                crate::vmx::guest_pt::IDENTITY_MTRR_UC_3G,
                ram_hpa,
                GUEST_UEFI_LOW_RAM_BYTES,
            ));
            serial::write_byte(b'\n');
            guest_uefi_split_low_ram_1g(used, ram_hpa);
            true
        }
        Err(e) => {
            serial::write_str("boot: guest-UEFI #PF identity fail=");
            match e {
                crate::vmx::guest_pt::IdentityMapError::OutOfRam => {
                    serial::write_str("oor");
                }
                crate::vmx::guest_pt::IdentityMapError::TableOutOfRam => {
                    serial::write_str("tbl");
                }
                crate::vmx::guest_pt::IdentityMapError::NeedAlloc => {
                    serial::write_str("alloc");
                }
                crate::vmx::guest_pt::IdentityMapError::AlreadyPresent => {
                    serial::write_str("present");
                }
            }
            serial::write_byte(b'\n');
            false
        }
    }
    }
}

/// SDM 28.2.1 CR-access: MOV to/from CR0/CR3/CR4, keep VMXE+OSXSAVE host-owned.
#[cfg(target_os = "uefi")]
unsafe fn handle_cr(qual: u64) -> bool {
    let n = CR_ACCESSES.fetch_add(1, Ordering::AcqRel);
    if n < 4 {
        serial::write_str("boot: guest-UEFI CR access cr=");
        write_dec(qual & 0xf);
        serial::write_str(" type=");
        write_dec((qual >> 4) & 3);
        serial::write_byte(b'\n');
    }
    let cr = (qual & 0xf) as u8;
    let typ = ((qual >> 4) & 3) as u8;
    let gpr = ((qual >> 8) & 0xf) as u8;
    match (cr, typ) {
        (0, 0) => {
            let _ = ops::vmwrite(GUEST_CR0, cr_gpr(gpr));
            sync_guest_efer_lma();
        }
        (3, 0) => {
            let val = cr_gpr(gpr);
            let _ = ops::vmwrite(GUEST_CR3, val);
            // Iron 19b0c11: firmware MOV CR3 to a 1GiB PDPT[0] covering
            // [32MiB, 1GiB). Split back to RAM-only 2MiB before RIP leaves.
            let ram_hpa = RAM_HPA.load(Ordering::Acquire);
            if ram_hpa != 0 && val != 0 {
                guest_uefi_split_low_ram_1g(val, ram_hpa);
            }
            guest_uefi_paint_live_uc_hole_now();
        }
        (4, 0) => {
            let val = apply_guest_cr4_write(cr_gpr(gpr));
            let _ = ops::vmwrite(GUEST_CR4, val);
            let _ = ops::vmwrite(CR4_READ_SHADOW, guest_cr4_read_shadow(val));
        }
        (0, 1) => set_cr_gpr(gpr, ops::vmread(GUEST_CR0).unwrap_or(0)),
        (3, 1) => set_cr_gpr(gpr, ops::vmread(GUEST_CR3).unwrap_or(0)),
        (4, 1) => set_cr_gpr(gpr, ops::vmread(CR4_READ_SHADOW).unwrap_or(0)),
        _ => {}
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn cr_gpr(idx: u8) -> u64 {
    match idx {
        0 => SAVED_RAX,
        1 => SAVED_RCX,
        2 => SAVED_RDX,
        3 => SAVED_RBX,
        4 => ops::vmread(GUEST_RSP).unwrap_or(0),
        5 => SAVED_RBP,
        6 => SAVED_RSI,
        7 => SAVED_RDI,
        8 => SAVED_R8,
        9 => SAVED_R9,
        10 => SAVED_R10,
        11 => SAVED_R11,
        12 => SAVED_R12,
        13 => SAVED_R13,
        14 => SAVED_R14,
        15 => SAVED_R15,
        _ => 0,
    }
}

#[cfg(target_os = "uefi")]
unsafe fn set_cr_gpr(idx: u8, val: u64) {
    match idx {
        0 => SAVED_RAX = val,
        1 => SAVED_RCX = val,
        2 => SAVED_RDX = val,
        3 => SAVED_RBX = val,
        4 => {
            let _ = ops::vmwrite(GUEST_RSP, val);
        }
        5 => SAVED_RBP = val,
        6 => SAVED_RSI = val,
        7 => SAVED_RDI = val,
        8 => SAVED_R8 = val,
        9 => SAVED_R9 = val,
        10 => SAVED_R10 = val,
        11 => SAVED_R11 = val,
        12 => SAVED_R12 = val,
        13 => SAVED_R13 = val,
        14 => SAVED_R14 = val,
        15 => SAVED_R15 = val,
        _ => {}
    }
}

#[cfg(target_os = "uefi")]
unsafe fn skip_rel8_if(linear: u64, rip: u64, pred: fn(u8, u8) -> bool) -> bool {
    let mut buf = [0u8; 2];
    if copy_guest_identity_bytes(linear, &mut buf) < 2 {
        return false;
    }
    if !pred(buf[0], buf[1]) {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(2)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn skip_spin_short_jmp(linear: u64, rip: u64) -> bool {
    skip_rel8_if(linear, rip, spin_short_jmp_should_skip)
}

/// Frame return-address GPA: `[RBP+8]` in long mode, `[EBP+4]` in 32-bit.
pub fn assert_deadloop_return_gpa(rbp: u64, long_mode: bool) -> u64 {
    if long_mode {
        rbp.wrapping_add(8)
    } else {
        u64::from((rbp as u32).wrapping_add(4))
    }
}

#[cfg(target_os = "uefi")]
unsafe fn peek_low_u64(linear: u64) -> u64 {
    let mut slot = [0u8; 16];
    let n = read_low_ram_insn(linear, &mut slot);
    if n < 8 {
        return 0;
    }
    let mut le = [0u8; 8];
    le.copy_from_slice(&slot[..8]);
    u64::from_le_bytes(le)
}

/// Store an 8-byte PTE into 32 MiB identity or a mapped report-RAM window.
#[cfg(target_os = "uefi")]
unsafe fn poke_guest_u64(linear: u64, val: u64) -> bool {
    if linear < GUEST_UEFI_LOW_RAM_BYTES {
        let hpa = RAM_HPA.load(Ordering::Acquire);
        if hpa == 0 {
            return false;
        }
        // SAFETY: exclusive guest-UEFI 32 MiB slab; firmware is VMX-halted.
        // KANI-TARGET: live-CR3 PAT-UC poke low RAM (outside Proven Core).
        let ram =
            core::slice::from_raw_parts_mut(hpa as *mut u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
        return store_low_ram_u64(ram, linear, val);
    }
    if !guest_uefi_report_ram_should_map(linear) {
        return false;
    }
    let hpa = report_ram_hpa_lookup(linear);
    if hpa == 0 {
        return false;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already mapped for this GPA.
    // KANI-TARGET: live-CR3 PAT-UC poke report-RAM (outside Proven Core).
    let page =
        core::slice::from_raw_parts_mut(hpa as *mut u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
    store_report_ram_u64(page, linear, val)
}

/// Iron `c70768b`: high CR3 hole is WB while MTRR UC. Paint PAT-UC.
/// Iron `4ae87de`: paint landed `pde8000=0x800000ff` then ASSERT `pde0=0xe3`.
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_paint_live_uc_hole_now() {
    let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
    if cr3 == 0 {
        return;
    }
    if guest_uefi_mtrr_uc_hole_live() {
        let n = guest_uefi_pt_paint_live_uc_hole(
            |g| unsafe { peek_low_u64(g) },
            |g, v| unsafe { poke_guest_u64(g, v) },
            cr3,
        );
        if n > 0 {
            let _ = ops::vmwrite(GUEST_CR3, cr3);
            if LIVE_UC_PT_PAINTED.fetch_add(1, Ordering::AcqRel) == 0 {
                serial::write_str("boot: guest-UEFI MTRR UC live PT painted n=");
                write_dec(u64::from(n));
                serial::write_str(" cr3=0x");
                write_hex(cr3);
                serial::write_str(" pde8000=0x");
                write_hex(dump_walk_pde(
                    cr3,
                    crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR,
                ));
                serial::write_byte(b'\n');
            }
        }
    }
    guest_uefi_split_gpa0_live_now(cr3);
    guest_uefi_patch_cpu_flush_all_mapped();
}

/// Iron `4ae87de`: live CR3 2 MiB at GPA 0 spans 1 MiB fixed-MTRR.
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_split_gpa0_live_now(cr3: u64) {
    if !guest_uefi_gpa0_split_now(
        guest_uefi_phys_width(),
        guest_uefi_host_hypervisor_present(),
    ) {
        return;
    }
    let pt = guest_uefi_gpa0_split_pt_gpa();
    let n = guest_uefi_pt_split_gpa0(
        |g| unsafe { peek_low_u64(g) },
        |g, v| unsafe { poke_guest_u64(g, v) },
        cr3,
        pt,
    );
    if n == 0 {
        return;
    }
    let _ = ops::vmwrite(GUEST_CR3, cr3);
    if LIVE_GPA0_SPLIT.fetch_add(1, Ordering::AcqRel) == 0 {
        serial::write_str("boot: guest-UEFI GPA0 4K live CR3 n=");
        write_dec(u64::from(n));
        serial::write_str(" cr3=0x");
        write_hex(cr3);
        serial::write_str(" pde0=0x");
        write_hex(dump_walk_pde(cr3, 0));
        serial::write_str(" pte0=0x");
        write_hex(dump_walk_pte(cr3, 0));
        serial::write_str(" pte1m=0x");
        write_hex(dump_walk_pte(
            cr3,
            crate::vmx::guest_pt::IDENTITY_FIXED_MTRR_1M,
        ));
        serial::write_byte(b'\n');
    }
}

/// Iron `957e0ad`: CR3 `0x7fa01000` is report-RAM; 32 MiB `identity_walk_*`
/// printed `pml4e=0`. Peek through mapped HPA (same as insn dump).
#[cfg(target_os = "uefi")]
unsafe fn dump_walk_pml4e(cr3: u64, gva: u64) -> u64 {
    guest_uefi_pt_walk_pml4e(|g| unsafe { peek_low_u64(g) }, cr3, gva)
}

#[cfg(target_os = "uefi")]
unsafe fn dump_walk_pdpte(cr3: u64, gva: u64) -> u64 {
    guest_uefi_pt_walk_pdpte(|g| unsafe { peek_low_u64(g) }, cr3, gva)
}

#[cfg(target_os = "uefi")]
unsafe fn dump_walk_pde(cr3: u64, gva: u64) -> u64 {
    guest_uefi_pt_walk_pde(|g| unsafe { peek_low_u64(g) }, cr3, gva)
}

#[cfg(target_os = "uefi")]
unsafe fn dump_walk_pte(cr3: u64, gva: u64) -> u64 {
    guest_uefi_pt_walk_pte(|g| unsafe { peek_low_u64(g) }, cr3, gva)
}

#[cfg(target_os = "uefi")]
unsafe fn dump_assert_deadloop_once(linear: u64) {
    if ASSERT_DEADLOOP_DUMP.fetch_add(1, Ordering::AcqRel) != 0 {
        return;
    }
    let ar = ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0);
    let long = (ar & (1 << 13)) != 0;
    let rbp = SAVED_RBP;
    let rsp = ops::vmread(GUEST_RSP).unwrap_or(0);
    let ret_at = assert_deadloop_return_gpa(rbp, long);
    let mut slot = [0u8; 16];
    let n = read_low_ram_insn(ret_at, &mut slot);
    let ret = if long && n >= 8 {
        let mut le = [0u8; 8];
        le.copy_from_slice(&slot[..8]);
        u64::from_le_bytes(le)
    } else if !long && n >= 4 {
        u64::from(u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]))
    } else {
        0
    };
    serial::write_str("boot: guest-UEFI ASSERT CpuDeadLoop noskip rbp=0x");
    write_hex(rbp);
    serial::write_str(" rsp=0x");
    write_hex(rsp);
    serial::write_str(" ret=0x");
    write_hex(ret);
    let prev_rbp = peek_low_u64(rbp);
    let site = assert_deadloop_return_gpa(prev_rbp, long);
    serial::write_str(" site=0x");
    write_hex(site);
    serial::write_str(" insn=");
    dump_low_ram_insn(linear);
    serial::write_str(" caller=");
    dump_low_ram_insn(ret);
    serial::write_str(" siteinsn=");
    dump_low_ram_insn(site);
    serial::write_str(" rcx=0x");
    write_hex(SAVED_RCX);
    serial::write_str(" rdx=0x");
    write_hex(SAVED_RDX);
    serial::write_str(" r8=0x");
    write_hex(SAVED_R8);
    serial::write_str(" r9=0x");
    write_hex(SAVED_R9);
    serial::write_str(" file=");
    dump_low_ram_cstr(SAVED_RCX);
    serial::write_str(" desc=");
    dump_low_ram_cstr(SAVED_R8);
    serial::write_str(" site0=");
    dump_low_ram_cstr(peek_low_u64(site));
    serial::write_str(" site1=");
    dump_low_ram_cstr(peek_low_u64(site.wrapping_add(8)));
    let caller_rip = peek_low_u64(site);
    serial::write_str(" callerrip=0x");
    write_hex(caller_rip);
    serial::write_str(" callercode=");
    dump_low_ram_insn(caller_rip.wrapping_sub(16));
    serial::write_byte(b' ');
    dump_low_ram_insn(caller_rip);
    let home0 = peek_low_u64(prev_rbp.wrapping_add(0x10));
    let home1 = peek_low_u64(prev_rbp.wrapping_add(0x18));
    let home2 = peek_low_u64(prev_rbp.wrapping_add(0x20));
    serial::write_str(" home0=0x");
    write_hex(home0);
    serial::write_str(" home1=0x");
    write_hex(home1);
    serial::write_str(" home2=0x");
    write_hex(home2);
    serial::write_str(" file1=");
    dump_low_ram_cstr(home1);
    serial::write_str(" file2=");
    dump_low_ram_cstr(home0);
    serial::write_str(" desc2=");
    dump_low_ram_cstr(home2);
    serial::write_str(" arg0=");
    dump_low_ram_cstr(home0);
    serial::write_str(" arg2=");
    dump_low_ram_cstr(home2);
    serial::write_str(" arg0hex=");
    dump_low_ram_hex(home0, 24);
    serial::write_str(" arg1hex=");
    dump_low_ram_hex(home1, 24);
    serial::write_str(" arg2hex=");
    dump_low_ram_hex(home2, 24);
    serial::write_str(" rcxhex=");
    dump_low_ram_hex(SAVED_RCX, 24);
    serial::write_str(" r8hex=");
    dump_low_ram_hex(SAVED_R8, 24);
    let loc0 = peek_low_u64(prev_rbp.wrapping_sub(0x08));
    let loc1 = peek_low_u64(prev_rbp.wrapping_sub(0x10));
    let loc2 = peek_low_u64(prev_rbp.wrapping_sub(0x18));
    serial::write_str(" loc0=0x");
    write_hex(loc0);
    serial::write_str(" loc1=0x");
    write_hex(loc1);
    serial::write_str(" loc2=0x");
    write_hex(loc2);
    serial::write_str(" loc1s=");
    dump_low_ram_cstr(loc1);
    serial::write_str(" loc2s=");
    dump_low_ram_cstr(loc2);
    serial::write_str(" imgbase=0x");
    write_hex(peek_low_u64(loc2.wrapping_add(GUEST_UEFI_LDRI_IMAGEBASE_OFF)));
    serial::write_str(" imgsize=0x");
    write_hex(peek_low_u64(loc2.wrapping_add(GUEST_UEFI_LDRI_IMAGESIZE_OFF)));
    serial::write_str(" imgtype=0x");
    write_hex(peek_low_u64(loc2.wrapping_add(GUEST_UEFI_LDRI_TYPE_OFF)));
    serial::write_str(" imgentry=0x");
    write_hex(peek_low_u64(loc2.wrapping_add(GUEST_UEFI_LDRI_ENTRY_OFF)));
    serial::write_str(" lastmsr=0x");
    write_hex(u64::from(LAST_GUEST_MSR.load(Ordering::Acquire)));
    serial::write_str(" efer=0x");
    write_hex(LAST_EFER.load(Ordering::Acquire));
    serial::write_str(" cr0=0x");
    write_hex(ops::vmread(GUEST_CR0).unwrap_or(0));
    serial::write_str(" pg=");
    write_dec(guest_uefi_cr0_is_paging(ops::vmread(GUEST_CR0).unwrap_or(0)) as u64);
    serial::write_str(" csl=");
    write_dec(guest_uefi_cs_ar_is_long(ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0)) as u64);
    serial::write_str(" filep=");
    dump_low_ram_cstr(peek_low_u64(SAVED_RCX));
    serial::write_str(" home1p=");
    dump_low_ram_cstr(peek_low_u64(home1));
    serial::write_str(" rax=0x");
    write_hex(SAVED_RAX);
    serial::write_str(" cr4=0x");
    write_hex(ops::vmread(GUEST_CR4).unwrap_or(0));
    serial::write_str(" mtrrdef=0x");
    write_hex(guest_uefi_mtrr_read(0x2FF).unwrap_or(0));
    serial::write_str(" mtrr0=0x");
    write_hex(guest_uefi_mtrr_read(0x200).unwrap_or(0));
    serial::write_str(" mtrr1=0x");
    write_hex(guest_uefi_mtrr_read(0x201).unwrap_or(0));
    serial::write_str(" mtrrv=");
    write_dec(u64::from(guest_uefi_mtrr_valid_var_pairs()));
    {
        let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
        let walk = if cr3 == 0 {
            crate::vmx::guest_pt::IDENTITY_HV_PML4
        } else {
            cr3
        };
        serial::write_str(" cr3=0x");
        write_hex(cr3);
        serial::write_str(" pml4e=0x");
        write_hex(dump_walk_pml4e(walk, 0));
        serial::write_str(" pml4e1=0x");
        write_hex(dump_walk_pml4e(
            walk,
            crate::vmx::guest_pt::IDENTITY_PML4E1_GVA,
        ));
        serial::write_str(" pdpte0=0x");
        write_hex(dump_walk_pdpte(walk, 0));
        serial::write_str(" pde0=0x");
        write_hex(dump_walk_pde(walk, 0));
        serial::write_str(" pte0=0x");
        write_hex(dump_walk_pte(walk, 0));
        serial::write_str(" pte1m=0x");
        write_hex(dump_walk_pte(
            walk,
            crate::vmx::guest_pt::IDENTITY_FIXED_MTRR_1M,
        ));
        serial::write_str(" pte_a0000=0x");
        write_hex(dump_walk_pte(
            walk,
            crate::vmx::guest_pt::IDENTITY_VGA_A0000,
        ));
        serial::write_str(" pte_c0000=0x");
        write_hex(dump_walk_pte(
            walk,
            crate::vmx::guest_pt::IDENTITY_VGA_C0000,
        ));
        serial::write_str(" pde20=0x");
        write_hex(dump_walk_pde(walk, GUEST_UEFI_LOW_RAM_BYTES));
        serial::write_str(" pde40=0x");
        write_hex(dump_walk_pde(walk, crate::vmx::guest_pt::IDENTITY_WB_64M));
        serial::write_str(" pde6e=0x");
        write_hex(dump_walk_pde(
            walk,
            crate::vmx::guest_pt::IDENTITY_CPU_DXE_IMG,
        ));
        serial::write_str(" pde4000=0x");
        write_hex(dump_walk_pde(walk, crate::vmx::guest_pt::IDENTITY_WB_1G));
        serial::write_str(" pdpte1=0x");
        write_hex(dump_walk_pdpte(walk, crate::vmx::guest_pt::IDENTITY_WB_1G));
        serial::write_str(" pdpte2=0x");
        write_hex(dump_walk_pdpte(
            walk,
            crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR,
        ));
        serial::write_str(" pde8000=0x");
        write_hex(dump_walk_pde(
            walk,
            crate::vmx::guest_pt::IDENTITY_MTRR_UC_FLOOR,
        ));
        serial::write_str(" pdefee=0x");
        write_hex(dump_walk_pde(walk, crate::vmx::guest_pt::IDENTITY_XAPIC_GPA));
        serial::write_str(" pdeffc=0x");
        write_hex(dump_walk_pde(
            walk,
            crate::vmx::guest_pt::IDENTITY_FLASH_FLOOR,
        ));
        serial::write_str(" pdpte3=0x");
        write_hex(dump_walk_pdpte(
            walk,
            crate::vmx::guest_pt::IDENTITY_MTRR_UC_3G,
        ));
        serial::write_str(" pat=0x");
        write_hex(ops::vmread(GUEST_IA32_PAT).unwrap_or(0));
        serial::write_str(" entry=0x");
        write_hex(ops::vmread(VM_ENTRY_CONTROLS).unwrap_or(0));
    }
    serial::write_str(" maxpa=");
    write_dec(u64::from(guest_uefi_phys_width()));
    let mut sig = [0u8; 16];
    let nsig = read_low_ram_insn(SAVED_R8, &mut sig);
    serial::write_str(" pcdsig=");
    write_dec(guest_uefi_is_pcd_database_sig(&sig[..nsig]) as u64);
    serial::write_str(" s0=");
    dump_low_ram_ascii(peek_low_u64(SAVED_RCX), 32);
    serial::write_str(" s1=");
    dump_low_ram_ascii(peek_low_u64(SAVED_RCX.wrapping_add(8)), 32);
    serial::write_str(" s2=");
    dump_low_ram_ascii(peek_low_u64(home1), 32);
    serial::write_str(" filehex=");
    dump_low_ram_hex(peek_low_u64(SAVED_RCX), 32);
    serial::write_str(" e820=");
    write_dec(crate::devices::guest_platform::fwcfg_e820_served() as u64);
    serial::write_str(" fwdir=");
    write_dec(crate::devices::guest_platform::fwcfg_file_dir_served() as u64);
    serial::write_str(" pei_did=");
    write_dec(crate::devices::guest_virtio_blk::pei_host_bridge_did() as u64);
    serial::write_byte(b'\n');
}

#[cfg(target_os = "uefi")]
unsafe fn skip_preempt_deadloop(linear: u64, rip: u64) -> bool {
    let mut buf = [0u8; 8];
    let n = copy_guest_identity_bytes(linear, &mut buf);
    if n == 0 {
        return false;
    }
    if preempt_deadloop_is_assert_epilogue(&buf[..n]) {
        dump_assert_deadloop_once(linear);
        return false;
    }
    let len = u64::from(preempt_deadloop_skip_len(&buf[..n]));
    if len == 0 {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(len)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn dump_low_ram_insn(linear: u64) {
    let mut buf = [0u8; 16];
    let n = copy_guest_identity_bytes(linear, &mut buf);
    for i in 0..n {
        write_hex2(buf[i]);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn dump_low_ram_cstr(linear: u64) {
    if linear == 0 {
        serial::write_byte(b'-');
        return;
    }
    let mut buf = [0u8; 48];
    let n = copy_guest_identity_bytes(linear, &mut buf);
    if n == 0 || buf[0] < 0x20 || buf[0] > 0x7e {
        serial::write_byte(b'-');
        return;
    }
    for &b in buf[..n].iter() {
        if b == 0 {
            break;
        }
        if b >= 0x20 && b <= 0x7e {
            serial::write_byte(b);
        } else {
            break;
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn dump_low_ram_ascii(linear: u64, nmax: usize) {
    if linear == 0 {
        serial::write_byte(b'-');
        return;
    }
    let mut buf = [0u8; 32];
    let n = nmax.min(32);
    let got = copy_guest_identity_bytes(linear, &mut buf[..n]);
    if got == 0 {
        serial::write_byte(b'-');
        return;
    }
    for &b in buf[..got].iter() {
        if b >= 0x20 && b <= 0x7e {
            serial::write_byte(b);
        } else {
            serial::write_byte(b'.');
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn dump_low_ram_hex(linear: u64, nmax: usize) {
    if linear == 0 {
        serial::write_byte(b'-');
        return;
    }
    let mut buf = [0u8; 24];
    let n = nmax.min(24);
    let got = copy_guest_identity_bytes(linear, &mut buf[..n]);
    if got == 0 {
        serial::write_byte(b'-');
        return;
    }
    for &b in buf[..got].iter() {
        write_hex2(b);
    }
}

#[cfg(target_os = "uefi")]
fn write_hex2(b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    serial::write_byte(HEX[(b >> 4) as usize]);
    serial::write_byte(HEX[(b & 0xf) as usize]);
}

#[cfg(target_os = "uefi")]
unsafe fn skip_insn() -> bool {
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(0);
    if len == 0 || len > 15 {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(len)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn store_guest_io(linear: u64, size: u8, val: u64) -> bool {
    if linear < GUEST_UEFI_LOW_RAM_BYTES {
        let hpa = RAM_HPA.load(Ordering::Acquire);
        if hpa == 0 {
            return false;
        }
        // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is halted in VMX.
        // KANI-TARGET: string INS store to guest RAM (outside Proven Core).
        let ram = core::slice::from_raw_parts_mut(hpa as *mut u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
        return store_low_ram_at(ram, linear, val, size) == usize::from(size);
    }
    if !guest_uefi_report_ram_should_map(linear) {
        return false;
    }
    let hpa = report_ram_hpa_lookup(linear);
    if hpa == 0 {
        return false;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already mapped for this GPA.
    // KANI-TARGET: string INS store to report-RAM (outside Proven Core).
    let page =
        core::slice::from_raw_parts_mut(hpa as *mut u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
    store_report_ram_at(page, linear, val, size) == usize::from(size)
}

#[cfg(target_os = "uefi")]
unsafe fn load_guest_io(linear: u64, size: u8) -> Option<u64> {
    if linear < GUEST_UEFI_LOW_RAM_BYTES {
        let hpa = RAM_HPA.load(Ordering::Acquire);
        if hpa == 0 {
            return None;
        }
        // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is halted in VMX.
        // KANI-TARGET: string OUTS load from guest RAM (outside Proven Core).
        let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
        return load_low_ram_at(ram, linear, size);
    }
    if !guest_uefi_report_ram_should_map(linear) {
        return None;
    }
    let hpa = report_ram_hpa_lookup(linear);
    if hpa == 0 {
        return None;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already mapped for this GPA.
    // KANI-TARGET: string OUTS load from report-RAM (outside Proven Core).
    let page = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
    load_report_ram_at(page, linear, size)
}

#[cfg(target_os = "uefi")]
unsafe fn emulate_io_port(port: u16, is_in: bool, size: u64) {
    if is_pci_config_port(port) || crate::devices::ide_cdrom::is_pci_data_port(port) {
        PCI_CONFIG_SEEN.store(true, Ordering::Release);
        maybe_print_past_sec(false);
        handle_pci(port, is_in, size as u8);
        maybe_print_cdrom();
        maybe_print_virtio();
        maybe_print_both();
        maybe_print_atapi();
        return;
    }
    if crate::devices::ide_cdrom::is_ata_primary_port(port) {
        let before = crate::devices::ide_cdrom::ata_commands();
        SAVED_RAX = crate::devices::ide_cdrom::ata_io(port, is_in, size as u8, SAVED_RAX);
        let n = crate::devices::ide_cdrom::ata_commands();
        if n > before && n <= 8 {
            serial::write_str("boot: guest-UEFI ata cmd=0x");
            write_hex_u32(u32::from(crate::devices::ide_cdrom::last_ata_cmd()));
            serial::write_str(" n=");
            write_dec(n as u64);
            serial::write_byte(b'\n');
        }
        maybe_print_cdrom();
        maybe_print_atapi();
        return;
    }
    if crate::devices::ide_cdrom::is_bmide_port(port) {
        SAVED_RAX = crate::devices::ide_cdrom::bmide_io(port, is_in, size as u8, SAVED_RAX);
        return;
    }
    if crate::devices::guest_platform::is_platform_io_port(port)
        || crate::devices::guest_platform::is_acpi_pm_timer_io(port, size as u8)
        || crate::devices::guest_platform::is_piix_pm_io(port)
    {
        SAVED_RAX = crate::devices::guest_platform::io(port, is_in, size as u8, SAVED_RAX);
        if crate::devices::guest_platform::is_kbc_port(port) && !is_in {
            let n = KBC_WR_N.fetch_add(1, Ordering::AcqRel);
            if n < 8 {
                serial::write_str("boot: guest-UEFI kbc wr port=0x");
                write_hex_u32(u32::from(port));
                serial::write_str(" val=0x");
                write_hex2(SAVED_RAX as u8);
                serial::write_byte(b'\n');
            }
        }
        maybe_print_dxe();
        return;
    }
    if is_com_uart_port(port) {
        handle_uart(port, is_in, size);
        return;
    }
    if is_debugcon_port(port) {
        handle_debugcon(is_in, size);
        return;
    }
    let k = IO_UNHANDLED_N.fetch_add(1, Ordering::AcqRel);
    if k < 12 {
        serial::write_str("boot: guest-UEFI io unhandled port=0x");
        write_hex_u32(u32::from(port));
        serial::write_str(" in=");
        write_dec(is_in as u64);
        serial::write_str(" size=");
        write_dec(size);
        serial::write_byte(b'\n');
    }
    if is_in {
        let mask = if size == 1 {
            0xffu64
        } else if size == 2 {
            0xffff
        } else {
            0xffff_ffff
        };
        SAVED_RAX = (SAVED_RAX & !mask) | mask;
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_io_string(qual: u64, port: u16, is_in: bool, size: u8) -> bool {
    let long = guest_uefi_cs_ar_is_long(ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0));
    let count = guest_uefi_io_string_count(qual, guest_uefi_io_addr_reg(SAVED_RCX, long));
    let rflags = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
    let df = (rflags & (1 << 10)) != 0;
    let mut addr = guest_uefi_io_addr_reg(if is_in { SAVED_RDI } else { SAVED_RSI }, long);
    let nlog = IO_STRING_N.fetch_add(1, Ordering::AcqRel);
    if nlog < 4 {
        serial::write_str("boot: guest-UEFI io string port=0x");
        write_hex_u32(u32::from(port));
        serial::write_str(" n=");
        write_dec(count);
        serial::write_str(" (rep insw)");
        serial::write_byte(b'\n');
    }
    for _ in 0..count {
        if !is_in {
            if let Some(v) = load_guest_io(addr, size) {
                let mask = if size == 1 {
                    0xffu64
                } else if size == 2 {
                    0xffff
                } else {
                    0xffff_ffff
                };
                SAVED_RAX = (SAVED_RAX & !mask) | (v & mask);
            }
        }
        emulate_io_port(port, is_in, u64::from(size));
        if is_in {
            let _ = store_guest_io(addr, size, SAVED_RAX);
        }
        addr = guest_uefi_io_string_advance(addr, size, df);
    }
    if is_in {
        SAVED_RDI = if long { addr } else { (SAVED_RDI & !0xFFFF_FFFF) | (addr & 0xFFFF_FFFF) };
    } else {
        SAVED_RSI = if long { addr } else { (SAVED_RSI & !0xFFFF_FFFF) | (addr & 0xFFFF_FFFF) };
    }
    if guest_uefi_io_qual_is_rep(qual) {
        let left = guest_uefi_io_addr_reg(SAVED_RCX, long).saturating_sub(count);
        SAVED_RCX = if long {
            left
        } else {
            (SAVED_RCX & !0xFFFF_FFFF) | (left & 0xFFFF_FFFF)
        };
        if left != 0 {
            return true;
        }
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn handle_io(qual: u64) -> bool {
    let size = (qual & 7) + 1;
    let is_in = (qual & (1 << 3)) != 0;
    let port = io_port_from_qual(qual);
    LAST_IO_PORT.store(u32::from(port), Ordering::Release);
    if guest_uefi_io_qual_is_string(qual) && guest_uefi_io_string_fills_ram(port) {
        return handle_io_string(qual, port, is_in, size as u8);
    }
    emulate_io_port(port, is_in, size);
    maybe_print_fwcfg_pei();
    skip_insn()
}

#[cfg(target_os = "uefi")]
fn maybe_print_fwcfg_pei() {
    static DIR: AtomicBool = AtomicBool::new(false);
    static E820: AtomicBool = AtomicBool::new(false);
    if crate::devices::guest_platform::fwcfg_file_dir_served() && !DIR.swap(true, Ordering::AcqRel)
    {
        serial::write_line("boot: guest-UEFI fw_cfg file dir (PEI FindFile)");
    }
    if crate::devices::guest_platform::fwcfg_e820_served() && !E820.swap(true, Ordering::AcqRel) {
        serial::write_line("boot: guest-UEFI fw_cfg etc/e820 selected (PEI ScanE820)");
    }
}

/// After PEIMs LZMA-decompress into low RAM, patch `cmp bx, 0x1237` there too.
/// Kept for the Stage 43 source gate. Not called: PEI HostBridgeDevId is
/// i440FX `0x1237` (stock QEMU MemMap VGA HOB). Remap would fork the merged
/// `[0, LowMemory)` SystemMemory range. Not two-phase DID in the flash copy.
#[cfg(target_os = "uefi")]
#[allow(dead_code)]
unsafe fn maybe_remap_guest_ram() {
    if RAM_REMAP_N.load(Ordering::Acquire) > 0 {
        return;
    }
    let tries = RAM_REMAP_TRIES.fetch_add(1, Ordering::AcqRel);
    if tries >= 8 {
        return;
    }
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 {
        return;
    }
    // SAFETY: exclusive guest-UEFI 32 MiB RAM slab; firmware is halted in VMX.
    // KANI-TARGET: remap decompressed OVMF in guest RAM (outside Proven Core).
    let n = crate::boot::ovmf_esp::remap_i440fx_did_imm(core::slice::from_raw_parts_mut(
        hpa as *mut u8,
        GUEST_UEFI_LOW_RAM_BYTES as usize,
    ));
    if n > 0 {
        RAM_REMAP_N.store(n, Ordering::Release);
        serial::write_str("boot: guest-UEFI ram remap i440FX DID->virtio n=");
        write_dec(n as u64);
        serial::write_byte(b'\n');
    } else if tries == 0 {
        serial::write_line("boot: guest-UEFI ram remap i440FX DID->virtio n=0");
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_pci(port: u16, is_in: bool, size: u8) {
    if port == 0xCF8 {
        if is_in {
            let mask = if size == 1 {
                0xffu64
            } else if size == 2 {
                0xffff
            } else {
                0xffff_ffff
            };
            let v = u64::from(crate::devices::ide_cdrom::pci_read_addr());
            SAVED_RAX = (SAVED_RAX & !mask) | (v & mask);
        } else {
            let addr = SAVED_RAX as u32;
            crate::devices::ide_cdrom::pci_write_addr(addr);
            crate::devices::guest_platform::pci_write_addr(addr);
            crate::devices::guest_virtio_blk::pci_write_addr(addr);
            LAST_CF8.store(addr, Ordering::Release);
            note_pci_cf8(addr);
        }
        return;
    }
    if crate::devices::ide_cdrom::is_pci_data_port(port) {
        if is_in {
            let mask = if size == 1 {
                0xffu64
            } else if size == 2 {
                0xffff
            } else {
                0xffff_ffff
            };
            let v = if let Some(p) = crate::devices::guest_platform::pci_read_data(port, size) {
                u64::from(p)
            } else if crate::devices::guest_virtio_blk::pci_addr_selects_virtio(
                crate::devices::guest_virtio_blk::pci_read_addr(),
            ) {
                u64::from(crate::devices::guest_virtio_blk::pci_read_data(port, size))
            } else {
                u64::from(crate::devices::ide_cdrom::pci_read_data(port, size))
            };
            SAVED_RAX = (SAVED_RAX & !mask) | (v & mask);
            let cfg = crate::devices::ide_cdrom::pci_read_addr()
                | u32::from(port.wrapping_sub(0xCFC) & 3);
            trace_pci_cfg(cfg, v as u32, size, false);
        } else {
            crate::devices::guest_platform::pci_write_data(port, size, SAVED_RAX as u32);
            crate::devices::ide_cdrom::pci_write_data(port, size, SAVED_RAX as u32);
            crate::devices::guest_virtio_blk::pci_write_data(port, size, SAVED_RAX as u32);
            let cfg = crate::devices::ide_cdrom::pci_read_addr()
                | u32::from(port.wrapping_sub(0xCFC) & 3);
            trace_pci_cfg(cfg, SAVED_RAX as u32, size, true);
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_ept(gpa: u64, qual: u64) -> bool {
    if crate::devices::guest_platform::is_xapic_2m_gpa(gpa) {
        serial::write_str("boot: guest-UEFI EPT xAPIC gpa=0x");
        write_hex(gpa);
        serial::write_byte(b'\n');
        return false;
    }
    // Iron b25d75b / 577c9eb: firmware PT stores in the MTRR/PCI hole must
    // land on a dedicated scratch HPA. Iron da2c9c4: bit 2 fetch + bit 8
    // walk (qual=0x184) is a PTE *read*; scratch only data-write (bit 1).
    let hole = guest_uefi_mmio_needs_scratch(gpa);
    // Iron 19b0c11: 1GiB PDPTE over 0-1GiB made RIP 0x27e22d5 present.
    // Split PDPT[0] back to RAM-only 2MiB. Do not load SEC CR3 here
    // (CR3=0 before 4G n=1 is a #PF path, not every EPT).
    let ram_hpa = RAM_HPA.load(Ordering::Acquire);
    let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0);
    if ram_hpa != 0 && cr3 != 0 {
        guest_uefi_split_low_ram_1g(cr3, ram_hpa);
    }
    if hole
        && guest_uefi_ept_qual_is_fetch(qual)
        && !guest_uefi_ept_qual_is_walk(qual)
    {
        serial::write_str("boot: guest-UEFI EPT fetch hole — not X gpa=0x");
        write_hex(guest_uefi_pf_gpa32(gpa) & !0x1F_FFFF);
        serial::write_str(" qual=0x");
        write_hex(qual);
        serial::write_byte(b'\n');
        return false;
    }
    if hole && guest_uefi_ept_scratch_on_qual(qual) {
        if ept_map_2m_scratch(gpa) {
            serial::write_str("boot: guest-UEFI EPT scratch gpa=0x");
            write_hex(guest_uefi_pf_gpa32(gpa) & !0x1F_FFFF);
            serial::write_str(" qual=0x");
            write_hex(qual);
            serial::write_byte(b'\n');
            maybe_print_dxe();
            return true;
        }
        serial::write_str("boot: guest-UEFI EPT scratch cap gpa=0x");
        write_hex(guest_uefi_pf_gpa32(gpa) & !0x1F_FFFF);
        serial::write_str(" qual=0x");
        write_hex(qual);
        serial::write_byte(b'\n');
        // Do not RW-sink a hole store onto the shared HPET zero page
        // (iron 577c9eb). Stop rather than discard PT writes.
        return false;
    }
    if hole
        && !guest_uefi_ept_scratch_on_qual(qual)
        && guest_uefi_ept_hole_ro_on_qual(qual)
        && ept_map_2m_hole_ro_sink(gpa)
    {
        let n = SINK_MAPS.fetch_add(1, Ordering::AcqRel);
        if n < 4 {
            serial::write_str("boot: guest-UEFI EPT hole ro gpa=0x");
            write_hex(guest_uefi_pf_gpa32(gpa) & !0x1F_FFFF);
            serial::write_str(" qual=0x");
            write_hex(qual);
            serial::write_byte(b'\n');
        }
        maybe_print_dxe();
        return true;
    }
    // Iron fad19b2: CMOS 2GiB LowMemory heap at gpa=0x7bddd000. Map a
    // dedicated 2MiB WB HPA (not UC scratch, not sink zeros). Cap stops.
    if guest_uefi_report_ram_should_map(gpa) {
        if ept_map_2m_report_ram(gpa) {
            let n = REPORT_RAM_MAPS.fetch_add(1, Ordering::AcqRel);
            if n < 8 {
                serial::write_str("boot: guest-UEFI EPT report-RAM gpa=0x");
                write_hex(gpa);
                serial::write_str(" hpa=0x");
                write_hex(report_ram_hpa_for(guest_uefi_report_ram_gpa_2m(gpa)));
                serial::write_byte(b'\n');
            }
            maybe_print_dxe();
            guest_uefi_paint_live_uc_hole_now();
            guest_uefi_patch_cpu_flush_all_mapped();
            return true;
        }
        serial::write_str("boot: guest-UEFI EPT report-RAM cap gpa=0x");
        write_hex(gpa);
        serial::write_byte(b'\n');
        return false;
    }
    // Iron cc7d78a: PCI-hole store (gpa=0xC01DF1B7) after 4G identity. Sink-resume
    // unbacked MMIO; do not identity-map as RAM (ADR-004).
    if crate::devices::guest_platform::is_platform_sink_gpa(gpa) && ept_map_2m_sink(gpa) {
        SINK_MAPS.fetch_add(1, Ordering::AcqRel);
        if SINK_MAPS.load(Ordering::Acquire) <= 8 {
            serial::write_str("boot: guest-UEFI EPT sink gpa=0x");
            write_hex(gpa);
            serial::write_byte(b'\n');
        }
        maybe_print_dxe();
        return true;
    }
    serial::write_str("boot: guest-UEFI EPT violation gpa=0x");
    write_hex(gpa);
    serial::write_byte(b'\n');
    false
}

/// Map a 4 KiB xAPIC page at `0xFEE00000` (rest of the 2 MiB → sink zeros).
///
/// INVARIANTS:
/// - Does not clobber an existing PD leaf (2 MiB sink must not already be there)
/// - Version register in `lapic_hpa` is [`crate::devices::lapic_virt::XAPIC_VERSION`]
#[cfg(target_os = "uefi")]
unsafe fn ept_install_xapic_4k(pt_hpa: u64, lapic_hpa: u64) -> bool {
    let pml4 = EPT_PML4.load(Ordering::Acquire);
    let sink = SINK_HPA.load(Ordering::Acquire);
    let gpa = crate::devices::lapic_virt::APIC_GPA;
    if pml4 == 0 || (pt_hpa & 0xfff) != 0 || (lapic_hpa & 0xfff) != 0 {
        return false;
    }
    let pml4_i = ((gpa >> 39) & 0x1ff) as usize;
    let e0 = core::ptr::read_volatile((pml4 as *const u64).add(pml4_i));
    if e0 & 0b111 == 0 {
        return false;
    }
    let pdpt = e0 & !0xfff;
    let pdpt_i = ((gpa >> 30) & 0x1ff) as usize;
    let e1 = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i));
    if e1 & 0b111 == 0 || (e1 & (1 << 7)) != 0 {
        return false;
    }
    let pd = e1 & !0xfff;
    let pd_i = ((gpa >> 21) & 0x1ff) as usize;
    let e2 = core::ptr::read_volatile((pd as *const u64).add(pd_i));
    if e2 & 0b111 != 0 {
        return false;
    }
    core::ptr::write_bytes(pt_hpa as *mut u8, 0, 4096);
    let pt = pt_hpa as *mut u64;
    let zero_hpa = if sink != 0 { sink } else { lapic_hpa };
    for i in 0..512u64 {
        let hpa = if i == 0 { lapic_hpa } else { zero_hpa };
        core::ptr::write_volatile(pt.add(i as usize), crate::memory::ept_hw::ept_leaf_4k(hpa, 0));
    }
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), crate::memory::ept_hw::ept_link(pt_hpa));
    crate::memory::ept_hw::invept_global();
    true
}

/// Link an empty PD under `pdpt_i` so 2 MiB sink-resume can fill leaves.
///
/// 4G identity CR3 (iron `cc7d78a`) makes 1–3 GiB present in guest PT;
/// firmware-alias EPT only had PDPT[0] (32 MiB RAM) and PDPT[3] (flash).
///
/// SAFETY: `pd_hpa` is an exclusive 4 KiB frame from the guest-UEFI allocator.
/// `EPT_PML4` is the private alias EPT (not E4 SHELL).
/// KANI-TARGET: guest-UEFI EPT PDPT link for PCI-hole sink (outside Proven Core).
#[cfg(target_os = "uefi")]
unsafe fn ept_link_empty_pd(pdpt_i: usize, pd_hpa: u64) -> bool {
    let pml4 = EPT_PML4.load(Ordering::Acquire);
    if pml4 == 0 || pdpt_i > 511 || (pd_hpa & 0xfff) != 0 {
        return false;
    }
    let e0 = core::ptr::read_volatile((pml4 as *const u64).add(0));
    if e0 & 0b111 == 0 {
        return false;
    }
    let pdpt = e0 & !0xfff;
    let e1 = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i));
    if e1 & 0b111 != 0 {
        return true;
    }
    core::ptr::write_bytes(pd_hpa as *mut u8, 0, 4096);
    core::ptr::write_volatile((pdpt as *mut u64).add(pdpt_i), crate::memory::ept_hw::ept_link(pd_hpa));
    crate::memory::ept_hw::invept_global();
    true
}

/// Map a 2 MiB EPT leaf for `gpa` to `hpa` (outside Proven Core).
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_hpa(gpa: u64, hpa: u64, replace: bool) -> bool {
    ept_map_2m_hpa_rwe(gpa, hpa, replace, true)
}

/// `write=false` is R only (no W, no X). Iron `19b0c11` R+X executed zeros.
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_hpa_rwe(gpa: u64, hpa: u64, replace: bool, write: bool) -> bool {
    ept_map_2m_hpa_mt(gpa, hpa, replace, write, 0)
}

/// `mt` is the EPT leaf memory type (0 = UC scratch/sink; 6 = WB RAM).
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_hpa_mt(gpa: u64, hpa: u64, replace: bool, write: bool, mt: u64) -> bool {
    let pml4 = EPT_PML4.load(Ordering::Acquire);
    if pml4 == 0 || hpa == 0 || (hpa & ((1 << 21) - 1)) != 0 {
        return false;
    }
    let pml4_i = ((gpa >> 39) & 0x1ff) as usize;
    let e0 = core::ptr::read_volatile((pml4 as *const u64).add(pml4_i));
    if e0 & 0b111 == 0 {
        return false;
    }
    let pdpt = e0 & !0xfff;
    let pdpt_i = ((gpa >> 30) & 0x1ff) as usize;
    let e1 = core::ptr::read_volatile((pdpt as *const u64).add(pdpt_i));
    if e1 & 0b111 == 0 || (e1 & (1 << 7)) != 0 {
        return false;
    }
    let pd = e1 & !0xfff;
    let pd_i = ((gpa >> 21) & 0x1ff) as usize;
    let e2 = core::ptr::read_volatile((pd as *const u64).add(pd_i));
    let mut want = crate::memory::ept_hw::ept_leaf_large(hpa, mt);
    if !write {
        // SDM 28.2.2: bit 0 read, bit 1 write, bit 2 execute.
        want &= !0b110;
        if guest_uefi_ept_hole_ro_allows_execute() {
            want |= 0b100;
        }
    }
    if e2 & 0b111 != 0 {
        if !replace || e2 == want {
            return true;
        }
    }
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), want);
    crate::memory::ept_hw::invept_global();
    true
}

/// Hole data-read: dedicated zero 2MiB as R (no X). Never SINK_HPA (live HPET).
/// Iron `f93caee`: RO-sink onto HPET then leftover CR2 `0x9896808086`
/// RIP `0x300001` poison fill. A store upgrades via scratch.
/// Iron `19b0c11`: R+X let fetch at `0x27e22d5` execute zeros.
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_hole_ro_sink(gpa: u64) -> bool {
    let zero = HOLE_ZERO_HPA.load(Ordering::Acquire);
    let sink = SINK_HPA.load(Ordering::Acquire);
    if !guest_uefi_hole_ro_uses_dedicated_zero(zero, sink) {
        return false;
    }
    let g = guest_uefi_pf_gpa32(gpa);
    ept_map_2m_hpa_rwe(g, zero, false, false)
}

/// Map a 2 MiB sink leaf for `gpa` in the private guest-UEFI EPT (outside Proven Core).
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_sink(gpa: u64) -> bool {
    let sink = SINK_HPA.load(Ordering::Acquire);
    ept_map_2m_hpa(gpa, sink, false)
}

/// Iron `b25d75b` / `577c9eb`: persist firmware stores in the MTRR/PCI hole
/// (not the HPET sink). One dedicated 2 MiB HPA per 2 MiB GPA.
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_scratch(gpa: u64) -> bool {
    if !guest_uefi_mmio_needs_scratch(gpa) {
        return false;
    }
    let g = guest_uefi_pf_gpa32(gpa);
    let hpa = mmio_scratch_hpa_for(g);
    if hpa == 0 {
        return false;
    }
    ept_map_2m_hpa(g, hpa, true)
}

/// Iron `d6b012a`: CpuFlush is LZMA in `OVMF.fd`. Patch decompressed report-RAM.
/// Map-time HPA is still empty (firmware memcpy fills after EPT resume). Scan
/// every mapped slot on later exits (tick / paint) before CpuDxe ASSERT.
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_patch_cpu_flush_mapped(hpa: u64) {
    if hpa == 0 || CPU_FLUSH_PATCHED.load(Ordering::Acquire) != 0 {
        return;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already EPT-mapped WB.
    // KANI-TARGET: live CpuFlush jnz nop (outside Proven Core).
    let buf =
        core::slice::from_raw_parts_mut(hpa as *mut u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
    let n = guest_uefi_patch_cpu_flush_unsupported(buf);
    if n == 0 {
        return;
    }
    let total = CPU_FLUSH_PATCHED.fetch_add(n, Ordering::AcqRel);
    if total == 0 {
        serial::write_str("boot: guest-UEFI CpuFlush FlushType-any WBINVD n=");
        write_dec(u64::from(n));
        serial::write_byte(b'\n');
    }
}

#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_patch_cpu_flush_all_mapped() {
    if CPU_FLUSH_PATCHED.load(Ordering::Acquire) != 0 {
        return;
    }
    for i in 0..GUEST_UEFI_REPORT_RAM_SLOTS {
        let hpa = REPORT_RAM_HPA[i].load(Ordering::Acquire);
        if hpa == 0 || REPORT_RAM_GPA[i].load(Ordering::Acquire) == u64::MAX {
            continue;
        }
        guest_uefi_patch_cpu_flush_mapped(hpa);
        if CPU_FLUSH_PATCHED.load(Ordering::Acquire) != 0 {
            return;
        }
    }
}

/// Iron `fad19b2`: persist firmware stores in reported LowMemory that launch
/// did not identity-map. One dedicated 2 MiB WB HPA per 2 MiB GPA.
///
/// SAFETY: pool HPA is an exclusive 2 MiB frame from `launch_uefi`.
/// KANI-TARGET: guest-UEFI lazy report-RAM EPT (outside Proven Core).
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_report_ram(gpa: u64) -> bool {
    if !guest_uefi_report_ram_should_map(gpa) {
        return false;
    }
    let g = guest_uefi_report_ram_gpa_2m(gpa);
    let hpa = report_ram_hpa_for(g);
    if hpa == 0 {
        return false;
    }
    ept_map_2m_hpa_mt(g, hpa, false, true, GUEST_UEFI_EPT_MT_WB)
}

fn report_ram_hpa_lookup(gpa: u64) -> u64 {
    let key = guest_uefi_report_ram_gpa_2m(gpa);
    for i in 0..GUEST_UEFI_REPORT_RAM_SLOTS {
        if REPORT_RAM_GPA[i].load(Ordering::Acquire) == key {
            return REPORT_RAM_HPA[i].load(Ordering::Acquire);
        }
    }
    0
}

fn report_ram_hpa_for(gpa: u64) -> u64 {
    let existing = report_ram_hpa_lookup(gpa);
    if existing != 0 {
        return existing;
    }
    let key = guest_uefi_report_ram_gpa_2m(gpa);
    for i in 0..GUEST_UEFI_REPORT_RAM_SLOTS {
        if REPORT_RAM_GPA[i].load(Ordering::Acquire) == u64::MAX {
            let hpa = REPORT_RAM_HPA[i].load(Ordering::Acquire);
            if hpa == 0 {
                continue;
            }
            REPORT_RAM_GPA[i].store(key, Ordering::Release);
            return hpa;
        }
    }
    0
}

/// Return guest-UEFI report-RAM 2 MiB frames to the E4 allocator.
///
/// Nested Intel `957e0ad`: pool=32 stole 64 MiB so Linux loaded at
/// `0xc400000` then `#DF` `rip=0x9e036`. Guest-UEFI has finished.
pub fn release_report_ram_for_e4(alloc: &mut FrameAllocator) -> u32 {
    let mut n = 0u32;
    for i in 0..GUEST_UEFI_REPORT_RAM_SLOTS {
        let hpa = REPORT_RAM_HPA[i].swap(0, Ordering::AcqRel);
        REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
        if hpa == 0 {
            continue;
        }
        let mut off = 0u64;
        while off < GUEST_UEFI_REPORT_RAM_PAGE {
            let _ = alloc.free_frame(PhysFrame::from_phys(hpa + off));
            off += 4096;
        }
        n = n.saturating_add(1);
    }
    REPORT_RAM_MAPS.store(0, Ordering::Release);
    n
}

fn mmio_scratch_hpa_for(gpa: u64) -> u64 {
    let key = gpa & !0x1F_FFFF;
    for i in 0..GUEST_UEFI_MMIO_SCRATCH_SLOTS {
        if MMIO_SCRATCH_GPA[i].load(Ordering::Acquire) == key {
            return MMIO_SCRATCH_HPA[i].load(Ordering::Acquire);
        }
    }
    for i in 0..GUEST_UEFI_MMIO_SCRATCH_SLOTS {
        if MMIO_SCRATCH_GPA[i].load(Ordering::Acquire) == u64::MAX {
            let hpa = MMIO_SCRATCH_HPA[i].load(Ordering::Acquire);
            if hpa == 0 {
                continue;
            }
            MMIO_SCRATCH_GPA[i].store(key, Ordering::Release);
            return hpa;
        }
    }
    0
}

#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_rip_is_poison_fill(rip: u64) -> bool {
    let hpa = RAM_HPA.load(Ordering::Acquire);
    if hpa == 0 || rip >= GUEST_UEFI_LOW_RAM_BYTES {
        return false;
    }
    let mut buf = [0u8; 4];
    // SAFETY: exclusive guest-UEFI 32 MiB slab; VMX-halted dump.
    // KANI-TARGET: poison-fill RIP peek (outside Proven Core).
    let ram = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_LOW_RAM_BYTES as usize);
    let n = copy_low_ram_at(ram, rip, &mut buf);
    n >= 4 && guest_uefi_insn_is_poison_fill(buf[0], buf[1], buf[2], buf[3])
}

/// 16550-compatible COM1/COM2. THR bytes go to host serial (firmware evidence).
#[cfg(target_os = "uefi")]
unsafe fn handle_uart(port: u16, is_in: bool, size: u64) {
    let off = port & 7;
    let com1 = (0x03F8..=0x03FF).contains(&port);
    let lcr_slot: &AtomicU8 = if com1 { &UART_LCR_COM1 } else { &UART_LCR_COM2 };
    let mask = if size == 1 {
        0xffu64
    } else if size == 2 {
        0xffff
    } else {
        0xffff_ffff
    };
    if is_in {
        let val = match off {
            2 => 0x01u64, // IIR: no interrupt
            5 => 0x60,    // LSR: THRE | TEMT
            _ => 0,
        };
        SAVED_RAX = (SAVED_RAX & !mask) | (val & mask);
    } else if off == 3 {
        lcr_slot.store(SAVED_RAX as u8, Ordering::Release);
    } else if off == 0 && (lcr_slot.load(Ordering::Acquire) & 0x80) == 0 {
        let b = SAVED_RAX as u8;
        if !COM_BANNER.swap(true, Ordering::AcqRel) {
            serial::write_line("boot: guest-UEFI firmware-serial begin");
        }
        serial::write_byte(b);
        COM_BYTES.fetch_add(1, Ordering::AcqRel);
        maybe_print_past_sec(false);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_cpuid() -> bool {
    let leaf = SAVED_RAX as u32;
    let sub = SAVED_RCX as u32;
    if note_unique_cpuid(leaf, sub) {
        serial::write_str("boot: guest-UEFI CPUID leaf=0x");
        write_hex(u64::from(leaf));
        serial::write_str(" sub=0x");
        write_hex(u64::from(sub));
        serial::write_byte(b'\n');
    }
    let r = guest_uefi_filter_cpuid(leaf, sub);
    SAVED_RAX = r.eax as u64;
    SAVED_RBX = r.ebx as u64;
    SAVED_RCX = r.ecx as u64;
    SAVED_RDX = r.edx as u64;
    skip_insn()
}

#[cfg(target_os = "uefi")]
fn note_unique_cpuid(leaf: u32, sub: u32) -> bool {
    let key = (u64::from(leaf) << 32) | u64::from(sub);
    let n = CPUID_SEEN_N.load(Ordering::Acquire).min(16) as usize;
    for slot in CPUID_SEEN.iter().take(n) {
        if slot.load(Ordering::Acquire) == key {
            return false;
        }
    }
    let i = CPUID_SEEN_N.fetch_add(1, Ordering::AcqRel) as usize;
    if i >= 16 {
        return false;
    }
    CPUID_SEEN[i].store(key, Ordering::Release);
    true
}

#[cfg(target_os = "uefi")]
unsafe fn handle_debugcon(is_in: bool, size: u64) {
    if is_in {
        let mask = if size == 1 {
            0xffu64
        } else if size == 2 {
            0xffff
        } else {
            0xffff_ffff
        };
        SAVED_RAX &= !mask;
        return;
    }
    if DBG_LINES.load(Ordering::Acquire) >= 32 {
        return;
    }
    let b = SAVED_RAX as u8;
    if b == b'\n' || b == b'\r' {
        flush_debugcon();
        return;
    }
    if b < 0x20 || b > 0x7e {
        return;
    }
    let i = DBG_LEN.load(Ordering::Acquire) as usize;
    if i < 80 {
        DBG_BUF[i].store(b, Ordering::Release);
        DBG_LEN.store((i as u32) + 1, Ordering::Release);
        if i + 1 == 80 {
            flush_debugcon();
        }
    }
}

#[cfg(target_os = "uefi")]
fn flush_debugcon() {
    let n = DBG_LEN.swap(0, Ordering::AcqRel) as usize;
    if n == 0 {
        return;
    }
    if DBG_LINES.fetch_add(1, Ordering::AcqRel) >= 32 {
        return;
    }
    serial::write_str("boot: guest-dbg: ");
    for slot in DBG_BUF.iter().take(n) {
        serial::write_byte(slot.load(Ordering::Acquire));
    }
    serial::write_byte(b'\n');
}

/// Snapshot host XCR0 / CR4.OSXSAVE before OVMF XSETBV.
#[cfg(target_os = "uefi")]
unsafe fn capture_host_xsave_before_guest_uefi() {
    let cr4 = cpu::read_cr4();
    let had = (cr4 & cpu::CR4_OSXSAVE) != 0;
    HOST_OSXSAVE_SAVED.store(u32::from(had), Ordering::Release);
    let xcr0 = if had {
        // SAFETY: CR4.OSXSAVE is set; XGETBV(0) is architectural.
        // KANI-TARGET: guest-UEFI host XCR0 capture (outside Proven Core).
        cpu::xgetbv(0)
    } else {
        1
    };
    HOST_XCR0_SAVED.store(xcr0, Ordering::Release);
    HOST_XSAVE_CAPTURED.store(true, Ordering::Release);
    HOST_XSAVE_RESTORED.store(false, Ordering::Release);
}

#[cfg(not(target_os = "uefi"))]
#[allow(dead_code)]
fn capture_host_xsave_before_guest_uefi() {}

/// Restore host XCR0 and CR4.OSXSAVE after guest-UEFI, before E4 Linux.
/// Nested Intel `73ed589`: ATAPI-OK then E4 `#DF` vec=8.
///
/// INVARIANTS:
/// - No-op if guest-UEFI never captured host XSAVE
/// - Idempotent (second call is a no-op)
/// - XSETBV happens with OSXSAVE set; OSXSAVE is then restored
///
/// VERIFICATION: L1 (host tests on the value helpers)
pub unsafe fn restore_host_xsave_after_guest_uefi() {
    #[cfg(target_os = "uefi")]
    {
        restore_host_xsave_after_guest_uefi_inner();
    }
}

#[cfg(target_os = "uefi")]
unsafe fn restore_host_xsave_after_guest_uefi_inner() {
    if !HOST_XSAVE_CAPTURED.load(Ordering::Acquire) {
        return;
    }
    if HOST_XSAVE_RESTORED.swap(true, Ordering::AcqRel) {
        return;
    }
    let host_mask = {
        let r = cpu::cpuid(0xD, 0);
        ((r.edx as u64) << 32) | (r.eax as u64)
    };
    let want = e4_restore_xcr0_value(
        HOST_XCR0_SAVED.load(Ordering::Acquire),
        true,
        host_mask,
    );
    let had_osxsave = HOST_OSXSAVE_SAVED.load(Ordering::Acquire) != 0;
    let cr4 = cpu::read_cr4();
    if cr4 & cpu::CR4_OSXSAVE == 0 {
        // SAFETY: OSXSAVE is not CR4-fixed0-forbidden; required for XSETBV.
        // KANI-TARGET: guest-UEFI host OSXSAVE for restore XCR0 (outside Proven Core).
        cpu::write_cr4(cr4 | cpu::CR4_OSXSAVE);
    }
    // SAFETY: CR4.OSXSAVE is set; want is masked to CPUID.0D:0 with x87 set.
    // KANI-TARGET: guest-UEFI restore host XCR0 (outside Proven Core).
    cpu::xsetbv(0, want);
    let after = e4_restore_cr4_osxsave(cpu::read_cr4(), had_osxsave);
    if after != cpu::read_cr4() {
        // SAFETY: OSXSAVE cleared only after XCR0 is the captured value.
        // KANI-TARGET: guest-UEFI restore host CR4.OSXSAVE (outside Proven Core).
        cpu::write_cr4(after);
    }
    serial::write_str("boot: guest-UEFI restore host xcr0=0x");
    write_hex(want);
    serial::write_str(" osxsave=");
    write_dec(u64::from(had_osxsave));
    serial::write_byte(b'\n');
    // Nested Intel 1a93cb8: after ATAPI-OK, E4 Linux `#DF` cr4=0x2060
    // (PAE+MCE+VMXE, no OSFXSR). Keep SSE on the host so E4 copies it.
    let sse = cpu::read_cr4() | cpu::CR4_OSFXSR | cpu::CR4_OSXMMEXCPT;
    if sse != cpu::read_cr4() {
        // SAFETY: OSFXSR/OSXMMEXCPT are not CR4-fixed0-forbidden on this CPU.
        // KANI-TARGET: guest-UEFI restore host OSFXSR for E4 Linux (outside Proven Core).
        cpu::write_cr4(sse);
    }
}

/// XSETBV always VM-exits. Skipping without writing XCR0 leaves XCR0=0,
/// then XSAVES #UD/#GP after INVPCID bits (iron COM2 next after RIP 0x109D).
#[cfg(target_os = "uefi")]
unsafe fn handle_xsetbv() -> bool {
    let xcr = SAVED_RCX as u32;
    if !xsetbv_accepts_xcr(xcr) {
        return skip_insn();
    }
    let value = (SAVED_RAX & 0xffff_ffff) | ((SAVED_RDX & 0xffff_ffff) << 32);
    let host_mask = {
        let r = cpu::cpuid(0xD, 0);
        ((r.edx as u64) << 32) | (r.eax as u64)
    };
    let v = xsetbv_masked_xcr0(value, host_mask);
    force_guest_cr4_osxsave();
    // SAFETY: CR4.OSXSAVE is set; v is masked to CPUID.0D:0 with x87 set.
    // KANI-TARGET: guest-UEFI XSETBV XCR0 mask (outside Proven Core).
    cpu::xsetbv(0, v);
    let n = XSETBV_N.fetch_add(1, Ordering::AcqRel);
    if n < 2 {
        serial::write_str("boot: guest-UEFI XSETBV xcr0=0x");
        write_hex(v);
        serial::write_byte(b'\n');
    }
    skip_insn()
}

/// CR0.PG does not exit. On every VM-exit, set LMA = LME && PG and match
/// the IA-32e VM-entry control so RDMSR EFER is architectural and the
/// next VMRESUME is legal (KVM vmx_set_efer).
#[cfg(target_os = "uefi")]
unsafe fn sync_guest_efer_lma() {
    let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
    let efer = ops::vmread(GUEST_IA32_EFER).unwrap_or(0);
    let with = guest_uefi_efer_with_lma(efer, guest_uefi_cr0_is_paging(cr0));
    LAST_EFER.store(with, Ordering::Release);
    let lma = (with & GUEST_UEFI_EFER_LMA) != 0;
    if with != efer {
        let _ = ops::vmwrite(GUEST_IA32_EFER, with);
        if lma {
            serial::write_line("boot: guest-UEFI EFER.LMA set (CR0.PG)");
        } else {
            serial::write_line("boot: guest-UEFI EFER.LMA cleared");
        }
    }
    let entry = ops::vmread(VM_ENTRY_CONTROLS).unwrap_or(0);
    let next = guest_uefi_ia32e_entry_ctls(entry, lma);
    if next != entry {
        let _ = ops::vmwrite(VM_ENTRY_CONTROLS, next);
        if lma {
            serial::write_line("boot: guest-UEFI IA-32e entry (EFER.LMA)");
        }
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_rdmsr() -> bool {
    let msr = SAVED_RCX as u32;
    LAST_GUEST_MSR.store(msr, Ordering::Release);
    let mut v = guest_uefi_rdmsr(msr);
    if msr == 0xC000_0080 {
        let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
        v = guest_uefi_efer_with_lma(v, guest_uefi_cr0_is_paging(cr0));
        LAST_EFER.store(v, Ordering::Release);
    }
    if msr != 0x10 && note_unique_rdmsr(msr) {
        serial::write_str("boot: guest-UEFI RDMSR index=0x");
        write_hex(u64::from(msr));
        serial::write_str(" val=0x");
        write_hex(v);
        serial::write_byte(b'\n');
    }
    if msr == 0x00FE || msr == 0x023F {
        guest_uefi_paint_live_uc_hole_now();
    }
    SAVED_RAX = v as u32 as u64;
    SAVED_RDX = (v >> 32) as u32 as u64;
    skip_insn()
}

#[cfg(target_os = "uefi")]
fn note_unique_rdmsr(msr: u32) -> bool {
    let n = MSR_SEEN_N.load(Ordering::Acquire).min(24) as usize;
    for slot in MSR_SEEN.iter().take(n) {
        if slot.load(Ordering::Acquire) == msr {
            return false;
        }
    }
    let i = MSR_SEEN_N.fetch_add(1, Ordering::AcqRel) as usize;
    if i >= 24 {
        return false;
    }
    MSR_SEEN[i].store(msr, Ordering::Release);
    true
}

#[cfg(target_os = "uefi")]
fn note_unique_wrmsr(msr: u32) -> bool {
    let n = WRMSR_SEEN_N.load(Ordering::Acquire).min(16) as usize;
    for slot in WRMSR_SEEN.iter().take(n) {
        if slot.load(Ordering::Acquire) == msr {
            return false;
        }
    }
    let i = WRMSR_SEEN_N.fetch_add(1, Ordering::AcqRel) as usize;
    if i >= 16 {
        return false;
    }
    WRMSR_SEEN[i].store(msr, Ordering::Release);
    true
}

#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_rdmsr(msr: u32) -> u64 {
    if msr == crate::arch::cpu::IA32_FEATURE_CONTROL {
        return GUEST_UEFI_FEATURE_CONTROL_VALUE;
    }
    if let Some(v) = guest_uefi_misc_enable_read(msr) {
        return v;
    }
    if let Some(v) = guest_uefi_mtrr_read(msr) {
        return v;
    }
    if let Some(v) = crate::devices::lapic_virt::rdmsr(msr) {
        return v;
    }
    match msr_firewall::classify_msr(msr, msr_firewall::MsrAccess::Read) {
        msr_firewall::MsrAction::HostPassthrough => cpu::rdmsr(msr),
        msr_firewall::MsrAction::VmcsEfer => ops::vmread(GUEST_IA32_EFER).unwrap_or(0),
        msr_firewall::MsrAction::VmcsPat => ops::vmread(GUEST_IA32_PAT).unwrap_or(0),
        msr_firewall::MsrAction::VmcsSysenterCs => ops::vmread(GUEST_IA32_SYSENTER_CS).unwrap_or(0),
        msr_firewall::MsrAction::VmcsSysenterEsp => ops::vmread(GUEST_IA32_SYSENTER_ESP).unwrap_or(0),
        msr_firewall::MsrAction::VmcsSysenterEip => ops::vmread(GUEST_IA32_SYSENTER_EIP).unwrap_or(0),
        msr_firewall::MsrAction::VmcsFsBase => ops::vmread(GUEST_FS_BASE).unwrap_or(0),
        msr_firewall::MsrAction::VmcsGsBase => ops::vmread(GUEST_GS_BASE).unwrap_or(0),
        msr_firewall::MsrAction::Shadow => {
            if msr == 0x1B {
                0xFEE0_0000 | (1 << 8) | (1 << 11)
            } else {
                msr_firewall::shadow_read(msr)
            }
        }
        msr_firewall::MsrAction::InjectGp
        | msr_firewall::MsrAction::ReadZero
        | msr_firewall::MsrAction::IgnoreWrite => 0,
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_wrmsr() -> bool {
    let msr = SAVED_RCX as u32;
    LAST_GUEST_MSR.store(msr, Ordering::Release);
    let mut v = (SAVED_RAX & 0xffff_ffff) | ((SAVED_RDX & 0xffff_ffff) << 32);
    if msr == 0xC000_0080 {
        let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
        v = guest_uefi_efer_with_lma(v, guest_uefi_cr0_is_paging(cr0));
        LAST_EFER.store(v, Ordering::Release);
    }
    if msr != 0x10 && note_unique_wrmsr(msr) {
        serial::write_str("boot: guest-UEFI WRMSR index=0x");
        write_hex(u64::from(msr));
        serial::write_str(" val=0x");
        write_hex(v);
        serial::write_byte(b'\n');
    }
    if crate::devices::lapic_virt::wrmsr(msr, v).is_some() {
        return skip_insn();
    }
    if guest_uefi_misc_enable_write(msr, v) {
        return skip_insn();
    }
    if guest_uefi_mtrr_write(msr, v) {
        return skip_insn();
    }
    match msr_firewall::classify_msr(msr, msr_firewall::MsrAccess::Write) {
        msr_firewall::MsrAction::VmcsEfer => {
            let _ = ops::vmwrite(GUEST_IA32_EFER, v);
            sync_guest_efer_lma();
        }
        msr_firewall::MsrAction::VmcsPat => {
            let _ = ops::vmwrite(GUEST_IA32_PAT, v);
        }
        msr_firewall::MsrAction::VmcsSysenterCs => {
            let _ = ops::vmwrite(GUEST_IA32_SYSENTER_CS, v);
        }
        msr_firewall::MsrAction::VmcsSysenterEsp => {
            let _ = ops::vmwrite(GUEST_IA32_SYSENTER_ESP, v);
        }
        msr_firewall::MsrAction::VmcsSysenterEip => {
            let _ = ops::vmwrite(GUEST_IA32_SYSENTER_EIP, v);
        }
        msr_firewall::MsrAction::VmcsFsBase => {
            let _ = ops::vmwrite(GUEST_FS_BASE, v);
        }
        msr_firewall::MsrAction::VmcsGsBase => {
            let _ = ops::vmwrite(GUEST_GS_BASE, v);
        }
        msr_firewall::MsrAction::Shadow => msr_firewall::shadow_write(msr, v),
        _ => {}
    }
    skip_insn()
}

#[cfg(target_os = "uefi")]
unsafe fn leave_to_e4() -> ! {
    restore_host_xsave_after_guest_uefi();
    let vmcs = SAVED_VMCS;
    if vmcs != 0 {
        let _ = ops::vmclear(vmcs);
    }
    let rsp = E4_RSP;
    let rip_cont = E4_RESUME;
    if rsp != 0 && rip_cont != 0 {
        core::arch::asm!(
            "mov rsp, {rsp}",
            "jmp {rip}",
            rsp = in(reg) rsp,
            rip = in(reg) rip_cont,
            options(noreturn),
        );
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Remember the E4 SHELL continuation (original UEFI stack).
pub unsafe fn arm_e4_resume(
    alloc: *mut FrameAllocator,
    life: *mut crate::vmx::lifecycle::VmxLifecycle,
    resume: extern "C" fn() -> !,
) {
    E4_ALLOC = alloc;
    E4_LIFE = life;
    E4_RESUME = resume as usize as u64;
    let mut rsp: u64;
    core::arch::asm!("mov {rsp}, rsp", rsp = out(reg) rsp);
    E4_RSP = rsp;
}

pub fn e4_alloc() -> *mut FrameAllocator {
    unsafe { E4_ALLOC }
}

pub fn e4_life() -> *mut crate::vmx::lifecycle::VmxLifecycle {
    unsafe { E4_LIFE }
}

#[cfg(target_os = "uefi")]
fn write_hex_inner(mut n: u64) {
    let mut buf = [0u8; 16];
    let mut i = 16;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 && i > 0 {
        i -= 1;
        let d = (n & 0xF) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(target_os = "uefi")]
fn write_hex(n: u64) {
    write_hex_inner(n);
}

#[cfg(target_os = "uefi")]
fn write_hex_u32(n: u32) {
    write_hex_inner(n as u64);
}

#[cfg(target_os = "uefi")]
fn write_dec(mut n: u64) {
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(test)]
#[path = "guest_uefi_test.rs"]
mod guest_uefi_test;
