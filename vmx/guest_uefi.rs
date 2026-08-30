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
    "residual: private guest-UEFI VMCS + EPT VMLAUNCH of retained ESP OVMF.fd; CR4.VMXE host-owned + CR4.OSXSAVE host-owned so OVMF SEC mov cr4,0x640 does not #GP and CpuDxe mov cr4,0x668 does not clear OSXSAVE; COM1/COM2 forwarded; past-SEC when linear leaves last 64KiB and PEI PCI or firmware serial or HLT; attach_cdrom_uefi after FirmwareArmed is GuestVisible (PCI IDE/ATAPI; IDE at 00:00.1); unarmed stays UnsupportedOnFirmware; CMOS/fw_cfg/i440fx platform; i440FX host at 00:08.0; PEI 00:00.0 DID is i440FX 0x1237 (PlatformMemMapInitialization VGA IoMemory HOB at 0xA0000-1MiB, stock QEMU map, not merged [0, LowMemory)); 00:00.0 stays i440FX so CpuDxe AcpiTimerLibConstructor HostBridgeDevId matches 0x1237 (PIIX4_PMBA_VALUE 0xB000) not ASSERT(FALSE) on virtio 0x1042; DXE latches virtio 0x1042 at 00:02.0 on other-BDF CF8 (PciBus/BOTH-OK); virtio Header Type is multifunction so a walk finds IDE fn1; slot-0 Header Type is multifunction so a walk finds IDE fn1; PIIX 00:01.1 is the same CD; PIIX4 PM at 00:01.3; remap i440FX DID in guest-private OVMF copy (cmp bx, not LZMA 37 12); CF8|CFC byte offset matches QEMU pci_host_data_read; EPT sink-resume for high MMIO; 4MiB flash window (VARS gap at 0xFFC00000); empty VARS _FVH; live HPET; HPET 1s step; stop RIP insn dump; spin jmp skip; past-PEI/DXE or CD boot attempt; empty virtio-blk at 00:02.0; product ISO virtio-pci queues gated on window; lab stub stays enum-only; fw_cfg bootorder CD then disk (PIIX ide@1,1 then virtio-fn1 ide@0,1, master drive@0, not slave drive@1; scsi@2 not scsi@0); ACPI PM timer (port 0 dword + PIIX 0x408 + 0xB008) so AcpiTimerLib Delay can end; iron 2cbf9e8 retcmp= cmp ax,0x1237/0x29C0/0x0D57 then mov word 0xB000/0x0600 default call DebugAssert (AcpiTimerLibConstructor; 00:00.0 was virtio 0x1042); post-DXE spends the 32768-exit cap until ATAPI sectors>0 (not virtio-alone; not both-enum-alone; 1b07692 n=1111 BOTH then stopped with sectors=0; 8e55abf n=2048 ata=0 unh=0 still PciBus cf8=0x80000838 ISA 00:01.0 offset 0x38; 5d9e346 n=8192 ataio=0 unh=3 port=0xcf8 empty-slot walk + KBC; 8192-exit cap ended on CF8; 2674629 n=32768 ataio=0 acpi=16612 port=0 in eax,dx); PIIX3 ISA PIRQ 0x60-0x63 default 0x80; HPET 1s on preemption/HLT not PCI I/O; HPET 1ms on CPUID/MSR/EPT; HPET TSC-delta on UART COM I/O cap 4us (not 1ms/byte; not PCI/ATA); Linux printk ticks every 4096 after #PF deliver (iron 115e5ee every-256 UART split PAT); guest UART nowait (do not clear COM2_LIVE); iron 115e5ee PAT freeze; Linux CPUID GenuineIntel + NX; guest UART TX ring drain; guest UART TX ring drain 4/exit (iron 45aec97 GenuineIntEl + NX missing then PAT); linux earlycon share TX ring (iron 202312f readable Linux version then e820 cut); linux earlycon quiet ticks; linux earlycon hush HV; linux earlycon share product ISO; cpu_flush on tick cadence even when share; linux earlycon share first CPUID; linux earlycon share first high-half; linux earlycon share first bootimg; guest UART TX drain COM2 independent; linux earlycon pace LSR THRE; linux earlycon skip #PF dump; linux earlycon skip exc deliver; poll ISO-INSTALL-OK every resume; EFER NXE after high-half; 8042 KBC 0x60/0x64; KeyboardWaitForValue; nested c19b91f BOTH-OK then n=32768 ataio=0 acpi=14903 port=0x64 (OBF never set after 0xAA); self-test 0x55 plus command ACK; ACPI PM 1s step; iron COM2 #UD RIP 0x109D pci_ide=0; iron 0ca02e6 skipped eb ec then #UD RIP 0x109D CR4=0x668 DebugLib dumped COM1 until cap; #UD intercept XSAVE retry/UD2 skip; iron d5f9431 #UD gone then n=1280..8192 reason=0x34 rip=0x6e81ca (pause CpuDeadLoop, no BOTH-OK); preempt pause/jcc skip; e2af81e missed GCC eb fc / 0F 84 rel32 (iron COM2 insn=ebec jmp -20); preempt eb/jcc32 skip; iron 891eb5b OSXSAVE CR4 intercept then skipped ebecc9c3 leave; ret then #UD 0x109D DAA PE header; do not skip jmp whose fallthrough is leave; ret; dump ASSERT retaddr; iron 17449e2 ASSERT noskip ret=0x6e8946 rip=0x6e81ca after host CPUID (Xeon topology+VMX); guest-UEFI CPUID uniprocessor hide VMX/x2APIC; FEATURE_CONTROL lock no VMX; iron ad78f12 CPUID uniprocessor then ASSERT ret=0x6e8946 after seven RDMSR 0x1B and CPUID 0x1cf11b5; xAPIC 2MiB was sink zeros (version 0); xAPIC 4K version 0x50014 not sink; iron 3f417ca xAPIC 4K mapped still ASSERT after MTRR walk 0xFE/0x2FF/0x250 (host MTRR passthrough + fixed reads 0); MTRR shadow VCNT=32 FIX WB VGA UC plus PCI hole UC 1GB at 0xC0000000; iron 408788c MTRR walk completed then still ASSERT ret=0x6e8946 after CPUID 0x1cf11b5 (not GetAllMtrrs); nested KVM sets hypervisor CPUID bit 31 plus KVMKVMKVM leaves, iron passthrough did not; guest-UEFI CPUID hypervisor present plus KVM signature; Linux high-half hides hypervisor bit plus 0x4000 scan (iron COM2 n=128 leaf=0x40003d00 n=256 leaf=0x4000bd00); Linux hypervisor_cpuid_base callee-saved GPR bump to 0x4000FF00 so each vendor is one CPUID (90c85d5 Loaded initrd then 0x4000 walk, HPET climbing); alpine-virt native_cpuid push rbx RSP slot (base in EBX, not R12); IA32_MISC_ENABLE shadowed not host; ASSERT dump callerrip plus home slots; unique RDMSR val=; iron 8700cbb hypervisor CPUID still ASSERT callerrip=0x1d25193 after WRMSR then RDMSR spin (MtrrLib WorkingRangeCount vs VCNT=8); fw_cfg bootorder NUL so ConnectDevicesFromQemu is not INVALID_PARAMETER; unique WRMSR; iron 0b7d647 VCNT=32 0xfe=0x520 PCI UC hole then firmware zeroed 0x200 still ASSERT callerrip=0x1d25193 lastmsr=EFER file=@B is pointer bytes; QEMU BOTH-OK skipped ebf3c9c3 (not ASSERT gone); EFER.LMA equals LME and CR0.PG plus IA-32e entry matches LMA; iron b4b4847 efer=0xd00 pg=1 csl=1 still ASSERT callerrip=0x1d25193 r8 is gPcdDataBaseSignatureGuid; debugcon 0x402 tee; unique CPUID; iron c40f4a8 pcdsig=1 after 32-pair MTRR walk still ASSERT; iron aee545f DXE assert skip caller=0x1d25193 then #UD linear=0x109d stop n=5364 sectors=0; revert iron ebec skip; MTRR power-on E=0 VCNT=8 no UC hole (firmware programs); iron 10cb881 VCNT=8 power-on still ASSERT callerrip=0x1d25193 mtrrdef=0xc06 mtrr0=0x80000000 noskip flood; VCNT=32 power-on no hole plus mtrr1/mtrrv dump; iron a9ffaa5 VCNT=32 power-on mtrr0=0x80000000 lastmsr=EFER still ASSERT; DxeCore CoreStartImage call EntryPoint (c6401801ff5020) loc2=ldri CpuDxe; MTRR GetAllMtrrs then paging refresh IsExecuteDisableEnabled; hide NX/1G/TME and strip EFER.NXE so CpuDxe does not ASSERT_EFI_ERROR SetMemorySpaceAttributes XP; dump ldri ImageBase; 80000008 subleaf 0; iron 5f59c86 efer=0x500 lastmsr=0x23f imgentry CpuDxe NXE-off still ASSERT (MtrrGetMemoryAttributes not XP); MAXPHYADDR [36,48] not clip-36; QEMU CI 17449e2 stuck ebf3c9c3 (jmp -13 leave;ret) — keep that skip (nested BOTH-OK); unguarded ebec skip was 891eb5b #UD; preempt noskip dump; guest-UEFI INVPCID/RDTSCP/XSAVES; XSETBV executes XCR0 (not skip_insn); fw_cfg etc/boot-menu-wait 0ms skip BdsWait; HLT skip so DXE can walk PCI; CR-access resume; firmware-simultaneous PCI enum; 8259 PIC RAZ/WI; fw_cfg etc/e820 32MiB; exception insn dump; ATAPI signature + PACKET interrupt-reason so firmware can READ(10); 8-byte IDE command BAR and BAR-relocated ATA; EXECUTE DEVICE DIAGNOSTIC 0x90 restores 0xEB14; BMIDE BAR4 RAZ/WI; first unhandled I/O traced; not firmware El Torito boot; not installer; not ISO-INSTALL-OK; no guest UEFI distro; iron d5fceb1 MAXPHYADDR unclip past CpuDxe then #PF err=0 CR2 0x80B000 MEMFD mov al,[disp32] (linear dump was RIP not CR2); identity_map_not_present NP 2M/4K in guest PT via ram_hpa; iron 3311ff3 #PF cr3=0x0 fail=alloc; build_identity_4g SEC PML4 0x800000; iron 7ea62ea fail=present SEC already mapped CR2 still VMWRITE CR3; iron 13e8bd2 CR3 identity then same #PF fail=present (walker present, CPU NP); rebuild SEC 4G identity once; hide LA57; iron COM2 after CR3 load #PF err=0x9 cr2=0xa027c8 (P+RSVD; NX-in-PTE with NXE=0); rebuild 4G on reserved-bit #PF; iron 101b8ec 4G n=1 n=2 then fail=present cr2=0x1ae7078 pde=0x30646870 (MEMFD heap clobber); HV identity PML4 at 0x200000 not 0x800000; e820 reserved 36KiB; always rebuild 4G; iron cc7d78a HV PML4 4G n=1 cr3=0x200000 then EPT violation gpa=0xc01df1b7 reason=0x30 (PCI hole; 4G identity present, EPT sink stopped at 1GiB); sink-resume PCI hole 0xC0000000; iron fdf07ba maps=4 EPT sink worked then #PF err=0x2 cr2=0x1e9000 pde=0xc0000083 4G n=2 then ASSERT callerrip=0x1d25193 lastmsr=0x23f mtrr0=0x80000000 (4G WB identity vs MTRR UC 2-4GiB); RAM-only identity leaves plus UC 2MiB sink #PF; nested 5db28e3 #PF cr2=0xffc00000 after RAM-only (flash NP) stop n=1007 BOTH missing; identity also maps flash 0xFFC00000 plus xAPIC 0xFEE00000; iron eb4b27d flash+xAPIC identity then #PF cr2=0x80000008 err=0xb pde=0xc0400083 (RSVD 1GiB PDPTE in MTRR UC hole); iron 73576cc bulk UC 2MiB identity then #PF cr2=0x1e9000 4G n=2 ASSERT callerrip=0x1d25193 lastmsr=0x23f (PAT UC- vs MTRR UC); iron a428202 on-demand mmio then #PF cr2=0x80000008 err=0xb pde=0xc0400083 identity MMIO fail (1GiB PDPTE after retargeted PDPT); split RSVD 1GiB into SEC PD even when PML4[0] is not pml4+0x1000; rebuild 4G then retry one PAT-UC 2MiB; hole stays NP at 4G rebuild; iron 124c1a8 identity MMIO n=2 then #PF cr2=0xffffffff96808086 err=0x2 pde=0 rip=0x300000 insn=afafafaf (sign-extended 32-bit 0x96808086 walks PML4[511] not low 4G); map high-half 2MiB to zero-extended GPA; e820 reserved 44KiB; iron b25d75b identity MMIO n=3 then #PF cr2=0x80000008 rip=0x30108e #UD linear=0x301093 insn=82bf (firmware PT stores at 0x80000008 hit shared HPET EPT sink); dedicated 2MiB UC scratch HPA for GPA 0x80000000 not zero sink; iron 577c9eb scratch 0x80000000 then EPT sink gpa=0xc0200000 then #PF cr2=0x9896808086 err=0x2 pde=0 rip=0x300001 insn=afafafaf (leftover-high 32-bit hole; PT stores at 0xC0200000 hit shared zero sink); scratch pool for hole PT pages except live HPET 2MiB; leftover-high CR2 overflow PML4[1]; poison-fill RIP not resume; iron 471391f pool=8 maps=2 then #PF cr2=0x1e9000 err=0x2 pde=0xc0000083 4G n=2 ASSERT callerrip=0x1d25193 lastmsr=0x23f; split 1GiB RAM PDPTE do not rebuild 4G; pre-scratch 0xC0000000+0xFCE00000; iron d757a0a SPLIT n=2 cr2=0x1e9000 then #PF err=0x9 pde=0xafafafafafafafaf cr2=0x1d1e6cb (firmware 0xAF-filled SEC PD after 1GiB); identity_refill_low4g_pd; stop n=1172 err=0x3 pde=0x1c000e7 rip=0x1de592 then E4 R640-BOOT-OK not Stage 44; iron 0bad45d refill then #PF 0x80000008 MMIO n=2 EPT scratch 0x80000000 plus 0xC0200000..0xC0A00000 then EPT sink gpa=0xc0c00000 leftover CR2 0x9896808086 rip=0xd00001 firmware-serial #DE RIP 0xCFFF9E DIV RCX=0 ASSERT ebec noskip; scratch pool 32 plus pre-scratch 0xC0000000..0xC0E00000 and 0x80000000; iron 5837243 pool=32 then EPT scratch walk 0xC1000000..0xC3A00000 then scratch cap gpa=0xc3c00000 sink RIP 0x3d00001 pci_ide=0; guest_uefi_ept_scratch_on_qual write/fetch only; EPT hole ro R+X sink for hole reads so a later store can upgrade; pre-scratch only 0x80000000; iron da2c9c4 pool=32 then EPT scratch 0xC0000000 plus 0x80000000 plus 0xC0200000..0xC3C00000 then scratch cap gpa=0xc3e00000 qual=0x184 RIP 0x3dfffff pci_ide=0; SDM bit 8 walk bits 2:0 are original access; guest_uefi_ept_scratch_on_qual is data-write only (not fetch); EPT hole ro R+X sink for hole reads so a later store can upgrade; iron f93caee write-only scratch then EPT hole ro gpa=0xc0000000 qual=0x184 plus scratch 0x80000000 plus hole ro 0xc0200000 plus scratch 0xc0000000 qual=0x1ab then #PF cr2=0x9896808086 rip=0x300001 insn=afafafaf poison fill (hole RO mapped live HPET SINK_HPA as PTEs); dedicated zero 2MiB for hole RO not SINK_HPA; HPET stays on SINK_HPA at 0xFEC00000/0xFED00000 only; not bulk 2-4GiB (73576cc ASSERT); not WB RAM (fdf07ba ASSERT); iron 06b011a hole-zero then #PF err=0x3 cr2=0x1d1abb8 pde=0x1c000e7 rip=0x1de592 (CR0.WP stack push in 2MiB identity; not leftover-high 0x9896808086); identity SPLIT4K 2MiB to 4K RW; nested Intel 06b011a BOTH-OK ataio=236 packet=0 (skip_insn after one word of rep insw); string/REP PIO so IDENTIFY lands; nested Intel 1e0f4a7 io string fw_cfg 0x511 then #PF cr2=0x205f18 4G n=2 cr2=-1 stop rip=0x28f402 BOTH missing; iron COM2 1e0f4a7 io string 0x511 n=4 then identity 4G n=1 cr2=0x80b000 ticks rip=0x3d2be4 ASSERT noskip callerrip=0x1f21193 lastmsr=0x23f mtrr0=0x80000000 imgentry=0x1dd97d3 pci_ide=0 (never SPLIT n=2); string RAM fill is ATA-only; iron COM2 54a8708 no 0x511 then identity SPLIT n=2 then SPLIT4K n=3 cr2=0x1d1abb8 pde=0x219067 then AlreadyPresent loop to identity cap n=256 stop n=1421 rip=0x1de592 pci_ide=0; SPLIT4K MOV CR3 after split; do not resume already-RW; iron COM2 19b0c11 hole-zero then identity MMIO n=2 cr2=0x80000008 then tick reason=0x34 rip=0x27e22d5 insn empty (RIP left 32MiB); hole RO was R+X so fetch executed dedicated zeros; leftover CR2 0x9896808086 rip=0x3ed00001 identity MMIO n=4..256 2MiB walk identity cap stop n=5687 pci_ide=0 then E4 R640-BOOT-OK not Stage 44; hole RO is R only (no X); do not identity-map [32MiB, 0x80000000); split PDPT[0] 1GiB on EPT, MOV CR3, and preemption while RIP is in 32MiB; iron COM2 89c3731 SPLIT PDPT0 then identity 4G n=1 hole ro then SPLIT4K n=2 cr2=0x1d1abb8 pde=0x219027 pte=0x1d1a067 already RW stop n=1168 rip=0x1de592 pci_ide=0 (RIP stayed in 32MiB; not 0x27e22d5); CR0.WP ANDs R/W through PML4/PDPT so OR walk R/W not only the 4K leaf; iron COM2 7413554 SPLIT4K n=2 resumed pml4e=0x5a6d (RO) pdpte=0x202067 then tick rip=0x1df1b5; then #PF cr2=0xfee00020 err=0x9 pdpte=0xc0600083 pml4e=0x5a6f stop n=1395 rip=0x1d84c7 pci_ide=0 (firmware 1GiB RSVD over xAPIC; not already-RW; not 0x27e22d5); map_mmio xAPIC RSVD 1GiB; iron COM2 32ee302 identity MMIO n=3 cr2=0xfee00020 then tick rip=0x1d6be4 then ASSERT noskip callerrip=0x1d25193 lastmsr=0x23f mtrrdef=0xc06 mtrr0=0x80000000 mtrr1=0x3fff80000800 imgentry=0x1bdd7d3 pci_ide=0 (WB xAPIC/flash 2MiB in MTRR UC 2-4GiB; not already-RW); PAT-UC PCD+PWT on flash+xAPIC identity; nested Intel 48c598a BOTH-OK ataio=1308 packet=0 insn=ef then edc9c3 (SET FEATURES 0xEF ABRT then IN EAX,DX poll; never PACKET); SET FEATURES succeeds DRDY not ABRT; iron COM2 855ba1c/48c598a PAT-UC then identity MMIO n=3 cr2=0xfee00020 ASSERT noskip callerrip=0x1d25193 lastmsr=0x23f mtrr1=0x3fff80000800 pci_ide=0 (PDPT[3] RSVD split; PDPT[2] 1GiB WB over 2-3GiB MTRR UC, no #PF); split sibling 1GiB in the UC hole; dump pdpte2; nested Intel 73ed589 BOTH-OK ATAPI-OK sectors=1 packet=9 scsi=0x28 ata=0xa0 ataio=982 then E4 Linux #DF vec=8 after BZIMAGE (OVMF XSETBV left host XCR0; E4 copies host CR4.OSXSAVE); restore host XCR0 and CR4.OSXSAVE after guest-UEFI before E4; iron COM2 pdpte2=0xc0400083 then MMIO n=4 pde=0xfee000ff ASSERT callerrip=0x1d25193 pci_ide=0 (CpuDxe software-walks 1GiB WB PDPT[2]; RAM SPLIT n=2 pdpt_i=0 never split the hole); identity_split_mtrr_uc_hole PDPT[2]+[3] on every identity map including 0x1e9000; dump pdpte2 after MMIO; iron COM2 8df2793 SPLIT PDPT0 then 4G n=1 then EPT hole ro gpa=0xc0000000 then SPLIT4K n=2 pdpte2=0x204067 (PD not 1GiB WB) no xAPIC #PF then ASSERT callerrip=0x1d25193 lastmsr=0x23f mtrr0=0x80000000 pci_ide=0 ataio=0 (CpuDxe software-walks NP 2-4GiB vs MTRR UC); PAT-UC 2-4GiB hole PCD+PWT at 4G rebuild not 73576cc UC-; dump pde8000 after 4G; iron COM2 d7bfb23 4G pde8000=0x800000ff SPLIT4K pml4e=0x5a6d pdpte2=0x204067 no xAPIC #PF still ASSERT callerrip=0x1d25193 (firmware PDPT 0x5000; PDPT[3] can stay 1GiB WB); identity_sync_live_mtrr_uc_hole live PDPT on SPLIT4K/4G/MMIO (not GPA 0x5000 until PML4[0] points there; not tick); dump pdpte3; iron COM2 1de9389 pdpte3=0x205067 PS clear pde8000=0x800000ff still ASSERT callerrip=0x1d25193 lastmsr=0x23f (1GiB PDPT[3] disproved); dump pml4e/pde8000/pdefee/pdeffc/pat at ASSERT; iron COM2 44c56db pde8000=0x800000ff pdpte3=0x205067 pdefee=0xfee000ff pdeffc=0xffc000ff pat=0x0 still ASSERT callerrip=0x1d25193 lastmsr=0x23f (VMCLEAR GUEST_IA32_PAT=0; Xeon VM-entry LOAD_PAT; PA0=UC vs MTRR WB RAM); init GUEST_IA32_PAT SDM reset 0x0007040600070406 plus HOST_IA32_PAT like E4 launch.rs; dump entry=; nested Intel 1a93cb8 ATAPI-OK sectors=1 then E4 #DF vec=8 cr4=0x2060 (startup_64 cr4&=0x1060 cleared OSFXSR); host-own OSFXSR+OSXMMEXCPT like VMXE; nested Intel ab25682 ATAPI-OK then ERROR unexpected CR-access rip=0x8400276 qual=0x4 cr4=0x2668 (startup_64 mov cr4 intercepted); emulate MOV CR4 keep VMXE+OSFXSR; iron COM2 1a93cb8 IA32_PAT guest=0x7010600070406 host=0x7010600070406 entry=0xd1fb then ASSERT pat=0x7010600070406 entry=0xd3fb pde8000=0x800000ff pdpte3=0x205067 lastmsr=0x23f mtrrdef=0xc06 mtrr0=0x80000000 (PAT WB proved; NP [32MiB, 2GiB) vs MTRR WB); guest PT WB [32MiB, 2GiB); do not EPT-map that window (89c3731); dump pde20; iron COM2 28f42d2 pde20=0x20000e7 pde8000=0x800000ff pat=0x7010600070406 still ASSERT callerrip=0x1d25193 lastmsr=0x23f (PDPT[0] mid-gap WB proved; live firmware PDPT[1] 1-2GiB NP vs MTRR WB); identity_ensure_pdpt_2m PDPT[1]; dump pde4000 pdpte1; iron COM2 be1b028 pde20=0x20000e7 pde4000=0x400000e7 pdpte1=0x203067 pde8000=0x800000ff pat=0x7010600070406 still ASSERT callerrip=0x1d25193 lastmsr=0x23f maxpa=46 mtrrdef=0xc06 pml4e=0x5a6f (0-4GiB guest PT matches MTRR WB+UC; NP [4GiB, 2^46) vs default WB; PML4E PWT); cap iron MAXPHYADDR 32 so GCD equals 4GiB identity (clip-36 left 4-64GiB NP); nested 36/40 stays; identity_clear_table_pwt_pcd live PML4E; iron COM2 162809f maxpa=32 mtrr1=0x80000800 pml4e=0x1a02023 (PWT clear) pde20=0x2000083 pde4000=0x400000e7 still ASSERT callerrip=0x1d25193 lastmsr=0x23f imgentry=0x6e87d3 no 4G n=1 (firmware PDPT 0x1a02000 sparse PDPT[0] NP vs MTRR WB); identity_refill_low4g_pd_keep_4k PDPT[0]; dump pde40 pdpte0 cr3; nested Intel 1b587dd BOTH-OK ataio=0 (ensure_pdpt_2m(0) on 1GiB retargeted SEC PD); keep_4k NP-only, do not split PDPT[0] 1GiB on sync; iron COM2 1b587dd/55d4dc6 keep_4k pde20=0x20000e7 pde40=0x40000e7 pde4000=0x400000e7 pde8000=0x800000ff maxpa=32 pml4e=0x1a02023 (no PWT) mtrr1=0x80000800 still ASSERT callerrip=0x1d25193 lastmsr=0x23f imgentry=0x6e87d3 pci_ide=0 ataio=0 (0-4GiB PT matches MTRR WB+UC; 2MiB at GPA 0 spans 1MiB fixed-MTRR); identity_split_gpa0_fixed_mtrr 4K at 0-2MiB; identity_clear_table_pwt_pcd also TABLE_FLAGS USER; dump pde0 pte0 pdpte2; iron COM2 659e7de SPLIT PDPT0 flood tick n=256 rip=0xfffcd6d6 (identity_map_mmio_2m 0x1E9000 smashed GPA0 4K every preempt); mmio 2m keeps 4K tables; nested Intel 61f84c6 GPA0 SPLIT4K then BOTH-OK pci_ide=1 ataio=0 3/3 (ATAPI-OK missing); guest_uefi_gpa0_fixed_mtrr_split iron maxpa=32 only; nested 36/40 keeps 2MiB at GPA 0; iron COM2 84171aa SPLIT4K GPA0 pde0=0x20b027 pte0=0x67 pde20=0x2000083 pde40=0x4000083 still ASSERT callerrip=0x1d25193 (GPA0 4K plus table USER proved; firmware 2MiB no USER); keep_4k OR LARGE_2M_FLAGS onto WB 2MiB (0x83 to 0xE7); dump pde6e; nested Intel 5811368 SPLIT4K GPA0 then BOTH-OK pci_ide=1 ataio=0 3/3 (host 46+ capped to 32); guest_uefi_gpa0_split_now skips GPA0 when host CPUID hypervisor bit is set; iron COM2 489d118 GPA0 4K pte0=0x67 pte1m=0x100067 pml4e1=0x0 pde20=0x20000e7 still ASSERT callerrip=0x1d25193 lastmsr=0x23f mtrr0=0x80000000 mtrr1=0x80000800 (0-4GiB PT matches MTRR; leftover-high NP; GCD untested spans PEI Uc32Base WB+UC); fw_cfg etc/e820 reserved PCI UC [2GiB,4GiB) so PlatformAddHobCB splits GCD at MTRR UC; iron COM2 38481d9 e820 type-2 reserved PCI UC still ASSERT pde8000=0x800000ff callerrip=0x1d25193 (GCD untested [32MiB,4GiB) mixed mid-gap WB + 4G PAT-UC; this OVMF.fd ignores type-2 below 4GiB); identity_set_pat_uc_hole WB 2-4GiB until firmware UC MTRR live then PAT-UC (not fdf07ba WB-while-UC-live; not 8df2793 NP); iron COM2 f07a597 PAT-UC+MTRR match still ASSERT pde8000=0x800000ff mtrr0=0x80000000 callerrip=0x1d25193 (guest PT family exhausted; GCD mixed range); hold valid UC variable MTRRs so CpuDxe RefreshGcd sees default WB (MTRR UC held (GCD)); guest_uefi_mtrr_set_admit_uc; iron COM2 22e0cb2 MTRR UC held mtrrv=0 pde8000=E7 still ASSERT callerrip=0x1d25193 (mixed MTRR disproved); e820 type-2 reserved [32MiB, 2GiB) mid-gap so GCD splits before Uc32Base (P3; 38481d9 PCI-hole type-2 ignored); iron COM2 f9a08c9 mid-gap reserved still ASSERT callerrip=0x1d25193 mtrrv=0 pde8000=E7 (e820 ignored); CMOS+fw_cfg LowMemory 2GiB so PEI HOB ends at Uc32Base (not EPT-map [32MiB, 2GiB)); iron COM2 fad19b2 CMOS 2GiB then EPT unbacked report-RAM gpa=0x7bddd000 reason=0x30 stop n=600 (firmware heap at top of LowMemory; ASSERT 0x1d25193 gone); lazy 2MiB WB EPT report-RAM pool (not identity 2GiB; not 89c3731); iron COM2 32e7d46 report-RAM pool=32 mapped gpa=0x7bddd000 then high heap; tick rip=0x7f8e21ca reason=0x34 same=376 lastmsr=0x23f insn empty (32MiB peek); peek report-RAM HPA for skip/ASSERT dump (do not skip ebecc9c3); iron COM2 957e0ad insn=ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f cr3=0x7fa01000 pml4e=0 pci_ide=0 (CpuDxe ASSERT relocated into report-RAM; 32MiB PT walk missed high CR3); P2 MTRR UC admitted (GCD) (hold left mtrrv=0 vs GCD UC at 2GiB); dump-walk CR3 via report-RAM peek; E4 hide LA57 (nested Intel ATAPI-OK then #DF trampoline 0x9e036); iron COM2 c70768b MTRR UC admitted mtrrv=1 mtrr0=0x80000000 pde8000=0x80000083 cr3=0x7fa01000 pml4e=0x7fa02023 insn=ebecc9c3 callerrip=0x7fd25193 pci_ide=0 (live report-RAM CR3 WB 2MiB vs admitted UC; 32MiB identity_sync missed pml4>=ram_len); guest_uefi_pt_paint_live_uc_hole peek/poke PAT-UC on high CR3 (CMOS 2GiB GCD split; not f07a597 low-CR3 paint; not skip ebecc9c3); iron COM2 4ae87de painted n=1029 pde8000=0x800000ff mtrrv=1 then ASSERT insn=ebecc9c3 pde0=0xe3 pte0=0 (PAT-UC+MTRR match on live CR3; 2MiB GPA0 spans 1MiB fixed-MTRR; identity_split_gpa0 TableOutOfRam); guest_uefi_pt_split_gpa0 peek/poke HV PT 0x20B000 on live PD[0]; iron COM2 7e5d70f GPA0 4K live CR3 n=513 pde0=0x20b027 pte0=0x67 pte1m=0x100067 pde8000=0x800000ff still ASSERT insn=ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f pci_ide=0 (PT matches MTRR on live high CR3; stop PT peek/poke; CpuDxe RefreshGcd GCD/HOB); do not lower CMOS 2GiB (32MiB LowMemory already ASSERTed); do not retry P3 mid-gap type-2; iron c1476d3 hypervisor etc/e820 VGA hole logged but PEI never opened the file (CMOS size to HOBs to GCD, not ScanE820); PEI 00:00.0 DID i440FX 0x1237 so PlatformMemMapInitialization adds IoMemory 0xA0000-1MiB; DXE latches virtio 0x1042 on other-BDF CF8; do not remap cmp bx 0x1237 while PEI captures HostBridgeDevId; dump e820= fwdir= pei_did=; iron f7620f6 PEI pci cfg=0x80000002 val=0x1237 pei_did=1 DXE virtio DID latch 00:01.03 then virtio 0x1042 VIRTIO-OK DXE-OK sectors=0 plat=1 e820=0 fwdir=0 remap n=0 still ASSERT ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f pde0=0x20b027 pte0=0x67 pte1m=0x100067 (DID fork closed); iron d6b012a pte_a0000=0xa0067 pte_c0000=0xc0067 (GPA0 identity WB; firmware FIX 0x250-0x26f are 0x06 WB; not GCD VGA punch; do not PAT-UC VGA); filehex test r9d jnz wbinvd mov rax EFI_UNSUPPORTED is CpuFlush FlushType!=0; nop jnz in live report-RAM so every FlushType WBINVD; dump r9=; iron f0781bb CpuFlush FlushType-any WBINVD n=2 filehex jnz-nop 9090 r9 leftover 0x21 still ASSERT ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f (CpuFlush leftover File dump not the ASSERT; P1 22e0cb2 hold ran while FIX was power-on 0 UC; firmware now FIX 0x06 WB); hold variable UC after FIX WB so GetMemoryAttributes is uniform on a spanning GCD (MTRR UC held after FIX WB (GCD)); scan every report-RAM CpuFlush copy (do not return after the first slot); dump flushjnz=; iron 6334704 MTRR UC held after FIX WB mtrrv=0 mtrr1=0x0 pde8000=0x80000083 flushjnz=0 filehex 9090 still ASSERT ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f (mixed variable-UC disproved with FIX WB; PEI i440FX VGA IoMemory HOB is GCD UC while firmware FIX 0x259-0x26f are WB 0x06; hold also left Uc32Base GCD UC vs default WB); admit variable UC again so the 2GiB hole matches; coerce FIX 0x259 and 0x268-0x26F to packed UC (MTRR VGA FIX UC (GCD)) so VGA GCD IoMemory matches; keep 0x250/0x258 WB; dump mtrr259=; iron ddbd866 MTRR UC admitted mtrrv=1 mtrr0=0x80000000 mtrr259=0x0 pde8000=0x800000ff flushjnz=0 still ASSERT ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f pte_a0000=0xa0067 pte_c0000=0xc0067 (GCD/MTRR VGA+hole matched; live GPA0 VGA PTEs still WB vs coerced FIX UC); PAT-UC 4K VGA leaves on live CR3 (guest_uefi_pt_paint_vga_uc; pte_a0000=0xa007f); dump calltgt=; iron e368e86 VGA 4K live PT PAT-UC n=96 cr3=0x800000 pte_a0000=0xa007f pte_c0000=0xc007f mtrr259=0x0 mtrrv=1 pde8000=0x800000ff flushjnz=0 still ASSERT ebecc9c3 callerrip=0x7fd25193 lastmsr=0x23f calltgt=0x7f8e21a5 tgthex=554889e54883ec10 (DebugAssert prologue, not RefreshGcd) pml4e=0x7fa02027 (PWT already clear); option-ROM C0000 PAT-UC vs firmware FIX 0x268-0x26F WB 0x06; coerce only FIX 0x259 (A0000-BFFFF packed UC); leave 0x268-0x26F firmware WB; PAT-UC only [0xA0000, 0xC0000); dump mtrr268=; expect pte_c0000=0xc0067 mtrr268=0x606060606060606; iron fd041bb VGA 4K live PT PAT-UC n=32 pte_a0000=0xa007f pte_c0000=0xc0067 then ASSERT mtrr259=0x0 mtrr268=0x0 (CpuDxe UC'd option-ROM after firmware WB) still ebecc9c3 calltgt=0x7f8e21a5; e368e86 full VGA UC matched and ASSERTed (GCD SystemMemory WB-only; SetMemorySpaceAttributes UC fails); hold FIX 0x259 and 0x268-0x26F WB after firmware 0x06 (MTRR VGA FIX WB held (GCD)); GPA0 4K leaves stay WB (pte_a0000=0xa0067); dump prehex=; iron 96ef961 MTRR VGA FIX WB held mtrr259=0x606060606060606 mtrr268=0x606060606060606 pte_a0000=0xa0067 pte_c0000=0xc0067 pde8000=0x800000ff flushjnz=0 still ASSERT ebecc9c3 callerrip=0x7fd25193 calltgt=0x7f8e21a5 prehex=66c705b30800000006eb05e85ff8ffff (mov word CacheWriteBack 6; jmp skip; call DebugAssert); VGA UC-match e368e86 and VGA WB-match 96ef961 both ASSERT at the same DebugAssert; dump 32-byte prehex immediately before call at 0x7fd25193 plus rax (EFI_STATUS); retpre= 32 bytes at CpuDxe ret-32 (ASSERT_EFI_ERROR site); keep PEI i440FX 0x1237 / DXE virtio 0x1042; keep FIX WB hold; no DID flip; no new PAT-UC; iron 6f077a3 prehex at 0x7fd25193 is DxeCore call [rax+0x20] rax=0 leftover (CpuDxe never returned); retpre switch stores UINT16 0x0600/0xB000 then jmp; default call DebugAssert (ASSERT(FALSE) not ASSERT_EFI_ERROR); dump retcmp= at ret-64 plus rbx rsi rdi g16=; iron 2cbf9e8 retcmp=000000e855f8ffff663d37127417663dc029741c663d570d752166c705cb0800 (cmp ax,0x1237 je PIIX4; cmp ax,0x29C0 je Q35; cmp ax,0x0D57 jne ASSERT; stores PIIX4_PMBA_VALUE 0xB000 / ICH9_PMBASE_VALUE 0x0600); g16=0000 rbx=0 rsi=0x7f6e1042 (virtio DID leftover) pci cfg=0x80000002 val=0x1042 after latch; keep 00:00.0 i440FX 0x1237; virtio at 00:02.0; do not skip ebecc9c3; iron bf696ca COM2 ATAPI-OK sectors=1 packet=9 scsi=0x28 ata=0xa0 ataio=982 stop n=30769 pci_ide=1 virtio=1 BOTH-OK n=12411 no AcpiTimerLib ASSERT; not El Torito; not installer; E4 SHELL then M4.2 G1 EPT GPA=0x10403000 fail-soft not Stage 44; Stage 45 keeps VMCS after first ATAPI sector until catalog+load READ plus payload COM RN-ELT or 131072-exit cap (post_atapi_should_stop does not apply the 32768 post-ATAPI tail after first ATAPI or after catalog+load READ; first sector is often LBA 0 dummy not catalog; maybe_print_eltorito; BAR-relocated ATA data-port rep insw fills RAM); El Torito catalog checksum plus FAT12 ESP EFI/BOOT/BOOTX64.EFI (not raw PE at load LBA); eltorito-progress catalog= bootimg=; iron COM2 ATAPI-OK n=30769 then catalog=1 bootimg=0 (Stage 45 kept VMCS; not ELTORITO-OK); nested 8881cdd catalog=1 bootimg=1 elt=0 at 131072-exit cap (DxeCore LoadImage SectionAlignment 0x1000 so ProtectUefiImage can set X); iron COM2 df7d158 catalog=1 bootimg=1 elt=0 com=0 sectors=107 packet=318 ataio=120786 stop n=131072 rip=0x7ee8786d port=0x1f7 scsi=0x0 (BDS ATA PIO; 131072-exit cap; not ELTORITO-OK; 512-byte FAT BPB then PVD root zeros last READ LBA 17; not 1a2b088 4K PE; E4 LINUX-EARLY then M4.2 G1 EPT GPA=0x10403000 fail-soft not Stage 45); 2048-byte FAT plus ISO9660 EFI/BOOT/BOOTX64.EFI; 262144-exit cap after iron+nested hit 131072-exit cap still in ATA/PCI; iron COM2 0be7283 firmware-serial RN-ELT RAYNU-V-M7-E5-OVMF-ELTORITO-OK n=197992 catalog=1 bootimg=1 magic=1 sectors=183 elt=1 packet=533 scsi=0x28 port=0x3f8 com=6; E4 LINUX-EARLY then M4.2 G1 EPT GPA=0x10403000 fail-soft not Stage 45; not ISO-INSTALL-OK; VMLAUNCH insn issued only when presence is true; Stage 46 product ISO PIC/IOAPIC inject (lab 8259 PIC RAZ/WI stays); Stage 46 product ISO 16550 + ttyS0 cmdline; Stage 46 product ISO SOL RX to guest COM1; Stage 46 product ISO Alpine serial auto-answer; 256MiB disk leftover report-RAM; report-RAM EPT pre-map; cpu_flush skip leftover pre-map; cpu_flush leftover per walk; linux unhandled nowait stop; virtio MMIO eax fallback; linux NMI inject; iron 1a2544d Freeing initrd then restore host xcr0; share hushes stop n=; virtio BAR trap over scratch (iron df0c118 Freeing initrd then silent; Linux BAR 0x80000000/0x80001000 on 2MiB UC scratch); PIIX3 ISA BAR RAZ; packed virtio common cfg; virtio MMIO raises PIT; virtio MMIO off=; virtio MMIO eax fallback size; packed virtio common cfg write; virtio MMIO polls lapic; linux I/O does not raise PIT (iron MADT stop); linux xAPIC EPT insn_len 0; linux preempt deadloop noskip; linux PIT prefer once; linux PIT prefer until DRIVER_OK; UART reassert RX not THRE; virtio drain every resume; linux virtio DRIVER_OK; product ISO fw_cfg ACPI MADT (iso=0 named files stay 3); linux PIC before LAPIC; linux PIC IRQ0; MADT IRQ0 ISO GSI 2; PIT skips IOAPIC pin 0; linux GSI 2 before PIC; fw_cfg IoReadFifo8 fills RAM (iron COM2 efi: no ACPI=; QemuFwCfgInitialize rep insb skipped); skip HV identity PML4 dest 0x205f18; PIIX4 PM1 SCI_EN; DSDT PCI0 _PRT; not ISO-INSTALL-OK";

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

