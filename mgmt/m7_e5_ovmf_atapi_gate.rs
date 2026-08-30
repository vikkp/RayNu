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
//! `ConnectDevicesFromQemu` is not `INVALID_PARAMETER`. Iron `0b7d647`:
//! `0xfe=0x520` and PCI UC hole present; firmware then zeroed `0x200`.
//! Same ASSERT `lastmsr=0xc0000080`. QEMU BOTH-OK skipped `eb f3`, not
//! an ASSERT fix. EFER.LMA = LME && CR0.PG; IA-32e entry matches LMA;
//! debugcon 0x402. Iron `b4b4847`: `efer=0xd00` `pg=1` `csl=1` still
//! ASSERT `callerrip=0x1d25193`; `r8` is `gPcdDataBaseSignatureGuid`.
//! Iron `c40f4a8`: `pcdsig=1` after 32-pair MTRR walk, same ASSERT.
//! Iron `aee545f`: DXE `eb ec` skip then `#UD` `0x109d`. Revert skip.
//! Iron `10cb881`: VCNT=8 power-on still ASSERT `mtrr0=0x80000000`.
//! VCNT=32 power-on, no UC hole. Iron `5f59c86`: NXE stripped
//! (`efer=0x500`) `imgentry` CpuDxe still ASSERT `lastmsr=0x23f`
//! (MtrrLib GetMemoryAttributes, not XP). MAXPHYADDR clip-36
//! left `[4GiB, 64GiB)` NP vs default WB (not a mask regression vs
//! `a9ffaa5`); nested 36/40 stays. Iron `be1b028` proved 0–4GiB
//! (`pde20` `pde4000` `pde8000`) then ASSERT `maxpa=46` `pml4e=0x5a6f`.
//! Cap iron width at 32; clear non-leaf PWT/PCD. Iron `162809f`:
//! `maxpa=32` `mtrr1=0x80000800` `pml4e=0x1a02023` `pde20=0x2000083`
//! no 4G — firmware PDPT sparse; refill PDPT[0] keep 4K. Iron `d5fceb1` unclip: ASSERT gone, then
//! `#PF` `err=0` `mov al,[0x80B000]` (MEMFD; dump `linear=` was RIP).
//! Identity-map NP 2M/4K in guest PT (`identity_map_not_present`). Iron
//! `3311ff3`: `#PF` `cr3=0x0` `fail=alloc` — load SEC PML4 (`build_identity_4g`).
//! Iron `7ea62ea`: `fail=present` — SEC already mapped CR2; still VMWRITE CR3. Iron
//! `13e8bd2`: CR3 identity `cr3=0x800000` then same `#PF` `fail=present` (walker
//! present, CPU NP). Rebuild SEC 4G identity once; hide LA57. Iron COM2 after
//! CR3 load: `#PF` `err=0x9` `cr2=0xa027c8` (P+RSVD; NX-in-PTE with NXE=0).
//! Rebuild 4G on reserved-bit #PF. Iron `101b8ec`: 4G n=1 n=2 then
//! `fail=present` `cr2=0x1ae7078` `pde=0x30646870` (MEMFD heap clobber).
//! HV identity PML4 at `0x200000`, e820 reserved 36KiB, always rebuild.
//! Iron `cc7d78a`: 4G n=1 `cr3=0x200000` then `EPT violation gpa=0xc01df1b7`
//! `reason=0x30` (PCI hole; sink range had stopped at 1GiB). Sink-resume
//! `[32MiB, flash)`. Iron `fdf07ba`: EPT sink `maps=4` then `#PF` `err=0x2`
//! `cr2=0x1e9000` `pde=0xc0000083` 4G n=2 then ASSERT `callerrip=0x1d25193`
//! `lastmsr=0x23f` `mtrr0=0x80000000` (4G WB vs MTRR UC). RAM-only identity
//! plus UC 2MiB sink `#PF`. Nested `5db28e3`: `#PF` `cr2=0xffc00000`
//! (flash NP) stop n=1007 BOTH missing. Identity also maps flash + xAPIC.
//! Iron `eb4b27d`: `#PF` `cr2=0x80000008` `err=0xb` `pde=0xc0400083`
//! (RSVD 1GiB PDPTE). Iron `73576cc`: bulk UC 2MiB for the MTRR hole
//! then `#PF` `0x1e9000` 4G n=2 ASSERT `callerrip=0x1d25193` (UC- vs UC).
//! On-demand PAT-UC 2MiB + split RSVD 1GiB. Iron `8df2793`: hole PD
//! `pdpte2=0x204067` then ASSERT — NP 2–4GiB vs MTRR UC. PAT-UC PCD+PWT
//! fills `[2GiB, 4GiB)` at 4G rebuild. Iron `1a93cb8`: PAT WB then
//! NP `[32MiB, 2GiB)` vs MTRR WB — guest PT WB, not EPT. Dump `pde20`.
//! Iron `28f42d2`: `pde20=0x20000e7` still ASSERT — live PDPT[1] 1–2GiB.
//! Dump `pde4000` / `pdpte1`.
//! Iron `be1b028`: `pde4000=0x400000e7` `pdpte1=0x203067` still ASSERT
//! `maxpa=46` `pml4e=0x5a6f` — NP above 4GiB vs default WB; PML4E PWT.
//! Iron `162809f`: `maxpa=32` `pde20=0x2000083` no 4G — PDPT[0] mid-gap.
//! Iron `84171aa`: GPA0 4K `pde0=0x20b027` `pte0=0x67` still ASSERT
//! with firmware 2 MiB `pde20=0x2000083` (no USER). keep_4k ORs
//! `LARGE_2M_FLAGS` (`0x83` → `0xE7`); dump `pde6e`.
//! Iron `5811368`: `pde20=0x20000e7` `pde40=0x40000e7` `pde6e=0x6000e7`
//! still ASSERT `callerrip=0x1d25193`. Nested Intel GPA0 on capped-32
//! BOTH-OK `ataio=0` — skip GPA0 when host hypervisor bit is set.
//! Iron COM2 `489d118`: GPA0 4K `pte0=0x67` `pte1m=0x100067` `pml4e1=0x0`
//! still ASSERT — 0–4 GiB PT matches MTRR; leftover-high NP. GCD
//! untested `[32MiB, 4GiB)` spans PEI `Uc32Base` (`mtrr0=0x80000000`).
//! `etc/e820` reserved PCI UC `[2GiB, 4GiB)` so `PlatformAddHobCB`
//! splits GCD. Iron `38481d9`: e820 type-2 still ASSERT `pde8000=0x800000ff`
//! — this OVMF.fd does not split. Iron `f07a597`: 4G WB then PAT-UC when
//! UC MTRR went live still ASSERT `pde8000=0x800000ff` `mtrr0=0x80000000`
//! — guest PT matches MTRR; stop paging paints. Hold valid UC variable
//! MTRRs so CpuDxe RefreshGcd sees default WB (`MTRR UC held (GCD)`).
//! Iron `22e0cb2`: hold ran (`mtrrv=0` `pde8000=E7`) still ASSERT
//! `callerrip=0x1d25193` — mixed MTRR disproved. Iron `f9a08c9` e820
//! type-2 mid-gap ignored (same dump). CMOS/fw_cfg LowMemory 2 GiB
//! so PEI HOB ends at `Uc32Base` (not identity-map the gap). Iron
//! `fad19b2`: CMOS 2 GiB then `EPT unbacked report-RAM gpa=0x7bddd000`
//! `reason=0x30` stop n=600 (`cmos=0x35`; ASSERT gone). Lazy 2 MiB WB
//! EPT (not 2 GiB identity; not `89c3731`). Iron `32e7d46`: maps worked
//! (`gpa=0x7bddd000` `hpa=0x7c00000`) then `rip=0x7f8e21ca` `reason=0x34`
//! `same=376` `lastmsr=0x23f` `insn=` empty (32 MiB peek). Peek report-RAM
//! HPA for skip/ASSERT dump. Do not skip `ebecc9c3`.
//! Iron `c70768b`: `MTRR UC admitted` `mtrrv=1` `mtrr0=0x80000000`
//! `pde8000=0x80000083` `cr3=0x7fa01000` `pml4e=0x7fa02023` still
//! ASSERT `insn=ebecc9c3` — live report-RAM CR3 is WB; 32MiB
//! `identity_sync_live_mtrr_uc_hole` missed it. Peek/poke PAT-UC on
//! that CR3 (P5 GCD split + P2 admit; not f07a597 low-CR3 paint).
//! Iron `4ae87de`: paint `n=1029` `pde8000=0x800000ff` still ASSERT
//! `pde0=0xe3` `pte0=0` — 2MiB GPA0 spans 1MiB fixed-MTRR on live CR3.
//! Peek/poke HV PT `0x20B000` into live PD[0]. Do not skip `ebecc9c3`.
//! Iron `7e5d70f`: `GPA0 4K live CR3 n=513` `pde0=0x20b027` `pte0=0x67`
//! `pte1m=0x100067` `pde8000=0x800000ff` still ASSERT `ebecc9c3`
//! `callerrip=0x7fd25193` — stop PT peek/poke. GCD/HOB: e820 type-1
//! `[0, 2MiB)` covered VGA UC. Do not lower CMOS (32MiB already
//! ASSERTed). Do not retry P3 mid-gap type-2. Classic VGA hole
//! `[640KiB, 1MiB)` not RAM. Keep 2GiB LowMemory.
//! Iron `c1476d3`: hypervisor `etc/e820` VGA hole logged; same ASSERT
//! `insn=ebecc9c3` `callerrip=0x7fd25193`. PEI uses CMOS size → HOBs →
//! GCD, not `QemuFwCfgFindFile("etc/e820")`. Host-bridge DID at PEI
//! `00:00.0` is the fork: i440FX `0x1237` → stock QEMU map including
//! VGA IoMemory HOB; virtio `0x1042` → merged `[0, LowMemory)`. PEI
//! DID is i440FX; DXE latches virtio on other-BDF CF8. Do not remap
//! `cmp bx, 0x1237` while PEI captures `HostBridgeDevId`. Dump `e820=`
//! `fwdir=` `pei_did=`.
//! Iron `f7620f6`: PEI `pci cfg=0x80000002 val=0x1237` `pei_did=1`, DXE
//! latch `00:01.03`, then virtio `0x1042` VIRTIO-OK DXE-OK `sectors=0`
//! `e820=0` `fwdir=0` still ASSERT `ebecc9c3` `callerrip=0x7fd25193`
//! `pte0=0x67` `pte1m=0x100067`. DID fork closed.
//! Iron `d6b012a`: `pte_a0000=0xa0067` `pte_c0000=0xc0067` — GPA0 identity
//! WB, firmware FIX `0x250–0x26f` are `0x06` WB. Not a GCD VGA punch.
//! Do not PAT-UC VGA. P1 hold (`22e0cb2`) already disproved mixed MTRR.
//! `filehex` is CpuFlush: `test r9d; jnz; wbinvd; mov rax, EFI_UNSUPPORTED`.
//! Nop that `jnz` in live report-RAM so every FlushType WBINVD. Dump `r9=`.
//! Iron `f0781bb`: CpuFlush `n=2` `filehex` `9090` `r9=0x21` still ASSERT
//! `ebecc9c3` `callerrip=0x7fd25193`. CpuFlush is leftover File dump.
//! P1 hold (`22e0cb2`) ran while FIX was power-on 0 (UC). Firmware now
//! FIX `0x06` WB. Hold variable UC after FIX WB
//! (`MTRR UC held after FIX WB (GCD)`). Scan every report-RAM CpuFlush
//! copy (do not return after the first slot). Dump `flushjnz=`.
//! Iron `6334704`: hold after FIX WB `mtrrv=0` `mtrr1=0x0`
//! `pde8000=0x80000083` `flushjnz=0` still ASSERT `ebecc9c3`
//! `callerrip=0x7fd25193`. Mixed variable-UC disproved with FIX WB.
//! PEI i440FX VGA IoMemory HOB is GCD UC; firmware FIX `0x259–0x26f`
//! are WB `0x06`. Hold also left Uc32Base GCD UC vs default WB.
//! Admit variable UC (2GiB hole). Coerce FIX `0x259` (VGA
//! `A0000–BFFFF`) to packed UC (`MTRR VGA FIX UC (GCD)`). Keep `0x250`/
//! `0x258` WB and `0x268–0x26F` firmware WB. Dump `mtrr259=` `mtrr268=`.
//! Iron `ddbd866`: admit+VGA FIX UC `mtrr259=0x0`
//! `mtrrv=1` `pde8000=0x800000ff` still ASSERT `ebecc9c3` with
//! `pte_a0000=0xa0067` (GPA0 WB vs coerced FIX UC). PAT-UC those 4K
//! leaves on the live CR3 (`guest_uefi_pt_paint_vga_uc`). Dump `calltgt=`.
//! Iron `e368e86`: `pte_a0000=0xa007f` `pte_c0000=0xc007f` `mtrr259=0x0`
//! still ASSERT `calltgt=0x7f8e21a5` (DebugAssert). PAT-UC only
//! `[0xA0000, 0xC0000)`. Iron `fd041bb`: `n=32` `pte_c0000=0xc0067`
//! then CpuDxe UC'd `mtrr268=0x0` still ASSERT. Hold FIX WB
//! (`MTRR VGA FIX WB held (GCD)`). Dump `prehex=`. Do not skip `ebecc9c3`.
//! Iron `96ef961`: hold landed `mtrr259=0x606…6` `mtrr268=0x606…6`
//! `pte_a0000=0xa0067` still ASSERT `ebecc9c3` `callerrip=0x7fd25193`
//! `prehex=66c705…` (`mov word, 6` / `jmp` / `call DebugAssert`).
//! Dump 32-byte `prehex=` immediately before `0x7fd25193` plus `rax`
//! (EFI_STATUS). `retpre=` 32 bytes at CpuDxe `ret-32`. Keep PEI
//! i440FX `0x1237` / DXE virtio `0x1042`. Keep FIX WB hold. No DID
//! flip. No new PAT-UC. Do not skip `ebecc9c3`.
//! Iron `6f077a3`: `prehex=` at `0x7fd25193` is DxeCore
//! `call [rax+0x20]`; `rax=0` leftover (CpuDxe never returned).
//! `retpre=` switch stores UINT16 `0x0600`/`0xB000` then `jmp`;
//! default `call DebugAssert` (`ASSERT(FALSE)`, not `ASSERT_EFI_ERROR`).
//! Dump `retcmp=` at `ret-64` plus `rbx`/`rsi`/`rdi`/`g16=`. Do not
//! skip `ebecc9c3`. No DID flip. No new PAT-UC.
//! Iron `2cbf9e8`: `retcmp=` `cmp ax, 0x1237` / `0x29C0` / `0x0D57`
//! is CpuDxe `AcpiTimerLibConstructor` (`PIIX4_PMBA_VALUE` `0xB000` /
//! `ICH9_PMBASE_VALUE` `0x0600`). `00:00.0` was virtio `0x1042` after
//! latch (`g16=0000` `rsi=0x7f6e1042`). Keep `00:00.0` i440FX `0x1237`;
//! latch virtio at `00:02.0`. Do not skip `ebecc9c3`.
//! Iron `bf696ca` COM2 **CLOSED** Stage 44: `OVMF-ATAPI-OK` `sectors=1`
//! `packet=9` `scsi=0x28` `ata=0xa0` `ataio=982` stop n=30769
//! `pci_ide=1 virtio=1`; BOTH-OK n=12411 virtio `00:02.0` + IDE
//! `00:00.1`; no AcpiTimerLib ASSERT. Not El Torito. Not installer.
//! E4 SHELL then M4.2 G1 EPT `GPA=0x10403000` fail-soft is not Stage 44.
//! Iron `a428202`: `#PF` `cr2=0x80000008` `err=0xb` `pde=0xc0400083`
//! then `identity MMIO fail` (1GiB PDPTE after retargeted PDPT).
//! Iron `124c1a8`: identity MMIO n=2 then `#PF` `cr2=0xffffffff96808086`
//! `err=0x2` `pde=0` `rip=0x300000` (sign-extended 32-bit GPA; PML4[511]).
//! Iron `b25d75b`: MMIO n=3 then `#UD` `linear=0x301093` (PT stores at
//! `0x80000008` hit the shared HPET EPT sink). Dedicated scratch 2MiB.
//! Iron `577c9eb`: scratch `0x80000000` then EPT sink `gpa=0xc0200000`
//! then `#PF` `cr2=0x9896808086` `rip=0x300001`. Scratch pool for hole
//! PT pages except live HPET. Leftover-high 32-bit CR2.
//! Iron `471391f`: pool=8 then `#PF` `cr2=0x1e9000` `pde=0xc0000083`
//! 4G n=2 ASSERT `callerrip=0x1d25193`. Split 1GiB RAM PDPTE.
//! Iron `d757a0a`: SPLIT n=2 then `#PF` `err=0x9` `pde=0xafafafafafafafaf`
//! `cr2=0x1d1e6cb`. Refill the RAM PD (do not restore a 0xAF-filled table).
//! Iron `0bad45d`: refill then MMIO `0x80000008` / scratch `0xC0A00000`
//! then `EPT sink gpa=0xc0c00000`, leftover CR2, `#DE` RIP `0xCFFF9E`.
//! Scratch pool 32. Iron `5837243`: pool=32 then a hole **read** walk
//! `0xC1000000..0xC3A00000` filled the pool; `EPT scratch cap`
//! `gpa=0xc3c00000` then sink; RIP `0x3d00001`; `pci_ide=0`. Scratch
//! only on EPT **data write**; hole reads and bit-8 page-walks get an
//! R+X sink (`EPT hole ro`). Iron `da2c9c4`: fetch+walk `qual=0x184`
//! still filled the pool (`gpa=0xc3e00000` RIP `0x3dfffff`).
//! Iron `f93caee`: write-only scratch then `EPT hole ro` `qual=0x184`
//! plus `qual=0x1ab` A/D scratch, then leftover CR2 `0x9896808086`
//! RIP `0x300001` poison fill (`SINK_HPA` is live HPET). Dedicated
//! zero 2MiB for hole RO; never RO-sink onto HPET. Nested Intel
//! `f93caee` BOTH-OK then `0xA1`×4 `ataio=408` `packet=0` (IDENTIFY
//! word 0 was `0x8500`; slave was a second CD). Word 0 is `0x85C0`;
//! slave absent. Nested Intel `48c598a` BOTH-OK then `ataio=1308`
//! `packet=0` (`insn=ef` then `edc9c3` IN EAX,DX poll) because SET
//! FEATURES `0xEF` ABRT'd. SET FEATURES succeeds with DRDY.
//! Iron COM2 PAT-UC still ASSERT `callerrip=0x1d25193` after
//! `identity MMIO n=3` — PDPT[2] 1GiB WB over 2–3GiB (no `#PF`).
//! Split the sibling 1GiB in the MTRR UC hole. Dump `pdpte2`.
//! Nested Intel `73ed589` BOTH-OK ATAPI-OK `sectors=1` then E4 Linux
//! `#DF` vec=8 (OVMF XSETBV left host XCR0). Restore host XCR0 and
//! CR4.OSXSAVE before E4. Iron COM2: `pdpte2=0xc0400083` then MMIO
//! `pde=0xfee000ff` still ASSERT — split PDPT[2]+[3] on RAM SPLIT too.
//! Iron COM2 `8df2793`: `pdpte2=0x204067` (PD) then ASSERT — NP vs MTRR
//! UC. PAT-UC 2–4GiB hole at 4G. Dump `pde8000`.
//! Iron COM2 `d7bfb23`: `pde8000=0x800000ff` still ASSERT — firmware
//! PDPT `0x5000`; sync live PDPT; dump `pdpte3`.
//! Iron COM2 `1de9389`: `pdpte3=0x205067` PS clear, `pde8000=0x800000ff`,
//! still ASSERT `callerrip=0x1d25193` `lastmsr=0x23f` — 1GiB PDPT[3]
//! disproved. Do not tick-sync (CpuDxe MTRR walk). Dump leaf PDEs at ASSERT.
//! Nested Intel `c19b91f` BOTH-OK then n=32768
//! `ataio=0` `acpi=14903` `port=0x64` (`KeyboardWaitForValue` Stall:
//! 8042 status `0x10` never set OBF after `0xAA`). Nested
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
use crate::devices::guest_platform::{boot_menu_wait_skips_bds, bootorder_nul_terminated, e820_splits_gcd_mid_gap, e820_splits_mtrr_uc_hole, e820_splits_vga_below_1m, platform_reports_2g_lowmem};
use crate::devices::ide_cdrom;
use crate::vmx::guest_uefi::{
    atapi_read_evidence, guest_uefi_cpuid_has_hypervisor, guest_uefi_cpuid_is_kvm,
    guest_uefi_cpuid_leaf1_is_uniprocessor, guest_uefi_cpuid_leaf_is_hypervisor_scan,
    guest_uefi_filter_cpuid, guest_uefi_filter_cpuid_for_linux,
    guest_uefi_cpuid_is_genuine_intel,
    guest_uefi_linux_hypervisor_scan_bump_gpr, GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST, guest_uefi_is_misc_enable,
    guest_uefi_is_mtrr_msr, guest_uefi_misc_enable_read, guest_uefi_mtrr_read,
    guest_uefi_mtrr_reset, guest_uefi_mtrr_write, guest_uefi_mtrr_pci_uc_hole,
    guest_uefi_mtrr_poweron_disabled, guest_uefi_mtrr_valid_var_pairs, guest_uefi_mtrr_uc_hole_live,
    guest_uefi_mtrr_fixed_is_vga_hole, guest_uefi_xapic_is_not_sink,
    guest_uefi_pci_hole_is_sink, hlt_should_resume,
    guest_uefi_report_ram_should_map, guest_uefi_string_ins_needs_report_ram_map, guest_uefi_report_ram_gpa_2m, guest_uefi_report_ram_page_off, copy_report_ram_at, store_report_ram_at, store_report_ram_u64, load_report_ram_at, guest_uefi_pt_pml4e_gpa, guest_uefi_pt_walk_pml4e, guest_uefi_pt_walk_pde, guest_uefi_pt_walk_pte, guest_uefi_pt_paint_live_uc_hole, guest_uefi_pt_pde_is_wb_hole, guest_uefi_pt_pde_pat_uc, guest_uefi_pt_split_gpa0, guest_uefi_pt_pde0_is_2m, guest_uefi_gpa0_split_pt_gpa,
    post_dxe_should_stop, preempt_deadloop_is_assert_epilogue, preempt_deadloop_should_skip,
    preempt_deadloop_skip_len, preempt_deadloop_delay_loop_skip_len,
    preempt_deadloop_delay_loop_sets_rax_one, preempt_deadloop_guarded_assert_skip_len,
    guest_uefi_assert_caller_is_dxe_ram, guest_uefi_efer_with_lma, guest_uefi_efer_with_lma_allow_nx, guest_uefi_phys_bits,
    guest_uefi_pf_should_identity_map, guest_uefi_pf_sec_cr3, guest_uefi_pf_should_load_sec_cr3, guest_uefi_pf_should_rebuild_sec_cr3, guest_uefi_pf_error_is_reserved, guest_uefi_pf_should_map_mmio, guest_uefi_pf_gpa32, guest_uefi_mmio_needs_scratch, guest_uefi_ept_scratch_on_qual, guest_uefi_ept_qual_is_walk, guest_uefi_ept_qual_is_fetch, guest_uefi_ept_hole_ro_on_qual, guest_uefi_ept_hole_ro_allows_execute, guest_uefi_rip_is_hole_execute, guest_uefi_hole_ro_uses_dedicated_zero, guest_uefi_insn_is_poison_fill, guest_uefi_pf_should_split_ram_1g, guest_uefi_pde_is_large, guest_uefi_pde_is_poison, guest_uefi_pf_should_fix_ram_wp, guest_uefi_pf_split4k_resume_already_rw, guest_uefi_io_qual_is_string, guest_uefi_io_qual_is_rep, guest_uefi_io_string_count, guest_uefi_io_string_fills_ram, spin_short_jmp_should_skip, e4_restore_xcr0_value, e4_restore_cr4_osxsave, E5_OVMF_VMLAUNCH_RESIDUAL_NOTE,
    GUEST_UEFI_FEATURE_CONTROL_VALUE, GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA, GUEST_UEFI_IRON_PF_CR2, GUEST_UEFI_IRON_PF_HEAP_WR_CR2, GUEST_UEFI_IRON_PF_POISON_CR2, GUEST_UEFI_IRON_PF_POISON_PDE, GUEST_UEFI_IRON_PF_MTRR_UC_CR2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2, GUEST_UEFI_IRON_PF_TRUNC32_CR2, GUEST_UEFI_IRON_MMIO_SCRATCH_GPA, GUEST_UEFI_IRON_SINK_PT_GPA, GUEST_UEFI_IRON_SCRATCH_CAP_GPA, GUEST_UEFI_IRON_SCRATCH_WALK_GPA, GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA, GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK, GUEST_UEFI_IRON_EPT_QUAL_AD_WALK, GUEST_UEFI_IRON_HOLE_RO_HPET_RIP, GUEST_UEFI_IRON_HOLE_X_RIP, GUEST_UEFI_IRON_ZERO_FILL_RIP, GUEST_UEFI_IRON_PF_WP_CR2, GUEST_UEFI_IRON_PF_WP_RIP, GUEST_UEFI_IRON_PF_WP_ERR, GUEST_UEFI_IRON_PF_WP_PDE, GUEST_UEFI_IRON_PF_WP_SPLIT_PDE, GUEST_UEFI_IRON_PF_WP_PML4E_RO, GUEST_UEFI_IRON_PF_XAPIC_CR2, GUEST_UEFI_IRON_PF_XAPIC_ERR, GUEST_UEFI_IRON_PF_XAPIC_PDPTE, GUEST_UEFI_IRON_PF_XAPIC_RIP, GUEST_UEFI_IO_QUAL_REP_INSW_1F0, GUEST_UEFI_IRON_PF_RSVD_CR2, GUEST_UEFI_HV_PML4, GUEST_UEFI_KVM_CPUID_LEAF, GUEST_UEFI_MMIO_SCRATCH_SLOTS, GUEST_UEFI_REPORT_RAM_SLOTS, GUEST_UEFI_IRON_REPORT_RAM_GPA, GUEST_UEFI_EPT_MT_WB, GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP,
    GUEST_UEFI_MEMFD_BASE, GUEST_UEFI_MISC_ENABLE_DEFAULT,
    GUEST_UEFI_MISC_ENABLE_MSR, GUEST_UEFI_POST_DXE_TAIL, M7_E5_OVMF_ATAPI_OK_MARKER,
    GUEST_UEFI_IRON_ASSERT_CALLER_RIP, GUEST_UEFI_ASSERT_PREHEX_BYTES, guest_uefi_assert_prehex_gpa,
    guest_uefi_assert_retcmp_gpa, guest_uefi_assert_retpre_word_gpa,
    GUEST_UEFI_IRON_HIGH_CR3, GUEST_UEFI_IRON_PDE8000_WB,
    GUEST_UEFI_IRON_PDE0_2M, GUEST_UEFI_PT_LARGE_2M_UC, GUEST_UEFI_PT_LEAF_4K,
    GUEST_UEFI_PT_LEAF_4K_UC, GUEST_UEFI_IRON_PTE_A0000_WB,
    guest_uefi_pt_paint_vga_uc, guest_uefi_pt_leaf_4k_for, guest_uefi_gpa_in_vga_fix_uc,
    guest_uefi_patch_cpu_flush_unsupported, guest_uefi_count_cpu_flush_jnz,
    GUEST_UEFI_CPU_FLUSH_UNSUPPORTED,
    GUEST_UEFI_CPU_FLUSH_JNZ_OFF, GUEST_UEFI_IRON_CPU_FLUSH_GPA,
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
    // Nested Intel 48c598a: firmware OUT 0xEF then polls IN EAX,DX.
    let _ = ide_cdrom::ata_io(0x01F1, false, 1, 0x03);
    let _ = ide_cdrom::ata_io(0x01F7, false, 1, 0xEF);
    let st = ide_cdrom::ata_io(0x01F7, true, 1, 0) as u8;
    if (st & 0x01) != 0 || (st & 0x40) == 0 || ide_cdrom::last_ata_cmd() != 0xEF {
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
    let launch = include_str!("../vmx/launch.rs");
    let gpt = include_str!("../vmx/guest_pt.rs");
    let ide = include_str!("../devices/ide_cdrom.rs");
    let plat = include_str!("../devices/guest_platform.rs");
    let virt = include_str!("../devices/guest_virtio_blk.rs");
    let uart = include_str!("../devices/guest_uart.rs");
    let flash = include_str!("../tools/flash-cruzer-esp.sh");
    let msr = include_str!("../sched/msr_firewall.rs");
    let main = include_str!("../src/main.rs");
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
        && guest.contains("fn guest_uefi_hpet_step_for_exit(")
        && guest.contains("fn guest_uefi_hpet_uart_tsc_step")
        && guest.contains("HPET TSC-delta on UART COM I/O")
        && guest.contains("Linux printk ticks every 4096")
        && guest.contains("guest UART nowait (do not clear COM2_LIVE)")
        && guest.contains("guest UART TX ring drain")
        && guest.contains("GUEST_TX_DRAIN_EXIT")
        && guest.contains("linux earlycon share TX ring")
        && guest.contains("linux earlycon quiet ticks")
        && guest.contains("linux earlycon hush HV")
        && guest.contains("linux earlycon share product ISO")
        && guest.contains("cpu_flush on tick cadence even when share")
        && guest.contains("linux earlycon share first CPUID")
        && guest.contains("linux earlycon share first high-half")
        && guest.contains("fn guest_uefi_linux_earlycon_share_on_vmexit")
        && guest.contains("linux earlycon share first bootimg")
        && guest.contains("fn guest_uefi_linux_earlycon_share_on_bootimg")
        && guest.contains("guest UART TX drain COM2 independent")
        && guest.contains("linux earlycon pace LSR THRE")
        && guest.contains("report-RAM EPT pre-map")
        && guest.contains("fn guest_uefi_report_ram_premap_gpa")
        && guest.contains("linux earlycon EPT cap nowait")
        && guest.contains("cpu_flush skip leftover pre-map")
        && guest.contains("fn guest_uefi_cpu_flush_skip_mapped")
        && guest.contains("cpu-flush skip leftover pre-map")
        && guest.contains("cpu_flush leftover per walk")
        && guest.contains("fn guest_uefi_cpu_flush_tick_scans_mapped")
        && guest.contains("cpu_flush skip leftover pre-map on tick")
        && guest.contains("linux unhandled nowait stop")
        && guest.contains("fn guest_uefi_linux_unhandled_should_skip")
        && guest.contains("fn guest_uefi_linux_guest_active")
        && guest.contains("fn virtio_mmio_eax_fallback")
        && guest.contains("iso=0 decode fail still stops")
        && guest.contains("fn virtio_mmio_retry_decode_len")
        && guest.contains("linux MMIO decode retry")
        && guest.contains("fn virtio_mmio_eax_fallback_len")
        && guest.contains("linux EAX fallback skip 3")
        && guest.contains("fn virtio_mmio_eax_fallback_size")
        && guest.contains("virtio MMIO eax fallback size")
        && guest.contains("fn guest_uefi_virtio_mmio_polls_lapic")
        && guest.contains("virtio MMIO polls lapic")
        && guest.contains("IOAPIC decode fail nowait")
        && guest.contains("fn guest_uefi_linux_mov_dr_len")
        && guest.contains("linux MOV DR skip")
        && guest.contains("fn guest_uefi_linux_unhandled_try_skip")
        && guest.contains("fn guest_uefi_nmi_entry_info")
        && guest.contains("linux NMI inject")
        && guest.contains("fn guest_uefi_virtio_bar_overlaps_scratch")
        && guest.contains("virtio BAR trap over scratch")
        && guest.contains("fn ept_split_2m_trap_4k")
        && guest.contains("fn guest_uefi_virtio_mmio_raises_pit")
        && guest.contains("virtio MMIO raises PIT")
        && guest.contains("virtio MMIO off=")
        && include_str!("../devices/guest_virtio_blk.rs").contains("fn common_cfg_byte")
        && include_str!("../devices/guest_virtio_blk.rs").contains("packed virtio common cfg")
        && include_str!("../devices/guest_virtio_blk.rs").contains("fn common_cfg_write_byte")
        && include_str!("../devices/guest_virtio_blk.rs").contains("packed virtio common cfg write")
        && plat.contains("PIIX3 ISA BAR RAZ")
        && include_str!("../devices/guest_virtio_blk.rs").contains("fn mmio_programmed_bar_gpas")
        && include_str!("../boot/serial.rs").contains("fn write_str_nowait")
        && uart.contains("Keep the 0x60/0x61 path until")
        && guest.contains("linux earlycon skip #PF dump")
        && guest.contains("linux earlycon skip exc deliver")
        && guest.contains("poll ISO-INSTALL-OK every resume")
        && guest.contains("256MiB disk leftover report-RAM")
        && guest.contains("fn guest_uefi_poll_iso_install_ok")
        && guest.contains("fn guest_uefi_linux_earlycon_share_on_linux_deliver")
        && guest.contains("set_linux_earlycon_share")
        && plat.contains("is_kbc_port")
        && plat.contains("KeyboardWaitForValue")
        && plat.contains("kbc_push")
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
        && guest.contains("preempt_deadloop_guarded_assert_skip_len")
        && guest.contains("guest_uefi_assert_caller_is_dxe_ram")
        && guest.contains("891eb5b")
        && guest.contains("leave; ret")
        && guest.contains("ebecc9c3")
        && guest.contains("ebf3c9c3")
        && guest.contains("guest_uefi_filter_cpuid")
        && guest.contains("fn guest_uefi_filter_cpuid_for_linux")
        && guest.contains("fn guest_uefi_cpuid_leaf_is_hypervisor_scan")
        && guest.contains("fn guest_uefi_linux_hypervisor_scan_bump_gpr")
        && guest.contains("GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST")
        && guest.contains("hypervisor-scan bump")
        && guest.contains("fn linux_hypervisor_scan_bump_native_cpuid_rbx_slot")
        && guest.contains("push %rbx")
        && guest.contains("zero-extended")
        && guest.contains("linux delay_loop skip")
        && guest.contains("0x40003d00")
        && guest.contains("0x4000bd00")
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
        && guest.contains("0b7d647")
        && guest.contains("EFER.LMA")
        && guest.contains("CR0.PG")
        && guest.contains("IA-32e entry")
        && guest.contains("debugcon 0x402")
        && guest.contains("b4b4847")
        && guest.contains("gPcdDataBaseSignatureGuid")
        && guest.contains("c40f4a8")
        && guest.contains("aee545f")
        && guest.contains("10cb881")
        && guest.contains("power-on E=0")
        && guest.contains("DXE assert skip")
        && guest.contains("guest_uefi_mtrr_poweron_disabled")
        && guest.contains("guest_uefi_mtrr_valid_var_pairs")
        && plat.contains("bootorder_nul_terminated")
        && plat.contains("HV_IDENTITY_PML4")
        && plat.contains("E820_RESERVED")
        && plat.contains("E820_FILE_BYTES")
        && plat.contains("E820_PCI_UC_BASE")
        && plat.contains("e820_splits_mtrr_uc_hole")
        && plat.contains("e820_splits_gcd_mid_gap")
        && plat.contains("E820_MID_GAP_BASE")
        && plat.contains("e820_splits_vga_below_1m")
        && plat.contains("E820_VGA_BASE")
        && plat.contains("platform_reports_2g_lowmem")
        && plat.contains("PLATFORM_REPORT_RAM_BYTES")
        && guest.contains("a9ffaa5")
        && guest.contains("GUEST_UEFI_EFER_NXE")
        && guest.contains("5f59c86")
        && guest.contains("lastmsr=0x23f")
        && guest.contains("MAXPHYADDR")
        && guest.contains("guest_uefi_phys_bits")
        && guest.contains("ldri ImageBase")
        && guest.contains("CoreStartImage")
        && guest.contains("d5fceb1")
        && guest.contains("0x80B000")
        && guest.contains("identity_map_not_present")
        && guest.contains("guest_uefi_pf_should_identity_map")
        && guest.contains("3311ff3")
        && guest.contains("fail=alloc")
        && guest.contains("build_identity_4g")
        && guest.contains("guest_uefi_pf_sec_cr3")
        && guest.contains("7ea62ea")
        && guest.contains("fail=present")
        && guest.contains("guest_uefi_pf_should_load_sec_cr3")
        && guest.contains("13e8bd2")
        && guest.contains("guest_uefi_pf_should_rebuild_sec_cr3")
        && guest.contains("Rebuild4G")
        && guest.contains("CPUID_LEAF7_ECX_LA57")
        && guest.contains("CPUID_LEAF7_EBX_CLWB")
        && guest.contains("0xa027c8")
        && guest.contains("guest_uefi_pf_error_is_reserved")
        && guest.contains("GUEST_UEFI_IRON_PF_RSVD_CR2")
        && guest.contains("101b8ec")
        && guest.contains("GUEST_UEFI_HV_PML4")
        && guest.contains("0x1ae7078")
        && guest.contains("0x30646870")
        && guest.contains("cc7d78a")
        && guest.contains("0xc01df1b7")
        && guest.contains("guest_uefi_pci_hole_is_sink")
        && plat.contains("PCI hole")
        && guest.contains("fdf07ba")
        && guest.contains("0x1e9000")
        && guest.contains("guest_uefi_pf_should_map_mmio")
        && gpt.contains("identity_map_mmio_2m")
        && gpt.contains("gpa < ram_len")
        && gpt.contains("IDENTITY_FLASH_FLOOR")
        && gpt.contains("IDENTITY_XAPIC_GPA")
        && gpt.contains("IDENTITY_MTRR_UC_FLOOR")
        && gpt.contains("0xc0400083")
        && gpt.contains("PCD | PWT")
        && gpt.contains("73576cc")
        && guest.contains("GUEST_UEFI_IRON_PF_MTRR_UC_CR2")
        && guest.contains("eb4b27d")
        && gpt.contains("IDENTITY_HV_PML4")
        && gpt.contains("a428202")
        && guest.contains("identity MMIO fail")
        && gpt.contains("124c1a8")
        && gpt.contains("96808086")
        && gpt.contains("identity_signext32_gpa")
        && gpt.contains("identity_trunc32_hole_gpa")
        && gpt.contains("identity_hole32_gpa")
        && guest.contains("GUEST_UEFI_IRON_PF_SIGNEXT_CR2")
        && guest.contains("GUEST_UEFI_IRON_PF_TRUNC32_CR2")
        && guest.contains("b25d75b")
        && guest.contains("guest_uefi_mmio_needs_scratch")
        && guest.contains("GUEST_UEFI_IRON_MMIO_SCRATCH_GPA")
        && guest.contains("577c9eb")
        && guest.contains("0x9896808086")
        && guest.contains("scratch pool")
        && guest.contains("GUEST_UEFI_MMIO_SCRATCH_SLOTS")
        && guest.contains("usize = 32")
        && guest.contains("0bad45d")
        && guest.contains("scratch cap")
        && guest.contains("GUEST_UEFI_IRON_SCRATCH_CAP_GPA")
        && guest.contains("5837243")
        && guest.contains("GUEST_UEFI_IRON_SCRATCH_WALK_GPA")
        && guest.contains("guest_uefi_ept_scratch_on_qual")
        && guest.contains("EPT hole ro")
        && guest.contains("0x3d00001")
        && guest.contains("da2c9c4")
        && guest.contains("GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA")
        && guest.contains("guest_uefi_ept_qual_is_walk")
        && guest.contains("0x3dfffff")
        && guest.contains("f93caee")
        && guest.contains("0x1ab")
        && guest.contains("0x300001")
        && guest.contains("poison fill")
        && guest.contains("dedicated zero")
        && guest.contains("guest_uefi_hole_ro_uses_dedicated_zero")
        && guest.contains("guest_uefi_ept_hole_ro_allows_execute")
        && guest.contains("guest_uefi_rip_is_hole_execute")
        && guest.contains("pml4e=0x")
        && guest.contains("pdpte=0x")
        && guest.contains("7413554")
        && guest.contains("0xfee00020")
        && guest.contains("map_mmio xAPIC")
        && gpt.contains("identity_map_mmio_splits_xapic_rsvd_1g")
        && gpt.contains("identity_mtrr_uc_sibling_pdpt")
        && gpt.contains("identity_split_mtrr_uc_hole")
        && gpt.contains("identity_pdpte_is_1g")
        && gpt.contains("identity_sync_live_mtrr_uc_hole")
        && gpt.contains("c70768b")
        && gpt.contains("IDENTITY_FW_PDPT_GPA")
        && gpt.contains("PAT-UC PCD+PWT")
        && gpt.contains("8df2793")
        && gpt.contains("d7bfb23")
        && gpt.contains("1de9389")
        && guest.contains("1de9389")
        && guest.contains("44c56db")
        && guest.contains("IA32_PAT_RESET")
        && guest.contains("1a93cb8")
        && launch.contains("E4_LINUX_CR4_FORBIDDEN")
        && msr.contains("CPUID_LEAF7_ECX_LA57")
        && main.contains("release_report_ram_for_e4")
        && launch.contains("e4_linux_guest_cr4")
        && launch.contains("Linux CR4.VMXE+OSFXSR host-owned")
        && launch.contains("e4_linux_apply_cr4_write")
        && launch.contains("handle_linux_cr_and_resume")
        && launch.contains("0x8400276")
        && guest.contains("pde20=0x")
        && guest.contains("pde4000=0x")
        && guest.contains("pdpte1=0x")
        && gpt.contains("identity_ensure_pdpt_2m")
        && guest.contains("pde8000=0x")
        && guest.contains("pdpte3=0x")
        && gpt.contains("Iron 1a93cb8")
        && gpt.contains("identity_clear_table_pwt_pcd")
        && gpt.contains("IDENTITY_IRON_PML4E_PWT")
        && gpt.contains("be1b028")
        && guest.contains("GUEST_UEFI_PHYS_BITS_IRON_CAP")
        && guest.contains("guest_uefi_gpa0_fixed_mtrr_split")
        && guest.contains("guest_uefi_gpa0_split_now")
        && guest.contains("5811368")
        && guest.contains("489d118")
        && guest.contains("38481d9")
        && guest.contains("guest_uefi_mtrr_uc_hole_live")
        && guest.contains("unsafe { ops::vmread(GUEST_CR3) }")
        && guest.contains("f07a597")
        && guest.contains("guest_uefi_mtrr_set_admit_uc")
        && guest.contains("MTRR UC held (GCD)")
        && guest.contains("22e0cb2")
        && guest.contains("e820 mid-gap reserved (GCD)")
        && guest.contains("CMOS LowMemory 2GiB (GCD)")
        && guest.contains("fad19b2")
        && guest.contains("0x7bddd000")
        && guest.contains("EPT report-RAM")
        && guest.contains("ept_map_2m_report_ram")
        && guest.contains("GUEST_UEFI_IRON_REPORT_RAM_GPA")
        && guest.contains("GUEST_UEFI_REPORT_RAM_SLOTS")
        && guest.contains("copy_guest_identity_bytes")
        && guest.contains("0x7f8e21ca")
        && guest.contains("GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP")
        && guest.contains("GUEST_UEFI_IRON_ASSERT_CALLER_RIP")
        && guest.contains("dump_walk_pde")
        && guest.contains("MTRR UC admitted (GCD)")
        && guest.contains("guest_uefi_pt_paint_live_uc_hole")
        && guest.contains("MTRR UC live PT painted")
        && guest.contains("c70768b")
        && guest.contains("0x80000083")
        && guest.contains("GUEST_UEFI_IRON_PDE8000_WB")
        && guest.contains("4ae87de")
        && guest.contains("guest_uefi_pt_split_gpa0")
        && guest.contains("GPA0 4K live CR3")
        && guest.contains("GUEST_UEFI_IRON_PDE0_2M")
        && guest.contains("7e5d70f")
        && plat.contains("e820_splits_vga_below_1m")
        && plat.contains("E820_VGA_BASE")
        && guest.contains("fw_cfg etc/e820 offered (PEI FindFile or CMOS HOBs)")
        && guest.contains("PEI 00:00.0 DID i440FX 0x1237 (MemMap VGA HOB)")
        && guest.contains("c1476d3")
        && guest.contains("latch_dxe_virtio_did")
        && guest.contains("fwcfg_file_dir_served")
        && virt.contains("pei_host_bridge_did")
        && guest.contains("release_report_ram_for_e4")
        && launch.contains("E4_LINUX_CR4_FORBIDDEN")
        && plat.contains("E820_MID_GAP_BYTES")
        && gpt.contains("identity_set_pat_uc_hole")
        && gpt.contains("38481d9")
        && gpt.contains("identity_refill_low4g_pd_keep_4k")
        && gpt.contains("IDENTITY_WB_64M")
        && gpt.contains("162809f")
        && gpt.contains("identity_split_gpa0_fixed_mtrr")
        && gpt.contains("Iron 659e7de")
        && gpt.contains("84171aa")
        && gpt.contains("IDENTITY_CPU_DXE_IMG")
        && gpt.contains("IDENTITY_FIXED_MTRR_1M")
        && gpt.contains("IDENTITY_PML4E1_GVA")
        && guest.contains("pde40=0x")
        && guest.contains("pde6e=0x")
        && guest.contains("pde0=0x")
        && guest.contains("pte0=0x")
        && guest.contains("pte1m=0x")
        && guest.contains("pte_a0000=0x")
        && guest.contains("pte_c0000=0x")
        && gpt.contains("IDENTITY_VGA_A0000")
        && gpt.contains("IDENTITY_VGA_C0000")
        && guest.contains("f7620f6")
        && guest.contains("d6b012a")
        && guest.contains("guest_uefi_patch_cpu_flush_unsupported")
        && guest.contains("guest_uefi_patch_cpu_flush_all_mapped")
        && guest.contains("guest_uefi_count_cpu_flush_jnz")
        && guest.contains("GUEST_UEFI_CPU_FLUSH_UNSUPPORTED")
        && guest.contains("r9=0x")
        && guest.contains("flushjnz=")
        && guest.contains("f0781bb")
        && guest.contains("MTRR UC held after FIX WB (GCD)")
        && guest.contains("6334704")
        && guest.contains("MTRR VGA FIX WB hold armed (GCD)")
        && guest.contains("MTRR VGA FIX WB held (GCD)")
        && guest.contains("prehex=")
        && guest.contains("retpre=")
        && guest.contains("GUEST_UEFI_ASSERT_PREHEX_BYTES")
        && guest.contains("guest_uefi_assert_prehex_gpa")
        && guest.contains("guest_uefi_assert_retcmp_gpa")
        && guest.contains("guest_uefi_assert_retpre_word_gpa")
        && guest.contains("retcmp=")
        && guest.contains("g16=")
        && guest.contains("rbx=0x")
        && guest.contains("2cbf9e8")
        && guest.contains("bf696ca")
        && guest.contains("scsi=0x28")
        && guest.contains("96ef961")
        && guest.contains("fd041bb")
        && guest.contains("guest_uefi_mtrr_fixed_is_vga_hole")
        && guest.contains("GUEST_UEFI_MTRR_UC_PACKED")
        && guest.contains("mtrr259=0x")
        && guest.contains("mtrr268=0x")
        && guest.contains("ddbd866")
        && guest.contains("guest_uefi_pt_paint_vga_uc")
        && guest.contains("VGA 4K live PT PAT-UC")
        && guest.contains("calltgt=")
        && guest.contains("e368e86")
        && guest.contains("0x7f8e21a5")
        && guest.contains("GUEST_UEFI_PT_LEAF_4K_UC")
        && guest.contains("GUEST_UEFI_IRON_PTE_A0000_WB")
        && guest.contains("pml4e1=0x")
        && guest.contains("pdefee=0x")
        && guest.contains("pdeffc=0x")
        && guest.contains("pat=0x")
        && gpt.contains("LARGE_2M_UC_FLAGS")
        && guest.contains("32ee302")
        && guest.contains("PAT-UC PCD+PWT")
        && guest.contains("48c598a")
        && guest.contains("SET FEATURES")
        && guest.contains("ataio=1308")
        && guest.contains("pdpte2=0x")
        && guest.contains("sibling 1GiB")
        && guest.contains("e4_restore_xcr0_value")
        && guest.contains("restore_host_xsave_after_guest_uefi")
        && ide.contains("ATA_CMD_SET_FEATURES")
        && guest.contains("HOLE_ZERO_HPA")
        && guest.contains("GUEST_UEFI_IRON_EPT_QUAL_AD_WALK")
        && ide.contains("0x85C0")
        && ide.contains("ata_is_slave")
        && guest.contains("471391f")
        && guest.contains("guest_uefi_pf_should_split_ram_1g")
        && guest.contains("identity SPLIT")
        && guest.contains("d757a0a")
        && guest.contains("guest_uefi_pde_is_poison")
        && gpt.contains("identity_refill_low4g_pd")
        && gpt.contains("IDENTITY_POISON_PTE")
        && gpt.contains("identity_walk_is_writable")
        && gpt.contains("identity_walk_pml4e")
        && guest.contains("17449e2")
        && guest.contains("uniprocessor")
        && guest.contains("pause CpuDeadLoop")
        && guest.contains("preempt noskip")
        && guest.contains("eb fc")
        && guest.contains("tick n=")
        && guest.contains("PIIX3 ISA PIRQ")
        && guest.contains("ataio=")
        && guest.contains(" ram=")
        && guest.contains("GRUB `rep insw` into GCD heap never EPT-walks")
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

/// Full E5 Stage 44 package. Host gate + QEMU marker. Iron COM2 `bf696ca`
/// closed Stage 44 (`OVMF-ATAPI-OK` `sectors=1`). Not El Torito. Not Everest E5.
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
        && preempt_deadloop_guarded_assert_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3], 0x6e81ca, 0x1d25193)
            == 0
        && preempt_deadloop_guarded_assert_skip_len(&[0xEB, 0xEC, 0xC9, 0xC3], 0x109D, 0x1d25193)
            == 0
        && guest_uefi_assert_caller_is_dxe_ram(0x1d25193)
        && !guest_uefi_assert_caller_is_dxe_ram(0x109D)
        && preempt_deadloop_is_assert_epilogue(&[0xEB, 0xEC, 0xC9, 0xC3])
        && !preempt_deadloop_is_assert_epilogue(&[0xEB, 0xF3, 0xC9, 0xC3])
        && !preempt_deadloop_is_assert_epilogue(&[0xEB, 0xFC, 0x90, 0x90])
        && !spin_short_jmp_should_skip(0xEB, 0xFC)
        && !spin_short_jmp_should_skip(0xEB, 0xEC)
        && !preempt_deadloop_should_skip(0x74, 0x02)
        && preempt_deadloop_skip_len(&[0xF3, 0x90]) == 2
        && preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xFB]) == 5
        && preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xFB, 0x48, 0xFF, 0xC8, 0x75, 0xE0])
            == 10
        && preempt_deadloop_delay_loop_skip_len(&[0x48, 0xFF, 0xC8, 0x75, 0xE0]) == Some(5)
        && preempt_deadloop_delay_loop_sets_rax_one(&[0x48, 0xFF, 0xC8, 0x75, 0xFB])
        && preempt_deadloop_skip_len(&[0x48, 0xFF, 0xC8]) == 0
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
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("KeyboardWaitForValue")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c19b91f")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("self-test 0x55")
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
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x40003d00")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x4000bd00")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("push rbx RSP slot")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("callerrip")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8700cbb")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("VCNT=32")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("bootorder NUL")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0b7d647")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EFER.LMA")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CR0.PG")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("IA-32e entry")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("debugcon 0x402")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("b4b4847")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("gPcdDataBaseSignatureGuid")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c40f4a8")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("aee545f")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("10cb881")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("power-on E=0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr0=0x80000000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("DXE assert skip")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pcdsig=1")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("a9ffaa5")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ldri ImageBase")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EFER.NXE")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CoreStartImage")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5f59c86")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("lastmsr=0x23f")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("clip-36")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d5fceb1")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80B000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_map_not_present")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("3311ff3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fail=alloc")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("build_identity_4g")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("7ea62ea")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fail=present")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("13e8bd2")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("rebuild SEC 4G")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("hide LA57")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xa027c8")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("err=0x9")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("101b8ec")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1ae7078")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x30646870")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x200000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cc7d78a")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc01df1b7")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("sink-resume PCI hole")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fdf07ba")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1e9000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("RAM-only identity")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5db28e3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xffc00000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("eb4b27d")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80000008")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0400083")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("73576cc")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("a428202")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("124c1a8")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xffffffff96808086")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("b25d75b")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x301093")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("577c9eb")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x9896808086")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0200000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("scratch pool")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("471391f")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d757a0a")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xafafafafafafafaf")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_refill_low4g_pd")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0bad45d")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0c00000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xCFFF9E")
        && GUEST_UEFI_MMIO_SCRATCH_SLOTS == 32
        && GUEST_UEFI_IRON_SCRATCH_CAP_GPA == 0xC0C0_0000
        && guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SCRATCH_CAP_GPA)
        && GUEST_UEFI_IRON_SCRATCH_WALK_GPA == 0xC3C0_0000
        && guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SCRATCH_WALK_GPA)
        && guest_uefi_ept_scratch_on_qual(2)
        && !guest_uefi_ept_scratch_on_qual(1)
        && !guest_uefi_ept_scratch_on_qual(4)
        && !guest_uefi_ept_scratch_on_qual(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK)
        && guest_uefi_ept_qual_is_walk(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK)
        && GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA == 0xC3E0_0000
        && guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SCRATCH_FETCH_WALK_GPA)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5837243")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc3c00000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3d00001")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_ept_scratch_on_qual")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("EPT hole ro")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("da2c9c4")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc3e00000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x184")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3dfffff")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("data-write only")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f93caee")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1ab")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("dedicated zero")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("poison fill")
        && guest_uefi_hole_ro_uses_dedicated_zero(0x3C0_0000, 0x380_0000)
        && !guest_uefi_hole_ro_uses_dedicated_zero(0x380_0000, 0x380_0000)
        && !guest_uefi_hole_ro_uses_dedicated_zero(0, 0x380_0000)
        && guest_uefi_ept_scratch_on_qual(GUEST_UEFI_IRON_EPT_QUAL_AD_WALK)
        && GUEST_UEFI_IRON_HOLE_RO_HPET_RIP == 0x300001
        && guest_uefi_pf_should_fix_ram_wp(GUEST_UEFI_IRON_PF_WP_ERR, GUEST_UEFI_IRON_PF_WP_CR2)
        && !guest_uefi_pf_should_identity_map(GUEST_UEFI_IRON_PF_WP_ERR, GUEST_UEFI_IRON_PF_WP_CR2)
        && GUEST_UEFI_IRON_PF_WP_CR2 == 0x1D1_ABB8
        && GUEST_UEFI_IRON_PF_WP_RIP == 0x1DE_592
        && GUEST_UEFI_IRON_PF_WP_PDE == 0x1C0_00E7
        && guest_uefi_io_qual_is_string(GUEST_UEFI_IO_QUAL_REP_INSW_1F0)
        && guest_uefi_io_qual_is_rep(GUEST_UEFI_IO_QUAL_REP_INSW_1F0)
        && guest_uefi_io_string_count(GUEST_UEFI_IO_QUAL_REP_INSW_1F0, 256) == 256
        && guest_uefi_io_string_fills_ram(0x1F0)
        && !guest_uefi_io_string_fills_ram(0x511)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("06b011a")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("SPLIT4K")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("rep insw")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ataio=236")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1d1abb8")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1e0f4a7")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ATA-only")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1f21193")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1dd97d3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3d2be4")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("54a8708")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x219067")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("already-RW")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("19b0c11")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x27e22d5")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x3ed00001")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("R only")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("preemption while RIP")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("89c3731")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x219027")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("walk R/W")
        && GUEST_UEFI_IRON_PF_WP_PML4E_RO == 0x5A6D
        && GUEST_UEFI_IRON_PF_XAPIC_CR2 == 0xFEE0_0020
        && GUEST_UEFI_IRON_PF_XAPIC_ERR == 0x9
        && GUEST_UEFI_IRON_PF_XAPIC_PDPTE == 0xC060_0083
        && GUEST_UEFI_IRON_PF_XAPIC_RIP == 0x1D8_4C7
        && guest_uefi_pf_error_is_reserved(GUEST_UEFI_IRON_PF_XAPIC_ERR)
        && guest_uefi_pf_should_map_mmio(
            GUEST_UEFI_IRON_PF_XAPIC_ERR,
            GUEST_UEFI_IRON_PF_XAPIC_CR2,
        )
        && !guest_uefi_pf_should_identity_map(
            GUEST_UEFI_IRON_PF_XAPIC_ERR,
            GUEST_UEFI_IRON_PF_XAPIC_CR2,
        )
        && !guest_uefi_pf_should_fix_ram_wp(
            GUEST_UEFI_IRON_PF_XAPIC_ERR,
            GUEST_UEFI_IRON_PF_XAPIC_CR2,
        )
        && guest_uefi_pde_is_large(GUEST_UEFI_IRON_PF_XAPIC_PDPTE)
        && !guest_uefi_rip_is_hole_execute(GUEST_UEFI_IRON_PF_XAPIC_RIP)
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("7413554")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xfee00020")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0600083")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x5a6d")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("map_mmio xAPIC")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("32ee302")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr1=0x3fff80000800")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x1bdd7d3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PAT-UC PCD+PWT")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("48c598a")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ataio=1308")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("SET FEATURES")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("edc9c3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pdpte2")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("sibling 1GiB")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xc0400083")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_split_mtrr_uc_hole")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("73ed589")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("restore host XCR0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("8df2793")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde8000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PAT-UC 2-4GiB hole")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d7bfb23")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_sync_live_mtrr_uc_hole")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c70768b")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pdpte3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1de9389")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x205067")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("44c56db")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pat=0x0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1a93cb8")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("OSFXSR+OSXMMEXCPT")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ab25682")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("emulate MOV CR4")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PAT WB proved")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest PT WB")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde20")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("28f42d2")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_ensure_pdpt_2m")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde4000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("be1b028")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_clear_table_pwt_pcd")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cap iron MAXPHYADDR 32")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("162809f")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_refill_low4g_pd_keep_4k")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1b587dd")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ensure_pdpt_2m(0)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_split_gpa0_fixed_mtrr")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("1MiB fixed-MTRR")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("TABLE_FLAGS USER")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("659e7de")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mmio 2m keeps 4K tables")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("61f84c6")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_gpa0_fixed_mtrr_split")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("84171aa")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x83 to 0xE7")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_gpa0_split_now")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("5811368")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("489d118")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PCI UC [2GiB,4GiB)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("38481d9")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("identity_set_pat_uc_hole")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f07a597")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR UC held (GCD)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("22e0cb2")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mixed MTRR disproved")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mid-gap")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f9a08c9")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("LowMemory 2GiB")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fad19b2")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7bddd000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("lazy 2MiB WB")
        && GUEST_UEFI_REPORT_RAM_SLOTS == 32
        && GUEST_UEFI_IRON_REPORT_RAM_GPA == 0x7BDD_D000
        && GUEST_UEFI_EPT_MT_WB == 6
        && guest_uefi_report_ram_should_map(GUEST_UEFI_IRON_REPORT_RAM_GPA)
        && guest_uefi_string_ins_needs_report_ram_map(GUEST_UEFI_IRON_REPORT_RAM_GPA)
        && !guest_uefi_string_ins_needs_report_ram_map(0x1000)
        && guest_uefi_report_ram_gpa_2m(GUEST_UEFI_IRON_REPORT_RAM_GPA) == 0x7BC0_0000
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7f8e21ca")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("peek report-RAM")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x7fd25193")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR UC admitted")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c70768b")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0x80000083")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_pt_paint_live_uc_hole")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("4ae87de")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pde0=0xe3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_pt_split_gpa0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("7e5d70f")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("c1476d3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PlatformMemMapInitialization")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PEI never opened")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f7620f6")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("pte_a0000")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("00:01.03")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("d6b012a")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("0xa0067")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("CpuFlush")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("f0781bb")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR UC held after FIX WB (GCD)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("flushjnz=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("6334704")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR VGA FIX UC (GCD)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr259=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ddbd866")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest_uefi_pt_paint_vga_uc")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("calltgt=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("e368e86")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("mtrr268=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("coerce only FIX 0x259")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("fd041bb")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("MTRR VGA FIX WB held")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("prehex=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("96ef961")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("retpre=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("no DID flip")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("6f077a3")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("retcmp=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("ASSERT(FALSE)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("2cbf9e8")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("AcpiTimerLibConstructor")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("00:02.0")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("bf696ca")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("scsi=0x28")
        && GUEST_UEFI_ASSERT_PREHEX_BYTES == 32
        && guest_uefi_assert_prehex_gpa(GUEST_UEFI_IRON_ASSERT_CALLER_RIP) == 0x7FD2_5173
        && guest_uefi_assert_retcmp_gpa(0x7F8E_2946) == 0x7F8E_2906
        && guest_uefi_assert_retpre_word_gpa(0x7F8E_2946, 0x8B3) == 0x7F8E_31F2
        && {
            let mut b = [0u8; 24];
            let n = GUEST_UEFI_CPU_FLUSH_UNSUPPORTED.len();
            b[..n].copy_from_slice(GUEST_UEFI_CPU_FLUSH_UNSUPPORTED);
            guest_uefi_patch_cpu_flush_unsupported(&mut b[..n]) == 1
                && b[GUEST_UEFI_CPU_FLUSH_JNZ_OFF] == 0x90
                && b[GUEST_UEFI_CPU_FLUSH_JNZ_OFF + 1] == 0x90
        }
        && {
            let pat = GUEST_UEFI_CPU_FLUSH_UNSUPPORTED;
            let mut two = [0u8; 64];
            let n = pat.len();
            two[..n].copy_from_slice(pat);
            two[32..32 + n].copy_from_slice(pat);
            guest_uefi_count_cpu_flush_jnz(&two) == 2
                && guest_uefi_patch_cpu_flush_unsupported(&mut two) == 2
                && guest_uefi_count_cpu_flush_jnz(&two) == 0
        }
        && GUEST_UEFI_IRON_CPU_FLUSH_GPA == 0x7EE6_8FA0
        && GUEST_UEFI_IRON_ASSERT_CALLER_RIP == 0x7FD2_5193
        && GUEST_UEFI_IRON_HIGH_CR3 == 0x7FA0_1000
        && GUEST_UEFI_IRON_PDE8000_WB == 0x8000_0083
        && guest_uefi_pt_pde_is_wb_hole(GUEST_UEFI_IRON_PDE8000_WB)
        && !guest_uefi_pt_pde_is_wb_hole(guest_uefi_pt_pde_pat_uc(0x8000_0000))
        && guest_uefi_pt_pde_pat_uc(0x8000_0000) == 0x8000_0000 | GUEST_UEFI_PT_LARGE_2M_UC
        && guest_uefi_pt_pml4e_gpa(GUEST_UEFI_IRON_HIGH_CR3, 0) == GUEST_UEFI_IRON_HIGH_CR3
        && guest_uefi_pt_walk_pml4e(|_| 0x7FA0_2003, GUEST_UEFI_IRON_HIGH_CR3, 0) == 0x7FA0_2003
        && GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP == 0x7F8E_21CA
        && guest_uefi_report_ram_should_map(GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP)
        && guest_uefi_report_ram_page_off(GUEST_UEFI_IRON_HIGH_DEADLOOP_RIP) == 0xE21CA
        && {
            let mut page = [0u8; 0x20];
            page[0x10] = 0xEB;
            page[0x11] = 0xF3;
            let mut out = [0u8; 2];
            copy_report_ram_at(&page, 0x7BC0_0010, &mut out) == 2
                && out == [0xEB, 0xF3]
                && store_report_ram_at(&mut page, 0x7BC0_0010, 0xC3C9, 2) == 2
                && load_report_ram_at(&page, 0x7BC0_0010, 2) == Some(0xC3C9)
        }
        && {
            use core::cell::RefCell;
            let page = RefCell::new([0u8; 0x7000]);
            store_report_ram_u64(
                &mut *page.borrow_mut(),
                GUEST_UEFI_IRON_HIGH_CR3,
                0x7FA0_2023,
            ) && store_report_ram_u64(&mut *page.borrow_mut(), 0x7FA0_2010, 0x7FA0_5003)
                && store_report_ram_u64(&mut *page.borrow_mut(), 0x7FA0_2018, 0x7FA0_6023)
                && store_report_ram_u64(
                    &mut *page.borrow_mut(),
                    0x7FA0_5000,
                    GUEST_UEFI_IRON_PDE8000_WB,
                )
                && {
                    let peek = |gpa: u64| {
                        let p = page.borrow();
                        let off = guest_uefi_report_ram_page_off(gpa) as usize;
                        if off.saturating_add(8) > p.len() {
                            0
                        } else {
                            let mut le = [0u8; 8];
                            le.copy_from_slice(&p[off..off + 8]);
                            u64::from_le_bytes(le)
                        }
                    };
                    let poke = |gpa: u64, val: u64| {
                        store_report_ram_u64(&mut *page.borrow_mut(), gpa, val)
                    };
                    guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0x8000_0000)
                        == GUEST_UEFI_IRON_PDE8000_WB
                        && guest_uefi_pt_paint_live_uc_hole(
                            peek,
                            poke,
                            GUEST_UEFI_IRON_HIGH_CR3,
                        ) >= 1
                        && guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0x8000_0000)
                            == guest_uefi_pt_pde_pat_uc(0x8000_0000)
                }
        }
        && GUEST_UEFI_IRON_PDE0_2M == 0xE3
        && guest_uefi_pt_pde0_is_2m(GUEST_UEFI_IRON_PDE0_2M)
        && guest_uefi_gpa0_split_pt_gpa() == 0x20B000
        && {
            use core::cell::RefCell;
            let high = RefCell::new([0u8; 0x4000]);
            let pt = RefCell::new([0u8; 4096]);
            let pt_gpa = guest_uefi_gpa0_split_pt_gpa();
            store_report_ram_u64(
                &mut *high.borrow_mut(),
                GUEST_UEFI_IRON_HIGH_CR3,
                0x7FA0_2023,
            ) && store_report_ram_u64(&mut *high.borrow_mut(), 0x7FA0_2000, 0x7FA0_3023)
                && store_report_ram_u64(
                    &mut *high.borrow_mut(),
                    0x7FA0_3000,
                    GUEST_UEFI_IRON_PDE0_2M,
                )
                && {
                    let peek = |gpa: u64| {
                        if gpa >= pt_gpa && gpa < pt_gpa + 4096 {
                            let off = (gpa - pt_gpa) as usize;
                            let p = pt.borrow();
                            if off.saturating_add(8) > p.len() {
                                0
                            } else {
                                let mut le = [0u8; 8];
                                le.copy_from_slice(&p[off..off + 8]);
                                u64::from_le_bytes(le)
                            }
                        } else {
                            let off = guest_uefi_report_ram_page_off(gpa) as usize;
                            let p = high.borrow();
                            if off.saturating_add(8) > p.len() {
                                0
                            } else {
                                let mut le = [0u8; 8];
                                le.copy_from_slice(&p[off..off + 8]);
                                u64::from_le_bytes(le)
                            }
                        }
                    };
                    let poke = |gpa: u64, val: u64| {
                        if gpa >= pt_gpa && gpa < pt_gpa + 4096 {
                            let off = (gpa - pt_gpa) as usize;
                            let mut p = pt.borrow_mut();
                            if off.saturating_add(8) > p.len() {
                                false
                            } else {
                                let bytes = val.to_le_bytes();
                                p[off..off + 8].copy_from_slice(&bytes);
                                true
                            }
                        } else {
                            store_report_ram_u64(&mut *high.borrow_mut(), gpa, val)
                        }
                    };
                    guest_uefi_pt_walk_pde(peek, GUEST_UEFI_IRON_HIGH_CR3, 0)
                        == GUEST_UEFI_IRON_PDE0_2M
                        && guest_uefi_pt_split_gpa0(
                            peek,
                            poke,
                            GUEST_UEFI_IRON_HIGH_CR3,
                            pt_gpa,
                        ) >= 512
                        && guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0)
                            == GUEST_UEFI_PT_LEAF_4K
                        && guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0x10_0000)
                            == 0x10_0000 | GUEST_UEFI_PT_LEAF_4K
                        && guest_uefi_pt_walk_pte(peek, GUEST_UEFI_IRON_HIGH_CR3, 0xA_0000)
                            == guest_uefi_pt_leaf_4k_for(0xA_0000)
                        && guest_uefi_pt_leaf_4k_for(0xA_0000)
                            == 0xA_0000 | GUEST_UEFI_PT_LEAF_4K
                        && guest_uefi_pt_leaf_4k_for(0xC_0000)
                            == 0xC_0000 | GUEST_UEFI_PT_LEAF_4K
                        && guest_uefi_gpa_in_vga_fix_uc(0xA_0000) == false
                        && !guest_uefi_gpa_in_vga_fix_uc(0xC_0000)
                        && !guest_uefi_gpa_in_vga_fix_uc(0x10_0000)
                        && GUEST_UEFI_IRON_PTE_A0000_WB == 0xA_0067
                        && GUEST_UEFI_PT_LEAF_4K_UC == GUEST_UEFI_PT_LEAF_4K | crate::vmx::guest_uefi::GUEST_UEFI_PT_PWT | crate::vmx::guest_uefi::GUEST_UEFI_PT_PCD
                        && guest_uefi_pt_paint_vga_uc(peek, poke, GUEST_UEFI_IRON_HIGH_CR3) == 0
                }
        }
        && !guest_uefi_report_ram_should_map(0x1F0_0000)
        && !guest_uefi_report_ram_should_map(0x8000_0000)
        && e4_restore_xcr0_value(0, false, 0x7) == 1
        && e4_restore_xcr0_value(0x7, true, 0x7) == 0x7
        && e4_restore_cr4_osxsave(0x640, false) == 0x640
        && GUEST_UEFI_IRON_HOLE_X_RIP == 0x27E_22D5
        && GUEST_UEFI_IRON_ZERO_FILL_RIP == 0x3ED0_0001
        && guest_uefi_rip_is_hole_execute(GUEST_UEFI_IRON_HOLE_X_RIP)
        && !guest_uefi_rip_is_hole_execute(0x1DF1B7)
        && !guest_uefi_ept_hole_ro_allows_execute()
        && guest_uefi_ept_qual_is_fetch(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK)
        && guest_uefi_ept_hole_ro_on_qual(GUEST_UEFI_IRON_EPT_QUAL_FETCH_WALK)
        && !guest_uefi_ept_hole_ro_on_qual(4)
        && !guest_uefi_pf_should_map_mmio(0, GUEST_UEFI_IRON_HOLE_X_RIP)
        && !guest_uefi_pf_should_map_mmio(0, GUEST_UEFI_IRON_ZERO_FILL_RIP)
        && !guest_uefi_pf_should_map_mmio(0, 0x3EE0_0000)
        && GUEST_UEFI_IRON_PF_WP_SPLIT_PDE == 0x219067
        && !guest_uefi_pf_split4k_resume_already_rw()
        && guest_uefi_pde_is_large(0xC000_0083)
        && guest_uefi_pde_is_poison(GUEST_UEFI_IRON_PF_POISON_PDE)
        && !guest_uefi_pde_is_poison(0xC000_0083)
        && guest_uefi_pf_should_split_ram_1g(0x2, GUEST_UEFI_IRON_PF_HEAP_WR_CR2, 0xC000_0083)
        && !guest_uefi_pf_should_split_ram_1g(0x2, 0x8000_0008, 0xC040_0083)
        && guest_uefi_pci_hole_is_sink()
        && GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA == 0xC01D_F1B7
        && GUEST_UEFI_IRON_PF_HEAP_WR_CR2 == 0x1E9000
        && GUEST_UEFI_IRON_PF_POISON_CR2 == 0x1D1_E6CB
        && GUEST_UEFI_IRON_PF_POISON_PDE == 0xAFAF_AFAF_AFAF_AFAF
        && guest_uefi_pf_should_identity_map(0x2, GUEST_UEFI_IRON_PF_HEAP_WR_CR2)
        && guest_uefi_pf_should_map_mmio(0, GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA)
        && !guest_uefi_pf_should_map_mmio(1, GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA)
        && !guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_EPT_PCI_HOLE_GPA)
        && !guest_uefi_pf_should_identity_map(0xb, GUEST_UEFI_IRON_PF_MTRR_UC_CR2)
        && guest_uefi_pf_should_map_mmio(0xb, GUEST_UEFI_IRON_PF_MTRR_UC_CR2)
        && GUEST_UEFI_IRON_PF_MTRR_UC_CR2 == 0x8000_0008
        && guest_uefi_pf_should_map_mmio(0x2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2)
        && !guest_uefi_pf_should_identity_map(0x2, GUEST_UEFI_IRON_PF_SIGNEXT_CR2)
        && guest_uefi_pf_gpa32(GUEST_UEFI_IRON_PF_SIGNEXT_CR2) == 0x9680_8086
        && guest_uefi_pf_gpa32(GUEST_UEFI_IRON_PF_TRUNC32_CR2) == 0x9680_8086
        && guest_uefi_pf_should_map_mmio(0x2, GUEST_UEFI_IRON_PF_TRUNC32_CR2)
        && guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_MMIO_SCRATCH_GPA)
        && guest_uefi_mmio_needs_scratch(0x8000_0008)
        && guest_uefi_mmio_needs_scratch(GUEST_UEFI_IRON_SINK_PT_GPA)
        && guest_uefi_mmio_needs_scratch(0xC020_0000)
        && !guest_uefi_mmio_needs_scratch(0xFED0_0000)
        && !guest_uefi_mmio_needs_scratch(0xFEC0_0000)
        && guest_uefi_insn_is_poison_fill(0xAF, 0xAF, 0xAF, 0xAF)
        && guest_uefi_pf_should_identity_map(0, crate::vmx::guest_uefi::GUEST_UEFI_FLASH_BASE)
        && guest_uefi_pf_should_identity_map(0, 0xFFFF_0000)
        && guest_uefi_pf_error_is_reserved(0x9)
        && guest_uefi_pf_should_identity_map(0x9, GUEST_UEFI_IRON_PF_RSVD_CR2)
        && GUEST_UEFI_IRON_PF_RSVD_CR2 == 0xA027C8
        && guest_uefi_pf_sec_cr3() == GUEST_UEFI_HV_PML4
        && guest_uefi_pf_sec_cr3() != GUEST_UEFI_MEMFD_BASE
        && guest_uefi_pf_should_load_sec_cr3(0)
        && !guest_uefi_pf_should_load_sec_cr3(GUEST_UEFI_MEMFD_BASE)
        && guest_uefi_pf_should_rebuild_sec_cr3(GUEST_UEFI_HV_PML4)
        && guest_uefi_pf_should_rebuild_sec_cr3(GUEST_UEFI_MEMFD_BASE)
        && !guest_uefi_pf_should_rebuild_sec_cr3(0)
        && guest_uefi_pf_should_identity_map(0, GUEST_UEFI_IRON_PF_CR2)
        && GUEST_UEFI_IRON_PF_CR2 == GUEST_UEFI_MEMFD_BASE + 0xB000
        && !guest_uefi_pf_should_identity_map(1, GUEST_UEFI_IRON_PF_CR2)
        && guest_uefi_xapic_is_not_sink()
        && guest_uefi_is_mtrr_msr(0x250)
        && guest_uefi_mtrr_read(0xFE)
            == Some(crate::vmx::guest_uefi::GUEST_UEFI_MTRRCAP)
        && !guest_uefi_mtrr_pci_uc_hole()
        && !guest_uefi_mtrr_uc_hole_live()
        && guest_uefi_mtrr_poweron_disabled()
        && guest_uefi_mtrr_valid_var_pairs() == 0
        && guest_uefi_mtrr_fixed_is_vga_hole(0x259)
        && guest_uefi_mtrr_fixed_is_vga_hole(0x26F)
        && guest_uefi_mtrr_fixed_is_vga_hole(0x268)
        && !guest_uefi_mtrr_fixed_is_vga_hole(0x250)
        && !guest_uefi_mtrr_fixed_is_vga_hole(0x258)
        && guest_uefi_mtrr_write(0x259, 0x0606_0606_0606_0606)
        && guest_uefi_mtrr_read(0x259) == Some(0x0606_0606_0606_0606)
        && guest_uefi_mtrr_write(0x259, 0)
        && guest_uefi_mtrr_read(0x259) == Some(0x0606_0606_0606_0606)
        && guest_uefi_mtrr_write(0x268, 0x0606_0606_0606_0606)
        && guest_uefi_mtrr_read(0x268) == Some(0x0606_0606_0606_0606)
        && guest_uefi_mtrr_write(0x268, 0)
        && guest_uefi_mtrr_read(0x268) == Some(0x0606_0606_0606_0606)
        && guest_uefi_mtrr_write(0x250, 0x0606_0606_0606_0606)
        && guest_uefi_mtrr_read(0x250) == Some(0x0606_0606_0606_0606)
        && bootorder_nul_terminated()
        && e820_splits_mtrr_uc_hole()
        && e820_splits_vga_below_1m()
        && !e820_splits_gcd_mid_gap()
        && platform_reports_2g_lowmem()
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
        && {
            let r = guest_uefi_filter_cpuid_for_linux(1, 0);
            !guest_uefi_cpuid_has_hypervisor(r.ecx)
                && guest_uefi_cpuid_leaf1_is_uniprocessor(r.ebx, r.edx)
        }
        && {
            let k = guest_uefi_filter_cpuid_for_linux(GUEST_UEFI_KVM_CPUID_LEAF, 0);
            k.eax == 0 && k.ebx == 0 && k.ecx == 0 && k.edx == 0
        }
        && guest_uefi_cpuid_leaf_is_hypervisor_scan(0x4000_3d00)
        && guest_uefi_cpuid_leaf_is_hypervisor_scan(0x4000_bd00)
        && !guest_uefi_cpuid_leaf_is_hypervisor_scan(1)
        && GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST == 0x4000_ff00
        && guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_3d00, 0x4000_3d00)
            == u64::from(GUEST_UEFI_LINUX_HYPERVISOR_SCAN_LAST)
        && guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_3d00, 0x7) == 0x7
        && guest_uefi_linux_hypervisor_scan_bump_gpr(0x4000_0000, 0xffff_8880_4000_0000)
            == 0xffff_8880_4000_0000
        && {
            let ext = guest_uefi_filter_cpuid(0x8000_0001, 0);
            ext.edx & crate::vmx::guest_uefi::CPUID_80000001_EDX_NX == 0
                && ext.edx & crate::vmx::guest_uefi::CPUID_80000001_EDX_PAGE1GB == 0
        }
        && {
            let linux0 = guest_uefi_filter_cpuid_for_linux(0, 0);
            guest_uefi_cpuid_is_genuine_intel(linux0.ebx, linux0.edx, linux0.ecx)
        }
        && {
            let linux_ext = guest_uefi_filter_cpuid_for_linux(0x8000_0001, 0);
            linux_ext.edx & crate::vmx::guest_uefi::CPUID_80000001_EDX_NX != 0
                && linux_ext.edx & crate::vmx::guest_uefi::CPUID_80000001_EDX_PAGE1GB == 0
        }
        && guest_uefi_filter_cpuid(7, 0).ecx & crate::vmx::guest_uefi::CPUID_LEAF7_ECX_TME_EN == 0
        && guest_uefi_filter_cpuid(7, 0).ecx & crate::vmx::guest_uefi::CPUID_LEAF7_ECX_LA57 == 0
        && guest_uefi_filter_cpuid(7, 0).ebx & crate::vmx::guest_uefi::CPUID_LEAF7_EBX_CLFLUSHOPT == 0
        && guest_uefi_filter_cpuid(7, 0).ebx & crate::vmx::guest_uefi::CPUID_LEAF7_EBX_CLWB == 0
        && guest_uefi_filter_cpuid(4, crate::vmx::guest_uefi::GUEST_UEFI_CPUID_LEAF4_LAST_SUB).eax
            == 0
        && guest_uefi_efer_with_lma(
            crate::vmx::guest_uefi::GUEST_UEFI_EFER_LME
                | crate::vmx::guest_uefi::GUEST_UEFI_EFER_NXE,
            true,
        ) & crate::vmx::guest_uefi::GUEST_UEFI_EFER_NXE
            == 0
        && guest_uefi_efer_with_lma_allow_nx(
            crate::vmx::guest_uefi::GUEST_UEFI_EFER_LME
                | crate::vmx::guest_uefi::GUEST_UEFI_EFER_NXE,
            true,
            true,
        ) & crate::vmx::guest_uefi::GUEST_UEFI_EFER_NXE
            != 0
        && guest_uefi_phys_bits(46) == crate::vmx::guest_uefi::GUEST_UEFI_PHYS_BITS_IRON_CAP
        && guest_uefi_phys_bits(32) == 36
        && guest_uefi_phys_bits(40) == 40
        && guest_uefi_phys_bits(52) == crate::vmx::guest_uefi::GUEST_UEFI_PHYS_BITS_IRON_CAP
        && crate::vmx::guest_uefi::guest_uefi_hpet_uart_tsc_step(u64::MAX)
            == crate::devices::guest_platform::HPET_UART_IO_STEP_CAP
        && crate::vmx::guest_uefi::guest_uefi_hpet_uart_tsc_step(0) == 0
        && crate::vmx::guest_uefi::guest_uefi_hpet_step_for_exit(30, false, false) == 0
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("HPET TSC-delta on UART COM I/O")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Linux printk ticks every 4096")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART nowait (do not clear COM2_LIVE)")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("Linux CPUID GenuineIntel + NX")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX ring drain")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX ring drain 4/exit")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share TX ring")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon quiet ticks")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon hush HV")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share product ISO")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush on tick cadence even when share")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first CPUID")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first high-half")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon share first bootimg")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("guest UART TX drain COM2 independent")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon pace LSR THRE")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon skip #PF dump")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux earlycon skip exc deliver")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("poll ISO-INSTALL-OK every resume")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("256MiB disk leftover report-RAM")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("report-RAM EPT pre-map")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush skip leftover pre-map")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("cpu_flush leftover per walk")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux unhandled nowait stop")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO eax fallback")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("linux NMI inject")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio BAR trap over scratch")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("PIIX3 ISA BAR RAZ")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("packed virtio common cfg")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO raises PIT")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO off=")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO eax fallback size")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("packed virtio common cfg write")
        && E5_OVMF_VMLAUNCH_RESIDUAL_NOTE.contains("virtio MMIO polls lapic")
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
