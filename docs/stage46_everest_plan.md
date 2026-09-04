# Stage 46 Option 2 — Everest move plan

Living tracker for draft [PR #229](https://github.com/vikkp/RayNu/pull/229)
(`cursor/e5-stage46-iso-a623`). Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.
#231 tip stays `8024439` (parked). #229 stays draft.

---

## Where this plan was wrong (2026-09-01 note)

The first Option 2 note said Everest was waiting on a Cruzer flash of
`2d6b109` (fw_cfg `rep insb`) and that later #229 SHAs were docs only.

That is stale:

| Old claim | Lived |
|-----------|--------|
| Flash `2d6b109` | **Refused.** Dest skip at HV identity `0x200000`. Never F11 again. |
| Iron last reached `Freeing initrd` without `ACPI=` | That dump was the unflashed-`2d6b109` era. Later #229 pins print `fw_cfg dest_ok fill` and `product ISO fw_cfg ACPI MADT`. |
| Later #229 SHAs are docs only | **False.** dest_ok, HLT policy, COMMAND, EnableAttributes, last CF8, ROM write are code + iron COM2. |
| Fail at step 3 → one dest / `rep insb` fix | dest / MADT is **DONE**. Step 3 split: 3a MADT closed; 3b Linux `ACPI=` never reached because `ataio=0`. |
| Nested F11 `--run 33440050729` stays on the parked fork | That pin is **#229** `b5c3a9c` (skip-after-inject). Refused. Not a #231 flash. |
| Flashable EFI is `2d6b109` | **Refused.** Then CF8/ROM pins through `118edcf`. Also refused. |

What the first note still got right: Option 2 is the path. Park #231.
Everest E5 is iron `ISO-INSTALL-OK`, not stamp persist, not nested
ParseBar. HDA iso ~99% is scaffolding. This pod has no VMX / Cruzer.
Host/CI never prints the iron marker. One SHA per failed COM2 step.

---

## Pivot: RayNu-F (ADR-016)

The `3k–3o` chain (`e0d5c55` → `4e16b59`) all shared one defect: **it mutated
OVMF's own internal state to force progress** (force-return without `*Index`,
gState poke, event-`#PF` inject). Each forced step created the next hang;
`4e16b59` made OVMF's own handler `#PF` + `CpuDeadLoop`. That is a
self-inflicted-wound generator (we do not own OVMF's invariants).

**Decision (ADR-016): be the guest firmware ourselves — RayNu-F.** Present our
own `EFI_SYSTEM_TABLE` + boot services (BlockIo/SimpleFileSystem over
virtio-blk + CD, LoadImage/StartImage, serial ConIn/ConOut, memory services,
an owned timer tick) and boot the ISO's own `\EFI\BOOT\BOOTX64.EFI`. We author
every structure, so there is no third-party firmware state to corrupt, and
every wait completes because *we* signal it. The retained-OVMF VMLAUNCH stays
as a read-only diagnostic/fallback with forcing disabled
(`RAYNU_F_NO_FW_STATE_MUTATION`). Skeleton: [`raynu_f/`](../raynu_f/mod.rs)
(`RAYNU-V-RAYNU-F-SCAFFOLD-OK`, not `ISO-INSTALL-OK`). Fast harness is
nested/QEMU (already reaches ATAPI-OK); reserve iron flashes for
iron-specific confirmation.

### RayNu-F ladder (tick only on evidence; details in ADR-016)

| # | Status | Proof |
|---|--------|-------|
| F0 | **DONE** | `RAYNU-V-RAYNU-F-SCAFFOLD-OK` (host) |
| F1 | **DONE (host)** | `RAYNU-V-RAYNU-F-TABLES-OK`: byte-exact UEFI 2.10 tables + valid CRC32s + trampolines round-trip + `ConOut.OutputString` lands on the sink; service port `0x5246` wired into `emulate_io_port`. No guest has run it. |
| F2a | **DONE (host)** | `RAYNU-V-RAYNU-F-LOADER-OK`: PE32+ loader with `DIR64` relocs; RayNu-F test app as a genuine PE32+ round-trips (relocated pointer verified at `0x900000`); F2 launch plan consistent (`RSP%16==8`, long-mode CRs/EFER). Nothing launched. |
| F2b | **DONE (nested VT-x, raynuvsrv1)** | EFI from run `33826787787` (`c975ade`): `RayNu-F VMLAUNCH entry=0x901000 system_table=0x801000 relocs=1` → `RN-F ConOut via RayNu-F tables` → **`RAYNU-V-RAYNU-F-CONOUT-OK`** → `guest HLT rip=0x901019` → `stop hlt exits=2 svc=1 conout_ok=1`. First guest execution of our own tables; two exits, nothing unexpected. GitHub runners `33826341793` / `33826787787` / `33827…` lacked nested VT-x (`VMXON-SKIP`), so the nested proof came from the server. Not iron. Optional iron confirmation: `flashcruzer.sh --no-linux-iso --raynu-f`. |
| F3 | **DONE (nested VT-x, raynuvsrv1 2026-09-04)** — `TIMER-OK` + `MEM-OK` + `CONOUT-OK` + `HLT path=OK`, `exits=7 svc=6 svc_err=0`. Host gate `RAYNU-V-RAYNU-F-SERVICES-OK`. | Memory services (pages/pool/`GetMemoryMap`/`ExitBootServices`) + **owned timer** on a TSC-calibrated host-side firmware clock (no guest IDT/PIT/LAPIC in the firmware phase) + TPL/`Stall`/`CopyMem`/`SetMem`/`CalculateCrc32` + real `ConIn.WaitForKey`. Test app v2 walks `Stall→CreateEvent→SetTimer→WaitForEvent→AllocatePages→OutputString` with OK/FAIL `hlt` addresses. Closes on nested `RAYNU-F-TIMER-OK` + `RAYNU-F-MEM-OK` + `HLT path=OK`. |
| F4 | **HOST-PROVEN** (`RAYNU-V-RAYNU-F-BLOCKIO-GATE-OK`) | Handle/protocol database (real UEFI GUIDs; `HandleProtocol`/`OpenProtocol`/`LocateHandle`/`LocateHandleBuffer`/`LocateProtocol`/`InstallProtocolInterface`) + two `EFI_BLOCK_IO_PROTOCOL` instances: CD (2048 B, read-only, retained ISO) and install disk (512 B, rw, virtio-blk). Spec §13.9 validation; host test reads a real ISO9660 PVD at LBA 16 and round-trips an `EFI PART` disk write. Closes on nested `RAYNU-F-BLOCKIO-OK`. |
| F5 | **HOST-PROVEN** (`RAYNU-F-FS-GATE-OK` + `RAYNU-F-IMAGE-GATE-OK`) | FAT12/16/32 reader; `SimpleFileSystem`/`EFI_FILE_PROTOCOL` exposed to guests; `LoadImage` over `GuestMem` with DIR64 relocs + `LoadedImage`; `StartImage` guest redirect. **The launcher now boots the ISO's real `\EFI\BOOT\BOOTX64.EFI`** (El Torito → FAT ESP → stage → load), test app as fallback. First nested run (`05655c5`) hit `El Torito image is not FAT lba=56`: `parse_catalog` took the BIOS default entry (isolinux, LBA 56) and never walked to the `0xEF` section (FAT12 ESP, LBA 77, 2880 sectors). Fixed to prefer the EFI section; proven on host against the real Alpine ISO (`RAYNU-V-RAYNU-F-REAL-ISO-PATH-OK`: GRUB 724 992 B, PE32+, 1851 DIR64 relocs, both loaders byte-identical). **Nested `7ee3a3b` (2026-09-04): GRUB ran on RayNu-F.** `FAT ESP mounted lba=77 efi=1` → `found \EFI\BOOT\BOOTX64.EFI bytes=724992` → `staged base=0xbb1000 entry=0xbb2000 relocs=1851` → `VMLAUNCH … image=ISO-BOOTX64` → GRUB printed its own `Could not malloc` / `Aborted. Press any key to exit.` through **our** `ConOut` (17 `OutputString` calls) and polled **our** `ReadKeyStroke` to the exit cap (`exits=1048577 svc=1048576`). F5c launch is proven; guest-side `FS-OK` still awaits GRUB getting past `mm_init`. |
| F6-prep | **NESTED-PROVEN** (`dd32bda`, run `33872241367`): `high RAM base=0x2000000 bytes=268435456 premapped=2078277632` → `OpenProtocol(LoadedImage)` OK → GRUB `mm_init` completes (`AllocatePages`/`AllocatePool`/`GetMemoryMap`/`FreePages`/`FreePool` all `0x0`) → `GetVariable` honest `UNSUPPORTED` (tolerated) → `SetWatchdogTimer` → `LocateHandle(BlockIo)` → per-handle `OpenProtocol` → stopped on **unhandled CPUID exit** at `rip=0xbb8ef7` (`grub_tsc_init` leaf 1), `exits=16 svc=15 svc_err=0`. CPUID now handled on the RayNu-F path with the firmware-phase filter. **Nested `2d34fff`: `BLOCKIO-OK`, console up, GRUB ran the Alpine menu entry and loaded kernel+initrd; its last call before the kernel, `InstallMultipleProtocolInterfaces` (initrd `LoadFile2`+`DevicePath`), hit our `EFI_UNSUPPORTED` → "Press any key to continue...".** F6-prep b (host-proven, rerun next): `InstallMultiple`/`UninstallMultiple`/`LocateDevicePath`; key-poll `NOT_READY` no longer counted as an error; stop line reports `blk_rd/blk_wr/allocs/free_pages`. **Nested `166377a`: GRUB `LoadImage`+`StartImage`d the Linux kernel (`START-IMAGE-OK`, our PE loader relocated the kernel to `0x2b12d4d`); Linux EFI stub ran on our tables and loaded the initrd via `LocateDevicePath` + GRUB's `LoadFile2` (`EFI stub: Loaded initrd from LINUX_EFI_INITRD_MEDIA_GUID device path`); stopped on `InstallConfigurationTable` → `UNSUPPORTED` then `Exit` → `UNSUPPORTED` → `#GP`.** F6-prep c (host-proven, rerun next): `EFI_CONFIGURATION_TABLE` array (16 entries) behind the system table with add/replace/remove + header re-CRC; `Exit` unwinds to the `StartImage` caller. Then: stub → `EBS-OK` → **F6a** ACPI via `ConfigurationTable` and **F6b** post-EBS hand-off to the Stage-46 Linux exit path. Tolerated `NOT_FOUND`s GRUB named: `SimpleTextInputEx` on the console handle; `DevicePath` on the install-disk handle (only the CD has one — GRUB skips that disk; publish a disk path later). Next expected: GRUB reads `grub.cfg` through **BlockIo** (its own ISO9660 reader) → `RAYNU-F-BLOCKIO-OK`, not SFS. Original faults GRUB named were ours: (1) `OpenProtocol(ImageHandle, LoadedImage)` → `NOT_FOUND` ×2 — the launcher staged GRUB directly and never published `LoadedImage`; now `publish_loaded_image()` on the F5 stage path. (2) `AllocatePool` → `OUT_OF_RESOURCES` + `AllocatePages(AllocateAddress)` → `NOT_FOUND`: GRUB `mm_init` wants a 32 MiB heap and `AllocateAddress`-walks every conventional descriptor; the pool was 20 MiB and the map advertised unmanaged slab slack as conventional. `PagePool` now spans the slab pool + a high region over the report-RAM already EPT pre-mapped contiguously from 32 MiB (pre-mapped slots only; cap 256 MiB); the map advertises as conventional exactly what `AllocateAddress` can serve. Failed service log lines now carry `a1..a4` + GUID. Rerun: `gh run download <run> -n r640-hypervisor.efi -D target/x86_64-unknown-uefi/release && SKIP_BUILD=1 RAYNU_F=1 REQUIRE_VMX=1 QEMU_ACCEL=kvm PRODUCT_ISO=… ./tools/qemu-boot-test.sh`; expect `high RAM base=0x2000000 bytes=…`, no `OpenProtocol … 0e` at the top, and GRUB past `mm_init`. |
| F6 | BLOCKED | iron `RAYNU-V-M7-ISO-INSTALL-OK` |

## Stop rules

- **Do not mutate third-party firmware internal state** (force returns, skip its
  checks, poke gState, inject faults to steer flow). That is the `3k–3o` wound
  (ADR-016). Own the firmware (RayNu-F) and signal the wait instead.
- Do not F11 `4e16b59` / `--run 33820727776` (event `#PF` inject → OVMF own `#PF` + `CpuDeadLoop`; still `ataio=0`).
- Do not F11 `9474ab6` / `--run 33817483733` (state4 poke `dest=0x7ff18340` then `#PF cr2=0xffffffffffffffb8` `rip=0x7ff0e018`; HV `#PF MMIO skip` stopped; still `ataio=0`).
- Do not F11 `d0e44d4` / `--run 33815993163` (WFE skip `len=12 rip=0x7ff0e7e8` then same spin; skip lands on `mov rax,3`; still `ataio=0`).
- Do not F11 `c8d504d` / `--run 33757018875` (ZeroMem ept fill never printed; `0x34` is preempt not EPT; noskip `endbr64+cmp [rip],4`; still `ataio=0`).
- Do not F11 `e0d5c55` / `--run 33753069821` (WFE return `caller=0x7feffe28` then preempt `0x34` `rip=0x7ec8f6ff`, still `ataio=0`).
- Do not F11 `0b770cd` / `--run 33701350767` (`rethx=0xe056ff41b84d8b48`, still `ataio=0`).
- Do not F11 `6c4bfde` / `--run 33699177232` (ConIn CR fired, still `ataio=0`).
- Do not F11 `2d4ab51` / `--run 33697154185` (HLT `ret=0x7ff0e055` DxeCore Wait, still `ataio=0`).
- Do not F11 `27eda8c` / `--run 33695570769` (hide-slot0, CDROM-OK via PIIX, still `ataio=0`).
- Do not F11 `118edcf` / `--run 33630723649` (`romwr=0xfffffffe` size probe still `ataio=0`).
- Do not F11 `7ba1ccf` / `--run 33627470674` (`cf8ide=0x80000930` PIIX ROM BAR still `ataio=0`).
- Do not F11 `5de9e1c` / `--run 33575888121` (`cf8en=0x80004008` host class still `ataio=0`).
- Do not F11 `61991be` / `--run 33573126367` (`cf8=0x0` still `ataio=0`).
- Do not F11 `c144001` / `--run 33571164257` (`pcicmd=0x5` still `ataio=0`).
- Do not F11 `060c504` / `--run 33569757025` (`seq=0,0,0,0,0,0`).
- Do not F11 `abba969` / `--run 33567464001` (honor `pcicmd=0` still `ataio=0`).
- Do not F11 `184ee61` / `--run 33562028442` (`OR 0x0001` hid disable).
- Do not F11 `21dc562` / `--run 33559849096` (skip-HLT then same CpuSleep).
- Do not F11 `e3cbfa5` / `--run 33558261624` (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip), `3b1cf51` (docs only), or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Not another HLT policy.
- Do not OR PCI command `0x0001`. COMMAND path is closed.
- Do not resume #231 for further `COMMAND.IO`.
- Do not print another CF8/ROM field. That ladder is closed.
- Do not hide PIIX. Do not revert hide-slot0 (it is correct, not sufficient).
- Do not start another ZeroMem EPT RIP. `0x34` is preempt, not EPT.
- Do not skip RIP on `endbr64+cmp [rip],4`. That skip is `mov rax,3`.
- Do not poke state=4 again. Do not skip the `evnt` signature cmp.

---

## Ladder (tick only the proof column)

| # | Status | Do | Proof |
|---|--------|----|-------|
| 0 | **DONE** | Park #231. Path is #229. | ADR-015 |
| 1 | **DONE** | Green CI on the live pin | 49/49 on `0b770cd` (`--run 33701350767`) — **refused after COM2** |
| 2 | **DONE** (many pins) | Flash Cruzer from clone, `--no-git --run <id>` | `FLASH-OK` on `0b770cd` / `33701350767` (EFI `b17314b7`). Never PERC. Never `8024439`. |
| 3a | **DONE** | COM2: fw_cfg + ACPI tables | `dest_ok fill dest=0x81ec98` **and** `product ISO fw_cfg ACPI MADT` (held through `0b770cd`) |
| 3b | **FAIL** | COM2: firmware starts ATA / Linux sees ACPI | Need `ataio>0` then Linux `efi:` contains `ACPI=`. Last COM2 (`0b770cd`): `rethx=` then CpuSleep `ataio=0`. Never reached Linux. |
| 3c | **FAIL** | COM2 of `61991be` last CF8 | HLT `cf8=0x0` — firmware wrote CONFIG_ADDRESS 0 after the PCI walk. Still `ataio=0`. |
| 3d | **FAIL** | COM2 of `5de9e1c` last enabled CF8 | HLT `cf8en=0x80004008` = i440FX host `00:08.0+08` (class). Not an IDE BDF. Still `ataio=0`. |
| 3e | **FAIL** | COM2 of `7ba1ccf` last IDE CF8 | HLT `cf8ide=0x80000930` = PIIX `00:01.1+30` (Expansion ROM). Still `ataio=0`. |
| 3f | **FAIL** | COM2 of `118edcf` last IDE ROM write | HLT `romwr=0xfffffffe` = standard ROM size probe (enable bit clear). No ghost ROM. Still `ataio=0`. |
| 3g | **FAIL** | COM2 of `27eda8c` hide slot-0 | Hide printed. CDROM-OK on `00:01.1`. `cmdwr=3`. Same hang `ataio=0`. BAR conflict is not the wall. |
| 3h | **FAIL** | COM2 of `2d4ab51` HLT retaddr | HLT `ret=0x7ff0e055` = DxeCore WaitForEvent (AcpiTimer `0x7ff635d2`), not PciBus. Still `ataio=0`. |
| 3i | **FAIL** | COM2 of `6c4bfde` ConIn CR | CR printed. Same hang `ret=0x7ff0e055` `ataio=0`. Timer wait, not serial. |
| 3j | **FAIL** | COM2 of `0b770cd` callsite | `rethx=0xe056ff41b84d8b48` = `call [r14-0x20]` CpuSleep. Still `ataio=0`. |
| 3k | **FAIL** | COM2 of `e0d5c55` WFE return | Poke **hit** (`caller=0x7feffe28`). Hang moved to preempt `0x34` `rip=0x7ec8f6ff` (misread as EPT). Still `ataio=0`. |
| 3l | **FAIL** | COM2 of `c8d504d` ZeroMem ept fill | Fill **never printed**. Last ticks are preempt `0x34` (`preempt noskip` `endbr64+cmp [rip],4` then `rip=0x7ec8f639`). Still `ataio=0`. |
| 3m | **FAIL** | COM2 of `d0e44d4` WFE preempt skip | Skip **hit** (`len=12 rip=0x7ff0e7e8`). Same insn re-entered. Skip-12 is `mov rax,3` (not-ready). Still `ataio=0`. |
| 3n | **FAIL** | COM2 of `9474ab6` state4 poke | Poke **hit** (`dest=0x7ff18340`). Then `#PF cr2=0xffffffffffffffb8` `rip=0x7ff0e018` (`cmp [r14-0x48], 'evnt'`). HV `#PF MMIO skip` stopped. Still `ataio=0`. |
| 3o | **FAIL (pivot)** | COM2 of `4e16b59` event `#PF` inject | Inject **fired**: OVMF's own handler dumped a real `#PF cr2=0xffffffffffffffb8 rip=0x7ff0e018` then `CpuDeadLoop`. Still `ataio=0`. This confirmed the `3k–3o` chain was self-inflicted corruption (forcing OVMF internal state). **Pivot: RayNu-F (ADR-016).** Forcing disabled. |
| 4 | BLOCKED | Linux stays up | `Linux version` then `/init` or `~#` |
| 5 | BLOCKED | `setup-disk` sees the disk | `/dev/vda`, not `No disks available`. Fail here → virtio, not #231. |
| 6 | OPEN | Installer writes GPT | COM2 `RAYNU-V-M7-ISO-INSTALL-OK` |

---

## Closed experiments (do not repeat)

| Pin | What we learned |
|-----|-----------------|
| `2d6b109` | dest skip. Not the ACPI EFI. |
| `b5c3a9c` | dest_ok + MADT, then skip-after-inject CpuSleep `ataio=0`. |
| `24c5fa6` / `e3cbfa5` / `21dc562` | HLT policy exhausted (wait-for-irq, one-shot, skip-after-oneshot). |
| `184ee61` | `OR 0x0001` left `pcicmd=0x1`. |
| `abba969` | Honor COMMAND: `pcicmd=0`. |
| `060c504` | Six COMMAND writes, all `0`. |
| `c144001` | EnableAttributes `pcicmd=0x5`. Still `ataio=0`. **COMMAND closed.** |
| `61991be` | HLT `cf8=0x0`. Firmware deselected CONFIG_ADDRESS after the walk. |
| `5de9e1c` | HLT `cf8en=0x80004008`. Last live BDF is host `00:08.0+08`, not IDE. Still `ataio=0`. |
| `7ba1ccf` | HLT `cf8ide=0x80000930`. Last IDE register is PIIX Expansion ROM. Still `ataio=0`. |
| `118edcf` | HLT `romwr=0xfffffffe`. Standard size probe, enable bit clear, RAZ/WI. **CF8/ROM closed.** |
| `27eda8c` | Hide slot-0. CDROM-OK via PIIX. Same CpuSleep. **BAR conflict closed.** |
| `2d4ab51` | HLT `ret=0x7ff0e055`. DxeCore WaitForEvent, not PciBus. Still `ataio=0`. **Caller named.** |
| `6c4bfde` | ConIn CR fired. Same hang. **Serial ConIn wait closed.** |
| `0b770cd` | `rethx=0xe056ff41b84d8b48` = `call [r14-0x20]` CpuSleep. Event never signals. **Callsite closed.** |
| `e0d5c55` | WFE return `caller=0x7feffe28`. Hang moved. Preempt `0x34` `rip=0x7ec8f6ff` `insn=31c031ff8903` (misread as EPT). Still `ataio=0`. **WFE unwind closed.** |
| `c8d504d` | ZeroMem ept fill never printed (`0x34` is preempt, not EPT `0x30`). `preempt noskip` `endbr64+cmp [rip],4` then `rip=0x7ec8f639`. Still `ataio=0`. **ZeroMem EPT fill closed as diagnosis.** |
| `d0e44d4` | WFE skip `len=12 rip=0x7ff0e7e8`. Same entry re-entered (`spin jmp skip`). Skip-12 lands on `mov rax,3`. Still `ataio=0`. **RIP skip of this cmp closed.** |
| `9474ab6` | State4 poke `dest=0x7ff18340`. Then `#PF cr2=0xffffffffffffffb8` `rip=0x7ff0e018` (`cmp [r14-0x48], 'evnt'`). HV `#PF MMIO skip — RIP left 32MiB RAM` then stop. Still `ataio=0`. **State4 poke closed.** |
| `4e16b59` | Event `#PF` inject fired: OVMF's own handler dumped a real `#PF cr2=0xffffffffffffffb8 rip=0x7ff0e018` then `CpuDeadLoop`. Still `ataio=0`. Confirmed `3k–3o` was self-inflicted (forcing OVMF internal state). **Whole forcing family closed — pivot to RayNu-F (ADR-016).** |

HLT policy is exhausted. COMMAND is closed. CF8/ROM diagnostics are
closed (`romwr=0xfffffffe`). Nested `cmdwr=0` / ParseBar is not the
iron picture. Further #231 IdeBus PCI slices cannot close E5.
Do not hide PIIX (`ea30da1`). Do not start another dest / HLT / COMMAND SHA.

### Gauge (are we on the right track?)

| Question | Answer |
|----------|--------|
| Diagnosis track | **Yes.** Each pin killed one hypothesis. WFE poke hit; hang named. |
| Product track (`ataio>0`) | **Stalled.** ~16 iron pins, `ataio` still 0. Informed ≠ closer to Linux. |
| Right next move | Deliver the named post-WFE+state4 `#PF` to firmware. Do not Stop. Do not skip the `evnt` cmp. |
| Wrong next moves | dest, HLT policy, COMMAND, CF8/ROM, hide-slot0/PIIX, ConIn, WFE unwind, another ZeroMem EPT RIP, another RIP skip of `cmp [rip],4`, another state=4 poke, #231 |

---

## Current wall

Firmware enumerates IDE (CDROM-OK, BOTH-OK), dest_ok + ACPI MADT land,
COMMAND on the wire is IO+BM (`pcicmd=0x5`). Firmware finishes the PCI
walk, last IDE poke is a ROM size probe (`romwr=0xfffffffe`), last
enabled CF8 is i440FX host `00:08.0` offset `0x08`, then writes
CONFIG_ADDRESS 0 and CpuSleep. `ataio=0` `pin14=0` `cmd=0x00`
(last ATA 0x1F7, not PCI COMMAND). ATA I/O is not gated on `pci_cmd`.
`00:00.1` and `00:01.1` share the same I/O BARs.

Hide-slot0 is kept. ConIn CR is kept (fired, not sufficient). Callsite
is `call [r14-0x20]` into CpuSleep (`rethx=0xe056ff41b84d8b48`).
`e0d5c55` unwound WaitForEvent as `EFI_SUCCESS` (`caller=0x7feffe28`).
`d0e44d4` skipped `endbr64+cmp [rip],4` at `rip=0x7ff0e7e8` (`len=12`).
That skip lands on `mov rax, 3` (not-ready); the dispatcher called the
same function again (`spin jmp skip` of the same insn). Still
`ataio=0`. `9474ab6` wrote `4` at the RIP-relative dest
(`dest=0x7ff18340`). Then `#PF cr2=0xffffffffffffffb8`
`rip=0x7ff0e018` (`cmp [r14-0x48], 'evnt'` with r14=0). HV
`#PF MMIO skip — RIP left 32MiB RAM` stopped the guest (DxeCore
RIP is report-RAM, not the 32 MiB hole). Still `ataio=0`. This
#229 HEAD delivers that `#PF` to firmware. Wait CI, then flash.
Do not F11 `9474ab6`. Do not F11 `d0e44d4`. Do not F11 `c8d504d`.
Do not F11 `e0d5c55`. Do not flash `3b1cf51`.

`4e16b59` delivered that `#PF` to firmware: OVMF's own handler dumped a
real `#PF` then `CpuDeadLoop`. That closed the whole forcing family —
each forced step corrupted the next. **We pivoted to RayNu-F (ADR-016).**
Do not F11 `4e16b59`, `9474ab6`, `d0e44d4`, `c8d504d`, `e0d5c55`.

Next proof is **not** another OVMF poke. It is RayNu-F standing up its
own boot services (system table → serial console → memory services →
owned timer tick → BlockIo/SimpleFileSystem over virtio-blk+CD →
LoadImage/StartImage of the ISO's `\EFI\BOOT\BOOTX64.EFI`), proven on
nested/QEMU first, then iron. Still not `ISO-INSTALL-OK`. TLS and guest
console stay residual; they are not this ladder.

---

## How we stay honest

- Tick only the proof column. Latitude / QEMU / nested ≠ R640 COM2.
- Fail at 3b → one SHA that is not dest, not HLT policy, not COMMAND.IO,
  not CF8/ROM print, not hide-slot0, not hide-PIIX, not retaddr print,
  not ConIn CR, not callsite print, not another WFE unwind, not another
  ZeroMem EPT RIP, not another RIP skip of `cmp [rip],4`, not another
  state=4 poke, not #231.
- Fail at 5 → virtio, not #231.
- Host/CI never prints `RAYNU-V-M7-ISO-INSTALL-OK`.
- Cruzer only: `0781:5151` / `RAYNUV` / `/dev/sdc`. Flash from
  `~/projects/raynu` with `--no-git --run <id>`. `artifact commit=` is
  leftover #231 HEAD — ignore it.

HDA: months_to_everest 0.5 held · overall 95% · ETA 2026-09.