/// QEMU / serial marker when firmware enumerated virtio `00:02.0` and IDE `00:00.1`
/// on the same boot. Not ATAPI sectors. Not installer.
pub const M7_E5_OVMF_BOTH_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-BOTH-OK";

/// QEMU / serial marker when firmware issued ATAPI READ and `sectors>0`.
/// Not a completed El Torito CD boot. Not installer.
pub const M7_E5_OVMF_ATAPI_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ATAPI-OK";

/// QEMU / serial marker when OVMF BDS loaded and ran the El Torito CD EFI.
/// Not installer. Not `ISO-INSTALL-OK`. Not `sectors>0` alone.
pub const M7_E5_OVMF_ELTORITO_OK_MARKER: &str = "RAYNU-V-M7-E5-OVMF-ELTORITO-OK";

/// Last 64 KiB of the 4 GiB space. OVMF 4M SEC / VTF lives here
/// (reset vector `0xFFFF_FFF0`; Stage 38 first exits at `0xFFFF_Fxxx`).
pub const GUEST_UEFI_SEC_TAIL_GPA: u64 = 0xFFFF_0000;

/// Resume cap after Stage 44's 32768-exit window — PEI/DXE + PciBus + ATAPI
/// plus Stage 45 El Torito load/StartImage. Nested VT-x `5d9e346`: BOTH-OK
/// then n=8192 `ataio=0` `unh=3` `port=0xcf8` (empty-slot walk + KBC).
/// Nested VT-x `2674629`: 32768 still `ataio=0` `acpi=16612` `port=0`
/// (BdsWait). Iron COM2 `bf696ca` ATAPI-OK at n=30769 then stopped on
/// first sector; Stage 45 needs headroom after that READ. 65536 left only
/// ~34k exits after n=30769. The 32768 post-ATAPI tail can also cut BDS
/// before catalog (first sector is often LBA 0 dummy) or StartImage after
/// catalog+load READs. Nested `8881cdd` and iron COM2 `df7d158` both hit
/// the 131072-exit cap with catalog=1 bootimg=1 elt=0 still in ATA/PCI
/// (iron stop n=131072 rip=0x7ee8786d port=0x1f7 scsi=0x0; not ELTORITO-OK).
/// Iron COM2 `0be7283`: `RN-ELT` + `OVMF-ELTORITO-OK` at n=197992 (catalog=1
/// bootimg=1 magic=1 sectors=183 elt=1 packet=533 scsi=0x28 port=0x3f8).
/// 262144 keeps the private VMCS until `RN-ELT` or the hard cap. Stage 45
/// does not apply the 32768 post-ATAPI tail after PACKET.
pub const GUEST_UEFI_RESUME_CAP: u32 = 262144;
/// Product ISO (Stage 46): stay in guest-UEFI past the lab RN-ELT stop.
/// Used whenever the window is armed (iron **and** nested QEMU `PRODUCT_ISO=`).
/// Lab-stub nested still uses [`GUEST_UEFI_NESTED_RESUME_CAP`]. Not `ISO-INSTALL-OK`.
pub const GUEST_UEFI_PRODUCT_ISO_RESUME_CAP: u32 = 16_777_216;
/// Nested KVM **lab stub** only. Iron ATAPI is n≈30769; El Torito StartImage is n=197992.
/// Nested CI that walks El Torito then Linux init SIGSEGV (CR2 in freed
/// report-RAM). 65536 keeps BOTH+ATAPI and returns to E4 before StartImage.
/// Cap alone is not enough: nested `4225b4d` still mapped 32 report-RAM
/// slots, freed them, then `load kernel=0x8200000`. See
/// [`report_ram_return_to_e4`]. Iron bit-31 clear still uses
/// [`GUEST_UEFI_RESUME_CAP`]. Armed product ISO does **not** use this cap.
pub const GUEST_UEFI_NESTED_RESUME_CAP: u32 = 65536;

/// Resume cap: iron lab stub 262144; nested lab stub 65536 (CI SHELL);
/// armed product ISO uses [`GUEST_UEFI_PRODUCT_ISO_RESUME_CAP`] on iron and
/// nested so QEMU `PRODUCT_ISO=` can pass OVMF StartImage. Lab `iso=0`
/// nested stays 65536.
pub fn guest_uefi_resume_cap(host_hypervisor: bool) -> u32 {
    if crate::devices::ide_cdrom::product_iso_window_armed() {
        GUEST_UEFI_PRODUCT_ISO_RESUME_CAP
    } else if host_hypervisor {
        GUEST_UEFI_NESTED_RESUME_CAP
    } else {
        GUEST_UEFI_RESUME_CAP
    }
}

/// After DXE evidence, spend this many exits unless firmware read an ATAPI
/// sector. Nested VT-x `1b07692`: BOTH-OK at n=1111 then the private VMCS
/// stopped with `sectors=0` — PciBus never reached PACKET.
pub const GUEST_UEFI_POST_DXE_TAIL: u32 = 32768;

/// Named Stage 44 window (32768). Stage 45 live stop does **not** apply this
/// after the first ATAPI sector — that READ is often LBA 0 dummy / PVD, and
/// BDS catalog + FatDxe + StartImage needs the 131072-exit cap plus the
/// 262144 hard resume (iron+nested hit 131072 still in BDS).
pub const GUEST_UEFI_POST_ATAPI_TAIL: u32 = 32768;

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

/// Exit qualification for `MOV CR8` (SDM: CR number in bits 3:0).
pub fn cr_access_is_cr8(qual: u64) -> bool {
    (qual & 0xf) == 8
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
/// 5: Linux `delay_loop` inner or outer `REX.W DEC rax; JNZ rel8`
///    (`48 FF C8 75 xx`, rel8 < 0). Nested `f1afc27` after leftover DRAM:
///    `rip=0xffffffffb7ae5940` `insn=48ffc875fb` `preempt noskip` (identity
///    peek empty on high-half). Skip-5 alone lands on `3: dec; jnz 1b`,
///    which re-enters the inner loop. Pair with
///    [`preempt_deadloop_delay_loop_sets_rax_one`] so `3:` falls through.
/// 10: inner `75 FB` plus outer `48 FF C8 75 xx` in one fetch — skip to the
///     compiler `ret`. Do not skip DEC alone (RAX unchanged → infinite
///     `jnz`).
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
    if let Some(n) = preempt_deadloop_delay_loop_skip_len(bytes) {
        return n;
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

/// Linux `arch/x86/lib/delay.c` `delay_loop`: inner `2: dec %rax; jnz 2b`
/// (`48 FF C8 75 FB`) then outer `3: dec %rax; jnz 1b`. Not `ISO-INSTALL-OK`.
pub fn preempt_deadloop_delay_loop_skip_len(bytes: &[u8]) -> Option<u8> {
    if bytes.len() < 5
        || bytes[0] != 0x48
        || bytes[1] != 0xFF
        || bytes[2] != 0xC8
        || bytes[3] != 0x75
        || (bytes[4] as i8) >= 0
    {
        return None;
    }
    if bytes.len() >= 10
        && bytes[4] == 0xFB
        && bytes[5] == 0x48
        && bytes[6] == 0xFF
        && bytes[7] == 0xC8
        && bytes[8] == 0x75
        && (bytes[9] as i8) < 0
    {
        return Some(10);
    }
    Some(5)
}

/// True when a delay_loop skip must leave `RAX=1` so `3: dec; jnz` falls
/// through instead of re-entering the inner loop. Not `ISO-INSTALL-OK`.
pub fn preempt_deadloop_delay_loop_sets_rax_one(bytes: &[u8]) -> bool {
    preempt_deadloop_delay_loop_skip_len(bytes).is_some()
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

/// Linux `cpuid_count(4, i)` terminator. Subleaf >= this returns type 0.
/// Not `ISO-INSTALL-OK`.
pub const GUEST_UEFI_CPUID_LEAF4_LAST_SUB: u32 = 4;
/// Cap CPUID.0 EAX so identify_cpu cannot walk a bogus max-leaf.
pub const GUEST_UEFI_CPUID_LEAF0_MAX: u32 = 0x1F;

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
        // Linux `intel_cacheinfo` loops `cpuid_count(4, i)` until EAX[4:0]=0.
        // If ECX is stale (MSR leftover `0xc0000101` on n=1) every probe
        // returns a live cache type and identify_cpu never leaves native_cpuid.
        // Iron COM2 a8b3547: ticks 437248/437504 reason=0xa same helper.
        4 => {
            let stale = subleaf >= GUEST_UEFI_CPUID_LEAF4_LAST_SUB;
            #[cfg(target_os = "uefi")]
            let too_many = PF_LINUX_DELIVER.load(Ordering::Acquire) != 0
                && LINUX_LEAF4.fetch_add(1, Ordering::AcqRel) >= GUEST_UEFI_CPUID_LEAF4_LAST_SUB;
            #[cfg(not(target_os = "uefi"))]
            let too_many = false;
            if stale || too_many {
                r.eax = 0;
                r.ebx = 0;
                r.ecx = 0;
                r.edx = 0;
            } else {
                r.eax &= !(0x3F << 26);
            }
        }
        7 if subleaf == 0 => {
            if r.eax > 1 {
                r.eax = 1;
            }
            r.ebx &= !((1 << 2) | (1 << 12) | (1 << 15));
            r.ebx &= !(CPUID_LEAF7_EBX_CLFLUSHOPT | CPUID_LEAF7_EBX_CLWB);
            r.ecx &= !(CPUID_LEAF7_ECX_TME_EN | CPUID_LEAF7_ECX_LA57);
        }
        7 if subleaf > 1 => {
            r.eax = 0;
            r.ebx = 0;
            r.ecx = 0;
            r.edx = 0;
        }
        0 => {
            if r.eax > GUEST_UEFI_CPUID_LEAF0_MAX {
                r.eax = GUEST_UEFI_CPUID_LEAF0_MAX;
            }
        }
        0x8000_0000 => {
            if r.eax > 0x8000_0008 {
                r.eax = 0x8000_0008;
            }
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

/// Linux `hypervisor_cpuid_base()` walks `[0x40000000, 0x40010000)` step `0x100`
/// (256 leaves per vendor: Xen, VMware, Hyper-V, KVM, …). Firmware still uses
/// [`guest_uefi_filter_cpuid`] so OVMF sees `KVMKVMKVM`. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_cpuid_leaf_is_hypervisor_scan(leaf: u32) -> bool {
    (GUEST_UEFI_KVM_CPUID_LEAF..GUEST_UEFI_KVM_CPUID_LEAF + 0x1_0000).contains(&leaf)
}

/// Last `base` in `hypervisor_cpuid_base` so the next `base += 0x100` exits.
/// Not `ISO-INSTALL-OK`.
pub const GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST: u32 =
    GUEST_UEFI_KVM_CPUID_LEAF + 0x1_0000 - 0x100;

/// Snap a callee-saved GPR that still holds the scan `base` (CPUID clobbers
/// EAX–EDX) to [`GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST`]. Zero leaves do not
/// exit the C loop; iron COM2 `90c85d5` still walked `n=256 leaf=0x4000bd00`
/// after `Loaded initrd` with HPET climbing. Firmware CPUID is unchanged.
///
/// alpine-virt 6.12.13 `hypervisor_cpuid_base.constprop.0` keeps `base` in
/// **EBX** and `native_cpuid` `push %rbx` before `0F A2` (iron RIP
/// `0xffffffffba081783` = KASLR of `ffffffff81081783`). The live RBX at
/// CPUID is the CPUID output (must stay 0); the loop copy is the 8-byte
/// slot at RSP. [`guest_uefi_linux_hypervisor_scan_bump_gpr`] also applies
/// to that stack word.
///
/// Match the **zero-extended** `u32` leaf only. Iron `73c2cab` logged
/// `hypervisor-scan bump leaf=0x40000000` then COM2 ended — a high-half
/// direct-map pointer `0xffff_8880_4000_0000` (GPA 1GiB) must not become
/// `0xffff_8880_4000_ff00`. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_hypervisor_scan_bump_gpr(leaf: u32, gpr: u64) -> u64 {
    if !guest_uefi_cpuid_leaf_is_hypervisor_scan(leaf) {
        return gpr;
    }
    if gpr != u64::from(leaf) {
        return gpr;
    }
    u64::from(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST)
}

/// Apply [`guest_uefi_linux_hypervisor_scan_bump_gpr`] to callee-saved GPRs.
/// Returns true when any register changed. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_hypervisor_scan_bump_gprs(leaf: u32, gprs: &mut [u64]) -> bool {
    let mut hit = false;
    for g in gprs.iter_mut() {
        let next = guest_uefi_linux_hypervisor_scan_bump_gpr(leaf, *g);
        if next != *g {
            *g = next;
            hit = true;
        }
    }
    hit
}

/// Linux `detect_hypervisor` ORs CPUID.1 ECX bit 31 then scans every
/// `0x4000_xx00` leaf per vendor. Iron COM2 after leftover+#PF: skip-2 works
/// (`rip=` / `n=16`/`32`/`64`) then `linux cpuid n=128 leaf=0x40003d00` and
/// `n=256 leaf=0x4000bd00` at frozen HPET -- looks dead, is an 8x256 walk.
/// Hide the hypervisor bit and return zeros in that range so `identify_cpu`
/// can leave `native_cpuid`. Firmware still uses [`guest_uefi_filter_cpuid`].
/// Iron `45aec97`: Linux printed `GenuineIntEl` unknown + `NX missing` then
/// froze at PAT — force GenuineIntel and restore NX on this path only.
/// Not `ISO-INSTALL-OK`.
pub fn guest_uefi_filter_cpuid_for_linux(leaf: u32, subleaf: u32) -> CpuidRegs {
    if guest_uefi_cpuid_leaf_is_hypervisor_scan(leaf) {
        return CpuidRegs {
            eax: 0,
            ebx: 0,
            ecx: 0,
            edx: 0,
        };
    }
    let mut r = guest_uefi_filter_cpuid(leaf, subleaf);
    if leaf == 1 {
        r.ecx &= !CPUID_ECX_HYPERVISOR;
    }
    if leaf == 0 {
        r.ebx = CPUID_GENUINEINTEL_EBX;
        r.edx = CPUID_GENUINEINTEL_EDX;
        r.ecx = CPUID_GENUINEINTEL_ECX;
    }
    if leaf == 0x8000_0001 {
        r.edx |= CPUID_80000001_EDX_NX;
        r.edx &= !CPUID_80000001_EDX_PAGE1GB;
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
/// CPUID.0 "GenuineIntel" (EBX `Genu`, EDX `ineI`, ECX `ntel`).
/// Iron `45aec97` printed `GenuineIntEl` unknown / generic init then PAT freeze.
pub const CPUID_GENUINEINTEL_EBX: u32 = 0x756e6547;
pub const CPUID_GENUINEINTEL_EDX: u32 = 0x49656e69;
pub const CPUID_GENUINEINTEL_ECX: u32 = 0x6c65746e;
/// IA32_EFER.NXE (SDM 2.2.1 bit 11). Firmware CpuDxe paging refresh ORs
/// `EFI_MEMORY_XP` into GCD when NXE=1, then `ASSERT_EFI_ERROR` on
/// `SetMemorySpaceAttributes`. Linux after high-half `#PF` keeps NXE.
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
/// CPUID.7.0 EBX.CLFLUSHOPT (bit 23) / CLWB (bit 24). Nested KVM #UD at
/// `66 0F AE F1` while host CPUID still advertises them (CI `34b5767`).
pub const CPUID_LEAF7_EBX_CLFLUSHOPT: u32 = 1 << 23;
pub const CPUID_LEAF7_EBX_CLWB: u32 = 1 << 24;
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
/// Product ISO retains extra 2 MiB WB slots (installer RAM); `iso=0` stays 32.
pub const GUEST_UEFI_REPORT_RAM_SLOTS: usize = 32;
/// Extra report-RAM slots when PRE-EBS retained a window-sized ISO.
/// `[32MiB, 2GiB)` is 1008×2 MiB; `iso=0` still allocates only 32.
/// Iron and nested product-ISO fill extras from leftover DRAM above PRECISE
/// (not invented HPA). Nested `iso=0` does not seed leftover (E4 SHELL).
pub const GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA: usize = 976;
const REPORT_RAM_ARRAY: usize =
    GUEST_UEFI_REPORT_RAM_SLOTS + GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA;

fn report_ram_slots_alloc() -> usize {
    if crate::mgmt::iso_install::product_iso_retained_bytes().is_some() {
        REPORT_RAM_ARRAY
    } else {
        GUEST_UEFI_REPORT_RAM_SLOTS
    }
}
pub const GUEST_UEFI_REPORT_RAM_PAGE: u64 = 0x20_0000;
const _: () = assert!(
    (GUEST_UEFI_REPORT_RAM_SLOTS + GUEST_UEFI_REPORT_RAM_PRODUCT_EXTRA) as u64
        * GUEST_UEFI_REPORT_RAM_PAGE
        == crate::devices::guest_platform::PLATFORM_REPORT_RAM_BYTES
            - crate::devices::guest_platform::PLATFORM_RAM_BYTES
);
/// Iron `fad19b2` first unbacked report-RAM GPA (top of 2 GiB LowMemory).
pub const GUEST_UEFI_IRON_REPORT_RAM_GPA: u64 = 0x7BDD_D000;
/// Iron `32e7d46`: after lazy WB map, CpuDeadLoop at top of LowMemory
/// (`reason=0x34` `same=376` `lastmsr=0x23f` `insn=` empty — 32 MiB peek).
pub const GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP: u64 = 0x7F8E_21CA;
/// Iron `957e0ad`: peek showed `insn=ebecc9c3` (noskip) and the same
/// CpuDxe ASSERT offset as `0x1d25193`, relocated into report-RAM.
pub const GUEST_UEFI_IRON_ASSERT_CALLER_RIP: u64 = 0x7FD2_5193;
/// 32-byte window immediately before a `call` RIP. Iron `96ef961`
/// `prehex=` 16 bytes at CpuDxe `ret-16` started at `mov word`
/// / `jmp` / `call DebugAssert`. Iron `6f077a3`: 32-byte `prehex=` at
/// `callerrip=0x7fd25193` is DxeCore `call [rax+0x20]`; `rax=0` is
/// leftover (EntryPoint never returned). Keep PEI i440FX `0x1237` at
/// `00:00.0`. DXE virtio `0x1042` at `00:02.0`. Keep FIX WB hold. No
/// slot-0 DID flip. No new PAT-UC.
pub const GUEST_UEFI_ASSERT_PREHEX_BYTES: usize = 32;

/// GPA of the 32-byte instruction window immediately before `rip`.
pub fn guest_uefi_assert_prehex_gpa(rip: u64) -> u64 {
    rip.wrapping_sub(GUEST_UEFI_ASSERT_PREHEX_BYTES as u64)
}

/// Iron `6f077a3`: 32-byte `prehex=` at `0x7fd25193` is DxeCore
/// `CoreStartImage` `call [rax+0x20]` (EntryPoint). `rax=0` is leftover
/// (CpuDxe never returned). `retpre=` is a CpuDxe switch: `mov word`
/// `0x0600` / `0xB000` then `jmp` over `call DebugAssert` — default is
/// `ASSERT(FALSE)`, not `ASSERT_EFI_ERROR`. Dump 32 bytes at `ret-64`
/// (`retcmp=`) plus `rbx`/`rsi`/`rdi` for the cmp. Iron `2cbf9e8`:
/// `cmp ax, 0x1237` / `0x29C0` / `0x0D57` is `AcpiTimerLibConstructor`
/// (`PIIX4_PMBA_VALUE` `0xB000` / `ICH9_PMBASE_VALUE` `0x0600`).
/// `00:00.0` stays i440FX. Keep FIX WB hold. No new PAT-UC.
pub fn guest_uefi_assert_retcmp_gpa(ret: u64) -> u64 {
    ret.wrapping_sub((GUEST_UEFI_ASSERT_PREHEX_BYTES * 2) as u64)
}

/// GPA of the UINT16 the `66 C7 05 disp32 imm16` at `ret-16` stores.
/// Iron `6f077a3`: `ret=0x7f8e2946` `disp=0x8B3` → `0x7f8e31F2`.
pub fn guest_uefi_assert_retpre_word_gpa(ret: u64, disp: u32) -> u64 {
    ret.wrapping_sub(7).wrapping_add(u64::from(disp))
}
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
/// 4 K PAT-UC leaf (PWT+PCD, PAT index 3). Iron `ddbd866` `pte_a0000=0xa0067`
/// was WB vs coerced VGA FIX UC (`mtrr259=0x0`). Iron `e368e86` painted
/// `pte_c0000=0xc007f` too; option-ROM stays WB (`guest_uefi_pt_leaf_4k_for`).
pub const GUEST_UEFI_PT_LEAF_4K_UC: u64 = GUEST_UEFI_PT_LEAF_4K | GUEST_UEFI_PT_PWT | GUEST_UEFI_PT_PCD;
/// Iron `ddbd866` GPA0 identity WB at VGA (MTRR FIX already UC).
pub const GUEST_UEFI_IRON_PTE_A0000_WB: u64 = 0xA_0067;
/// Iron `d6b012a` `filehex` at `rcx=0x7ee68fa0`: CpuFlush only WBINVD when
/// `FlushType==0`, else `mov rax, EFI_UNSUPPORTED` (`0x8000000000000003`).
/// `test r9d; jnz +4; wbinvd; jmp; mov rax, UNSUPPORTED`. Iron `f0781bb`
/// noped that `jnz` (`n=2` `9090`) and still ASSERTed `ebecc9c3`. Do not
/// skip `ebecc9c3`.
pub const GUEST_UEFI_CPU_FLUSH_UNSUPPORTED: &[u8] = &[
    0x45, 0x85, 0xC9, 0x75, 0x04, 0x0F, 0x09, 0xEB, 0x12, 0x48, 0xB8, 0x03, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
];
/// Offset of `jnz +4` (`75 04`) inside [`GUEST_UEFI_CPU_FLUSH_UNSUPPORTED`].
pub const GUEST_UEFI_CPU_FLUSH_JNZ_OFF: usize = 3;
/// Iron `d6b012a` decompressed CpuFlush in report-RAM.
pub const GUEST_UEFI_IRON_CPU_FLUSH_GPA: u64 = 0x7EE6_8FA0;
/// OVMF GCD heap floor for CpuFlush scans (128 MiB below 2 GiB).
/// Iron `f0eb84e`: pre-map `n=1008` then tick `n=256` scanned ~2 GiB of
/// leftover DRAM for `jnz` (byte walk) and COM2 died. CpuFlush lives
/// here (`0x7EE68FA0`). Keep EPT pre-map leaves. iso=0 stays lazy.
/// cpu_flush skip leftover pre-map.
pub const GUEST_UEFI_CPU_FLUSH_HEAP_GPA: u64 = 0x7800_0000;
const _: () = assert!(GUEST_UEFI_IRON_CPU_FLUSH_GPA >= GUEST_UEFI_CPU_FLUSH_HEAP_GPA);
/// Leftover 2 MiB slots byte-scanned per CpuFlush walk (high GPA first).
/// Iron `abfb008`: skip `n=944` still scanned 64 heap leftover slots
/// (~128 MiB) on tick `n=256` and hung. cpu_flush leftover per walk.
pub const GUEST_UEFI_CPU_FLUSH_LEFTOVER_PER_WALK: u32 = 2;

/// Nop `jnz` so CpuFlush WBINVD for every FlushType (EFI_UNSUPPORTED → SUCCESS).
///
/// Iron `d6b012a`: `pte_a0000=0xa0067` is GPA0 identity WB, firmware FIX
/// MTRRs are `0x06` WB — not a GCD VGA punch. `filehex` is this stub.
/// Pattern is LZMA-compressed in `OVMF.fd`; patch live report-RAM.
/// Iron `f0781bb`: first mapped slot had `n=2`; CpuDxe at `0x7f8e1000` is
/// a later slot — scan every mapped copy.
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

/// Count unpatched CpuFlush `test r9d; jnz` stubs (iron `f0781bb`).
pub fn guest_uefi_count_cpu_flush_jnz(buf: &[u8]) -> u32 {
    let pat = GUEST_UEFI_CPU_FLUSH_UNSUPPORTED;
    let mut n = 0u32;
    let mut i = 0usize;
    while i.saturating_add(pat.len()) <= buf.len() {
        if &buf[i..i + pat.len()] == pat {
            n = n.saturating_add(1);
            i = i.saturating_add(pat.len());
        } else {
            i = i.saturating_add(1);
        }
    }
    n
}

/// Skip leftover/pre-mapped report-RAM CpuFlush walks that hung iron `f0eb84e`.
///
/// INVARIANTS:
/// - GPA below [`GUEST_UEFI_LOW_RAM_BYTES`] is never skipped (identity slab)
/// - After `patched`, every report-RAM GPA is skipped
/// - Unpatched leftover is not skipped here; the walk caps
///   [`GUEST_UEFI_CPU_FLUSH_LEFTOVER_PER_WALK`] (iron `abfb008` 64×2 MiB hang)
///
/// VERIFICATION: L1 (host tests)
/// Keep EPT pre-map leaves. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_cpu_flush_skip_mapped(gpa: u64, patched: bool) -> bool {
    if gpa < GUEST_UEFI_LOW_RAM_BYTES {
        return false;
    }
    patched
}

/// Product-ISO pre-map tick must not byte-scan leftover (iron `abfb008`).
///
/// iso=0 stays on the tick cadence (`maps<=32`). cpu_flush skip leftover
/// pre-map on tick.
pub fn guest_uefi_cpu_flush_tick_scans_mapped(maps: u32) -> bool {
    maps <= GUEST_UEFI_REPORT_RAM_SLOTS as u32
}

/// Product-ISO Linux is in the private VMCS (share, high-half RIP, or a
/// delivered `#PF`). iso=0 never latches share. linux unhandled nowait stop.
pub fn guest_uefi_linux_guest_active(share: bool, high_half: bool, pf_delivered: bool) -> bool {
    share || high_half || pf_delivered
}

/// Skip an unhandled VM-exit when Linux is active and VMCS length is 1–15.
///
/// Iron `1a2544d`: PAT / initrd then `restore host xcr0` with no
/// `guest-UEFI stop n=` (share hushes HV `write_str`). MOV DR / GDTR /
/// other mandatory exits must not drop to E4 hold. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_unhandled_should_skip(linux: bool, insn_len: u64) -> bool {
    linux && insn_len >= 1 && insn_len <= 15
}

/// Try skip on unhandled Linux exits, including VMCS length 0 (high-half
/// fetch via [`skip_insn`] / [`guest_uefi_linux_fixed_skip_len`]).
/// Triple-fault and VM-entry failure still stop. linux MOV DR skip.
pub fn guest_uefi_linux_unhandled_try_skip(linux: bool, insn_len: u64, reason: u32) -> bool {
    if !linux {
        return false;
    }
    if reason == crate::vmx::fields::EXIT_REASON_TRIPLE_FAULT
        || reason == crate::vmx::fields::EXIT_REASON_VMENTRY_GUEST_STATE
        || reason == crate::vmx::fields::EXIT_REASON_VMENTRY_MSR_LOAD
    {
        return false;
    }
    guest_uefi_linux_unhandled_should_skip(linux, insn_len) || insn_len == 0
}

/// Hardware exceptions that push an error code (SDM 6.13).
pub fn guest_uefi_linux_exc_error_code(vec: u8) -> bool {
    matches!(vec, 8 | 10 | 11 | 12 | 14 | 17)
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

/// True when a virtio BAR sits in the 2 MiB UC scratch at [`GUEST_UEFI_IRON_MMIO_SCRATCH_GPA`].
///
/// Iron `df0c118`: Linux assigned `00:02.0`/`00:03.0` BAR0 to
/// `0x80001000`/`0x80000000` (PCI hole start). That 2 MiB was already a
/// present scratch leaf for firmware PT stores (`b25d75b`), so virtio MMIO
/// was RAM and `handle_virtio_bar_ept` never ran (`Freeing initrd` then
/// silent; no `stop n=`). virtio BAR trap over scratch.
/// iso=0 default BAR stays `0xFE000000` (unmapped). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_virtio_bar_overlaps_scratch(bar: u64) -> bool {
    bar != 0 && (bar & !0x1F_FFFF) == GUEST_UEFI_IRON_MMIO_SCRATCH_GPA
}

/// 4 KiB-aligned programmed BAR that should become an EPT trap.
pub fn guest_uefi_virtio_bar_should_trap(bar: u64) -> bool {
    bar != 0
        && (bar & 0xfff) == 0
        && bar != u64::from(crate::devices::guest_virtio_blk::GUEST_VIRTIO_BAR0_SIZE_MASK)
}

/// Linux `virtio_reset` / `msleep(1)` needs PIT IRQ 0. `idle=poll` never
/// HLT; VMX preempt is the only PIT source and is starved by tight BAR EPT.
/// Raise PIT on each Linux virtio MMIO so jiffies can move. iso=0 does not.
/// virtio MMIO raises PIT. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_virtio_mmio_raises_pit(linux: bool, product_iso: bool) -> bool {
    linux && product_iso
}

/// Linux `clocksource=tsc` still uses the LAPIC timer for `jiffies` /
/// `msleep`. Tight virtio BAR EPT resets the VMX preemption timer, so
/// `poll_timer_expiry` on HLT/preempt never runs. Poll on each Linux
/// product-ISO virtio MMIO. iso=0 does not.
/// virtio MMIO polls lapic. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_virtio_mmio_polls_lapic(linux: bool, product_iso: bool) -> bool {
    linux && product_iso
}

/// VM-entry injects PIC IRQ 0 before leftover IOAPIC→LAPIC **unless**
/// Linux has programmed MADT GSI 2 (ACPI timer). PIC-first while GSI 2 is
/// armed injects vector 0x20 into an IOAPIC IDT. linux PIC before LAPIC.
/// linux GSI 2 before PIC. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_pic_before_lapic(pic_ready: bool, gsi2_armed: bool) -> bool {
    pic_ready && !gsi2_armed
}

/// Linux programmed IOAPIC pin 2 (MADT IRQ0 ISO). Prefer that path.
/// linux GSI 2 before PIC. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_gsi2_before_pic(gsi2_armed: bool) -> bool {
    gsi2_armed
}

/// Linux PIC ICW2 0x20 + IRQ 0. COM2 one-shot through earlycon hush.
/// linux PIC IRQ0. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_pic_irq0_vec(vec: u32) -> bool {
    vec == 0x20
}

/// PIT is PIC IRQ 0 + MADT GSI 2, never IOAPIC pin 0 (OVMF leftover RTE).
/// PIT skips IOAPIC pin 0. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_pit_skips_ioapic_pin0() -> bool {
    true
}

/// Do not raise PIT on general Linux I/O / CPUID / MSR / EPT / PAUSE.
///
/// Iron `bc6fb70`: `APIC: ACPI MADT or MP tables are not detected` then
/// `restore host xcr0` (PIC ICW2 + PIT IRR injects vector 0x20 before IDT
/// is ready). `4b0d96a` reached `Freeing initrd` without this flood.
/// virtio MMIO / HLT / VMX preemption still raise. iso=0 does not.
/// linux I/O does not raise PIT (iron MADT stop). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_io_raises_pit(linux: bool, product_iso: bool) -> bool {
    let _ = (linux, product_iso);
    false
}

/// OVMF CpuDeadLoop skip stays. Linux `pause` / `delay_loop` must run so
/// `msleep` and `poll_idle` are not rewritten. iso=0 firmware still skips.
/// linux preempt deadloop noskip. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_preempt_deadloop_noskip(linux: bool, product_iso: bool) -> bool {
    linux && product_iso
}

/// GPA in the 2 GiB LowMemory lie that launch does not identity-map.
///
/// INVARIANTS:
/// - Iron `fad19b2` `0x7bddd000` is true
/// - `[0, 32MiB)` and `[2GiB, …)` are false
pub fn guest_uefi_report_ram_should_map(gpa: u64) -> bool {
    crate::devices::guest_platform::is_unbacked_report_ram_gpa(gpa)
}

/// `rep insw` / string INS into reported LowMemory that is not the 32 MiB
/// identity slab. The guest never EPT-walks that GPA; the emulator writes
/// host-side. An unmapped slot used to drop the FIFO bytes (zeros → EFI
/// stub `uncompression error`). Virtqueue already lazy-maps via
/// [`guest_uefi_gpa_to_hpa`]; string I/O must do the same.
///
/// INVARIANTS:
/// - Iron `fad19b2` `0x7bddd000` is true
/// - Low 32 MiB identity is false (already backed)
pub fn guest_uefi_string_ins_needs_report_ram_map(linear: u64) -> bool {
    linear >= GUEST_UEFI_LOW_RAM_BYTES && guest_uefi_report_ram_should_map(linear)
}

/// 2 MiB-align a report-RAM GPA. Iron `fad19b2`: `0x7bddd000` → `0x7BC00000`.
pub fn guest_uefi_report_ram_gpa_2m(gpa: u64) -> u64 {
    gpa & !(GUEST_UEFI_REPORT_RAM_PAGE - 1)
}

/// Product ISO pre-maps leftover-backed report-RAM EPT at launch.
///
/// Iron `113a08a`: hush-on-bootimg + LSR pacing printed a complete PAT
/// line, then COM2 went quiet. Linux `init_mem_mapping` after
/// `pat_bp_init` walks ~2 GiB; each first touch was an EPT miss, and an
/// EPT cap stop is hushed. Not GPA=HPA identity (`89c3731`). iso=0 stays
/// lazy. report-RAM EPT pre-map. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_report_ram_should_premap(product_iso: bool) -> bool {
    product_iso
}

/// GPA for report-RAM slot `i` covering `[32MiB, 2GiB)` in order.
///
/// INVARIANTS:
/// - Slot 0 is [`crate::devices::guest_platform::PLATFORM_RAM_BYTES`]
/// - Last valid slot is 2 MiB below
///   [`crate::devices::guest_platform::PLATFORM_REPORT_RAM_BYTES`]
/// - `iso=0` (`n==32`) only covers `[32MiB, 96MiB)`
///
/// VERIFICATION: L1 (host tests)
pub fn guest_uefi_report_ram_premap_gpa(slot: usize, n: usize) -> Option<u64> {
    if slot >= n {
        return None;
    }
    let gpa = crate::devices::guest_platform::PLATFORM_RAM_BYTES.saturating_add(
        (slot as u64).saturating_mul(GUEST_UEFI_REPORT_RAM_PAGE),
    );
    if gpa < crate::devices::guest_platform::PLATFORM_REPORT_RAM_BYTES {
        Some(gpa)
    } else {
        None
    }
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
        let gpa = i * 4096;
        let leaf = guest_uefi_pt_leaf_4k_for(gpa);
        if poke(pt_gpa.wrapping_add(i * 8), leaf) {
            n = n.saturating_add(1);
        }
    }
    if poke(pd, pt_gpa | GUEST_UEFI_PT_TABLE) {
        n = n.saturating_add(1);
    }
    n
}

/// Iron `e368e86` PAT-UC `[0xA0000, 1MiB)` (`pte_a0000=0xa007f`) and
/// still ASSERTed. Iron `fd041bb` left `C0000` WB; CpuDxe then UC'd
/// `mtrr268`. GCD SystemMemory is WB-only — forcing UC is the failure.
/// Always false: GPA0 4K leaves stay WB (`0x67`).
pub fn guest_uefi_gpa_in_vga_fix_uc(_gpa: u64) -> bool {
    false
}

/// 4 K identity leaf: PAT-UC in the VGA FIX hole, WB elsewhere.
pub fn guest_uefi_pt_leaf_4k_for(gpa: u64) -> u64 {
    let f = if guest_uefi_gpa_in_vga_fix_uc(gpa) {
        GUEST_UEFI_PT_LEAF_4K_UC
    } else {
        GUEST_UEFI_PT_LEAF_4K
    };
    (gpa & !0xFFFu64) | f
}

/// Paint live CR3 GPA0 4 K VGA framebuffer leaves to PAT-UC.
/// Iron `ddbd866`: `pte_a0000=0xa0067` WB vs coerced FIX UC.
/// Iron `e368e86`: `pte_a0000=0xa007f` `pte_c0000=0xc007f` still ASSERT
/// `calltgt=0x7f8e21a5` (DebugAssert prologue). Paint `[0xA0000, 0xC0000)`
/// only. Do **not** skip `ebecc9c3`.
pub fn guest_uefi_pt_paint_vga_uc<Peek, Poke>(peek: Peek, poke: Poke, cr3: u64) -> u32
where
    Peek: Fn(u64) -> u64,
    Poke: Fn(u64, u64) -> bool,
{
    if cr3 == 0 {
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
    if (e2 & GUEST_UEFI_PT_PRESENT) == 0 || (e2 & GUEST_UEFI_PT_LARGE) != 0 {
        return 0;
    }
    let pt = e2 & GUEST_UEFI_PT_ADDR_MASK;
    if pt == 0 {
        return 0;
    }
    let mut n = 0u32;
    let mut gpa = crate::vmx::guest_pt::IDENTITY_VGA_A0000;
    while gpa < crate::vmx::guest_pt::IDENTITY_VGA_C0000 {
        let i = gpa >> 12;
        let slot = pt.wrapping_add(i * 8);
        let want = guest_uefi_pt_leaf_4k_for(gpa);
        let e = peek(slot);
        if e != want && poke(slot, want) {
            n = n.saturating_add(1);
        }
        gpa = gpa.wrapping_add(4096);
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
/// `pci_ide=0` (never `identity SPLIT n=2`). ATA data-register FIFOs
/// (`IoReadFifo16` IDENTIFY / PACKET), including BAR-relocated command
/// blocks. Status/control ports are not FIFOs.
///
/// Iron COM2 after product-ISO ACPI files: Linux `efi:` had no `ACPI=`
/// and `ACPI: OSL: System description tables not found` because OVMF
/// `QemuFwCfgInitialize` is `IoReadFifo8` (`rep insb` from `0x511`).
/// One-shot + `skip_insn` left the signature buffer as zeros, so
/// `mQemuFwCfgSupported=FALSE` and `InstallQemuFwCfgTables` never
/// selected `etc/table-loader`. fw_cfg IoReadFifo8 fills RAM. Dest
/// overlapping HV identity PML4 (`0x205f18`) is still skipped.
pub fn guest_uefi_io_string_fills_ram(port: u16) -> bool {
    crate::devices::ide_cdrom::is_ata_data_port(port)
        || guest_uefi_fwcfg_string_fills_ram(port)
}

/// OVMF `IoReadFifo8` / `QemuFwCfgReadBytes` from fw_cfg data `0x511`.
/// fw_cfg IoReadFifo8. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_fwcfg_string_fills_ram(port: u16) -> bool {
    crate::devices::guest_platform::is_fwcfg_data_port(port)
}

/// Do not DMA fw_cfg FIFO bytes onto the HV identity PML4 (iron `1e0f4a7`
/// `cr2=0x205f18`). skip HV identity PML4 dest. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_io_string_dest_ok(linear: u64) -> bool {
    let start = crate::devices::guest_platform::HV_IDENTITY_PML4;
    let end = start.saturating_add(crate::devices::guest_platform::HV_IDENTITY_PML4_BYTES);
    linear < start || linear >= end
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

/// Iron `e40bee0`: extra DRAM `pool=1008 extra=846 no-zero` then
/// `Loaded initrd` then long-mode `#PF` `rip=0xffffffffbee19755`
/// `cr2=0xffff88807e2a3000` `err=0` `cr3=0xdeee000`. OVMF identity
/// fixup does not apply. Not `ISO-INSTALL-OK`.
pub const GUEST_UEFI_IRON_LINUX_PF_CR2: u64 = 0xffff_8880_7e2a_3000;
pub const GUEST_UEFI_IRON_LINUX_PF_RIP: u64 = 0xffff_ffff_bee1_9755;
/// Iron COM2 after `#PF linux deliver` `err=0x0`: `native_cpuid` helper.
pub const GUEST_UEFI_IRON_LINUX_CPUID_RIP: u64 = 0xffff_ffff_8408_1783;
/// Linux 4-level `PAGE_OFFSET` direct map. Not a sign-extended 32-bit hole.
pub const GUEST_UEFI_LINUX_DIRECT_MAP: u64 = 0xffff_8880_0000_0000;
pub const GUEST_UEFI_LINUX_DIRECT_MAP_MASK: u64 = 0xffff_fff0_0000_0000;

/// Linux 4-level direct-map CR2 (`0xffff8880_…`).
pub fn guest_uefi_pf_is_linux_direct_map(cr2: u64) -> bool {
    (cr2 & GUEST_UEFI_LINUX_DIRECT_MAP_MASK) == GUEST_UEFI_LINUX_DIRECT_MAP
}

/// High-half RIP: Linux kernel, not OVMF identity (`0x7ee…` / `0x3xxxxx`).
///
/// Deliver `#PF` to the guest IDT (`early_make_pgtable`) instead of
/// stopping or rebuilding SEC page tables.
pub fn guest_uefi_pf_should_deliver_to_guest(rip: u64) -> bool {
    (rip & (1u64 << 63)) != 0
}

/// SDM 24.8.3: VM-entry interruption type = NMI.
pub const GUEST_UEFI_INTR_TYPE_NMI: u32 = 2;
/// SDM 24.8.3: VM-entry interruption type = hardware exception.
pub const GUEST_UEFI_INTR_TYPE_HW_EXCEPTION: u32 = 3;
/// SDM 24.8.3: deliver error code with the injected exception.
pub const GUEST_UEFI_INTR_DELIVER_CODE: u32 = 1 << 11;
/// SDM 24.8.3: valid bit in VM-entry interruption-information.
pub const GUEST_UEFI_INTR_INFO_VALID: u32 = 1 << 31;
/// Packed VM-entry interruption-info for guest `#PF` (vector 14).
///
/// Drop exception-bitmap bit 14 **before** this inject: an injected `#PF`
/// that is still intercepted immediately VM-exits (SDM 26.5).
pub const GUEST_UEFI_LINUX_PF_ENTRY_INFO: u32 = 14
    | (GUEST_UEFI_INTR_TYPE_HW_EXCEPTION << 8)
    | GUEST_UEFI_INTR_DELIVER_CODE
    | GUEST_UEFI_INTR_INFO_VALID;

/// Packed VM-entry interruption-info for a guest NMI (vector 2, type 2).
///
/// Pin-based NMI exiting delivers `reason=0` `vec=2`. Inject as NMI, not a
/// hardware exception (type 3 would #UD-entry). linux NMI inject.
pub fn guest_uefi_nmi_entry_info() -> u32 {
    2 | (GUEST_UEFI_INTR_TYPE_NMI << 8) | GUEST_UEFI_INTR_INFO_VALID
}

/// Product-ISO Linux: re-inject a pin-exited NMI. iso=0 firmware resumes.
pub fn guest_uefi_linux_nmi_should_inject(linux: bool, vec: u8) -> bool {
    linux && vec == 2
}

/// virtio MMIO eax fallback (iron `1a2544d` Freeing initrd then xcr0).
/// `virtio_pci` is a `device_initcall` after `populate_rootfs`; decode fail
/// stopped the private VMCS (IOAPIC skips; xAPIC EAX-fallbacks). Same skip
/// window as [`xapic_fetch_miss_eax_fallback`]. iso=0 decode fail still stops
/// so E4 SHELL is not starved of leftover DRAM (nested `1a4b687` `/init`
/// SIGSEGV `exitcode=0xb` CR2 `ffff888000000413`). Empty peek + VMCS len 0
/// still EAX-fallbacks with skip 3 on Linux only (linux EAX fallback skip 3).
pub fn virtio_mmio_eax_fallback(linux: bool, fetched_n: usize, insn_len: u64) -> bool {
    virtio_mmio_eax_fallback_len(linux, fetched_n, insn_len) != 0
}

/// When VMCS/effective length is 0 or longer than the peek, decode with
/// `min(fetched, 15)` so Linux `movl mem, %reg` (`"=r"` not `"=a"`) is not
/// an EAX guess. A 16-byte peek still decodes with cap 15. linux MMIO decode retry.
pub fn virtio_mmio_retry_decode_len(fetched_n: usize, insn_len: u64) -> u64 {
    if insn_len >= 1 && insn_len <= 15 && insn_len <= fetched_n as u64 {
        0
    } else {
        let cap = fetched_n.min(15) as u64;
        if cap >= 1 {
            cap
        } else {
            0
        }
    }
}

/// Linux EAX fallback skip length after decode still fails.
///
/// Prefer a valid VMCS 1–15, else fetched 1–15 (not a 16-byte peek), else
/// skip 3 when the peek is empty (`movl r/m32, r32`). iso=0 stays 0.
pub fn virtio_mmio_eax_fallback_len(linux: bool, fetched_n: usize, insn_len: u64) -> u64 {
    if !linux {
        return 0;
    }
    if insn_len >= 1 && insn_len <= 15 {
        insn_len
    } else if fetched_n >= 1 && fetched_n <= 15 {
        fetched_n as u64
    } else if fetched_n == 0 && (insn_len == 0 || insn_len > 15) {
        3
    } else {
        0
    }
}

/// Access width for Linux EAX fallback (virtio-pci packed common cfg).
///
/// Status `0x14` is a byte. A 32-bit store of stale EAX after a failed
/// `movb $0` leaves `virtio_reset` spinning on nonzero status.
/// virtio MMIO eax fallback size. Not `ISO-INSTALL-OK`.
pub fn virtio_mmio_eax_fallback_size(off: u16) -> u8 {
    match off {
        0x10 | 0x12 | 0x16 | 0x18 | 0x1A | 0x1C | 0x1E => 2,
        0x14 | 0x15 => 1,
        0x100 => 1,
        _ => 4,
    }
}

/// Pack VM-entry interruption-info for a hardware exception.
pub fn guest_uefi_hw_exception_entry_info(vector: u8, deliver_code: bool) -> u32 {
    let mut info = u32::from(vector)
        | (GUEST_UEFI_INTR_TYPE_HW_EXCEPTION << 8)
        | GUEST_UEFI_INTR_INFO_VALID;
    if deliver_code {
        info |= GUEST_UEFI_INTR_DELIVER_CODE;
    }
    info
}

/// Pack VM-entry interruption-info for a Linux `#PF` inject.
pub fn guest_uefi_linux_pf_entry_info() -> u32 {
    guest_uefi_hw_exception_entry_info(14, true)
}

/// After high-half Linux takes over, use G0's bitmap: do not intercept
/// `#PF` / `#UD` / `#GP` (M3.10 `#UD` at serial8250). Keep `#DF`.
pub fn guest_uefi_linux_exception_bitmap() -> u32 {
    crate::vmx::fields::LINUX_EXCEPTION_BITMAP
}

/// PIC/LAPIC must not steal `VM_ENTRY_INTERRUPTION_INFO` while a Linux
/// exception inject is pending (that would clobber CR2 in the IRQ handler).
pub fn guest_uefi_linux_pf_blocks_irq(pending_cr2: u64) -> bool {
    pending_cr2 != 0
}

/// Same as [`guest_uefi_linux_pf_blocks_irq`] plus a non-#PF inject (e.g. `#UD`).
pub fn guest_uefi_linux_exc_blocks_irq(pending_cr2: u64, inject: bool) -> bool {
    pending_cr2 != 0 || inject
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

pub fn guest_uefi_cpuid_is_genuine_intel(ebx: u32, edx: u32, ecx: u32) -> bool {
    ebx == CPUID_GENUINEINTEL_EBX
        && edx == CPUID_GENUINEINTEL_EDX
        && ecx == CPUID_GENUINEINTEL_ECX
}

/// Linux after high-half `#PF` / high-half RIP may keep EFER.NXE.
/// Firmware always strips it (CpuDxe `EFI_MEMORY_XP` ASSERT).
pub fn guest_uefi_efer_allow_nx(linux: bool) -> bool {
    linux
}

/// Keep EFER.LMA consistent with LME && CR0.PG (SDM 9.8.5).
/// Firmware strips NXE so CpuDxe paging refresh does not advertise
/// `EFI_MEMORY_XP`. Linux (`allow_nx`) keeps NXE so PAT / kernel maps
/// are not `NX missing` (iron `45aec97`).
pub fn guest_uefi_efer_with_lma_allow_nx(efer: u64, paging: bool, allow_nx: bool) -> u64 {
    let efer = if allow_nx {
        efer
    } else {
        efer & !GUEST_UEFI_EFER_NXE
    };
    if (efer & GUEST_UEFI_EFER_LME) != 0 && paging {
        efer | GUEST_UEFI_EFER_LMA
    } else {
        efer & !GUEST_UEFI_EFER_LMA
    }
}

/// Keep EFER.LMA consistent with LME && CR0.PG (SDM 9.8.5).
/// Strip NXE so CpuDxe paging refresh does not advertise `EFI_MEMORY_XP`.
pub fn guest_uefi_efer_with_lma(efer: u64, paging: bool) -> u64 {
    guest_uefi_efer_with_lma_allow_nx(efer, paging, false)
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
/// Stage 44 live stop. Stage 45 live stop is [`post_atapi_should_stop`] (do not
/// stop on the first sector).
///
/// INVARIANTS:
/// - `false` until DXE printed (PEI still needs the full resume cap)
/// - `true` as soon as DXE printed **and** `sectors > 0` (honest PACKET READ)
/// - `true` after `GUEST_UEFI_POST_DXE_TAIL` exits past the DXE print
/// - both PCI enums alone do **not** stop (Stage 43 `1b07692` n=1111 BOTH then
///   stopped with `sectors=0`; firmware never issued PACKET)
///
/// Nested VT-x: PEI only `inw` DID of `00:00.0` (i440FX). IDE is fn1 `00:00.1`.
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

/// Honest El Torito boot: catalog READ + boot-image READ + CD EFI ran (COM magic).
/// Do not fake. `sectors>0` alone is Stage 44 ATAPI-OK, not El Torito.
pub fn eltorito_boot_evidence(catalog: bool, boot_image: bool, payload_ran: bool) -> bool {
    catalog && boot_image && payload_ran
}

/// Step the CD-EFI COM matcher. Host tests + firmware UART share this.
pub fn eltorito_com_match_step(matched: u8, byte: u8) -> u8 {
    let mag = crate::devices::ide_cdrom::ELTORITO_PAYLOAD_MAGIC;
    let i = matched as usize;
    if i < mag.len() && byte == mag[i] {
        matched.saturating_add(1)
    } else if byte == mag[0] {
        1
    } else {
        0
    }
}

/// True when the matcher has seen the full [`ELTORITO_PAYLOAD_MAGIC`] sequence.
pub fn eltorito_payload_ran(matched: u8) -> bool {
    (matched as usize) >= crate::devices::ide_cdrom::ELTORITO_PAYLOAD_MAGIC.len()
}

/// When to print a guest-UEFI tick on COM2.
///
/// Dense through BOTH/ATAPI (`n<=16384`), then every 4096 so RN-ELT stays
/// readable. After El Torito bootimg, every 1024 so EFI stub / kernel
/// ExitBootServices is not a 4096-exit blind spot (iron COM2 `Loaded initrd`
/// with no further tick). After Linux high-half `#PF` deliver, every 4096
/// (iron `115e5ee` every-256 UART ticks split `Linux version` / PAT) unless
/// `share` — linux earlycon quiet ticks so HV tick lines do not fill the
/// guest UART TX ring (iron `202312f` `n=438272` interleaved e820; `9a3cbfa`
/// still printed ticks into the same FIFO as printk). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_tick_should_print(n: u32, bootimg: bool, linux: bool, share: bool) -> bool {
    if share {
        return false;
    }
    if n == 0 || n % 256 != 0 {
        return false;
    }
    if linux {
        return n % 4096 == 0;
    }
    n <= 16384 || n % 4096 == 0 || (bootimg && n % 1024 == 0)
}

/// Bytes to drain from the guest UART TX ring on a VM-exit.
///
/// iso=0 / firmware keep [`GUEST_TX_DRAIN_EXIT`] (nested `be0f1cd` `/init`
/// SIGSEGV at 64/exit). Do not linux earlycon drain CHUNK on every exit
/// during share (SOL flood). Hush HV `write_byte` instead. `share` is
/// accepted so call sites stay uniform. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_earlycon_drain(_share: bool) -> usize {
    crate::boot::serial::GUEST_TX_DRAIN_EXIT
}

/// Latch HV hush / quiet ticks only on product-ISO Linux.
///
/// iso=0 / lab stub must not hush `write_byte` after a high-half RIP `#PF`
/// (OVMF also runs high-half). Nested QEMU `e0019a3` / `4f875d6` `/init`
/// SIGSEGV 3/3 after quiet ticks skipped `cpu_flush`. linux earlycon share product ISO.
/// Iron `9a3cbfa`: `linux cpuid n=1` ran **before** `#PF linux deliver`; latch
/// on that CPUID too (linux earlycon share first CPUID). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_earlycon_share_on_linux_deliver(linux: bool, product_iso: bool) -> bool {
    linux && product_iso
}

/// Latch HV hush on the first product-ISO Linux high-half VM-exit.
///
/// Iron `202312f`: readable `Linux version` then e820 cut by a blocking
/// hypervisor-scan bump; ticks interleaved before share (share waited for
/// CPUID / `#PF`). Iron `9a3cbfa`: printk shredded by HV `write_byte`.
/// Firmware RIP `0xFFFCFxxx` does not set bit 63. iso=0 does not latch.
/// linux earlycon share first high-half. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_earlycon_share_on_vmexit(rip: u64, product_iso: bool) -> bool {
    guest_uefi_linux_earlycon_share_on_linux_deliver(
        guest_uefi_pf_should_deliver_to_guest(rip),
        product_iso,
    )
}

/// Latch HV hush after El Torito bootimg on product ISO.
///
/// Iron `b983ef8`: 256MiB disk, `Loaded initrd`, then readable
/// `Linux version 6.12.13-0-virt` / e820 while ticks still used blocking
/// `write_byte` at OVMF RIP `0x7ee5dbe4`. Identity-map earlycon does not
/// set bit 63, so share-on-high-half waits until after printk has already
/// started. linux earlycon share first bootimg.
/// iso=0 does not latch. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_earlycon_share_on_bootimg(product_iso: bool, bootimg: bool) -> bool {
    product_iso && bootimg
}

/// Nested KVM never prints the iron marker. Iron polls every resume so a
/// GPT write is not missed if later exits are virtio IN-only.
/// poll ISO-INSTALL-OK every resume. Not `ISO-INSTALL-OK` by itself.
pub fn guest_uefi_poll_iso_install_ok(nested: bool) -> bool {
    !nested
}

/// Drain virtio notifies on every product-ISO resume so a missed BAR kick
/// still completes IN/OUT (GPT write can be IN-only later). iso=0 does not.
/// virtio drain every resume. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_virtio_drain_every_resume(product_iso: bool) -> bool {
    product_iso
}

/// First non-I/O VM-exit after the El Torito boot image was read.
///
/// Iron COM2 after gzip: last line was EFI stub `Loaded initrd` (often the
/// last ConOut before ExitBootServices). A CR/CPUID/EPT/HLT after that is
/// kernel/stub past PIO. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_post_cd_non_io(bootimg: bool, already: bool, io_exit: bool) -> bool {
    bootimg && !already && !io_exit
}

/// True when El Torito evidence should stop the private guest-UEFI VMCS.
///
/// Stage 45 lab stub (72 KiB RN-ELT) stops so E4 `LINUX-EARLY` still runs.
/// Stage 46 product ISO (`len > GUEST_CD_ISO_CAP`) continues. Not installer.
pub fn eltorito_stops_guest_uefi(eltorito: bool) -> bool {
    eltorito && crate::devices::ide_cdrom::is_lab_eltorito_media()
}

/// Stage 45 live stop: El Torito boot, or post-ATAPI tail, or post-DXE tail if no PACKET.
///
/// INVARIANTS:
/// - `false` until DXE printed
/// - first ATAPI sector does **not** stop (Stage 44 did)
/// - `true` when [`eltorito_boot_evidence`] holds **and** the CD is the lab stub
/// - product ISO does not stop on El Torito evidence (Stage 46)
/// - after PACKET, does **not** apply the 32768 post-ATAPI tail (first
///   sector is often LBA 0 dummy; BDS catalog/FatDxe/StartImage still
///   needs the 131072-exit cap, then 262144 after iron `df7d158` hit
///   n=131072 still in ATA PIO). `atapi_at` / catalog / boot_image are
///   live-serial evidence, not a short-tail trigger. first ATAPI is often LBA 0 dummy.
/// - `true` after [`GUEST_UEFI_POST_DXE_TAIL`] past DXE if `sectors==0`
pub fn post_atapi_should_stop(
    dxe_printed: bool,
    exit_n: u32,
    dxe_at: u32,
    atapi_at: u32,
    sectors: u32,
    catalog: bool,
    boot_image: bool,
    eltorito: bool,
) -> bool {
    let _ = (atapi_at, catalog, boot_image);
    if !dxe_printed {
        return false;
    }
    if eltorito_stops_guest_uefi(eltorito) {
        return true;
    }
    if atapi_read_evidence(sectors) {
        return false;
    }
    exit_n.saturating_sub(dxe_at) >= GUEST_UEFI_POST_DXE_TAIL
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

/// Offset into the 4 MiB pflash window, or `None` if `gpa` is not flash.
///
/// Iron COM2: xAPIC SVR `gpa=0xfee000f0` `rip=0xfffcfc86` `insn=` empty —
/// identity peek only covered 32 MiB RAM, so firmware RIP fetched 0 bytes.
pub fn guest_uefi_flash_off(gpa: u64) -> Option<u64> {
    if gpa >= GUEST_UEFI_FLASH_BASE
        && gpa < GUEST_UEFI_FLASH_BASE + GUEST_UEFI_FLASH_WINDOW
    {
        Some(gpa - GUEST_UEFI_FLASH_BASE)
    } else {
        None
    }
}

/// Copy instruction bytes from the guest-private OVMF flash HPA.
pub fn copy_flash_at(flash: &[u8], gpa: u64, out: &mut [u8]) -> usize {
    let Some(start) = guest_uefi_flash_off(gpa) else {
        return 0;
    };
    let start = start as usize;
    if out.is_empty() || start >= flash.len() {
        return 0;
    }
    let n = out.len().min(flash.len() - start);
    out[..n].copy_from_slice(&flash[start..start + n]);
    n
}

/// When decode fails, still finish a 32-bit EAX MOV if skip-len is 1–15 even if peek got bytes.
/// LocalApicLib is `mov [svr], eax` / `mov eax, [svr]`. Peek `n=0` is a
/// fetch-miss; `n>0` is bytes we could not decode (VMCS or `mmio_decoded_len`
/// still yielded a skip). Do not skip a 16-byte peek. `fetched_n` is kept
/// for the COM2 `n=` log at the call site.
///
/// Iron COM2 `e3f56aa`: `gpa=0xfee000f0 insn=` empty at `rip=0xfffcfc86`.
/// Linux high-half APIC MMIO often has VMCS `insn_len` 0 (EPT); skip 3.
pub fn xapic_eax_fallback_skip_len(insn_len: u64) -> u64 {
    if insn_len >= 1 && insn_len <= 15 {
        insn_len
    } else if insn_len == 0 {
        3
    } else {
        0
    }
}

/// Iron COM2 `e3f56aa`: `gpa=0xfee000f0 insn=` empty at `rip=0xfffcfc86`.
/// Empty peek + `insn_len` 0 still EAX-fallbacks (linux xAPIC EPT insn_len 0).
pub fn xapic_fetch_miss_eax_fallback(fetched_n: usize, insn_len: u64) -> bool {
    if insn_len >= 1 && insn_len <= 15 {
        return true;
    }
    fetched_n == 0 && insn_len == 0
}

/// RIP skip length after MMIO emulate. Prefer a valid VMCS 1–15; else the
/// length decoded from fetched bytes. Never skip a 16-byte peek (`fetched_n`).
///
/// Iron COM2 may fetch flash bytes (`n>0`) while VMCS `insn_len` is 0, so
/// decode and skip both need the decoded length.
pub fn guest_uefi_mmio_skip_len(vmcs_len: u64, fetched_len: u64) -> u64 {
    if vmcs_len >= 1 && vmcs_len <= 15 {
        vmcs_len
    } else if fetched_len >= 1 && fetched_len <= 15 {
        fetched_len
    } else {
        0
    }
}

/// High-half Linux: VMCS `insn_len` can be 0 while identity peek is empty.
/// Two-byte exits: CPUID `0F A2`, WRMSR `0F 30`, RDTSC `0F 31`, RDMSR
/// `0F 32`, INVD `0F 08`, WBINVD `0F 09`. PAUSE `F3 90`. HLT `F4` is one
/// byte. INVLPG `0F 01 /7` is variable (ModRM/SIB/disp); empty fetch stays 0.
///
/// Iron `d0735bd` after `#PF linux deliver`: tick `reason=0xa`
/// `rip=0xffffffffb8081783` `insn=` empty. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_fixed_skip_len(bytes: &[u8]) -> u64 {
    if !bytes.is_empty() && bytes[0] == 0xF4 {
        return 1;
    }
    if bytes.len() >= 2 && bytes[0] == 0xF3 && bytes[1] == 0x90 {
        return 2;
    }
    let invlpg = guest_uefi_linux_invlpg_len(bytes);
    if invlpg != 0 {
        return invlpg;
    }
    let mov_dr = guest_uefi_linux_mov_dr_len(bytes);
    if mov_dr != 0 {
        return mov_dr;
    }
    if bytes.len() >= 2
        && bytes[0] == 0x0F
        && matches!(bytes[1], 0xA2 | 0x30 | 0x31 | 0x32 | 0x08 | 0x09)
    {
        2
    } else {
        0
    }
}

/// MOV DR `0F 21 /r` / `0F 23 /r` (optional REX). Register form is 3 bytes
/// plus REX. Empty fetch stays 0 (do not guess). linux MOV DR skip.
pub fn guest_uefi_linux_mov_dr_len(bytes: &[u8]) -> u64 {
    let mut i = 0usize;
    if let Some(&b) = bytes.first() {
        if (0x40..=0x4F).contains(&b) {
            i = 1;
        }
    }
    if bytes.len() >= i + 3 && bytes[i] == 0x0F && (bytes[i + 1] == 0x21 || bytes[i + 1] == 0x23) {
        (i + 3) as u64
    } else {
        0
    }
}

/// INVLPG `0F 01 /7` memory operand (SDM). Not `SWAPGS` (`0F 01 F8`).
///
/// Variable length: prefixes + 0F 01 + ModRM [+ SIB] [+ disp]. Empty
/// fetch stays 0 (do not guess). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_invlpg_len(bytes: &[u8]) -> u64 {
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if (0x40..=0x4F).contains(&b)
            || matches!(
                b,
                0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3
            )
        {
            i += 1;
            continue;
        }
        break;
    }
    if bytes.len().saturating_sub(i) < 3 {
        return 0;
    }
    if bytes[i] != 0x0F || bytes[i + 1] != 0x01 {
        return 0;
    }
    let modrm = bytes[i + 2];
    if (modrm >> 3) & 7 != 7 {
        return 0;
    }
    let mod_ = modrm >> 6;
    let rm = modrm & 7;
    if mod_ == 3 {
        return 0;
    }
    let mut n = i + 3;
    let sib = rm == 4;
    if sib {
        if n >= bytes.len() {
            return 0;
        }
        n += 1;
    }
    let disp = match mod_ {
        0 => {
            if rm == 5
                || (sib && {
                    let sib_b = bytes[n - 1];
                    (sib_b & 7) == 5
                })
            {
                4
            } else {
                0
            }
        }
        1 => 1,
        2 => 4,
        _ => 0,
    };
    n += disp;
    if n > bytes.len() {
        0
    } else {
        n as u64
    }
}

/// Fallback skip after a 2-byte intercept (CPUID / RDMSR / WRMSR / RDTSC /
/// INVD / WBINVD / PAUSE).
///
/// Prefer VMCS 1–15 (caller already skipped). Else decode those opcodes.
/// Else high-half RIP + `insn_len` 0 still skip 2 (iron `d0735bd` fetch
/// miss / extra-DRAM CR3). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_cpuid_msr_skip(rip: u64, vmcs_len: u64, bytes: &[u8]) -> u64 {
    if vmcs_len >= 1 && vmcs_len <= 15 {
        return 0;
    }
    let decoded = guest_uefi_linux_fixed_skip_len(bytes);
    if decoded != 0 {
        return decoded;
    }
    if guest_uefi_pf_should_deliver_to_guest(rip) {
        2
    } else {
        0
    }
}

/// Iron COM2 after leftover DRAM + `#PF linux deliver` `err=0x0`: ticks
/// `n=437248`/`437504` `reason=0xa` `rip=0xffffffff84081783`
/// `insn=0fa24189…` (Linux `native_cpuid`). Same helper RIP is expected
/// while skip advances; if GUEST_RIP is still the exit RIP after
/// [`guest_uefi_linux_cpuid_msr_skip`], force +2. Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_cpuid_force_skip(rip_before: u64, rip_after: u64) -> u64 {
    if guest_uefi_pf_should_deliver_to_guest(rip_before) && rip_after == rip_before {
        2
    } else {
        0
    }
}

/// Linux `native_cpuid` is `0F A2` (2 bytes, no prefix on iron COM2).
/// Always skip 2 on high-half; do not fail-closed if VMCS `insn_len` is 0.
/// Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_cpuid_exit_skip(rip: u64) -> u64 {
    if guest_uefi_pf_should_deliver_to_guest(rip) {
        2
    } else {
        0
    }
}

/// Log the first 8 Linux CPUIDs, then powers of two and every 256 so a
/// short COM2 paste shows whether leaves are changing. `n` is 1-based.
/// Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_cpuid_should_log(n: u32) -> bool {
    n > 0 && (n <= 8 || n.is_power_of_two() || n % 256 == 0)
}

/// Fallback skip after HLT. One byte (`F4`). High-half + `insn_len` 0
/// still skip 1 (iron `d0735bd` fetch miss). Not `ISO-INSTALL-OK`.
pub fn guest_uefi_linux_hlt_skip(rip: u64, vmcs_len: u64, bytes: &[u8]) -> u64 {
    if vmcs_len >= 1 && vmcs_len <= 15 {
        return 0;
    }
    if !bytes.is_empty() && bytes[0] == 0xF4 {
        return 1;
    }
    if guest_uefi_pf_should_deliver_to_guest(rip) {
        1
    } else {
        0
    }
}

/// Linear address of the instruction to emulate. 64-bit CS ignores CS.base
/// (SDM); 16/32-bit CS uses `CS.base + RIP`. MMIO handlers must not peek
/// raw `GUEST_RIP` while the exit log uses `cs_base + rip`.
pub fn guest_uefi_insn_linear(rip: u64, cs_base: u64, cs_long: bool) -> u64 {
    if cs_long {
        rip
    } else {
        cs_base.wrapping_add(rip)
    }
}

/// Peek GPA for MMIO insn fetch. Prefer CS.base+RIP (or RIP in 64-bit).
/// If that is outside the 4 MiB flash window but `GUEST_RIP` is inside
/// (iron `e3f56aa` `rip=0xfffcfc86` with leftover real-mode CS.base),
/// peek RIP so xAPIC SVR is not `insn=` empty.
pub fn guest_uefi_mmio_peek_linear(rip: u64, cs_base: u64, cs_long: bool) -> u64 {
    let linear = guest_uefi_insn_linear(rip, cs_base, cs_long);
    if guest_uefi_flash_off(linear).is_some() || guest_uefi_flash_off(rip).is_none() {
        linear
    } else {
        rip
    }
}

/// 2 MiB slots to leave after a disk larger than 64 MiB. Scratch-only
/// (32×2 MiB). Iron `9a3cbfa` leftover DRAM `extra=846` already fills
/// report-RAM above PRECISE, so the old 96 precise-pool GCD floor
/// (`pool=194`) starved a 256 MiB disk on a ~480 MiB post-fw window.
/// 256MiB disk leftover report-RAM. Do not steal leftover for the disk
/// (Linux report-RAM; ADR-004). Do not invent HPA.
pub const PRODUCT_ISO_DISK_LEAVE_2M_SLOTS: usize = GUEST_UEFI_MMIO_SCRATCH_SLOTS;

pub fn product_iso_disk_leave_pages() -> u64 {
    (PRODUCT_ISO_DISK_LEAVE_2M_SLOTS as u64) * (GUEST_UEFI_REPORT_RAM_PAGE / 4096)
}

/// Allocate a contiguous install-disk run. Largest size that still leaves
/// scratch wins (leftover DRAM backs report-RAM). Sizes ≤64 MiB skip the
/// floor so a tight pool still gets a GPT-capable disk.
///
/// INVARIANTS:
/// - Does not invent an HPA; only [`FrameAllocator::allocate_contiguous`]
/// - Nested stays 1 MiB; iron tries 1 GiB then 256/64/32/16/1 MiB
///
/// Call **before** greedy 2 MiB report-RAM so Alpine sys-mode gets ≥64 MiB.
pub fn try_alloc_product_iso_install_disk(
    alloc: &mut FrameAllocator,
    nested: bool,
) -> Option<(PhysFrame, usize)> {
    let leave = if nested {
        0
    } else {
        product_iso_disk_leave_pages()
    };
    let keep = crate::mgmt::iso::DEFAULT_INSTALL_DISK_BYTES as usize;
    for &want in crate::mgmt::iso_install::product_iso_install_disk_try_sizes(nested) {
        let pages = (want / 4096) as u64;
        if pages == 0 {
            continue;
        }
        let remaining = alloc.capacity().saturating_sub(alloc.allocated_count());
        if remaining < pages {
            continue;
        }
        if want > keep && remaining - pages < leave {
            continue;
        }
        if let Some(frame) = alloc.allocate_contiguous(pages) {
            return Some((frame, want));
        }
    }
    None
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
/// Decoded MMIO instruction length for this exit. Zeroed at each
/// [`guest_uefi_vmexit`] so HLT/INVD cannot skip a stale length.
#[cfg(target_os = "uefi")]
static MMIO_INSN_LEN: AtomicU64 = AtomicU64::new(0);
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
static ELTORITO_PRINTED: AtomicBool = AtomicBool::new(false);
static ELTORITO_CATALOG_PRINTED: AtomicBool = AtomicBool::new(false);
static ELTORITO_BOOTIMG_PRINTED: AtomicBool = AtomicBool::new(false);
static POST_CD_NON_IO: AtomicBool = AtomicBool::new(false);
static ELTORITO_COM_MATCH: AtomicU8 = AtomicU8::new(0);
#[cfg(target_os = "uefi")]
static GPA0_SPLIT_PRINTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static PDPT0_SPLIT_PRINTED: AtomicBool = AtomicBool::new(false);
static DXE_AT_N: AtomicU32 = AtomicU32::new(0);
static ATAPI_AT_N: AtomicU32 = AtomicU32::new(0);
static EPT_PML4: AtomicU64 = AtomicU64::new(0);
static SINK_HPA: AtomicU64 = AtomicU64::new(0);
static HOLE_ZERO_HPA: AtomicU64 = AtomicU64::new(0);
static MMIO_SCRATCH_HPA: [AtomicU64; GUEST_UEFI_MMIO_SCRATCH_SLOTS] =
    [const { AtomicU64::new(0) }; GUEST_UEFI_MMIO_SCRATCH_SLOTS];
static MMIO_SCRATCH_GPA: [AtomicU64; GUEST_UEFI_MMIO_SCRATCH_SLOTS] =
    [const { AtomicU64::new(u64::MAX) }; GUEST_UEFI_MMIO_SCRATCH_SLOTS];
static REPORT_RAM_HPA: [AtomicU64; REPORT_RAM_ARRAY] =
    [const { AtomicU64::new(0) }; REPORT_RAM_ARRAY];
static REPORT_RAM_GPA: [AtomicU64; REPORT_RAM_ARRAY] =
    [const { AtomicU64::new(u64::MAX) }; REPORT_RAM_ARRAY];
static REPORT_RAM_MAPS: AtomicU32 = AtomicU32::new(0);
static CPU_FLUSH_PATCHED: AtomicU32 = AtomicU32::new(0);
static CPU_FLUSH_SKIP_LOG: AtomicBool = AtomicBool::new(false);
static LIVE_UC_PT_PAINTED: AtomicU32 = AtomicU32::new(0);
static LIVE_GPA0_SPLIT: AtomicU32 = AtomicU32::new(0);
static LIVE_VGA_PT_PAINTED: AtomicU32 = AtomicU32::new(0);
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
/// Exclusive 4 MiB guest-private OVMF copy (`alias_gpa=0xFFC00000`).
static FLASH_HPA: AtomicU64 = AtomicU64::new(0);
static FLASH_LEN: AtomicU64 = AtomicU64::new(0);
static RAM_REMAP_N: AtomicU32 = AtomicU32::new(0);
static RAM_REMAP_TRIES: AtomicU32 = AtomicU32::new(0);
static HPET_TICKS: AtomicU32 = AtomicU32::new(0);
static LAST_HPET_TSC: AtomicU64 = AtomicU64::new(0);
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
static PF_LINUX_DELIVER: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static PF_LINUX_CR2: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "uefi")]
static LINUX_EXC_INJECT: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static LINUX_CPUID: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static LINUX_HV_SCAN_BUMP: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static LINUX_DELAY_LOOP_SKIP: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static UART_HPET_LOG: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static LINUX_VIRTIO_DRIVER_OK_LOG: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static LINUX_PIC_IRQ0_LOG: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static LINUX_GSI2_LOG: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "uefi")]
static LINUX_LEAF4: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static LINUX_SKIP2: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static LINUX_UNHANDLED_SKIP: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "uefi")]
static LINUX_INVLPG_MISS: AtomicU32 = AtomicU32::new(0);
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

/// 16-byte-aligned XMM0–15 snapshot. Host trampoline `movdqu` after GPRs.
#[cfg(target_os = "uefi")]
#[repr(align(16))]
struct SavedXmm([u8; 256]);

#[cfg(target_os = "uefi")]
static mut SAVED_XMM: SavedXmm = SavedXmm([0; 256]);

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
/// Eight packed UC types (Intel MTRR type 0). VGA hole FIX override.
pub const GUEST_UEFI_MTRR_UC_PACKED: u64 = 0;
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
static MTRR_VGA_FIX_WB_HELD: AtomicU32 = AtomicU32::new(0);

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

/// FIX covering `[0xA0000, 1MiB)` (`0x259`, `0x268`–`0x26F`).
/// Hold firmware WB against CpuDxe UC (iron `fd041bb` `mtrr268=0x0`).
/// `0x250`/`0x258` stay as written.
pub fn guest_uefi_mtrr_fixed_is_vga_hole(msr: u32) -> bool {
    msr == 0x259 || (0x268..=0x26F).contains(&msr)
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
    MTRR_VGA_FIX_WB_HELD.store(0, Ordering::Release);
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

/// Tests / P1 hold: `false` drops valid UC variable MTRRs. P2 admitted UC
/// after CMOS 2 GiB (`c70768b`). Iron `6334704` hold after FIX WB still
/// ASSERTed (`mtrrv=0` `pde8000=0x80000083`); live path admits UC again
/// and coerces VGA FIX to UC so GCD IoMemory matches. Iron `e368e86`
/// full UC still ASSERTed; live path holds FIX WB (`fd041bb`).
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
        let prev = GUEST_MTRR_FIXED[i].load(Ordering::Acquire);
        let v = if guest_uefi_mtrr_fixed_is_vga_hole(msr)
            && value == GUEST_UEFI_MTRR_UC_PACKED
            && prev == GUEST_UEFI_MTRR_WB_PACKED
        {
            let n = MTRR_VGA_FIX_WB_HELD.fetch_add(1, Ordering::AcqRel);
            #[cfg(target_os = "uefi")]
            if n == 0 {
                serial::write_line("boot: guest-UEFI MTRR VGA FIX WB held (GCD)");
            }
            prev
        } else {
            value
        };
        GUEST_MTRR_FIXED[i].store(v, Ordering::Release);
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
    ELTORITO_PRINTED.store(false, Ordering::Release);
    ELTORITO_CATALOG_PRINTED.store(false, Ordering::Release);
    ELTORITO_BOOTIMG_PRINTED.store(false, Ordering::Release);
    POST_CD_NON_IO.store(false, Ordering::Release);
    ELTORITO_COM_MATCH.store(0, Ordering::Release);
    DXE_AT_N.store(0, Ordering::Release);
    ATAPI_AT_N.store(0, Ordering::Release);
    EPT_PML4.store(0, Ordering::Release);
    SINK_HPA.store(0, Ordering::Release);
    HOLE_ZERO_HPA.store(0, Ordering::Release);
    for i in 0..GUEST_UEFI_MMIO_SCRATCH_SLOTS {
        MMIO_SCRATCH_HPA[i].store(0, Ordering::Release);
        MMIO_SCRATCH_GPA[i].store(u64::MAX, Ordering::Release);
    }
    for i in 0..REPORT_RAM_ARRAY {
        REPORT_RAM_HPA[i].store(0, Ordering::Release);
        REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
    }
    REPORT_RAM_MAPS.store(0, Ordering::Release);
    CPU_FLUSH_PATCHED.store(0, Ordering::Release);
    CPU_FLUSH_SKIP_LOG.store(false, Ordering::Release);
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
    FLASH_HPA.store(0, Ordering::Release);
    FLASH_LEN.store(0, Ordering::Release);
    RAM_REMAP_N.store(0, Ordering::Release);
    RAM_REMAP_TRIES.store(0, Ordering::Release);
    HPET_TICKS.store(0, Ordering::Release);
    LAST_HPET_TSC.store(0, Ordering::Release);
    PREEMPT_RELOAD.store(0, Ordering::Release);
    IO_UNHANDLED_N.store(0, Ordering::Release);
    guest_uefi_mtrr_reset();
    crate::boot::serial::guest_tx_clear();
    crate::boot::serial::set_linux_earlycon_share(false);
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

pub fn guest_uefi_eltorito() -> bool {
    ELTORITO_PRINTED.load(Ordering::Acquire)
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

/// Product-ISO virtio-blk backing. No-op when the window is idle or a disk
/// is already attached. `warn` prints only when this call is the last chance.
#[cfg(target_os = "uefi")]
unsafe fn attach_product_iso_install_disk(alloc: &mut FrameAllocator, warn: bool) {
    if !crate::devices::ide_cdrom::product_iso_window_armed() {
        return;
    }
    if crate::devices::guest_virtio_blk::disk_bytes() != 0 {
        return;
    }
    let nested = guest_uefi_host_hypervisor_present();
    let Some((frame, disk_bytes)) = try_alloc_product_iso_install_disk(alloc, nested) else {
        if warn {
            serial::write_line("boot: WARN — Stage 46 virtio-blk install disk alloc failed");
        }
        return;
    };
    // SAFETY: exclusive FrameAllocator pages; guest-UEFI owns them until stop.
    // KANI-TARGET: product ISO virtio-blk attach (outside Proven Core).
    let _ = crate::devices::guest_virtio_blk::attach_disk(frame.to_phys(), disk_bytes);
    serial::write_str("boot: Stage 46 virtio-blk install disk bytes=");
    write_dec(crate::devices::guest_virtio_blk::disk_bytes());
    serial::write_line(" (not ISO-INSTALL-OK)");
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
    // Iron `6334704`: hold after FIX WB still ASSERT `mtrrv=0`
    // `pde8000=0x80000083` `flushjnz=0`. Variable-UC mix is not the
    // RefreshGcd failure. Admit the 2GiB hole. Iron `e368e86` PAT-UC
    // VGA still ASSERTed; `fd041bb` CpuDxe UC'd `mtrr268`. Hold FIX WB.
    guest_uefi_mtrr_set_admit_uc(true);
    serial::write_line("boot: guest-UEFI MTRR UC admitted (GCD)");
    serial::write_line("boot: guest-UEFI MTRR VGA FIX WB hold armed (GCD)");
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
    FLASH_HPA.store(fw_hpa, Ordering::Release);
    FLASH_LEN.store(fw_len, Ordering::Release);

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
        // Arm the product ISO window *before* disk attach. Iron COM2: first
        // attach was a no-op (window idle), then report-RAM ate the 512 MiB
        // pool and the late attach got 1 MiB.
        let _ = crate::mgmt::iso_install::present_product_iso_if_retained();
        // Reserve the install disk first (before scratch *and* report-RAM).
        // Do not invent HPA on GPA miss (ADR-004).
        attach_product_iso_install_disk(alloc, false);
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
        let mut extra_n = 0u64;
        let report_slots = report_ram_slots_alloc();
        for i in 0..report_slots {
            let (report_hpa, extra) =
                if let Some(report_frame) = alloc.allocate_contiguous_aligned(512, 512) {
                    (report_frame.to_phys(), false)
                } else if let Some(h) = crate::boot::handoff::take_report_ram_extra_2m() {
                    extra_n += 1;
                    (h, true)
                } else {
                    break;
                };
            if !extra {
                core::ptr::write_bytes(report_hpa as *mut u8, 0, 2 * 1024 * 1024);
            }
            REPORT_RAM_HPA[i].store(report_hpa, Ordering::Release);
            REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
            report_n += 1;
        }
        if report_n != 0 {
            serial::write_str("boot: guest-UEFI report-RAM pool=");
            write_dec(report_n);
            if extra_n != 0 {
                serial::write_str(" extra=");
                write_dec(extra_n);
                serial::write_str(" no-zero");
            }
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
        // report-RAM EPT pre-map: product ISO fills `[32MiB, 2GiB)` WB
        // leaves from leftover HPA so Linux `init_mem_mapping` after PAT
        // is not ~1000 EPT misses (iron `113a08a`). iso=0 stays lazy.
        if guest_uefi_report_ram_should_premap(
            crate::mgmt::iso_install::product_iso_retained_bytes().is_some(),
        ) {
            let mut pre = 0u64;
            for i in 0..report_n as usize {
                let Some(gpa) = guest_uefi_report_ram_premap_gpa(i, report_n as usize) else {
                    break;
                };
                let hpa = REPORT_RAM_HPA[i].load(Ordering::Acquire);
                if hpa == 0 {
                    break;
                }
                REPORT_RAM_GPA[i].store(gpa, Ordering::Release);
                if ept_map_2m_hpa_mt(gpa, hpa, false, true, GUEST_UEFI_EPT_MT_WB, false) {
                    pre += 1;
                } else {
                    REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
                }
            }
            if pre != 0 {
                crate::memory::ept_hw::invept_global();
                REPORT_RAM_MAPS.store(pre as u32, Ordering::Release);
                serial::write_str("boot: guest-UEFI report-RAM EPT pre-map n=");
                write_dec(pre);
                serial::write_byte(b'\n');
            }
        }
        // Stage 46: present the retained product ISO before the HPET 2 MiB
        // leaf so IOAPIC can be a 4 KiB trap instead of sink zeros.
        let _ = crate::mgmt::iso_install::present_product_iso_if_retained();
        // Present EPT leaves so early hole walks do not EPT-fault.
        // Iron 5837243: pre-scratch of 0xC0000000..0xC0E00000 plus a
        // read-walk of 0xC1000000..0xC3A00000 filled pool=32; cap at
        // 0xC3C00000 then sink; RIP 0x3d00001. Only pre-scratch the
        // known PT-store GPA. Hole reads are R-only dedicated zero (not HPET
        // SINK_HPA; iron f93caee). Do not bulk 2–4GiB (73576cc).
        let _ = ept_map_2m_scratch(0x8000_0000);
        let mut hpet_2m = true;
        if crate::devices::ide_cdrom::product_iso_window_armed() {
            if let Some(pt) = alloc_phys(alloc) {
                if ept_install_ioapic_trap(pt) {
                    hpet_2m = false;
                    serial::write_line(
                        "boot: guest-UEFI IOAPIC trap 4K (Stage 46; not ISO-INSTALL-OK)",
                    );
                }
            }
        }
        if hpet_2m {
            for &mm in &[0xFEC0_0000u64, 0xFED0_0000] {
                if ept_map_2m_sink(mm) {
                    SINK_MAPS.fetch_add(1, Ordering::AcqRel);
                }
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
            let trap = crate::devices::ide_cdrom::product_iso_window_armed();
            if ept_install_xapic_4k(pt, lapic, trap) {
                if trap {
                    serial::write_line(
                        "boot: guest-UEFI xAPIC 4K trap (Stage 46; not ISO-INSTALL-OK)",
                    );
                } else {
                    serial::write_str("boot: guest-UEFI xAPIC 4K hpa=0x");
                    write_hex(lapic);
                    serial::write_str(" ver=0x");
                    write_hex(u64::from(crate::devices::lapic_virt::XAPIC_VERSION));
                    serial::write_byte(b'\n');
                }
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

    let _ = crate::mgmt::iso_install::present_product_iso_if_retained();
    if crate::devices::ide_cdrom::present_placeholder_if_idle() {
        serial::write_str("boot: guest-UEFI CD GuestVisible iso=");
        write_dec(crate::devices::ide_cdrom::retained_iso_id());
        serial::write_str(" bytes=");
        write_dec(crate::devices::ide_cdrom::retained_len() as u64);
        serial::write_byte(b'\n');
        if crate::devices::ide_cdrom::product_iso_window_armed() {
            serial::write_line(
                "boot: guest-UEFI product ISO window (Stage 46; not ISO-INSTALL-OK)",
            );
        }
    }
    attach_product_iso_install_disk(alloc, true);
    if crate::devices::guest_virtio_blk::present() {
        if crate::devices::guest_virtio_blk::queues_armed() {
            serial::write_line(
                "boot: guest-UEFI virtio-pci queues (Stage 46; not ISO-INSTALL-OK)",
            );
            if crate::devices::guest_virtio_blk::iso_visible() {
                serial::write_line(
                    "boot: Stage 46 virtio-iso 00:03.0 read-only (not ISO-INSTALL-OK)",
                );
            }
        } else {
            serial::write_line("boot: guest-UEFI virtio-blk empty CD→disk order");
        }
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
    // Same wanted bits as E4, plus CR8 load/store exiting so Linux
    // `write_cr8`/`read_cr8` syncs `lapic_virt` TPR (no VMCS GUEST_CR8).
    // Do not OR CR8 bits into the E4 SHELL VMCS. Then drop unconditional
    // I/O if bitmaps won (SDM: the two I/O-exit controls must not both be 1).
    let mut primary = adjust_vmx_controls(
        CPU_BASED_HLT_EXITING
            | CPU_BASED_CR8_LOAD_EXITING
            | CPU_BASED_CR8_STORE_EXITING
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
        serial::write_line("boot: guest-UEFI WARN — TPR shadow forced (no virt-APIC; CR8 may not exit)");
    }
    if primary & CPU_BASED_CR8_LOAD_EXITING == 0
        || primary & CPU_BASED_CR8_STORE_EXITING == 0
    {
        serial::write_line("boot: guest-UEFI WARN — CR8 load/store exiting not allowed");
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
        "mov rax, cr4",
        "or rax, {osfxsr}",
        "mov cr4, rax",
        "lea rax, [rip + {xmm}]",
        "movdqu [rax], xmm0",
        "movdqu [rax + 16], xmm1",
        "movdqu [rax + 32], xmm2",
        "movdqu [rax + 48], xmm3",
        "movdqu [rax + 64], xmm4",
        "movdqu [rax + 80], xmm5",
        "movdqu [rax + 96], xmm6",
        "movdqu [rax + 112], xmm7",
        "movdqu [rax + 128], xmm8",
        "movdqu [rax + 144], xmm9",
        "movdqu [rax + 160], xmm10",
        "movdqu [rax + 176], xmm11",
        "movdqu [rax + 192], xmm12",
        "movdqu [rax + 208], xmm13",
        "movdqu [rax + 224], xmm14",
        "movdqu [rax + 240], xmm15",
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
        xmm = sym SAVED_XMM,
        osfxsr = const crate::arch::cpu::CR4_OSFXSR,
        cont = sym guest_uefi_vmexit,
    );
}

#[cfg(target_os = "uefi")]
#[unsafe(naked)]
unsafe extern "C" fn guest_uefi_vmresume() -> ! {
    core::arch::naked_asm!(
        "lea rax, [rip + {xmm}]",
        "movdqu xmm0, [rax]",
        "movdqu xmm1, [rax + 16]",
        "movdqu xmm2, [rax + 32]",
        "movdqu xmm3, [rax + 48]",
        "movdqu xmm4, [rax + 64]",
        "movdqu xmm5, [rax + 80]",
        "movdqu xmm6, [rax + 96]",
        "movdqu xmm7, [rax + 112]",
        "movdqu xmm8, [rax + 128]",
        "movdqu xmm9, [rax + 144]",
        "movdqu xmm10, [rax + 160]",
        "movdqu xmm11, [rax + 176]",
        "movdqu xmm12, [rax + 192]",
        "movdqu xmm13, [rax + 208]",
        "movdqu xmm14, [rax + 224]",
        "movdqu xmm15, [rax + 240]",
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
        xmm = sym SAVED_XMM,
        fail = sym guest_uefi_resume_failed,
    );
}

#[cfg(target_os = "uefi")]
unsafe extern "C" fn guest_uefi_resume_failed() -> ! {
    serial::write_line("boot: guest-UEFI VMRESUME failed — continuing E4 SHELL");
    leave_to_e4();
}

/// HPET main-counter step for this VM-exit.
///
/// INVARIANTS:
/// - Preemption / HLT / ACPI PM / HPET-EPT use [`crate::devices::guest_platform::HPET_MAIN_STEP`] (~1s)
/// - CPUID / RDMSR / WRMSR / non-HPET EPT use [`crate::devices::guest_platform::HPET_INSN_STEP`] (~1ms)
/// - UART COM I/O uses [`guest_uefi_hpet_uart_tsc_step`] (host TSC delta, capped)
/// - PCI config and ATA I/O stay 0
///
/// VERIFICATION: L1 (host tests)
pub fn guest_uefi_hpet_step_for_exit(basic: u32, hpet_ept: bool, acpi_io: bool) -> u64 {
    const CPUID: u32 = 10;
    const HLT: u32 = 12;
    const MSR_READ: u32 = 31;
    const MSR_WRITE: u32 = 32;
    const EPT: u32 = 48;
    const PREEMPT: u32 = 52;
    if basic == PREEMPT || basic == HLT || acpi_io || (basic == EPT && hpet_ept) {
        crate::devices::guest_platform::HPET_MAIN_STEP
    } else if basic == CPUID || basic == MSR_READ || basic == MSR_WRITE || basic == EPT {
        crate::devices::guest_platform::HPET_INSN_STEP
    } else {
        0
    }
}

/// HPET ticks for a COM1/COM2 I/O VM-exit from host TSC delta.
///
/// INVARIANTS:
/// - Not a fixed 1 ms (`HPET_INSN_STEP`); printk does several in/out per char
/// - One exit injects at most [`crate::devices::guest_platform::HPET_UART_IO_STEP_CAP`]
/// - PCI/ATA I/O does not call this
///
/// VERIFICATION: L1 (host tests)
pub fn guest_uefi_hpet_uart_tsc_step(tsc_delta: u64) -> u64 {
    crate::devices::guest_platform::hpet_ticks_from_tsc_delta(tsc_delta)
}

#[cfg(target_os = "uefi")]
fn tick_hpet_on_exit(basic: u32, gpa: u64, qual: u64) {
    let sink = SINK_HPA.load(Ordering::Acquire);
    if sink == 0 {
        return;
    }
    let now = cpu::rdtsc();
    let prev = LAST_HPET_TSC.swap(now, Ordering::AcqRel);
    let tsc_delta = if prev == 0 {
        0
    } else {
        now.wrapping_sub(prev)
    };
    let port = io_port_from_qual(qual);
    let size = ((qual & 7) + 1) as u8;
    let acpi_io = basic == EXIT_REASON_IO_INSTRUCTION
        && crate::devices::guest_platform::is_acpi_pm_timer_io(port, size);
    let uart_io = basic == EXIT_REASON_IO_INSTRUCTION && is_com_uart_port(port);
    let step = if uart_io {
        let s = guest_uefi_hpet_uart_tsc_step(tsc_delta);
        if UART_HPET_LOG
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            serial::write_line(
                "boot: guest-UEFI UART HPET TSC delta (Stage 46; not ISO-INSTALL-OK)",
            );
        }
        s
    } else {
        guest_uefi_hpet_step_for_exit(
            basic,
            crate::devices::guest_platform::is_hpet_gpa(gpa),
            acpi_io,
        )
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
    if crate::devices::ide_cdrom::product_iso_window_armed()
        && (basic == EXIT_REASON_PREEMPTION_TIMER || basic == EXIT_REASON_HLT)
    {
        let _ = crate::devices::lapic_virt::poll_timer_expiry();
        crate::devices::guest_irq::raise_pit();
        if serial::linux_earlycon_share() {
            linux_prefer_pit_until_driver_ok();
        }
    }
}

/// HOST_RIP continuation for the private guest-UEFI VMCS. Not the E4 SHELL landing.
#[cfg(target_os = "uefi")]
pub unsafe extern "C" fn guest_uefi_vmexit() -> ! {
    LAUNCH_ENTERED.store(true, Ordering::Release);
    MMIO_INSN_LEN.store(0, Ordering::Relaxed);
    let reason = ops::vmread(EXIT_REASON).unwrap_or(0xFFFF) as u32;
    let qual = ops::vmread(EXIT_QUALIFICATION).unwrap_or(0);
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let cs_base = ops::vmread(GUEST_CS_BASE).unwrap_or(0);
    let gpa = ops::vmread(GUEST_PHYSICAL_ADDRESS).unwrap_or(0);
    let intr = ops::vmread(VM_EXIT_INTR_INFO).unwrap_or(0);
    if guest_uefi_linux_earlycon_share_on_bootimg(
        crate::devices::ide_cdrom::product_iso_window_armed(),
        crate::devices::ide_cdrom::eltorito_boot_image_read(),
    ) || guest_uefi_linux_earlycon_share_on_vmexit(
        rip,
        crate::devices::ide_cdrom::product_iso_window_armed(),
    ) {
        // linux earlycon share first bootimg (iron b983ef8 Loaded initrd
        // then `[` at identity RIP; high-half share was too late).
        // linux earlycon share first high-half (iron 202312f e820 tick /
        // scan bump before share; 9a3cbfa printk vs HV write_byte).
        // linux earlycon pace LSR THRE (iron 029ac8f/3dc7d11 hush-on-bootimg
        // still cut at `[` after two e820 lines; guest LSR always THRE).
        serial::set_linux_earlycon_share(true);
    }
    tick_hpet_on_exit(reason & 0xFFFF, gpa, qual);
    // linux earlycon hush HV (do not drain CHUNK on every exit).
    let _ = serial::drain_guest_tx(guest_uefi_linux_earlycon_drain(
        serial::linux_earlycon_share(),
    ));
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
    } else if guest_uefi_tick_should_print(
        n,
        crate::devices::ide_cdrom::eltorito_boot_image_read(),
        PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
        serial::linux_earlycon_share(),
    ) {
        // Iron COM2 0be7283 flooded SOL with a tick every 256 I/O exits
        // (same=1 PCI/ATA poll). Keep dense ticks through BOTH/ATAPI, then
        // every 4096 so RN-ELT / ELTORITO-OK stay readable. After bootimg
        // every 1024 (iron Loaded initrd had no further tick). After Linux
        // #PF deliver, every 4096 (iron 115e5ee every-256 UART split PAT)
        // unless share (linux earlycon quiet ticks).
        // Do not skip ebecc9c3.
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
        serial::write_str(" ram=");
        write_dec(REPORT_RAM_MAPS.load(Ordering::Acquire) as u64);
        serial::write_str(" msr=0x");
        write_hex(u64::from(LAST_GUEST_MSR.load(Ordering::Acquire)));
        serial::write_str(" insn=");
        dump_low_ram_insn(linear);
        serial::write_byte(b'\n');
        let _ = serial::drain_guest_tx(serial::GUEST_TX_DRAIN_CHUNK);
    }
    // cpu_flush on tick cadence even when share (nested e0019a3 / 4f875d6
    // skipped CpuFlush patches after a high-half RIP latched share).
    // cpu_flush skip leftover pre-map on tick (iron `abfb008` scanned
    // 64 leftover heap slots after skip n=944 and hung at tick n=256).
    if guest_uefi_tick_should_print(
        n,
        crate::devices::ide_cdrom::eltorito_boot_image_read(),
        PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
        false,
    ) && guest_uefi_cpu_flush_tick_scans_mapped(REPORT_RAM_MAPS.load(Ordering::Acquire))
    {
        guest_uefi_patch_cpu_flush_all_mapped();
    }

    if guest_uefi_post_cd_non_io(
        crate::devices::ide_cdrom::eltorito_boot_image_read(),
        POST_CD_NON_IO.load(Ordering::Acquire),
        basic == EXIT_REASON_IO_INSTRUCTION,
    ) && !POST_CD_NON_IO.swap(true, Ordering::AcqRel)
    {
        serial::write_str("boot: guest-UEFI post-CD non-io n=");
        write_dec(n as u64);
        serial::write_str(" reason=0x");
        write_hex_u32(reason);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_str(" ram=");
        write_dec(REPORT_RAM_MAPS.load(Ordering::Acquire) as u64);
        serial::write_byte(b'\n');
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
        serial::write_line_nowait("boot: guest-UEFI VM-entry/fetch failed — marker not claimed");
    }

    let mut resume = false;
    if !entry_fail && !tf && !fetch_fail && n < guest_uefi_resume_cap(guest_uefi_host_hypervisor_present()) {
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
                    skip_hlt()
                } else {
                    false
                }
            }
            EXIT_REASON_EPT_VIOLATION => handle_ept(gpa, qual),
            EXIT_REASON_CR_ACCESS => handle_cr(qual),
            EXIT_REASON_EXCEPTION_NMI => handle_exception_nmi(intr, rip, linear, qual),
            EXIT_REASON_EXTERNAL_INTERRUPT => true,
            EXIT_REASON_INTERRUPT_WINDOW => true,
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
            // INVD / RDTSC / PAUSE / WBINVD — 2 bytes. INVLPG is skip-decoded
            // (variable length); empty fetch does not guess. Do not clear
            // CPU_BASED_INVLPG_EXITING (Xeon allowed0=1). Do not raise PIT on
            // Linux PAUSE (iron MADT stop). virtio MMIO / HLT / preempt still
            // raise so poll_idle jiffies can move after DRIVER_OK.
            13 | 16 | 40 | 54 => skip_cpuid_msr(),
            14 => skip_invlpg(),
            // linux unhandled nowait stop: MOV DR (29) / GDTR-IDTR (46) /
            // LDTR-TR (47) / INVPCID (58) / XSAVES (63) / XRSTORS (64)
            // skip via VMCS length so iron 1a2544d does not drop to E4 hold.
            // VMCS len 0 still tries skip_insn (high-half MOV DR).
            _ => {
                let linux = guest_uefi_linux_guest_active(
                    serial::linux_earlycon_share(),
                    guest_uefi_pf_should_deliver_to_guest(rip),
                    PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
                );
                let len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(0);
                if guest_uefi_linux_unhandled_try_skip(linux, len, basic) {
                    let k = LINUX_UNHANDLED_SKIP.fetch_add(1, Ordering::AcqRel);
                    if k < 8 {
                        serial::write_str_nowait(
                            "boot: guest-UEFI linux unhandled resume reason=0x",
                        );
                        write_hex_u32_nowait(reason);
                        serial::write_str_nowait(" len=");
                        write_dec_nowait(len);
                        serial::write_str_nowait(" rip=0x");
                        write_hex_nowait(rip);
                        serial::write_byte_nowait(b'\n');
                    }
                    skip_insn()
                } else {
                    false
                }
            }
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
                let linux = guest_uefi_linux_guest_active(
                    serial::linux_earlycon_share(),
                    guest_uefi_pf_should_deliver_to_guest(rip),
                    PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
                );
                if guest_uefi_linux_preempt_deadloop_noskip(
                    linux,
                    crate::devices::ide_cdrom::product_iso_window_armed(),
                ) {
                    false
                } else {
                    skip_preempt_deadloop(linear, rip)
                }
            } else if guest_uefi_pf_should_deliver_to_guest(rip) {
                // Identity peek of high-half is empty or leftover DRAM.
                // Do not let firmware `eb f3` skip rewrite Linux RIP.
                false
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
            && post_atapi_should_stop(
                DXE_PRINTED.load(Ordering::Acquire),
                n,
                DXE_AT_N.load(Ordering::Acquire),
                ATAPI_AT_N.load(Ordering::Acquire),
                crate::devices::ide_cdrom::sectors_read(),
                crate::devices::ide_cdrom::eltorito_catalog_read(),
                crate::devices::ide_cdrom::eltorito_boot_image_read(),
                eltorito_boot_evidence(
                    crate::devices::ide_cdrom::eltorito_catalog_read(),
                    crate::devices::ide_cdrom::eltorito_boot_image_read(),
                    eltorito_payload_ran(ELTORITO_COM_MATCH.load(Ordering::Acquire)),
                ),
            )
        {
            resume = false;
        }
    }

    if resume {
        drain_virtio_product_iso();
        try_inject_guest_irq();
        if guest_uefi_poll_iso_install_ok(guest_uefi_host_hypervisor_present())
            && crate::devices::guest_virtio_blk::take_iso_install_ok()
        {
            serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
        }
        let reload = PREEMPT_RELOAD.load(Ordering::Acquire);
        if reload != 0 {
            let _ = ops::vmwrite(VMX_PREEMPTION_TIMER_VALUE, u64::from(reload));
        }
        let linux_cr2 = PF_LINUX_CR2.swap(0, Ordering::AcqRel);
        if linux_cr2 != 0 {
            // SAFETY: VMX-root; restore guest #PF linear after any host walk.
            // KANI-TARGET: guest-UEFI Linux #PF CR2 restore (outside Proven Core).
            cpu::write_cr2(linux_cr2);
        }
        CONTINUE_GUEST.store(true, Ordering::Release);
        guest_uefi_vmresume();
    }
    if serial::linux_earlycon_share() {
        serial::write_line_nowait("boot: guest-UEFI stop during earlycon share");
    }
    // linux unhandled nowait stop (iron 1a2544d Freeing initrd then
    // restore host xcr0; share hushes HV `write_str` stop n=).
    serial::write_str_nowait("boot: guest-UEFI stop n=");
    write_dec_nowait(n as u64);
    serial::write_str_nowait(" reason=0x");
    write_hex_u32_nowait(reason);
    serial::write_str_nowait(" rip=0x");
    write_hex_nowait(rip);
    serial::write_str_nowait(" tf=");
    write_dec_nowait(u64::from(tf));
    serial::write_str_nowait(" entry=");
    write_dec_nowait(u64::from(entry_fail));
    serial::write_str_nowait(" intr=0x");
    write_hex_nowait(intr);
    serial::write_byte_nowait(b'\n');
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
    serial::write_str(" catalog=");
    write_dec(crate::devices::ide_cdrom::eltorito_catalog_read() as u64);
    serial::write_str(" bootimg=");
    write_dec(crate::devices::ide_cdrom::eltorito_boot_image_read() as u64);
    serial::write_str(" readlba=");
    write_dec(crate::devices::ide_cdrom::last_read_lba() as u64);
    serial::write_str(" elt=");
    write_dec(ELTORITO_PRINTED.load(Ordering::Acquire) as u64);
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
    if atapi_read_evidence(sectors) {
        if ATAPI_AT_N.load(Ordering::Acquire) == 0 {
            ATAPI_AT_N.store(EXIT_COUNT.load(Ordering::Acquire), Ordering::Release);
        }
        if !ATAPI_PRINTED.swap(true, Ordering::AcqRel) {
            serial::write_line(M7_E5_OVMF_ATAPI_OK_MARKER);
            serial::write_str("boot: guest-UEFI atapi sectors=");
            write_dec(sectors as u64);
            serial::write_str(" catalog=");
            write_dec(crate::devices::ide_cdrom::eltorito_catalog_read() as u64);
            serial::write_str(" bootimg=");
            write_dec(crate::devices::ide_cdrom::eltorito_boot_image_read() as u64);
            serial::write_str(" packet=");
            write_dec(crate::devices::ide_cdrom::packet_commands() as u64);
            serial::write_str(" scsi=0x");
            write_hex_u32(u32::from(crate::devices::ide_cdrom::last_scsi()));
            serial::write_byte(b'\n');
            audit_log!(AuditEvent::OvmfGuestUefiAtapi {
                exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
                sectors: sectors as u64,
            });
        }
    }
    maybe_print_eltorito_progress();
    maybe_print_eltorito();
    maybe_print_dxe();
}

#[cfg(target_os = "uefi")]
fn maybe_print_eltorito_progress() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    let cat = crate::devices::ide_cdrom::eltorito_catalog_read();
    let img = crate::devices::ide_cdrom::eltorito_boot_image_read();
    if cat && !ELTORITO_CATALOG_PRINTED.swap(true, Ordering::AcqRel) {
        serial::write_str("boot: guest-UEFI eltorito-progress catalog=1 bootimg=");
        write_dec(img as u64);
        serial::write_byte(b'\n');
    }
    if img && !ELTORITO_BOOTIMG_PRINTED.swap(true, Ordering::AcqRel) {
        serial::write_str("boot: guest-UEFI eltorito-progress catalog=");
        write_dec(cat as u64);
        serial::write_str(" bootimg=1");
        serial::write_byte(b'\n');
    }
}

#[cfg(target_os = "uefi")]
fn maybe_print_eltorito() {
    if !PAST_SEC_PRINTED.load(Ordering::Acquire) {
        return;
    }
    if !eltorito_boot_evidence(
        crate::devices::ide_cdrom::eltorito_catalog_read(),
        crate::devices::ide_cdrom::eltorito_boot_image_read(),
        eltorito_payload_ran(ELTORITO_COM_MATCH.load(Ordering::Acquire)),
    ) {
        return;
    }
    if ELTORITO_PRINTED.swap(true, Ordering::AcqRel) {
        return;
    }
    serial::write_line(M7_E5_OVMF_ELTORITO_OK_MARKER);
    serial::write_str("boot: guest-UEFI eltorito catalog=1 bootimg=1 magic=1 sectors=");
    write_dec(crate::devices::ide_cdrom::sectors_read() as u64);
    serial::write_byte(b'\n');
    audit_log!(AuditEvent::OvmfGuestUefiEltorito {
        exits: NON_TF_EXITS.load(Ordering::Acquire) as u64,
        catalog: 1,
        boot_image: 1,
    });
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
    if guest_uefi_flash_off(linear).is_some() {
        let hpa = FLASH_HPA.load(Ordering::Acquire);
        let len = FLASH_LEN.load(Ordering::Acquire) as usize;
        if hpa == 0 || len == 0 {
            return 0;
        }
        // SAFETY: exclusive 4 MiB guest-private OVMF copy; VMX-root peek.
        // KANI-TARGET: identity peek flash (outside Proven Core).
        let flash = core::slice::from_raw_parts(hpa as *const u8, len);
        return copy_flash_at(flash, linear, buf);
    }
    if !guest_uefi_report_ram_should_map(linear) {
        return 0;
    }
    let hpa = report_ram_hpa_lookup_or_map(linear);
    if hpa == 0 {
        return 0;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already mapped for this GPA.
    // KANI-TARGET: identity peek report-RAM (outside Proven Core).
    let page = core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
    copy_report_ram_at(page, linear, buf)
}

/// One page of instruction bytes: identity GPA, then guest CR3 walk.
/// Linux virtio/IOAPIC runs with high-half RIP (not identity).
#[cfg(target_os = "uefi")]
unsafe fn copy_guest_linear_one_page(linear: u64, buf: &mut [u8]) -> usize {
    let n = copy_guest_identity_bytes(linear, buf);
    if n != 0 {
        return n;
    }
    let Some(gpa) = guest_linear_to_gpa(linear) else {
        return 0;
    };
    copy_guest_gpa_bytes(gpa, buf)
}

/// Instruction fetch for MMIO emulate. Loops across 4 KiB pages so a
/// `movl` that straddles a page is not truncated (`insn_len` then fails
/// decode and the EPT handler would spin).
#[cfg(target_os = "uefi")]
unsafe fn copy_guest_linear_bytes(linear: u64, buf: &mut [u8]) -> usize {
    let mut done = 0usize;
    while done < buf.len() {
        let n = copy_guest_linear_one_page(linear.wrapping_add(done as u64), &mut buf[done..]);
        if n == 0 {
            break;
        }
        done = done.saturating_add(n);
    }
    done
}

#[cfg(target_os = "uefi")]
unsafe fn copy_guest_gpa_bytes(gpa: u64, buf: &mut [u8]) -> usize {
    let Some(hpa) = guest_uefi_gpa_to_hpa(gpa) else {
        return 0;
    };
    let off = (gpa & 0xfff) as usize;
    let n = buf.len().min(4096 - off);
    if n == 0 {
        return 0;
    }
    // SAFETY: translate returned a host pointer in guest-UEFI RAM / report-RAM.
    // KANI-TARGET: MMIO insn fetch from GPA (outside Proven Core).
    core::ptr::copy_nonoverlapping(hpa as *const u8, buf.as_mut_ptr(), n);
    n
}

#[cfg(target_os = "uefi")]
unsafe fn write_guest_identity_bytes(linear: u64, buf: &[u8]) -> usize {
    let page_left = (0x1000 - (linear & 0xfff)) as usize;
    let want = buf.len().min(page_left);
    if want == 0 {
        return 0;
    }
    if linear < GUEST_UEFI_LOW_RAM_BYTES {
        let hpa = RAM_HPA.load(Ordering::Acquire);
        if hpa == 0 {
            return 0;
        }
        let start = linear as usize;
        if start >= GUEST_UEFI_LOW_RAM_BYTES as usize {
            return 0;
        }
        let n = want.min(GUEST_UEFI_LOW_RAM_BYTES as usize - start);
        // SAFETY: exclusive guest-UEFI 32 MiB slab; firmware is VMX-halted.
        // KANI-TARGET: identity poke low RAM for PUSH/POP (outside Proven Core).
        core::ptr::copy_nonoverlapping(buf.as_ptr(), (hpa as *mut u8).add(start), n);
        return n;
    }
    if !guest_uefi_report_ram_should_map(linear) {
        return 0;
    }
    let hpa = report_ram_hpa_lookup_or_map(linear);
    if hpa == 0 {
        return 0;
    }
    let off = guest_uefi_report_ram_page_off(linear) as usize;
    let n = want.min(GUEST_UEFI_REPORT_RAM_PAGE as usize - off);
    if n == 0 {
        return 0;
    }
    // SAFETY: exclusive 2 MiB report-RAM HPA already mapped for this GPA.
    // KANI-TARGET: identity poke report-RAM for PUSH/POP (outside Proven Core).
    core::ptr::copy_nonoverlapping(buf.as_ptr(), (hpa as *mut u8).add(off), n);
    n
}

#[cfg(target_os = "uefi")]
unsafe fn write_guest_gpa_bytes(gpa: u64, buf: &[u8]) -> usize {
    let Some(hpa) = guest_uefi_gpa_to_hpa(gpa) else {
        return 0;
    };
    let off = (gpa & 0xfff) as usize;
    let n = buf.len().min(4096 - off);
    if n == 0 {
        return 0;
    }
    // SAFETY: translate returned a host pointer in guest-UEFI RAM / report-RAM.
    // KANI-TARGET: MMIO PUSH/POP store to GPA (outside Proven Core).
    core::ptr::copy_nonoverlapping(buf.as_ptr(), hpa as *mut u8, n);
    n
}

#[cfg(target_os = "uefi")]
unsafe fn write_guest_linear_one_page(linear: u64, buf: &[u8]) -> usize {
    let n = write_guest_identity_bytes(linear, buf);
    if n != 0 {
        return n;
    }
    let Some(gpa) = guest_linear_to_gpa(linear) else {
        return 0;
    };
    write_guest_gpa_bytes(gpa, buf)
}

#[cfg(target_os = "uefi")]
unsafe fn write_guest_linear_bytes(linear: u64, buf: &[u8]) -> bool {
    let mut done = 0usize;
    while done < buf.len() {
        let n = write_guest_linear_one_page(linear.wrapping_add(done as u64), &buf[done..]);
        if n == 0 {
            return false;
        }
        done = done.saturating_add(n);
    }
    true
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_stack_op_size(op: crate::devices::guest_virtio_blk::MmioInsn) -> u8 {
    crate::devices::guest_virtio_blk::mmio_stack_width(
        op.size,
        guest_uefi_cs_ar_is_long(ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0)),
    )
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_stack_push(val: u64, size: u8) -> bool {
    let n = usize::from(size);
    if n == 0 || n > 8 {
        return false;
    }
    let rsp = ops::vmread(GUEST_RSP).unwrap_or(0);
    let new_rsp = rsp.wrapping_sub(n as u64);
    let bytes = val.to_le_bytes();
    if !write_guest_linear_bytes(new_rsp, &bytes[..n]) {
        return false;
    }
    ops::vmwrite(GUEST_RSP, new_rsp).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_stack_pop(size: u8) -> Option<u64> {
    let n = usize::from(size);
    if n == 0 || n > 8 {
        return None;
    }
    let rsp = ops::vmread(GUEST_RSP).unwrap_or(0);
    let mut buf = [0u8; 8];
    if copy_guest_linear_bytes(rsp, &mut buf[..n]) < n {
        return None;
    }
    let mut tmp = [0u8; 8];
    tmp[..n].copy_from_slice(&buf[..n]);
    let val = u64::from_le_bytes(tmp);
    if ops::vmwrite(GUEST_RSP, rsp.wrapping_add(n as u64)).is_err() {
        return None;
    }
    Some(val)
}

/// Near CALL (`FF /2`) or JMP (`FF /4`) through MMIO. Sets RIP to `target`.
/// CALL pushes RIP+len first. True = RIP already written (do not skip_insn).
/// False = stack GPA miss or VMWRITE fail (do not invent HPA).
#[cfg(target_os = "uefi")]
unsafe fn mmio_near_xfer(
    op: crate::devices::guest_virtio_blk::MmioInsn,
    target: u64,
    is_call: bool,
) -> bool {
    let size = mmio_stack_op_size(op);
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    if is_call {
        let len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(0);
        if !mmio_stack_push(rip.wrapping_add(len), size) {
            return false;
        }
    }
    let new_rip = if size == 2 {
        (rip & !0xFFFFu64) | (target & 0xFFFF)
    } else if size == 4 {
        target & 0xFFFF_FFFF
    } else {
        target
    };
    ops::vmwrite(GUEST_RIP, new_rip).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_set_addr_gpr(idx: u8, val: u64, long: bool) {
    if long {
        set_cr_gpr(idx, val);
    } else {
        set_cr_gpr(idx, (cr_gpr(idx) & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF));
    }
}

/// One MOVS/STOS/LODS/CMPS/SCAS element. `op.has_imm` is REP.
/// CMPS/SCAS: `op.imm != 0` is F2 REPNE; else F3 REPE. Stop on ZF.
/// None = RAM GPA miss (do not invent HPA).
/// Some(true) = insn done (skip). Some(false) = REP remaining (keep RIP).
#[cfg(target_os = "uefi")]
unsafe fn mmio_string_step(
    op: crate::devices::guest_virtio_blk::MmioInsn,
    ept_write: bool,
    read_bar: impl FnOnce() -> u64,
    write_bar: impl FnOnce(u64),
    gpa: u64,
) -> Option<bool> {
    let size = op.size;
    if size == 0 || size > 8 {
        return None;
    }
    let n = usize::from(size);
    let long = guest_uefi_cs_ar_is_long(ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0));
    let rcx = guest_uefi_io_addr_reg(cr_gpr(1), long);
    if op.has_imm && rcx == 0 {
        return Some(true);
    }
    let df = (ops::vmread(GUEST_RFLAGS).unwrap_or(0x2) & (1 << 10)) != 0;
    let rsi = guest_uefi_io_addr_reg(cr_gpr(6), long);
    let rdi = guest_uefi_io_addr_reg(cr_gpr(7), long);
    let mut newf: Option<u64> = None;
    if crate::devices::guest_virtio_blk::mmio_alu_is_movs(op.alu) {
        if ept_write {
            let mut buf = [0u8; 8];
            if copy_guest_linear_bytes(rsi, &mut buf[..n]) < n {
                return None;
            }
            let mut tmp = [0u8; 8];
            tmp[..n].copy_from_slice(&buf[..n]);
            write_bar(u64::from_le_bytes(tmp));
        } else {
            let val = read_bar();
            let bytes = val.to_le_bytes();
            if !write_guest_linear_bytes(rdi, &bytes[..n]) {
                return None;
            }
        }
        mmio_set_addr_gpr(6, guest_uefi_io_string_advance(rsi, size, df), long);
        mmio_set_addr_gpr(7, guest_uefi_io_string_advance(rdi, size, df), long);
    } else if crate::devices::guest_virtio_blk::mmio_alu_is_stos(op.alu) {
        write_bar(mmio_gpr_in(op));
        mmio_set_addr_gpr(7, guest_uefi_io_string_advance(rdi, size, df), long);
    } else if crate::devices::guest_virtio_blk::mmio_alu_is_lods(op.alu) {
        mmio_gpr_out(op, read_bar());
        mmio_set_addr_gpr(6, guest_uefi_io_string_advance(rsi, size, df), long);
    } else if crate::devices::guest_virtio_blk::mmio_alu_is_cmps(op.alu) {
        let rsi_g = guest_linear_to_gpa(rsi);
        let rdi_g = guest_linear_to_gpa(rdi);
        let rsi_bar = rsi_g == Some(gpa);
        let rdi_bar = rdi_g == Some(gpa);
        let (left, right) = if rsi_bar || (!rdi_bar && !ept_write) {
            let mut buf = [0u8; 8];
            if copy_guest_linear_bytes(rdi, &mut buf[..n]) < n {
                return None;
            }
            let mut tmp = [0u8; 8];
            tmp[..n].copy_from_slice(&buf[..n]);
            (read_bar(), u64::from_le_bytes(tmp))
        } else {
            let mut buf = [0u8; 8];
            if copy_guest_linear_bytes(rsi, &mut buf[..n]) < n {
                return None;
            }
            let mut tmp = [0u8; 8];
            tmp[..n].copy_from_slice(&buf[..n]);
            (u64::from_le_bytes(tmp), read_bar())
        };
        let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
        let f = crate::devices::guest_virtio_blk::mmio_cmp_rflags(oldf, left, right, size);
        let _ = ops::vmwrite(GUEST_RFLAGS, f);
        newf = Some(f);
        mmio_set_addr_gpr(6, guest_uefi_io_string_advance(rsi, size, df), long);
        mmio_set_addr_gpr(7, guest_uefi_io_string_advance(rdi, size, df), long);
    } else if crate::devices::guest_virtio_blk::mmio_alu_is_scas(op.alu) {
        let left = mmio_gpr_in(op);
        let right = read_bar();
        let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
        let f = crate::devices::guest_virtio_blk::mmio_cmp_rflags(oldf, left, right, size);
        let _ = ops::vmwrite(GUEST_RFLAGS, f);
        newf = Some(f);
        mmio_set_addr_gpr(7, guest_uefi_io_string_advance(rdi, size, df), long);
    } else {
        return None;
    }
    if op.has_imm {
        let left = rcx.saturating_sub(1);
        mmio_set_addr_gpr(1, left, long);
        let keep = if left == 0 {
            false
        } else if crate::devices::guest_virtio_blk::mmio_alu_is_cmps(op.alu)
            || crate::devices::guest_virtio_blk::mmio_alu_is_scas(op.alu)
        {
            let zf = newf.map(|f| (f & (1 << 6)) != 0).unwrap_or(false);
            if op.imm != 0 {
                !zf
            } else {
                zf
            }
        } else {
            true
        };
        if keep {
            return Some(false);
        }
    }
    Some(true)
}

#[cfg(target_os = "uefi")]
unsafe fn read_guest_pte(gpa: u64) -> Option<u64> {
    let hpa = guest_uefi_gpa_to_hpa(gpa)?;
    // SAFETY: 8-byte PTE in guest-UEFI RAM / report-RAM.
    Some(core::ptr::read_unaligned(hpa as *const u64))
}

/// 4-level walk of the live guest CR3 (Linux high-half RIP → GPA).
#[cfg(target_os = "uefi")]
unsafe fn guest_linear_to_gpa(linear: u64) -> Option<u64> {
    let cr3 = ops::vmread(GUEST_CR3).unwrap_or(0) & !0xfff;
    if cr3 == 0 {
        return None;
    }
    let pml4e = read_guest_pte(cr3 + 8 * ((linear >> 39) & 0x1ff))?;
    if pml4e & 1 == 0 {
        return None;
    }
    let pdpt = pml4e & 0x000f_ffff_ffff_f000;
    let pdpte = read_guest_pte(pdpt + 8 * ((linear >> 30) & 0x1ff))?;
    if pdpte & 1 == 0 {
        return None;
    }
    if pdpte & (1 << 7) != 0 {
        return Some((pdpte & 0x000f_ffff_c000_0000) | (linear & 0x3fff_ffff));
    }
    let pd = pdpte & 0x000f_ffff_ffff_f000;
    let pde = read_guest_pte(pd + 8 * ((linear >> 21) & 0x1ff))?;
    if pde & 1 == 0 {
        return None;
    }
    if pde & (1 << 7) != 0 {
        return Some((pde & 0x000f_ffff_ffe0_0000) | (linear & 0x1f_ffff));
    }
    let pt = pde & 0x000f_ffff_ffff_f000;
    let pte = read_guest_pte(pt + 8 * ((linear >> 12) & 0x1ff))?;
    if pte & 1 == 0 {
        return None;
    }
    Some((pte & 0x000f_ffff_ffff_f000) | (linear & 0xfff))
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

/// Re-inject a pin-exited NMI into product-ISO Linux (type 2, not HW #2).
///
/// Iron `1a2544d`: `Freeing initrd` then `restore host xcr0`. linux NMI
/// inject. Share hushes a `write_str` dump. iso=0 does not take this path.
#[cfg(target_os = "uefi")]
unsafe fn handle_linux_nmi() -> bool {
    LINUX_EXC_INJECT.store(true, Ordering::Release);
    let _ = ops::vmwrite(
        VM_ENTRY_INTERRUPTION_INFO,
        u64::from(guest_uefi_nmi_entry_info()),
    );
    true
}

#[cfg(target_os = "uefi")]
unsafe fn handle_linux_hw_exception(vec: u8, deliver_code: bool) -> bool {
    LINUX_EXC_INJECT.store(true, Ordering::Release);
    let mut bmp = guest_uefi_linux_exception_bitmap();
    if vec < 32 {
        bmp &= !(1u32 << vec);
    }
    let _ = ops::vmwrite(EXCEPTION_BITMAP, u64::from(bmp));
    if deliver_code {
        let err = ops::vmread(VM_EXIT_INTR_ERROR_CODE).unwrap_or(0);
        let _ = ops::vmwrite(VM_ENTRY_EXCEPTION_ERROR_CODE, err);
    }
    let _ = ops::vmwrite(
        VM_ENTRY_INTERRUPTION_INFO,
        u64::from(guest_uefi_hw_exception_entry_info(vec, deliver_code)),
    );
    let n = PF_LINUX_DELIVER.fetch_add(1, Ordering::AcqRel);
    if n == 0
        && guest_uefi_linux_earlycon_share_on_linux_deliver(
            true,
            crate::devices::ide_cdrom::product_iso_window_armed(),
        )
    {
        // linux earlycon share TX ring: HV ticks / scan bump enqueue so they
        // do not wait THR_WAIT_SPINS while guest printk is live.
        serial::set_linux_earlycon_share(true);
    }
    let linux_iso = guest_uefi_linux_earlycon_share_on_linux_deliver(
        true,
        crate::devices::ide_cdrom::product_iso_window_armed(),
    );
    if n < 4 && !linux_iso {
        // linux earlycon skip exc deliver (same hush as skip #PF dump).
        serial::write_str("boot: guest-UEFI linux exc deliver n=");
        write_dec(u64::from(n) + 1);
        serial::write_str(" vec=0x");
        write_hex(u64::from(vec));
        serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
    }
    true
}

#[cfg(target_os = "uefi")]
unsafe fn handle_exception_nmi(intr: u64, rip: u64, linear: u64, qual: u64) -> bool {
    let valid = (intr & (1u64 << 31)) != 0;
    let vec = (intr & 0xff) as u8;
    if valid && vec == 6 {
        if guest_uefi_pf_should_deliver_to_guest(rip) {
            return handle_linux_hw_exception(6, false);
        }
        return handle_ud(rip, linear);
    }
    if valid && vec == 13 && guest_uefi_pf_should_deliver_to_guest(rip) {
        return handle_linux_hw_exception(13, true);
    }
    if valid && vec == 14 {
        return handle_pf(rip, linear, qual);
    }
    let linux = guest_uefi_linux_guest_active(
        serial::linux_earlycon_share(),
        guest_uefi_pf_should_deliver_to_guest(rip),
        PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
    );
    if valid && guest_uefi_linux_nmi_should_inject(linux, vec) {
        return handle_linux_nmi();
    }
    if valid && linux {
        // linux unhandled nowait stop: LINUX_EXCEPTION_BITMAP #DE/#DF/#AC
        // must not silent-stop after initrd (iron 1a2544d). Inject; drop
        // the vector from the bitmap so the inject does not re-exit.
        return handle_linux_hw_exception(vec, guest_uefi_linux_exc_error_code(vec));
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
    let linux_iso = guest_uefi_linux_earlycon_share_on_linux_deliver(
        guest_uefi_pf_should_deliver_to_guest(rip),
        crate::devices::ide_cdrom::product_iso_window_armed(),
    );
    if linux_iso {
        // linux earlycon skip #PF dump (iron 9a3cbfa dump + cpuid shredded printk).
        serial::set_linux_earlycon_share(true);
    }
    if !linux_iso {
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
    }
    if guest_uefi_pf_should_deliver_to_guest(rip) {
        // Iron e40bee0: long-mode Linux #PF on the direct map after extra
        // DRAM + Loaded initrd. Do not rebuild OVMF identity tables.
        // Drop #PF/#UD/#GP intercept first (injected #PF would re-exit;
        // M3.10: leftover #UD intercept skips Linux ud2 / alternatives).
        // Restore CR2, then VM-entry inject vector 14 so PIC/LAPIC cannot
        // steal the entry. early_make_pgtable stays in the guest after this.
        // SAFETY: VMX-root; cr2 is the guest #PF linear (canonical).
        // KANI-TARGET: guest-UEFI Linux #PF CR2 restore (outside Proven Core).
        cpu::write_cr2(cr2);
        PF_LINUX_CR2.store(cr2, Ordering::Release);
        LINUX_EXC_INJECT.store(true, Ordering::Release);
        let _ = ops::vmwrite(
            EXCEPTION_BITMAP,
            u64::from(guest_uefi_linux_exception_bitmap()),
        );
        let _ = ops::vmwrite(VM_ENTRY_EXCEPTION_ERROR_CODE, err);
        let _ = ops::vmwrite(
            VM_ENTRY_INTERRUPTION_INFO,
            u64::from(guest_uefi_linux_pf_entry_info()),
        );
        let n = PF_LINUX_DELIVER.fetch_add(1, Ordering::AcqRel);
        if n == 0
            && guest_uefi_linux_earlycon_share_on_linux_deliver(
                true,
                crate::devices::ide_cdrom::product_iso_window_armed(),
            )
        {
            // linux earlycon share TX ring (iron 202312f e820 cut after
            // blocking hypervisor-scan bump).
            serial::set_linux_earlycon_share(true);
        }
        if n < 4 && !linux_iso {
            serial::write_str("boot: guest-UEFI #PF linux deliver n=");
            write_dec(u64::from(n) + 1);
            serial::write_str(" cr2=0x");
            write_hex(cr2);
            serial::write_str(" err=0x");
            write_hex(err);
            serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
        }
        return true;
    }
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
        // CR8 is emulated (no VMCS GUEST_CR8). Store: APIC_TPR = (val&0xF)<<4.
        (8, 0) => crate::devices::lapic_virt::set_cr8(cr_gpr(gpr)),
        (8, 1) => set_cr_gpr(gpr, crate::devices::lapic_virt::cr8()),
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
unsafe fn mmio_gpr_in(op: crate::devices::guest_virtio_blk::MmioInsn) -> u64 {
    if op.size == 1 && !op.rex && (4..8).contains(&op.reg) {
        (cr_gpr(op.reg - 4) >> 8) & 0xff
    } else {
        cr_gpr(op.reg)
    }
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_gpr_out(op: crate::devices::guest_virtio_blk::MmioInsn, val: u64) {
    if op.size == 1 && !op.rex && (4..8).contains(&op.reg) {
        let g = op.reg - 4;
        let old = cr_gpr(g);
        set_cr_gpr(g, (old & !0xFF00) | ((val & 0xff) << 8));
        return;
    }
    set_cr_gpr(
        op.reg,
        merge_mmio_gpr(cr_gpr(op.reg), val, op.size, op.zero_ext, op.sign_ext),
    );
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_xmm_in(reg: u8) -> u128 {
    let i = (reg as usize) & 15;
    // SAFETY: SAVED_XMM is 16-byte aligned; VMX-root exclusive after trampoline.
    // KANI-TARGET: guest-UEFI XMM snapshot (outside Proven Core).
    core::ptr::read(SAVED_XMM.0.as_ptr().cast::<u128>().add(i))
}

#[cfg(target_os = "uefi")]
unsafe fn mmio_xmm_out(reg: u8, val: u128) {
    let i = (reg as usize) & 15;
    // SAFETY: SAVED_XMM is 16-byte aligned; VMX-root exclusive before resume.
    // KANI-TARGET: guest-UEFI XMM snapshot (outside Proven Core).
    core::ptr::write(SAVED_XMM.0.as_mut_ptr().cast::<u128>().add(i), val);
}

/// TEST/CMP: update GUEST_RFLAGS, do not store. Returns true when handled.
#[cfg(target_os = "uefi")]
unsafe fn mmio_apply_test_cmp(op: crate::devices::guest_virtio_blk::MmioInsn, cur: u64) -> bool {
    if !op.test && !op.cmp {
        return false;
    }
    let rhs = if op.has_imm {
        op.imm
    } else {
        mmio_gpr_in(op)
    };
    let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
    let newf = if op.cmp {
        let (left, right) = if op.cmp_reg_left {
            (rhs, cur)
        } else {
            (cur, rhs)
        };
        crate::devices::guest_virtio_blk::mmio_cmp_rflags(oldf, left, right, op.size)
    } else {
        crate::devices::guest_virtio_blk::mmio_test_rflags(oldf, cur & rhs, op.size)
    };
    let _ = ops::vmwrite(GUEST_RFLAGS, newf);
    true
}

/// ALU RMW: dest is MMIO (`left = mem`) or GPR (`alu_reg_left`). Updates RFLAGS.
#[cfg(target_os = "uefi")]
unsafe fn mmio_alu_result(op: crate::devices::guest_virtio_blk::MmioInsn, mem: u64) -> u64 {
    let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
    let cf = (oldf & 1) != 0;
    if crate::devices::guest_virtio_blk::mmio_alu_is_scan(op.alu) {
        let (idx, src_zero) = crate::devices::guest_virtio_blk::mmio_scan_apply(
            mem,
            op.size,
            op.alu == crate::devices::guest_virtio_blk::MMIO_ALU_BSR,
        );
        let _ = ops::vmwrite(
            GUEST_RFLAGS,
            crate::devices::guest_virtio_blk::mmio_scan_rflags(oldf, src_zero),
        );
        if src_zero {
            return mmio_gpr_in(op);
        }
        return idx;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_count_zero(op.alu) {
        let (idx, src_zero) = if op.alu == crate::devices::guest_virtio_blk::MMIO_ALU_TZCNT {
            crate::devices::guest_virtio_blk::mmio_tzcnt_apply(mem, op.size)
        } else {
            crate::devices::guest_virtio_blk::mmio_lzcnt_apply(mem, op.size)
        };
        let _ = ops::vmwrite(
            GUEST_RFLAGS,
            crate::devices::guest_virtio_blk::mmio_tzcnt_rflags(oldf, idx, src_zero),
        );
        return idx;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_popcnt(op.alu) {
        let result = crate::devices::guest_virtio_blk::mmio_popcnt_apply(mem, op.size);
        let src_zero = crate::devices::guest_virtio_blk::mmio_eq(mem, 0, op.size);
        let _ = ops::vmwrite(
            GUEST_RFLAGS,
            crate::devices::guest_virtio_blk::mmio_popcnt_rflags(oldf, src_zero),
        );
        return result;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_shift(op.alu) {
        let count = if op.has_imm {
            op.imm
        } else {
            cr_gpr(1) & 0xff
        };
        let result = crate::devices::guest_virtio_blk::mmio_shift_apply(
            mem, count, op.alu, op.size, cf,
        );
        let newf = crate::devices::guest_virtio_blk::mmio_shift_rflags(
            oldf, mem, count, result, op.alu, op.size,
        );
        let _ = ops::vmwrite(GUEST_RFLAGS, newf);
        return result;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_double_shift(op.alu) {
        let count = if op.has_imm {
            op.imm
        } else {
            cr_gpr(1) & 0xff
        };
        let src = mmio_gpr_in(op);
        let result = crate::devices::guest_virtio_blk::mmio_double_shift_apply(
            mem, src, count, op.alu, op.size,
        );
        let newf = crate::devices::guest_virtio_blk::mmio_double_shift_rflags(
            oldf, mem, src, count, result, op.alu, op.size,
        );
        let _ = ops::vmwrite(GUEST_RFLAGS, newf);
        return result;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_mul_pair(op.alu) {
        let mut ax_src = op;
        ax_src.reg = 0;
        let ax = mmio_gpr_in(ax_src);
        let signed = op.alu == crate::devices::guest_virtio_blk::MMIO_ALU_IMUL1;
        let (lo, hi, overflow) =
            crate::devices::guest_virtio_blk::mmio_mul_pair_apply(ax, mem, op.size, signed);
        let _ = ops::vmwrite(
            GUEST_RFLAGS,
            crate::devices::guest_virtio_blk::mmio_imul_rflags(oldf, overflow),
        );
        let mut dest = op;
        dest.reg = 0;
        dest.size = if op.size == 1 { 2 } else { op.size };
        dest.zero_ext = dest.size == 4;
        dest.rex = true;
        mmio_gpr_out(dest, lo);
        if op.size > 1 {
            dest.reg = 2;
            mmio_gpr_out(dest, hi);
        }
        return lo;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_imul(op.alu) {
        let gpr = mmio_gpr_in(op);
        let (left, right) = if op.has_imm {
            (mem, op.imm)
        } else {
            (gpr, mem)
        };
        let (result, overflow) =
            crate::devices::guest_virtio_blk::mmio_imul_apply(left, right, op.size);
        let _ = ops::vmwrite(
            GUEST_RFLAGS,
            crate::devices::guest_virtio_blk::mmio_imul_rflags(oldf, overflow),
        );
        return result;
    }
    let other = if op.has_imm {
        op.imm
    } else {
        mmio_gpr_in(op)
    };
    let (left, right) = if op.alu_reg_left {
        (other, mem)
    } else {
        (mem, other)
    };
    let result =
        crate::devices::guest_virtio_blk::mmio_alu_apply_cf(left, right, op.alu, cf);
    let newf = crate::devices::guest_virtio_blk::mmio_alu_rflags(
        oldf, left, right, result, op.alu, op.size,
    );
    let _ = ops::vmwrite(GUEST_RFLAGS, newf);
    result
}

/// DIV/IDIV into AX or DX:AX. Returns false when the guest must take #DE.
#[cfg(target_os = "uefi")]
unsafe fn mmio_div_pair_commit(
    op: crate::devices::guest_virtio_blk::MmioInsn,
    mem: u64,
) -> bool {
    let mut ax_src = op;
    ax_src.reg = 0;
    let ax = mmio_gpr_in(ax_src);
    let dx = if op.size > 1 {
        let mut dx_src = op;
        dx_src.reg = 2;
        mmio_gpr_in(dx_src)
    } else {
        0
    };
    let signed = op.alu == crate::devices::guest_virtio_blk::MMIO_ALU_IDIV;
    let Some((lo, hi)) =
        crate::devices::guest_virtio_blk::mmio_div_apply(ax, dx, mem, op.size, signed)
    else {
        return false;
    };
    let mut dest = op;
    dest.reg = 0;
    dest.size = if op.size == 1 { 2 } else { op.size };
    dest.zero_ext = dest.size == 4;
    dest.rex = true;
    mmio_gpr_out(dest, lo);
    if op.size > 1 {
        dest.reg = 2;
        mmio_gpr_out(dest, hi);
    }
    true
}

/// Inject #DE at the faulting RIP. Do not skip the insn (SDM: #DE has no error code).
#[cfg(target_os = "uefi")]
unsafe fn inject_mmio_div_de() -> bool {
    let _ = ops::vmwrite(
        VM_ENTRY_INTERRUPTION_INFO,
        crate::devices::guest_virtio_blk::MMIO_DIV_DE_INTR_INFO,
    );
    true
}

/// BT/BTS/BTR/BTC: CF = old bit; store for all but BT. Returns new mem value.
#[cfg(target_os = "uefi")]
unsafe fn mmio_apply_bt(op: crate::devices::guest_virtio_blk::MmioInsn, cur: u64) -> u64 {
    let bit = if op.has_imm {
        op.imm
    } else {
        mmio_gpr_in(op)
    };
    let (new, was) =
        crate::devices::guest_virtio_blk::mmio_bt_apply(cur, bit, op.size, op.bt);
    let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
    let _ = ops::vmwrite(
        GUEST_RFLAGS,
        crate::devices::guest_virtio_blk::mmio_bt_rflags(oldf, was),
    );
    new
}

/// CMPXCHG / XADD / CMPXCHG8B. Returns `Some(new_mem)` when MMIO must be stored.
#[cfg(target_os = "uefi")]
unsafe fn mmio_apply_atomic(
    op: crate::devices::guest_virtio_blk::MmioInsn,
    cur: u64,
) -> Option<u64> {
    if op.atomic == crate::devices::guest_virtio_blk::MMIO_CMPXCHG {
        let mut acc = op;
        acc.reg = 0;
        acc.zero_ext = op.size == 4;
        let a = mmio_gpr_in(acc);
        let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
        let newf =
            crate::devices::guest_virtio_blk::mmio_cmp_rflags(oldf, a, cur, op.size);
        let _ = ops::vmwrite(GUEST_RFLAGS, newf);
        if crate::devices::guest_virtio_blk::mmio_eq(a, cur, op.size) {
            Some(mmio_gpr_in(op))
        } else {
            mmio_gpr_out(acc, cur);
            None
        }
    } else if op.atomic == crate::devices::guest_virtio_blk::MMIO_XADD {
        let r = mmio_gpr_in(op);
        let sum = crate::devices::guest_virtio_blk::mmio_alu_apply(
            cur,
            r,
            crate::devices::guest_virtio_blk::MMIO_ALU_ADD,
        );
        let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
        let newf = crate::devices::guest_virtio_blk::mmio_alu_rflags(
            oldf,
            cur,
            r,
            sum,
            crate::devices::guest_virtio_blk::MMIO_ALU_ADD,
            op.size,
        );
        let _ = ops::vmwrite(GUEST_RFLAGS, newf);
        mmio_gpr_out(op, cur);
        Some(sum)
    } else if op.atomic == crate::devices::guest_virtio_blk::MMIO_CMPXCHG8B {
        let mut half = op;
        half.size = 4;
        half.zero_ext = true;
        half.rex = true;
        half.reg = 0;
        let eax = mmio_gpr_in(half) & 0xffff_ffff;
        half.reg = 2;
        let edx = mmio_gpr_in(half) & 0xffff_ffff;
        half.reg = 3;
        let ebx = mmio_gpr_in(half) & 0xffff_ffff;
        half.reg = 1;
        let ecx = mmio_gpr_in(half) & 0xffff_ffff;
        let acc = (edx << 32) | eax;
        let desired = (ecx << 32) | ebx;
        let (out, matched) =
            crate::devices::guest_virtio_blk::mmio_cmpxchg8b_apply(cur, acc, desired);
        let oldf = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
        let newf =
            crate::devices::guest_virtio_blk::mmio_cmp_rflags(oldf, acc, cur, 8);
        let _ = ops::vmwrite(GUEST_RFLAGS, newf);
        if matched {
            Some(out)
        } else {
            half.reg = 0;
            mmio_gpr_out(half, cur);
            half.reg = 2;
            mmio_gpr_out(half, cur >> 32);
            None
        }
    } else {
        None
    }
}

/// CMOVcc: GPR = mem if taken. SETcc: Some(0/1) to store.
#[cfg(target_os = "uefi")]
unsafe fn mmio_apply_cc(op: crate::devices::guest_virtio_blk::MmioInsn, cur: u64) -> Option<u64> {
    let flags = ops::vmread(GUEST_RFLAGS).unwrap_or(0x2);
    let taken = crate::devices::guest_virtio_blk::mmio_cc_taken(op.cc, flags);
    if op.is_write {
        Some(if taken { 1 } else { 0 })
    } else {
        if taken {
            mmio_gpr_out(op, cur);
        }
        None
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
    let hpa = report_ram_hpa_lookup_or_map(linear);
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
    guest_uefi_paint_vga_uc_live_now(cr3);
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

/// Iron `ddbd866`: GPA0 VGA PTEs stayed WB (`pte_a0000=0xa0067`) after
/// coerced FIX UC. PAT-UC framebuffer 4 K leaves on the live CR3.
/// Iron `e368e86`: also painted option-ROM `C0000`; leave that WB.
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_paint_vga_uc_live_now(cr3: u64) {
    if cr3 == 0 {
        return;
    }
    let n = guest_uefi_pt_paint_vga_uc(
        |g| unsafe { peek_low_u64(g) },
        |g, v| unsafe { poke_guest_u64(g, v) },
        cr3,
    );
    if n == 0 {
        return;
    }
    let _ = ops::vmwrite(GUEST_CR3, cr3);
    if LIVE_VGA_PT_PAINTED.fetch_add(1, Ordering::AcqRel) == 0 {
        serial::write_str("boot: guest-UEFI VGA 4K live PT PAT-UC n=");
        write_dec(u64::from(n));
        serial::write_str(" cr3=0x");
        write_hex(cr3);
        serial::write_str(" pte_a0000=0x");
        write_hex(dump_walk_pte(
            cr3,
            crate::vmx::guest_pt::IDENTITY_VGA_A0000,
        ));
        serial::write_str(" pte_c0000=0x");
        write_hex(dump_walk_pte(
            cr3,
            crate::vmx::guest_pt::IDENTITY_VGA_C0000,
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
    {
        let mut call = [0u8; 16];
        let nc = read_low_ram_insn(ret.wrapping_sub(5), &mut call);
        if nc >= 5 && call[0] == 0xE8 {
            let rel = i32::from_le_bytes([call[1], call[2], call[3], call[4]]);
            let tgt = ret.wrapping_add(rel as u64);
            serial::write_str(" calltgt=0x");
            write_hex(tgt);
            serial::write_str(" tgthex=");
            dump_low_ram_hex(tgt, 16);
        }
    }
    serial::write_str(" retcmp=");
    dump_low_ram_hex(
        guest_uefi_assert_retcmp_gpa(ret),
        GUEST_UEFI_ASSERT_PREHEX_BYTES,
    );
    serial::write_str(" retpre=");
    dump_low_ram_hex(
        ret.wrapping_sub(GUEST_UEFI_ASSERT_PREHEX_BYTES as u64),
        GUEST_UEFI_ASSERT_PREHEX_BYTES,
    );
    {
        let mut mov = [0u8; 16];
        let nm = read_low_ram_insn(ret.wrapping_sub(16), &mut mov);
        if nm >= 9 && mov[0] == 0x66 && mov[1] == 0xC7 && mov[2] == 0x05 {
            let disp = u32::from_le_bytes([mov[3], mov[4], mov[5], mov[6]]);
            serial::write_str(" g16=");
            dump_low_ram_hex(guest_uefi_assert_retpre_word_gpa(ret, disp), 2);
        }
    }
    serial::write_str(" rbx=0x");
    write_hex(SAVED_RBX);
    serial::write_str(" rsi=0x");
    write_hex(SAVED_RSI);
    serial::write_str(" rdi=0x");
    write_hex(SAVED_RDI);
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
    serial::write_str(" prehex=");
    dump_low_ram_hex(
        guest_uefi_assert_prehex_gpa(caller_rip),
        GUEST_UEFI_ASSERT_PREHEX_BYTES,
    );
    serial::write_str(" rax=0x");
    write_hex(SAVED_RAX);
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
    serial::write_str(" mtrr259=0x");
    write_hex(guest_uefi_mtrr_read(0x259).unwrap_or(0));
    serial::write_str(" mtrr268=0x");
    write_hex(guest_uefi_mtrr_read(0x268).unwrap_or(0));
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
    serial::write_str(" flushjnz=");
    write_dec(u64::from(guest_uefi_count_cpu_flush_jnz_mapped()));
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
    let mut buf = [0u8; 16];
    // High-half Linux RIP is not identity (nested f1afc27 delay_loop).
    // Walk guest CR3 the same as the tick insn dump.
    let n = copy_guest_linear_bytes(linear, &mut buf);
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
    // Inner skip-5 lands on `3: dec %rax; jnz 1b`. RAX=1 → 3: becomes 0
    // and falls through to ret. Skip-10 already lands on ret.
    if preempt_deadloop_delay_loop_sets_rax_one(&buf[..n]) {
        SAVED_RAX = 1;
        if LINUX_DELAY_LOOP_SKIP
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            serial::write_line(
                "boot: guest-UEFI linux delay_loop skip (Stage 46; not ISO-INSTALL-OK)",
            );
        }
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(len)).is_ok()
}

#[cfg(target_os = "uefi")]
unsafe fn dump_low_ram_insn(linear: u64) {
    let mut buf = [0u8; 16];
    let n = copy_guest_linear_bytes(linear, &mut buf);
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
    let mut buf = [0u8; 32];
    let n = nmax.min(32);
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
    let vmcs = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(0);
    // Prefer VMCS 1-15, then this-exit MMIO scratch (zeroed at vmexit so
    // CPUID/HLT/INVLPG cannot consume a prior BAR length). High-half
    // virtio MMIO still skips via that scratch when copy_mmio_insn ran.
    let mut len = guest_uefi_mmio_skip_len(vmcs, MMIO_INSN_LEN.load(Ordering::Relaxed));
    if len == 0 && guest_uefi_pf_should_deliver_to_guest(rip) {
        let mut buf = [0u8; 16];
        let n = copy_guest_linear_bytes(rip, &mut buf);
        len = guest_uefi_linux_fixed_skip_len(&buf[..n]);
    }
    if len == 0 {
        return false;
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(len)).is_ok()
}

/// 2-byte intercept skip: VMCS len, else decode, else high-half +2.
/// CPUID / RDMSR / WRMSR / RDTSC / INVD / WBINVD / PAUSE.
#[cfg(target_os = "uefi")]
unsafe fn skip_cpuid_msr() -> bool {
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let mut skipped = skip_insn();
    if !skipped {
        let extra = guest_uefi_linux_cpuid_msr_skip(rip, 0, &[]);
        if extra != 0 {
            let k = LINUX_SKIP2.fetch_add(1, Ordering::AcqRel);
            if k < 8 {
                serial::write_str("boot: guest-UEFI linux skip-2 n=");
                write_dec(u64::from(k) + 1);
                serial::write_str(" rip=0x");
                write_hex(rip);
                serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
            }
            skipped = ops::vmwrite(GUEST_RIP, rip.wrapping_add(extra)).is_ok();
        }
    }
    let after = ops::vmread(GUEST_RIP).unwrap_or(0);
    let force = guest_uefi_linux_cpuid_force_skip(rip, after);
    if force != 0 {
        let k = LINUX_SKIP2.fetch_add(1, Ordering::AcqRel);
        if k < 8 {
            serial::write_str("boot: guest-UEFI linux skip-2 force n=");
            write_dec(u64::from(k) + 1);
            serial::write_str(" rip=0x");
            write_hex(rip);
            serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
        }
        return ops::vmwrite(GUEST_RIP, rip.wrapping_add(force)).is_ok();
    }
    skipped
}

/// HLT: skip VMCS len, else `F4`, else high-half +1.
#[cfg(target_os = "uefi")]
unsafe fn skip_hlt() -> bool {
    if skip_insn() {
        return true;
    }
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let extra = guest_uefi_linux_hlt_skip(rip, 0, &[]);
    if extra == 0 {
        return false;
    }
    let k = LINUX_SKIP2.fetch_add(1, Ordering::AcqRel);
    if k < 8 {
        serial::write_str("boot: guest-UEFI linux skip-1 n=");
        write_dec(u64::from(k) + 1);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
    }
    ops::vmwrite(GUEST_RIP, rip.wrapping_add(extra)).is_ok()
}

/// INVLPG: VMCS len, else decode `0F 01 /7`. Empty fetch does not guess.
#[cfg(target_os = "uefi")]
unsafe fn skip_invlpg() -> bool {
    if skip_insn() {
        return true;
    }
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let k = LINUX_INVLPG_MISS.fetch_add(1, Ordering::AcqRel);
    if k < 8 {
        serial::write_str("boot: guest-UEFI linux invlpg miss n=");
        write_dec(u64::from(k) + 1);
        serial::write_str(" rip=0x");
        write_hex(rip);
        serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
    }
    false
}

/// Fetch MMIO instruction bytes at CS.base+RIP (or RIP in 64-bit CS).
/// If that miss is outside flash but GUEST_RIP is inside, peek RIP.
/// Returns `(fetched_n, effective_len)` where `effective_len` is VMCS 1–15
/// or the length decoded from those bytes when VMCS `insn_len` is 0.
#[cfg(target_os = "uefi")]
unsafe fn copy_mmio_insn(buf: &mut [u8]) -> (usize, u64) {
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    let cs_base = ops::vmread(GUEST_CS_BASE).unwrap_or(0);
    let ar = ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0);
    let long64 = guest_uefi_cs_ar_is_long(ar);
    let peek = guest_uefi_mmio_peek_linear(rip, cs_base, long64);
    let mut n = copy_guest_linear_bytes(peek, buf);
    if n == 0 && rip != peek {
        n = copy_guest_linear_bytes(rip, buf);
    }
    let vmcs_len = ops::vmread(VM_EXIT_INSTRUCTION_LEN).unwrap_or(0);
    let effective = crate::devices::guest_virtio_blk::mmio_effective_len(
        &buf[..n],
        vmcs_len,
        long64,
    );
    MMIO_INSN_LEN.store(effective, Ordering::Relaxed);
    (n, effective)
}

#[cfg(target_os = "uefi")]
unsafe fn store_guest_io(linear: u64, size: u8, val: u64) -> bool {
    if !guest_uefi_io_string_dest_ok(linear) {
        return false;
    }
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
    // Iron COM2: GRUB `rep insw` into GCD heap never EPT-walks, so an
    // unmapped report-RAM GPA silently dropped ATAPI bytes (EFI stub
    // gzip Z_DATA_ERROR with a full-looking PIO). PUSH/POP / virtqueue
    // share [`report_ram_hpa_lookup_or_map`]. Do not invent HPA (ADR-004).
    let hpa = report_ram_hpa_lookup_or_map(linear);
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
    let hpa = report_ram_hpa_lookup_or_map(linear);
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
            if !guest_uefi_io_string_dest_ok(addr) {
                static HV: AtomicBool = AtomicBool::new(false);
                if !HV.swap(true, Ordering::AcqRel) {
                    serial::write_line(
                        "boot: guest-UEFI fw_cfg string skip HV identity (not ISO-INSTALL-OK)",
                    );
                }
            } else if !store_guest_io(addr, size, SAVED_RAX) {
                static DROP: AtomicBool = AtomicBool::new(false);
                if !DROP.swap(true, Ordering::AcqRel) {
                    serial::write_str("boot: guest-UEFI string INS drop gpa=0x");
                    write_hex(addr);
                    serial::write_byte(b'\n');
                }
            }
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

/// Drain pending virtio kicks then log OUT/IN. Called every product-ISO
/// resume so ISO-INSTALL-OK is not missed after an IN-only exit.
/// virtio drain every resume. Not `ISO-INSTALL-OK`.
#[cfg(target_os = "uefi")]
unsafe fn drain_virtio_product_iso() {
    if !guest_uefi_virtio_drain_every_resume(
        crate::devices::ide_cdrom::product_iso_window_armed(),
    ) {
        return;
    }
    let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
    if wrote != 0 {
        serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
        write_dec(u64::from(wrote));
        serial::write_line(" (not ISO-INSTALL-OK)");
    }
    if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
        serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
        write_dec(n);
        serial::write_line(" (not ISO-INSTALL-OK)");
    }
}

/// Prefer PIT over UART only while virtio probe still needs kworker.
/// After both `00:02.0` and `00:03.0` reach DRIVER_OK, UART beats PIT.
/// linux PIT prefer until DRIVER_OK. Not `ISO-INSTALL-OK`.
#[cfg(target_os = "uefi")]
fn linux_prefer_pit_until_driver_ok() {
    let need = crate::devices::guest_virtio_blk::virtio_needs_pit_over_uart();
    crate::devices::guest_irq::prefer_pit_until_driver_ok(need);
    if !need
        && crate::devices::guest_virtio_blk::queues_armed()
        && LINUX_VIRTIO_DRIVER_OK_LOG
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    {
        serial::write_line(
            "boot: guest-UEFI linux virtio DRIVER_OK (Stage 46; not ISO-INSTALL-OK)",
        );
    }
}

/// Linux product-ISO general I/O does not raise PIT (iron MADT stop).
/// virtio MMIO / HLT / preempt still raise. Not `ISO-INSTALL-OK`.
#[cfg(target_os = "uefi")]
unsafe fn linux_product_iso_raise_pit(uart: bool) {
    if uart {
        return;
    }
    let linux = guest_uefi_linux_guest_active(
        serial::linux_earlycon_share(),
        guest_uefi_pf_should_deliver_to_guest(ops::vmread(GUEST_RIP).unwrap_or(0)),
        PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
    );
    if !guest_uefi_linux_io_raises_pit(
        linux,
        crate::devices::ide_cdrom::product_iso_window_armed(),
    ) {
        return;
    }
    crate::devices::guest_irq::raise_pit();
    linux_prefer_pit_until_driver_ok();
    let _ = crate::devices::lapic_virt::poll_timer_expiry();
}

#[cfg(target_os = "uefi")]
unsafe fn handle_io(qual: u64) -> bool {
    let size = (qual & 7) + 1;
    let is_in = (qual & (1 << 3)) != 0;
    let port = io_port_from_qual(qual);
    LAST_IO_PORT.store(u32::from(port), Ordering::Release);
    linux_product_iso_raise_pit(is_com_uart_port(port));
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
            } else if crate::devices::guest_virtio_blk::pci_addr_selects_owned(
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
            // virtio BAR trap over scratch: OVMF/Linux BAR0 write onto the
            // 0x80000000 UC 2MiB leaf must unmap 4K so MMIO EPT-traps.
            let bar_off = crate::devices::guest_platform::pci_cfg_offset(
                crate::devices::guest_virtio_blk::pci_read_addr(),
                port,
            );
            if (bar_off & 0xFC) == 0x10 {
                ept_trap_programmed_virtio_bars();
            }
        }
    }
}

#[cfg(target_os = "uefi")]
fn merge_mmio_gpr(old: u64, val: u64, size: u8, zero_ext: bool, sign_ext: bool) -> u64 {
    if sign_ext {
        return match size {
            1 => val as i8 as i64 as u64,
            2 => val as i16 as i64 as u64,
            4 => val as i32 as i64 as u64,
            _ => val,
        };
    }
    if zero_ext {
        return match size {
            1 => val & 0xff,
            2 => val & 0xffff,
            4 => val & 0xffff_ffff,
            _ => val,
        };
    }
    match size {
        1 => (old & !0xff) | (val & 0xff),
        2 => (old & !0xffff) | (val & 0xffff),
        4 => val & 0xffff_ffff,
        _ => val,
    }
}

/// Decode BAR/IOAPIC/xAPIC MMIO. If VMCS length is 0 or longer than the
/// peek, retry with [`virtio_mmio_retry_decode_len`] so Linux `ioread`
/// (`"=r"`) is not an EAX guess. linux MMIO decode retry.
#[cfg(target_os = "uefi")]
unsafe fn decode_mmio_or_retry(
    buf: &[u8],
    n: usize,
    insn_len: u64,
) -> Option<crate::devices::guest_virtio_blk::MmioInsn> {
    let slice = &buf[..n];
    if let Some(op) =
        crate::devices::guest_virtio_blk::decode_mmio_insn(slice, insn_len as usize)
    {
        return Some(op);
    }
    let cap = virtio_mmio_retry_decode_len(n, insn_len);
    if cap == 0 {
        return None;
    }
    let op = crate::devices::guest_virtio_blk::decode_mmio_insn(slice, cap as usize)?;
    let ar = ops::vmread(GUEST_CS_ACCESS_RIGHTS).unwrap_or(0);
    let long64 = guest_uefi_cs_ar_is_long(ar);
    let skip = crate::devices::guest_virtio_blk::mmio_decoded_len(slice, long64)
        .map(|d| d as u64)
        .filter(|d| *d >= 1 && *d <= 15)
        .unwrap_or(cap);
    MMIO_INSN_LEN.store(skip, Ordering::Relaxed);
    Some(op)
}

/// Trap-and-emulate virtio-pci BAR MMIO (product ISO queues only).
#[cfg(target_os = "uefi")]
unsafe fn handle_virtio_bar_ept(gpa: u64, qual: u64) -> bool {
    let is_write = (qual & 2) != 0;
    let Some(bar) = crate::devices::guest_virtio_blk::mmio_bar_base_for_gpa(gpa) else {
        return false;
    };
    let linux = guest_uefi_linux_guest_active(
        serial::linux_earlycon_share(),
        guest_uefi_pf_should_deliver_to_guest(ops::vmread(GUEST_RIP).unwrap_or(0)),
        PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
    );
    if guest_uefi_virtio_mmio_raises_pit(
        linux,
        crate::devices::ide_cdrom::product_iso_window_armed(),
    ) {
        crate::devices::guest_irq::raise_pit();
        linux_prefer_pit_until_driver_ok();
    }
    if guest_uefi_virtio_mmio_polls_lapic(
        linux,
        crate::devices::ide_cdrom::product_iso_window_armed(),
    ) {
        let _ = crate::devices::lapic_virt::poll_timer_expiry();
    }
    static HIT_N: AtomicU32 = AtomicU32::new(0);
    let n = HIT_N.fetch_add(1, Ordering::AcqRel);
    if n < 32 || n % 256 == 0 {
        serial::write_str_nowait("boot: guest-UEFI virtio MMIO gpa=0x");
        write_hex_nowait(gpa);
        serial::write_str_nowait(" off=0x");
        write_hex_nowait(gpa.wrapping_sub(bar));
        serial::write_str_nowait(" wr=");
        write_dec_nowait(u64::from(is_write));
        if n >= 32 {
            serial::write_str_nowait(" n=");
            write_dec_nowait(u64::from(n) + 1);
        }
        serial::write_byte_nowait(b'\n');
    }
    let mut buf = [0u8; 16];
    let (n, insn_len) = copy_mmio_insn(&mut buf);
    let Some(op) = decode_mmio_or_retry(&buf, n, insn_len) else {
        static FAIL_N: AtomicU32 = AtomicU32::new(0);
        if FAIL_N.fetch_add(1, Ordering::AcqRel) < 8 {
            serial::write_str_nowait("boot: guest-UEFI virtio MMIO decode fail gpa=0x");
            write_hex_nowait(gpa);
            serial::write_str_nowait(" n=");
            write_dec_nowait(n as u64);
            serial::write_str_nowait(" len=");
            write_dec_nowait(insn_len);
            serial::write_byte_nowait(b'\n');
        }
        let skip = virtio_mmio_eax_fallback_len(linux, n, insn_len);
        if skip != 0 {
            if FAIL_N.load(Ordering::Acquire) <= 8 {
                serial::write_line_nowait(
                    "boot: guest-UEFI virtio MMIO eax fallback (not ISO-INSTALL-OK)",
                );
            }
            MMIO_INSN_LEN.store(skip, Ordering::Relaxed);
            let sz = virtio_mmio_eax_fallback_size((gpa.wrapping_sub(bar)) as u16);
            if is_write {
                crate::devices::guest_virtio_blk::mmio_write_at(gpa, sz, cr_gpr(0));
                let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
                if wrote != 0 {
                    serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
                    write_dec(u64::from(wrote));
                    serial::write_line(" (not ISO-INSTALL-OK)");
                }
                if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
                    serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
                    write_dec(n);
                    serial::write_line(" (not ISO-INSTALL-OK)");
                }
                if !guest_uefi_host_hypervisor_present()
                    && crate::devices::guest_virtio_blk::take_iso_install_ok()
                {
                    serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
                }
            } else {
                let mem = crate::devices::guest_virtio_blk::mmio_read_at(gpa, sz);
                set_cr_gpr(0, mem);
            }
            return skip_insn();
        }
        return false;
    };
    if crate::devices::guest_virtio_blk::mmio_alu_is_hint(op.alu) {
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_sse(op.alu) {
        if op.is_write {
            crate::devices::guest_virtio_blk::mmio_write_sse_at(gpa, mmio_xmm_in(op.reg), op.size);
            let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
            if wrote != 0 {
                serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
                write_dec(u64::from(wrote));
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
                serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
                write_dec(n);
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if !guest_uefi_host_hypervisor_present()
                && crate::devices::guest_virtio_blk::take_iso_install_ok()
            {
                serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
            }
        } else {
            let mem = crate::devices::guest_virtio_blk::mmio_read_sse_at(gpa, op.size);
            mmio_xmm_out(
                op.reg,
                crate::devices::guest_virtio_blk::mmio_sse_from_mem(mem, op.size),
            );
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_string(op.alu) {
        let ept_write = is_write;
        let Some(done) = mmio_string_step(
            op,
            ept_write,
            || crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size),
            |val| crate::devices::guest_virtio_blk::mmio_write_at(gpa, op.size, val),
            gpa,
        ) else {
            return false;
        };
        let bar_write = crate::devices::guest_virtio_blk::mmio_alu_is_stos(op.alu)
            || (crate::devices::guest_virtio_blk::mmio_alu_is_movs(op.alu) && ept_write);
        if bar_write {
            let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
            if wrote != 0 {
                serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
                write_dec(u64::from(wrote));
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
                serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
                write_dec(n);
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if !guest_uefi_host_hypervisor_present()
                && crate::devices::guest_virtio_blk::take_iso_install_ok()
            {
                serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
            }
        }
        if done {
            return skip_insn();
        }
        return true;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_push(op.alu) {
        let size = mmio_stack_op_size(op);
        let val = crate::devices::guest_virtio_blk::mmio_read_at(gpa, size);
        if !mmio_stack_push(val, size) {
            return false;
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_pop(op.alu) {
        let size = mmio_stack_op_size(op);
        let Some(val) = mmio_stack_pop(size) else {
            return false;
        };
        crate::devices::guest_virtio_blk::mmio_write_at(gpa, size, val);
        let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
        if wrote != 0 {
            serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
            write_dec(u64::from(wrote));
            serial::write_line(" (not ISO-INSTALL-OK)");
        }
        if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
            serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
            write_dec(n);
            serial::write_line(" (not ISO-INSTALL-OK)");
        }
        if !guest_uefi_host_hypervisor_present()
            && crate::devices::guest_virtio_blk::take_iso_install_ok()
        {
            serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_call(op.alu)
        || crate::devices::guest_virtio_blk::mmio_alu_is_jmp(op.alu)
    {
        let size = mmio_stack_op_size(op);
        let target = crate::devices::guest_virtio_blk::mmio_read_at(gpa, size);
        if !mmio_near_xfer(
            op,
            target,
            crate::devices::guest_virtio_blk::mmio_alu_is_call(op.alu),
        ) {
            return false;
        }
        return true;
    }
    if op.xchg {
        let oldr = mmio_gpr_in(op);
        let oldm = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        crate::devices::guest_virtio_blk::mmio_write_at(gpa, op.size, oldr);
        mmio_gpr_out(op, oldm);
        let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
        if wrote != 0 {
            serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
            write_dec(u64::from(wrote));
            serial::write_line(" (not ISO-INSTALL-OK)");
        }
        if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
            serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
            write_dec(n);
            serial::write_line(" (not ISO-INSTALL-OK)");
        }
        if !guest_uefi_host_hypervisor_present()
            && crate::devices::guest_virtio_blk::take_iso_install_ok()
        {
            serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
        }
        return skip_insn();
    }
    if op.atomic != 0 {
        let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        if let Some(val) = mmio_apply_atomic(op, cur) {
            crate::devices::guest_virtio_blk::mmio_write_at(gpa, op.size, val);
            let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
            if wrote != 0 {
                serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
                write_dec(u64::from(wrote));
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
                serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
                write_dec(n);
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if !guest_uefi_host_hypervisor_present()
                && crate::devices::guest_virtio_blk::take_iso_install_ok()
            {
                serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
            }
        }
        return skip_insn();
    }
    if op.cc != 0 {
        let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        if let Some(val) = mmio_apply_cc(op, cur) {
            crate::devices::guest_virtio_blk::mmio_write_at(gpa, op.size, val);
            let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
            if wrote != 0 {
                serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
                write_dec(u64::from(wrote));
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
                serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
                write_dec(n);
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if !guest_uefi_host_hypervisor_present()
                && crate::devices::guest_virtio_blk::take_iso_install_ok()
            {
                serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
            }
        }
        return skip_insn();
    }
    if op.test || op.cmp {
        let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        if mmio_apply_test_cmp(op, cur) {
            return skip_insn();
        }
    }
    if op.bt != 0 {
        let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        let val = mmio_apply_bt(op, cur);
        if op.is_write {
            crate::devices::guest_virtio_blk::mmio_write_at(gpa, op.size, val);
            let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
            if wrote != 0 {
                serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
                write_dec(u64::from(wrote));
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
                serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
                write_dec(n);
                serial::write_line(" (not ISO-INSTALL-OK)");
            }
            if !guest_uefi_host_hypervisor_present()
                && crate::devices::guest_virtio_blk::take_iso_install_ok()
            {
                serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
            }
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_div_pair(op.alu) {
        let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        if mmio_div_pair_commit(op, cur) {
            return skip_insn();
        }
        return inject_mmio_div_de();
    }
    if op.alu != 0 && (op.alu_reg_left || !op.is_write) {
        let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        let val = mmio_alu_result(op, cur);
        if !crate::devices::guest_virtio_blk::mmio_alu_is_mul_pair(op.alu) {
            mmio_gpr_out(op, val);
        }
        return skip_insn();
    }
    if op.is_write != is_write {
        return false;
    }
    if is_write {
        let val = if op.alu != 0 {
            let cur = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
            mmio_alu_result(op, cur)
        } else if op.has_imm {
            op.imm
        } else {
            mmio_gpr_in(op)
        };
        crate::devices::guest_virtio_blk::mmio_write_at(gpa, op.size, val);
        let wrote = crate::devices::guest_virtio_blk::drain_queue(guest_uefi_gpa_to_hpa);
        if wrote != 0 {
            serial::write_str("boot: Stage 46 virtio-blk OUT bytes=");
            write_dec(u64::from(wrote));
            serial::write_line(" (not ISO-INSTALL-OK)");
        }
        if let Some(n) = crate::devices::guest_virtio_blk::take_iso_read_note() {
            serial::write_str("boot: Stage 46 virtio-iso IN bytes=");
            write_dec(n);
            serial::write_line(" (not ISO-INSTALL-OK)");
        }
        if !guest_uefi_host_hypervisor_present()
            && crate::devices::guest_virtio_blk::take_iso_install_ok()
        {
            serial::write_line_nowait(crate::mgmt::iso_install::M7_ISO_INSTALL_OK_MARKER);
        }
    } else {
        let val = crate::devices::guest_virtio_blk::mmio_read_at(gpa, op.size);
        mmio_gpr_out(op, val);
    }
    skip_insn()
}

/// Trap-and-emulate IOAPIC MMIO (product ISO only).
#[cfg(target_os = "uefi")]
unsafe fn handle_ioapic_ept(gpa: u64, qual: u64) -> bool {
    let is_write = (qual & 2) != 0;
    let off = (gpa.wrapping_sub(crate::devices::guest_irq::IOAPIC_GPA)) as u16;
    let mut buf = [0u8; 16];
    let (n, insn_len) = copy_mmio_insn(&mut buf);
    let Some(op) = decode_mmio_or_retry(&buf, n, insn_len) else {
        // IOAPIC decode fail nowait (share hushes write_str).
        static FAIL_N: AtomicU32 = AtomicU32::new(0);
        if FAIL_N.fetch_add(1, Ordering::AcqRel) < 8 {
            serial::write_str_nowait("boot: guest-UEFI IOAPIC decode fail gpa=0x");
            write_hex_nowait(gpa);
            serial::write_str_nowait(" n=");
            write_dec_nowait(n as u64);
            serial::write_str_nowait(" len=");
            write_dec_nowait(insn_len);
            serial::write_byte_nowait(b'\n');
        }
        let linux = guest_uefi_linux_guest_active(
            serial::linux_earlycon_share(),
            guest_uefi_pf_should_deliver_to_guest(ops::vmread(GUEST_RIP).unwrap_or(0)),
            PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
        );
        let skip = virtio_mmio_eax_fallback_len(linux, n, insn_len);
        if skip != 0 {
            MMIO_INSN_LEN.store(skip, Ordering::Relaxed);
            if is_write {
                crate::devices::guest_irq::ioapic_write(off, cr_gpr(0) as u32);
            } else {
                set_cr_gpr(0, u64::from(crate::devices::guest_irq::ioapic_read(off)));
            }
            return skip_insn();
        }
        return skip_insn();
    };
    if crate::devices::guest_virtio_blk::mmio_alu_is_hint(op.alu) {
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_sse(op.alu) {
        if op.is_write {
            crate::devices::guest_irq::ioapic_write(off, mmio_xmm_in(op.reg) as u32);
        } else {
            let mem = u128::from(crate::devices::guest_irq::ioapic_read(off));
            mmio_xmm_out(
                op.reg,
                crate::devices::guest_virtio_blk::mmio_sse_from_mem(mem, op.size.min(4)),
            );
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_string(op.alu) {
        let Some(done) = mmio_string_step(
            op,
            is_write,
            || u64::from(crate::devices::guest_irq::ioapic_read(off)),
            |val| crate::devices::guest_irq::ioapic_write(off, val as u32),
            gpa,
        ) else {
            return skip_insn();
        };
        if done {
            return skip_insn();
        }
        return true;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_push(op.alu) {
        let size = mmio_stack_op_size(op);
        let val = u64::from(crate::devices::guest_irq::ioapic_read(off));
        if !mmio_stack_push(val, size) {
            return skip_insn();
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_pop(op.alu) {
        let size = mmio_stack_op_size(op);
        if let Some(val) = mmio_stack_pop(size) {
            crate::devices::guest_irq::ioapic_write(off, val as u32);
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_call(op.alu)
        || crate::devices::guest_virtio_blk::mmio_alu_is_jmp(op.alu)
    {
        let target = u64::from(crate::devices::guest_irq::ioapic_read(off));
        if !mmio_near_xfer(
            op,
            target,
            crate::devices::guest_virtio_blk::mmio_alu_is_call(op.alu),
        ) {
            return skip_insn();
        }
        return true;
    }
    if op.xchg {
        let oldr = mmio_gpr_in(op);
        let oldm = u64::from(crate::devices::guest_irq::ioapic_read(off));
        crate::devices::guest_irq::ioapic_write(off, oldr as u32);
        mmio_gpr_out(op, oldm);
        return skip_insn();
    }
    if op.atomic != 0 {
        let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
        if let Some(val) = mmio_apply_atomic(op, cur) {
            crate::devices::guest_irq::ioapic_write(off, val as u32);
        }
        return skip_insn();
    }
    if op.cc != 0 {
        let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
        if let Some(val) = mmio_apply_cc(op, cur) {
            crate::devices::guest_irq::ioapic_write(off, val as u32);
        }
        return skip_insn();
    }
    if op.test || op.cmp {
        let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
        if mmio_apply_test_cmp(op, cur) {
            return skip_insn();
        }
    }
    if op.bt != 0 {
        let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
        let val = mmio_apply_bt(op, cur);
        if op.is_write {
            crate::devices::guest_irq::ioapic_write(off, val as u32);
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_div_pair(op.alu) {
        let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
        if mmio_div_pair_commit(op, cur) {
            return skip_insn();
        }
        return inject_mmio_div_de();
    }
    if op.alu != 0 && (op.alu_reg_left || !op.is_write) {
        let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
        let val = mmio_alu_result(op, cur);
        if !crate::devices::guest_virtio_blk::mmio_alu_is_mul_pair(op.alu) {
            mmio_gpr_out(op, val);
        }
        return skip_insn();
    }
    if op.is_write != is_write {
        return skip_insn();
    }
    if is_write {
        let val = if op.alu != 0 {
            let cur = u64::from(crate::devices::guest_irq::ioapic_read(off));
            mmio_alu_result(op, cur)
        } else if op.has_imm {
            op.imm
        } else {
            mmio_gpr_in(op)
        };
        crate::devices::guest_irq::ioapic_write(off, val as u32);
    } else {
        let val = u64::from(crate::devices::guest_irq::ioapic_read(off));
        mmio_gpr_out(op, val);
    }
    skip_insn()
}

/// Trap-and-emulate local APIC MMIO (product ISO 4 KiB hole).
#[cfg(target_os = "uefi")]
unsafe fn handle_xapic_ept(gpa: u64, qual: u64) -> bool {
    if crate::devices::lapic_virt::mmio_access(gpa, false, 0).is_none() {
        return false;
    }
    let is_write = (qual & 2) != 0;
    let mut buf = [0u8; 16];
    let (n, insn_len) = copy_mmio_insn(&mut buf);
    let Some(op) = decode_mmio_or_retry(&buf, n, insn_len)
    else {
        static FAIL_N: AtomicU32 = AtomicU32::new(0);
        if FAIL_N.fetch_add(1, Ordering::AcqRel) < 8 {
            serial::write_str("boot: guest-UEFI xAPIC MMIO decode fail gpa=0x");
            write_hex(gpa);
            serial::write_str(" insn=");
            let show = n.min(8);
            let mut i = 0usize;
            while i < show {
                write_hex_u8(buf[i]);
                i += 1;
            }
            serial::write_str(" n=");
            write_dec(n as u64);
            serial::write_str(" len=");
            write_dec(insn_len);
            serial::write_byte(b'\n');
        }
        if xapic_fetch_miss_eax_fallback(n, insn_len) {
            if is_write {
                let val = cr_gpr(0) as u32;
                let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val);
            } else {
                let mem = crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                    .and_then(|v| v)
                    .unwrap_or(0);
                set_cr_gpr(0, u64::from(mem));
            }
            if FAIL_N.load(Ordering::Acquire) <= 8 {
                serial::write_line("boot: guest-UEFI xAPIC MMIO eax fallback (not ISO-INSTALL-OK)");
            }
            if skip_insn() {
                return true;
            }
            let skip = xapic_eax_fallback_skip_len(insn_len);
            if skip != 0 {
                let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
                return ops::vmwrite(GUEST_RIP, rip.wrapping_add(skip)).is_ok();
            }
        }
        serial::write_str_nowait("boot: guest-UEFI xAPIC decode fail stop gpa=0x");
        write_hex_nowait(gpa);
        serial::write_str_nowait(" len=");
        write_dec_nowait(insn_len);
        serial::write_byte_nowait(b'\n');
        return false;
    };
    if crate::devices::guest_virtio_blk::mmio_alu_is_hint(op.alu) {
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_sse(op.alu) {
        if op.is_write {
            let _ = crate::devices::lapic_virt::mmio_access(gpa, true, mmio_xmm_in(op.reg) as u32);
        } else {
            let mem = u128::from(
                crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                    .and_then(|v| v)
                    .unwrap_or(0),
            );
            mmio_xmm_out(
                op.reg,
                crate::devices::guest_virtio_blk::mmio_sse_from_mem(mem, op.size.min(4)),
            );
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_string(op.alu) {
        let Some(done) = mmio_string_step(
            op,
            is_write,
            || {
                u64::from(
                    crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                        .and_then(|v| v)
                        .unwrap_or(0),
                )
            },
            |val| {
                let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
            },
            gpa,
        ) else {
            return false;
        };
        if done {
            return skip_insn();
        }
        return true;
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_push(op.alu) {
        let size = mmio_stack_op_size(op);
        let val = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        if !mmio_stack_push(val, size) {
            return false;
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_pop(op.alu) {
        let size = mmio_stack_op_size(op);
        let Some(val) = mmio_stack_pop(size) else {
            return false;
        };
        let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_call(op.alu)
        || crate::devices::guest_virtio_blk::mmio_alu_is_jmp(op.alu)
    {
        let target = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        if !mmio_near_xfer(
            op,
            target,
            crate::devices::guest_virtio_blk::mmio_alu_is_call(op.alu),
        ) {
            return false;
        }
        return true;
    }
    if op.xchg {
        let oldr = mmio_gpr_in(op) as u32;
        let oldm = crate::devices::lapic_virt::mmio_access(gpa, false, 0)
            .and_then(|v| v)
            .unwrap_or(0);
        let _ = crate::devices::lapic_virt::mmio_access(gpa, true, oldr);
        mmio_gpr_out(op, u64::from(oldm));
        return skip_insn();
    }
    if op.atomic != 0 {
        let cur = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        if let Some(val) = mmio_apply_atomic(op, cur) {
            let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
        }
        return skip_insn();
    }
    if op.cc != 0 {
        let cur = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        if let Some(val) = mmio_apply_cc(op, cur) {
            let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
        }
        return skip_insn();
    }
    if op.test || op.cmp {
        let cur = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        if mmio_apply_test_cmp(op, cur) {
            return skip_insn();
        }
    }
    if op.bt != 0 {
        let cur = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        let val = mmio_apply_bt(op, cur);
        if op.is_write {
            let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
        }
        return skip_insn();
    }
    if crate::devices::guest_virtio_blk::mmio_alu_is_div_pair(op.alu) {
        let cur = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        if mmio_div_pair_commit(op, cur) {
            return skip_insn();
        }
        return inject_mmio_div_de();
    }
    if op.alu != 0 {
        let cur = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        let val = mmio_alu_result(op, cur);
        if op.alu_reg_left || !op.is_write {
            if !crate::devices::guest_virtio_blk::mmio_alu_is_mul_pair(op.alu) {
                mmio_gpr_out(op, val);
            }
        } else {
            let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
        }
        return skip_insn();
    }
    if op.is_write != is_write {
        return false;
    }
    if is_write {
        let val = if op.has_imm {
            op.imm
        } else {
            mmio_gpr_in(op)
        };
        let _ = crate::devices::lapic_virt::mmio_access(gpa, true, val as u32);
    } else {
        let val = u64::from(
            crate::devices::lapic_virt::mmio_access(gpa, false, 0)
                .and_then(|v| v)
                .unwrap_or(0),
        );
        mmio_gpr_out(op, val);
    }
    skip_insn()
}

/// VM-entry inject of virtio INTx / ATA IRQ 14 / LAPIC LVT when the product ISO is armed.
#[cfg(target_os = "uefi")]
unsafe fn try_inject_guest_irq() {
    if guest_uefi_linux_exc_blocks_irq(
        PF_LINUX_CR2.load(Ordering::Acquire),
        LINUX_EXC_INJECT.swap(false, Ordering::AcqRel),
    ) {
        return;
    }
    if !crate::devices::ide_cdrom::product_iso_window_armed() {
        return;
    }
    crate::devices::guest_uart::poll_host_rx();
    crate::devices::guest_uart::reassert_irq();
    // PIC first (Linux virtual-wire / no MADT) unless GSI 2 is armed.
    // Iron `a525340`: OVMF left IOAPIC unmasked, `raise_pit` latched pin 0
    // into LAPIC, and inject preferred that vector so PIC IRQ 0 never
    // reached `rest_init`. After ACPI, Linux unmasks pin 2; PIC IRQ 0
    // leftover would inject 0x20 into that IDT. linux GSI 2 before PIC.
    // linux PIC before LAPIC. Not `ISO-INSTALL-OK`.
    let gsi2_armed = crate::devices::guest_irq::ioapic_gsi2_armed();
    let pic_ready = crate::devices::guest_irq::pic_has_deliverable();
    let use_pic = guest_uefi_linux_pic_before_lapic(pic_ready, gsi2_armed);
    if !use_pic {
        if let Some(vec) = crate::devices::guest_irq::take_ioapic_vector() {
            crate::devices::lapic_virt::latch_irr(vec);
        }
    }
    let lapic = crate::devices::lapic_virt::has_deliverable_irr();
    if !use_pic && !lapic {
        let _ = set_guest_uefi_interrupt_window(false);
        return;
    }
    let rflags = ops::vmread(GUEST_RFLAGS).unwrap_or(0);
    let int_state = ops::vmread(GUEST_INTERRUPTIBILITY_STATE).unwrap_or(0);
    if (rflags & (1 << 9)) == 0 || (int_state & 0x3) != 0 {
        let _ = set_guest_uefi_interrupt_window(true);
        return;
    }
    let Some(vec) = (if use_pic {
        crate::devices::guest_irq::take_pic_vector().map(u32::from)
    } else {
        crate::devices::lapic_virt::take_deliverable_vector()
    }) else {
        let _ = set_guest_uefi_interrupt_window(false);
        return;
    };
    if let Ok(info) = crate::sched::interrupt::prepare_external_inject(vec) {
        let _ = set_guest_uefi_interrupt_window(false);
        let _ = ops::vmwrite(VM_ENTRY_INTERRUPTION_INFO, info as u64);
        let _ = ops::vmwrite(GUEST_INTERRUPTIBILITY_STATE, 0);
        let _ = ops::vmwrite(GUEST_ACTIVITY_STATE, 0);
        static INJECT_N: AtomicU32 = AtomicU32::new(0);
        let n = INJECT_N.fetch_add(1, Ordering::AcqRel);
        if n < 8 {
            serial::write_str_nowait("boot: Stage 46 inject vec=0x");
            write_hex_nowait(u64::from(vec));
            serial::write_line_nowait(" (not ISO-INSTALL-OK)");
        }
        if use_pic
            && guest_uefi_linux_pic_irq0_vec(vec)
            && LINUX_PIC_IRQ0_LOG
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            serial::write_line_nowait(
                "boot: guest-UEFI linux PIC IRQ0 (Stage 46; not ISO-INSTALL-OK)",
            );
        }
        if gsi2_armed
            && guest_uefi_linux_gsi2_before_pic(true)
            && LINUX_GSI2_LOG
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            serial::write_line_nowait(
                "boot: guest-UEFI linux PIT GSI 2 (Stage 46; not ISO-INSTALL-OK)",
            );
        }
    } else {
        let _ = set_guest_uefi_interrupt_window(true);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn set_guest_uefi_interrupt_window(on: bool) -> Result<(), ()> {
    let cur = ops::vmread(PRIMARY_PROC_BASED_VM_EXEC_CONTROL).map_err(|_| ())? as u32;
    let next = if on {
        cur | CPU_BASED_INTERRUPT_WINDOW_EXITING
    } else {
        cur & !CPU_BASED_INTERRUPT_WINDOW_EXITING
    };
    ops::vmwrite(PRIMARY_PROC_BASED_VM_EXEC_CONTROL, next as u64).map_err(|_| ())
}

#[cfg(target_os = "uefi")]
unsafe fn handle_ept(gpa: u64, qual: u64) -> bool {
    if crate::devices::guest_virtio_blk::is_virtio_bar_gpa(gpa) {
        return handle_virtio_bar_ept(gpa, qual);
    }
    if crate::devices::guest_irq::is_ioapic_gpa(gpa) {
        return handle_ioapic_ept(gpa, qual);
    }
    if crate::devices::lapic_virt::is_xapic_mmio_gpa(gpa) {
        return handle_xapic_ept(gpa, qual);
    }
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
        // linux earlycon EPT cap nowait (share hushes write_str; iron
        // `113a08a` PAT then quiet would hide a cap stop).
        if serial::linux_earlycon_share() {
            serial::write_line_nowait("boot: guest-UEFI EPT report-RAM cap");
        } else {
            serial::write_str("boot: guest-UEFI EPT report-RAM cap gpa=0x");
            write_hex(gpa);
            serial::write_byte(b'\n');
        }
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
    if serial::linux_earlycon_share() {
        serial::write_line_nowait("boot: guest-UEFI EPT violation");
    } else {
        serial::write_str("boot: guest-UEFI EPT violation gpa=0x");
        write_hex(gpa);
        serial::write_byte(b'\n');
    }
    false
}

/// Map a 4 KiB xAPIC page at `0xFEE00000` (rest of the 2 MiB → sink zeros).
/// Product ISO: PTE[0] stays empty so GetApicVersion / CUR_COUNT / EOI trap
/// into [`handle_xapic_ept`]. Lab stub keeps the filled static page.
///
/// INVARIANTS:
/// - Does not clobber an existing PD leaf (2 MiB sink must not already be there)
/// - Lab: version register in `lapic_hpa` is [`crate::devices::lapic_virt::XAPIC_VERSION`]
/// - Product: 4 KiB[0] unmapped (trap); not a zero sink (iron `ad78f12`)
#[cfg(target_os = "uefi")]
unsafe fn ept_install_xapic_4k(pt_hpa: u64, lapic_hpa: u64, trap: bool) -> bool {
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
        if i == 0 && trap {
            continue;
        }
        let hpa = if i == 0 { lapic_hpa } else { zero_hpa };
        core::ptr::write_volatile(pt.add(i as usize), crate::memory::ept_hw::ept_leaf_4k(hpa, 0));
    }
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), crate::memory::ept_hw::ept_link(pt_hpa));
    crate::memory::ept_hw::invept_global();
    true
}

/// Split the HPET 2 MiB sink: 4 KiB[0] unmapped (IOAPIC trap), rest UC sink.
///
/// INVARIANTS:
/// - Product ISO only; lab keeps the 2 MiB HPET leaf
/// - PD leaf at `0xFEC00000` must be empty (do not map 2 MiB first)
/// - HPET at `0xFED00000` stays on `SINK_HPA + HPET_SINK_OFF`
#[cfg(target_os = "uefi")]
unsafe fn ept_install_ioapic_trap(pt_hpa: u64) -> bool {
    let pml4 = EPT_PML4.load(Ordering::Acquire);
    let sink = SINK_HPA.load(Ordering::Acquire);
    let gpa = crate::devices::guest_platform::HPET_SINK_PAGE;
    if pml4 == 0 || sink == 0 || (pt_hpa & 0xfff) != 0 || (sink & 0x1F_FFFF) != 0 {
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
    for i in 1..512u64 {
        let hpa = sink + i * 4096;
        core::ptr::write_volatile(pt.add(i as usize), crate::memory::ept_hw::ept_leaf_4k(hpa, 0));
    }
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), crate::memory::ept_hw::ept_link(pt_hpa));
    crate::memory::ept_hw::invept_global();
    true
}

/// Unmap the 4 KiB virtio BAR inside a present 2 MiB EPT leaf (scratch).
///
/// INVARIANTS:
/// - Empty PD leaf already traps (return true)
/// - Large page is split to 4 KiB; BAR index is not-present
/// - Existing PT: only that PTE is cleared
/// - iso=0 never calls this (queues off → BAR GPA 0)
///
/// virtio BAR trap over scratch. Not `ISO-INSTALL-OK`.
#[cfg(target_os = "uefi")]
unsafe fn ept_split_2m_trap_4k(gpa: u64) -> bool {
    let pml4 = EPT_PML4.load(Ordering::Acquire);
    if pml4 == 0 || !guest_uefi_virtio_bar_should_trap(gpa) {
        return false;
    }
    let pml4_i = ((gpa >> 39) & 0x1ff) as usize;
    let e0 = core::ptr::read_volatile((pml4 as *const u64).add(pml4_i));
    if e0 & 0b111 == 0 {
        return true;
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
    let pt_i = ((gpa >> 12) & 0x1ff) as usize;
    if e2 & 0b111 == 0 {
        return true;
    }
    if (e2 & (1 << 7)) == 0 {
        let pt = e2 & !0xfff;
        core::ptr::write_volatile((pt as *mut u64).add(pt_i), 0);
        crate::memory::ept_hw::invept_global();
        return true;
    }
    let alloc = e4_alloc();
    if alloc.is_null() {
        return false;
    }
    let Some(pt_hpa) = alloc_phys(&mut *alloc) else {
        return false;
    };
    if (pt_hpa & 0xfff) != 0 {
        return false;
    }
    let hpa_2m = e2 & !0x1F_FFFF;
    let mt = (e2 >> 3) & 7;
    core::ptr::write_bytes(pt_hpa as *mut u8, 0, 4096);
    let pt = pt_hpa as *mut u64;
    for i in 0..512u64 {
        if i as usize == pt_i {
            continue;
        }
        let hpa = hpa_2m + i * 4096;
        core::ptr::write_volatile(pt.add(i as usize), crate::memory::ept_hw::ept_leaf_4k(hpa, mt));
    }
    core::ptr::write_volatile((pd as *mut u64).add(pd_i), crate::memory::ept_hw::ept_link(pt_hpa));
    crate::memory::ept_hw::invept_global();
    true
}

/// Trap programmed virtio BAR0 pages so Linux ioremap is not scratch RAM.
#[cfg(target_os = "uefi")]
unsafe fn ept_trap_programmed_virtio_bars() {
    if !crate::devices::guest_virtio_blk::queues_armed() {
        return;
    }
    for bar in crate::devices::guest_virtio_blk::mmio_programmed_bar_gpas() {
        if !guest_uefi_virtio_bar_should_trap(bar) {
            continue;
        }
        let ok = ept_split_2m_trap_4k(bar);
        if guest_uefi_virtio_bar_overlaps_scratch(bar) {
            static N: AtomicU32 = AtomicU32::new(0);
            if N.fetch_add(1, Ordering::AcqRel) < 4 {
                serial::write_str_nowait("boot: guest-UEFI virtio BAR trap gpa=0x");
                write_hex_nowait(bar);
                serial::write_str_nowait(" ok=");
                write_dec_nowait(u64::from(ok));
                serial::write_line_nowait(" (not ISO-INSTALL-OK)");
            }
        }
    }
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
    ept_map_2m_hpa_mt(gpa, hpa, replace, write, 0, true)
}

/// `mt` is the EPT leaf memory type (0 = UC scratch/sink; 6 = WB RAM).
/// `invept=false` batches product-ISO report-RAM EPT pre-map (one INVEPT).
#[cfg(target_os = "uefi")]
unsafe fn ept_map_2m_hpa_mt(
    gpa: u64,
    hpa: u64,
    replace: bool,
    write: bool,
    mt: u64,
    invept: bool,
) -> bool {
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
    if invept {
        crate::memory::ept_hw::invept_global();
    }
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
/// Iron `f0781bb`: first slot `n=2` then return left CpuDxe unpatched.
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_patch_cpu_flush_mapped(hpa: u64) {
    if hpa == 0 {
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
    } else {
        serial::write_str("boot: guest-UEFI CpuFlush extra WBINVD n=");
        write_dec(u64::from(n));
        serial::write_str(" total=");
        write_dec(u64::from(total.saturating_add(n)));
        serial::write_byte(b'\n');
    }
}

#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_patch_cpu_flush_all_mapped() {
    // cpu_flush leftover per walk; high GPA first (heap / iron 0x7EE68FA0).
    let mut patched = CPU_FLUSH_PATCHED.load(Ordering::Acquire) != 0;
    let mut budget = if patched {
        0
    } else {
        GUEST_UEFI_CPU_FLUSH_LEFTOVER_PER_WALK
    };
    let mut skipped = 0u32;
    for i in (0..REPORT_RAM_ARRAY).rev() {
        let hpa = REPORT_RAM_HPA[i].load(Ordering::Acquire);
        let gpa = REPORT_RAM_GPA[i].load(Ordering::Acquire);
        if hpa == 0 || gpa == u64::MAX {
            continue;
        }
        if guest_uefi_cpu_flush_skip_mapped(gpa, patched) || budget == 0 {
            skipped = skipped.saturating_add(1);
            continue;
        }
        budget = budget.saturating_sub(1);
        guest_uefi_patch_cpu_flush_mapped(hpa);
        if CPU_FLUSH_PATCHED.load(Ordering::Acquire) != 0 {
            patched = true;
            budget = 0;
        }
    }
    if skipped != 0 && !CPU_FLUSH_SKIP_LOG.swap(true, Ordering::AcqRel) {
        serial::write_str("boot: guest-UEFI cpu-flush skip leftover pre-map n=");
        write_dec(u64::from(skipped));
        serial::write_byte(b'\n');
    }
}

#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_count_cpu_flush_jnz_mapped() -> u32 {
    let mut n = 0u32;
    let patched = CPU_FLUSH_PATCHED.load(Ordering::Acquire) != 0;
    for i in 0..REPORT_RAM_ARRAY {
        let hpa = REPORT_RAM_HPA[i].load(Ordering::Acquire);
        let gpa = REPORT_RAM_GPA[i].load(Ordering::Acquire);
        if hpa == 0 || gpa == u64::MAX {
            continue;
        }
        if guest_uefi_cpu_flush_skip_mapped(gpa, patched)
            || gpa < GUEST_UEFI_CPU_FLUSH_HEAP_GPA
        {
            continue;
        }
        // SAFETY: exclusive 2 MiB report-RAM HPA already EPT-mapped WB.
        // KANI-TARGET: count leftover CpuFlush jnz (outside Proven Core).
        let buf =
            core::slice::from_raw_parts(hpa as *const u8, GUEST_UEFI_REPORT_RAM_PAGE as usize);
        n = n.saturating_add(guest_uefi_count_cpu_flush_jnz(buf));
    }
    n
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
    ept_map_2m_hpa_mt(g, hpa, false, true, GUEST_UEFI_EPT_MT_WB, true)
}

fn report_ram_hpa_lookup(gpa: u64) -> u64 {
    let key = guest_uefi_report_ram_gpa_2m(gpa);
    for i in 0..REPORT_RAM_ARRAY {
        if REPORT_RAM_GPA[i].load(Ordering::Acquire) == key {
            return REPORT_RAM_HPA[i].load(Ordering::Acquire);
        }
    }
    0
}

/// Resolve a report-RAM HPA. Lazy 2MiB WB map like an EPT miss.
///
/// String INS, PUSH/POP, insn peek, and virtqueue do not EPT-walk the GPA.
/// A lookup miss used to drop bytes (iron EFI stub gzip zeros). Do not
/// invent HPA (ADR-004).
fn report_ram_hpa_lookup_or_map(gpa: u64) -> u64 {
    let existing = report_ram_hpa_lookup(gpa);
    if existing != 0 {
        return existing;
    }
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: exclusive report-RAM pool; guest is VM-exited.
        // KANI-TARGET: lazy report-RAM for string/PUSH/virtqueue (outside Proven Core).
        if unsafe { ept_map_2m_report_ram(gpa) } {
            let hpa = report_ram_hpa_lookup(gpa);
            if hpa != 0 {
                let n = REPORT_RAM_MAPS.fetch_add(1, Ordering::AcqRel);
                if n < 8 {
                    serial::write_str("boot: guest-UEFI lazy report-RAM gpa=0x");
                    write_hex(gpa);
                    serial::write_str(" hpa=0x");
                    write_hex(hpa);
                    serial::write_byte(b'\n');
                }
                return hpa;
            }
        }
    }
    0
}

/// GPA → HPA for guest-UEFI low RAM, OVMF flash, and mapped report-RAM.
pub fn guest_uefi_gpa_to_hpa(gpa: u64) -> Option<u64> {
    if gpa < GUEST_UEFI_LOW_RAM_BYTES {
        let hpa = RAM_HPA.load(Ordering::Acquire);
        if hpa == 0 {
            return None;
        }
        return Some(hpa + gpa);
    }
    if let Some(off) = guest_uefi_flash_off(gpa) {
        let hpa = FLASH_HPA.load(Ordering::Acquire);
        let len = FLASH_LEN.load(Ordering::Acquire);
        if hpa == 0 || off >= len {
            return None;
        }
        return Some(hpa + off);
    }
    if guest_uefi_report_ram_should_map(gpa) {
        let base = report_ram_hpa_lookup_or_map(gpa);
        if base != 0 {
            return Some(base + guest_uefi_report_ram_page_off(gpa));
        }
        return None;
    }
    None
}

fn report_ram_hpa_for(gpa: u64) -> u64 {
    let existing = report_ram_hpa_lookup(gpa);
    if existing != 0 {
        return existing;
    }
    let key = guest_uefi_report_ram_gpa_2m(gpa);
    for i in 0..REPORT_RAM_ARRAY {
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

/// Nested KVM must not return firmware-scratched report-RAM to E4.
///
/// Nested `4225b4d`: freed slots became `load kernel=0x8200000` then
/// `/init` SIGSEGV CR2 `0x8a00000` (direct-map of a report-RAM HPA).
/// Nested `957e0ad` withhold loaded at `0xc400000` then `#DF` `rip=0x9e036`
/// (LA57 trampoline; now stripped by `E4_LINUX_CR4_FORBIDDEN`). Iron still
/// returns the frames (pool starts lower; SHELL at `0x7c00000`).
pub fn report_ram_return_to_e4(host_hypervisor: bool) -> bool {
    !host_hypervisor
}

/// Finish guest-UEFI report-RAM: zero each 2 MiB HPA, then free to E4
/// (iron) or withhold (nested KVM).
///
/// INVARIANTS:
/// - Slot tracking is cleared either way
/// - Nested: allocator bits stay allocated so bzImage cannot land on them
/// - Iron: frames return to the pool after zero
///
/// Nested Intel `957e0ad`: pool=32 stole 64 MiB so Linux loaded at
/// `0xc400000` then `#DF` `rip=0x9e036`. Guest-UEFI has finished.
pub fn release_report_ram_for_e4(alloc: &mut FrameAllocator, return_to_e4: bool) -> u32 {
    let mut n = 0u32;
    for i in 0..REPORT_RAM_ARRAY {
        let hpa = REPORT_RAM_HPA[i].swap(0, Ordering::AcqRel);
        REPORT_RAM_GPA[i].store(u64::MAX, Ordering::Release);
        if hpa == 0 {
            continue;
        }
        #[cfg(target_os = "uefi")]
        {
            // SAFETY: exclusive 2 MiB report-RAM HPA; guest-UEFI VMCS halted.
            // KANI-TARGET: zero report-RAM before E4 (outside Proven Core).
            unsafe {
                core::ptr::write_bytes(hpa as *mut u8, 0, GUEST_UEFI_REPORT_RAM_PAGE as usize);
            }
        }
        if return_to_e4 && alloc.owns_phys_range(hpa, GUEST_UEFI_REPORT_RAM_PAGE) {
            let mut off = 0u64;
            while off < GUEST_UEFI_REPORT_RAM_PAGE {
                let _ = alloc.free_frame(PhysFrame::from_phys(hpa + off));
                off += 4096;
            }
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

/// 16550-compatible COM1/COM2. THR bytes go to host serial (firmware evidence)
/// via [`crate::boot::serial::write_byte_nowait`] so a guest `out` cannot stall
/// the HV on iDRAC SOL THRE (guest UART nowait; do not clear COM2_LIVE).
/// Iron `f423d03`: enqueue + drain (guest UART TX ring drain) so HV ticks
/// do not drop Linux printk.
/// Product ISO uses a scratch/FIFO 16550 so Linux 8250 autoconfig can bind ttyS0.
#[cfg(target_os = "uefi")]
unsafe fn handle_uart(port: u16, is_in: bool, size: u64) {
    if crate::devices::ide_cdrom::product_iso_window_armed() {
        handle_uart_product(port, is_in, size);
        return;
    }
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
        emit_guest_uart_byte(SAVED_RAX as u8);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn handle_uart_product(port: u16, is_in: bool, size: u64) {
    let mask = if size == 1 {
        0xffu64
    } else if size == 2 {
        0xffff
    } else {
        0xffff_ffff
    };
    let (out, thr, _) = crate::devices::guest_uart::pio(port, is_in, SAVED_RAX as u8);
    if is_in {
        SAVED_RAX = (SAVED_RAX & !mask) | (u64::from(out) & mask);
    } else if let Some(b) = thr {
        emit_guest_uart_byte(b);
    }
}

#[cfg(target_os = "uefi")]
unsafe fn emit_guest_uart_byte(b: u8) {
    if !COM_BANNER.swap(true, Ordering::AcqRel) {
        serial::write_line("boot: guest-UEFI firmware-serial begin");
    }
    // Guest UART nowait (do not clear COM2_LIVE): iron 115e5ee froze SOL
    // at PAT after blocking THR wait on this tee. Iron f423d03 dropped
    // Linux printk (guest UART TX ring drain retries when THRE returns).
    serial::write_byte_nowait(b);
    COM_BYTES.fetch_add(1, Ordering::AcqRel);
    let m = eltorito_com_match_step(ELTORITO_COM_MATCH.load(Ordering::Acquire), b);
    ELTORITO_COM_MATCH.store(m, Ordering::Release);
    maybe_print_past_sec(false);
    maybe_print_eltorito();
}

/// CPUID clobbers EAX–EDX; alpine-virt keeps `base` in EBX and
/// `native_cpuid` pushes it. Patch the 8-byte RSP slot (not live RBX —
/// that is the CPUID EBX output). Also snap RBP/R10–R15 for inlined
/// scans. Firmware CPUID (low RIP) is not touched. Not `ISO-INSTALL-OK`.
#[cfg(target_os = "uefi")]
unsafe fn linux_hypervisor_scan_bump_callee_gprs(leaf: u32) -> bool {
    let mut gprs = [
        SAVED_RBP, SAVED_R10, SAVED_R11, SAVED_R12, SAVED_R13, SAVED_R14, SAVED_R15,
    ];
    let gpr_hit = guest_uefi_linux_hypervisor_scan_bump_gprs(leaf, &mut gprs);
    if gpr_hit {
        SAVED_RBP = gprs[0];
        SAVED_R10 = gprs[1];
        SAVED_R11 = gprs[2];
        SAVED_R12 = gprs[3];
        SAVED_R13 = gprs[4];
        SAVED_R14 = gprs[5];
        SAVED_R15 = gprs[6];
    }
    let stack_hit = linux_hypervisor_scan_bump_native_cpuid_rbx_slot(leaf);
    if !(gpr_hit || stack_hit) {
        return false;
    }
    if LINUX_HV_SCAN_BUMP
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        serial::write_str("boot: guest-UEFI linux hypervisor-scan bump leaf=0x");
        write_hex(u64::from(leaf));
        serial::write_str(" gpr=");
        write_dec(gpr_hit as u64);
        serial::write_str(" stack=");
        write_dec(stack_hit as u64);
        serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
    }
    true
}

/// alpine-virt `native_cpuid`: `push %rbx; …; cpuid`. RSP is the saved
/// `base`. Do not write SAVED_RBX (that stores into signature[0]).
/// Not `ISO-INSTALL-OK`.
#[cfg(target_os = "uefi")]
unsafe fn linux_hypervisor_scan_bump_native_cpuid_rbx_slot(leaf: u32) -> bool {
    if !guest_uefi_cpuid_leaf_is_hypervisor_scan(leaf) {
        return false;
    }
    let rsp = ops::vmread(GUEST_RSP).unwrap_or(0);
    let mut buf = [0u8; 8];
    if copy_guest_linear_bytes(rsp, &mut buf) < 8 {
        return false;
    }
    let word = u64::from_le_bytes(buf);
    let next = guest_uefi_linux_hypervisor_scan_bump_gpr(leaf, word);
    if next == word {
        return false;
    }
    write_guest_linear_bytes(rsp, &next.to_le_bytes())
}

#[cfg(target_os = "uefi")]
unsafe fn handle_cpuid() -> bool {
    let leaf = SAVED_RAX as u32;
    let sub = SAVED_RCX as u32;
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    if guest_uefi_linux_earlycon_share_on_linux_deliver(
        guest_uefi_pf_should_deliver_to_guest(rip),
        crate::devices::ide_cdrom::product_iso_window_armed(),
    ) {
        // linux earlycon share first CPUID (iron 9a3cbfa n=1 before #PF;
        // blocking write_byte while printk is live).
        serial::set_linux_earlycon_share(true);
    }
    if guest_uefi_pf_should_deliver_to_guest(rip) {
        let k = LINUX_CPUID.fetch_add(1, Ordering::AcqRel);
        let n = k.saturating_add(1);
        if guest_uefi_linux_cpuid_should_log(n) {
            serial::write_str("boot: guest-UEFI linux cpuid n=");
            write_dec(u64::from(n));
            serial::write_str(" leaf=0x");
            write_hex(u64::from(leaf));
            serial::write_str(" sub=0x");
            write_hex(u64::from(sub));
            serial::write_str(" rip=0x");
            write_hex(rip);
            serial::write_line(" (Stage 46; not ISO-INSTALL-OK)");
        }
        linux_hypervisor_scan_bump_callee_gprs(leaf);
    }
    if note_unique_cpuid(leaf, sub) {
        serial::write_str("boot: guest-UEFI CPUID leaf=0x");
        write_hex(u64::from(leaf));
        serial::write_str(" sub=0x");
        write_hex(u64::from(sub));
        serial::write_byte(b'\n');
    }
    let r = if guest_uefi_pf_should_deliver_to_guest(rip) {
        guest_uefi_filter_cpuid_for_linux(leaf, sub)
    } else {
        guest_uefi_filter_cpuid(leaf, sub)
    };
    SAVED_RAX = r.eax as u64;
    SAVED_RBX = r.ebx as u64;
    SAVED_RCX = r.ecx as u64;
    SAVED_RDX = r.edx as u64;
    let extra = guest_uefi_linux_cpuid_exit_skip(rip);
    if extra != 0 {
        let _ = ops::vmwrite(GUEST_RIP, rip.wrapping_add(extra));
        return true;
    }
    skip_cpuid_msr()
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
    crate::boot::serial::guest_tx_clear();
    crate::boot::serial::set_linux_earlycon_share(false);
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
    serial::write_str(" reason=0x");
    write_hex_u32(LAST_EXIT_REASON.load(Ordering::Acquire));
    serial::write_str(" rip=0x");
    write_hex(LAST_GUEST_RIP.load(Ordering::Acquire));
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

/// High-half Linux (or after `#PF` deliver) may keep EFER.NXE.
#[cfg(target_os = "uefi")]
unsafe fn guest_uefi_linux_allow_efer_nx() -> bool {
    let rip = ops::vmread(GUEST_RIP).unwrap_or(0);
    guest_uefi_efer_allow_nx(
        guest_uefi_pf_should_deliver_to_guest(rip)
            || PF_LINUX_DELIVER.load(Ordering::Acquire) != 0,
    )
}

/// CR0.PG does not exit. On every VM-exit, set LMA = LME && PG and match
/// the IA-32e VM-entry control so RDMSR EFER is architectural and the
/// next VMRESUME is legal (KVM vmx_set_efer).
#[cfg(target_os = "uefi")]
unsafe fn sync_guest_efer_lma() {
    let cr0 = ops::vmread(GUEST_CR0).unwrap_or(0);
    let efer = ops::vmread(GUEST_IA32_EFER).unwrap_or(0);
    let with = guest_uefi_efer_with_lma_allow_nx(
        efer,
        guest_uefi_cr0_is_paging(cr0),
        guest_uefi_linux_allow_efer_nx(),
    );
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
        v = guest_uefi_efer_with_lma_allow_nx(
            v,
            guest_uefi_cr0_is_paging(cr0),
            guest_uefi_linux_allow_efer_nx(),
        );
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
    skip_cpuid_msr()
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
        v = guest_uefi_efer_with_lma_allow_nx(
            v,
            guest_uefi_cr0_is_paging(cr0),
            guest_uefi_linux_allow_efer_nx(),
        );
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
        return skip_cpuid_msr();
    }
    if guest_uefi_misc_enable_write(msr, v) {
        return skip_cpuid_msr();
    }
    if guest_uefi_mtrr_write(msr, v) {
        return skip_cpuid_msr();
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
    skip_cpuid_msr()
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

#[cfg(target_os = "uefi")]
fn write_hex_inner_nowait(mut n: u64) {
    let mut buf = [0u8; 16];
    let mut i = 16;
    if n == 0 {
        serial::write_byte_nowait(b'0');
        return;
    }
    while n > 0 && i > 0 {
        i -= 1;
        let d = (n & 0xF) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf[i..] {
        serial::write_byte_nowait(b);
    }
}

#[cfg(target_os = "uefi")]
fn write_hex_nowait(n: u64) {
    write_hex_inner_nowait(n);
}

#[cfg(target_os = "uefi")]
fn write_hex_u32_nowait(n: u32) {
    write_hex_inner_nowait(n as u64);
}

#[cfg(target_os = "uefi")]
fn write_dec_nowait(mut n: u64) {
    if n == 0 {
        serial::write_byte_nowait(b'0');
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
        serial::write_byte_nowait(b);
    }
}

#[cfg(test)]
#[path = "guest_uefi_test.rs"]
mod guest_uefi_test;
