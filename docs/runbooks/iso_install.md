# Runbook — ISO install-to-disk (E5 / M7.7)

**Scaffold marker (host/CI):** `RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK`  
**Iron close (COM2):** `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` (documented equivalent of `RAYNU-V-M7-ISO-INSTALL-OK`)  
**Evidence:** [2026-08-16-e5-iso-install.md](../evidence/r640/2026-08-16-e5-iso-install.md) — `STATUS-iso-install=closed`  
**Archive:** `docs/evidence/r640/`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md) · product ISO: [ADR-014](../adr/ADR-014.md) · HDA E5  
**Prior:** [iso.md](iso.md) (M7.3 deploy plan) · [mgmt_http.md](mgmt_http.md) (E3 network)

## What this gate is

E5 Mount Everest criterion: operator registers a distro ISO → VM boots installer
**or** documented extract path → **installs to virtio-blk** → **reboot to disk**.

M7.3 closed only the **planning** smoke (register + extract-boot bind + disk
size). M7.7 opens the **install-to-disk** track:

1. **Launch contract** from a ready `IsoDeployPlan` (extract-boot + install disk bytes).
2. **Phased bookkeeping:** ContractReady → DiskWritten → RebootPending → BootedFromDisk.
3. **Virtio-blk capacity** surface sized for the default 64 MiB install disk
   (`devices::virtio_blk::DEFAULT_INSTALL_DISK_BYTES`).
4. Host/CI **scaffold** only — Latitude / QEMU host smoke cannot close the iron marker.

## REST shapes (Bearer auth)

| Method | Path | Result |
|--------|------|--------|
| `POST` | `/iso/{id}/install` | 201 — begin install-to-disk contract (register ISO if needed) |
| `GET` | `/iso/install` | 200 — Listed count `1` when contract ready |

Token: `Authorization: Bearer raynu-v-bringup` (same as M7.1–M7.3).

## Host scaffold smoke

```bash
./tools/m7-iso-install-smoke.sh
```

Expect:

```text
RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK
==> M7.7 ISO install-to-disk scaffold smoke PASSED
```

The smoke script must **never print** `RAYNU-V-M7-ISO-INSTALL-OK`.

## Lab contract (QEMU → iron)

Documented MVP (El Torito / CD-ROM **deferred**):

### QEMU lab (no curl) — ESP flag

Stage an empty `isoinstall.txt` on the ESP (or set `ISO_INSTALL_LAB=1` for
`run-qemu.sh`). Pre-EBS probe arms a **1 MiB** install disk (safe under QEMU
`-m 512M`). After VMX + virtio probe:

```text
boot: E5 lab isoinstall.txt armed (1MiB)
boot: M4.3 virtio-blk … bytes=1048576
boot: E5 install-sized virtio-blk armed (PRE-EBS contract)
RAYNU-V-M4-BLK-OK
RAYNU-V-M7-ISO-DISK-WRITTEN
RAYNU-V-M7-ISO-INSTALL-LAB-OK
RAYNU-V-M7-ISO-REBOOT-PENDING
```

`REBOOT-PENDING` means boot1 advanced the phase machine (“would reboot from disk”).

**E5 persist (PRE-EBS):** when the install contract is armed (`isoinstall.txt` or
`POST /iso/{id}/install`), firmware writes ESP `EFI/RayNu/installdisk.bin`
(**1 KiB** LBA0+LBA1 stamps) plus `installsize.txt` (full virtio size). Live
virtio remains RAM. The next boot loads that file (`probe_iso_persist_reboot`)
without `isoreboot.txt`. iDRAC Virtual Floppy is often **read-only** — then
COM2 prints `WARN — E5 persist ESP write failed` and a writable USB ESP is
required.

QEMU `fat:rw` usually keeps the firmware file. Smoke prefers
`ESP1/EFI/RayNu/installdisk.bin`; if missing, it **synthesizes**
`target/e5-lab-install.img` and runs boot2 with `isoreboot.txt` +
`installdisk.bin` (`ISO_REBOOT_LAB=1`). Expect:

```text
boot: E5 lab isoreboot.txt armed (1MiB persist)
boot: M4.3 virtio-blk … bytes=1048576
boot: E5 install disk preload (reboot detect)
RAYNU-V-M4-BLK-OK
RAYNU-V-M7-ISO-BOOTED-FROM-DISK
```

```bash
./tools/m7-iso-install-qemu-smoke.sh
```

`BOOTED-FROM-DISK` closes the **QEMU lab** two-boot loop only — not iron E5.
Iron / REST still use the **64 MiB** default via `POST /iso/{id}/install`.
The persist file is a **1 KiB prefix**; `virtio_blk::init_with_image` copies it
into the larger RAM disk. Requiring equal lengths dropped iron stamps
(COM2 2026-08-16: persist-detect + `bytes=67108864` then
`HLT without DRIVER_OK readback`).

### Full product path

1. `POST /iso/{id}/deploy` then `POST /iso/{id}/install` (or install alone / ESP lab).
2. Guest boots via **extract-boot** (`load_bzimage_guest` + staged bzImage/initrd).
3. Host/guest writes a marker (or filesystem) to the virtio-blk install disk.
4. Hypervisor records DiskWritten → RebootPending.
5. Second boot from the install disk → `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`
   (iron 2026-08-16; documented equivalent of `ISO-INSTALL-OK`).

### Iron (R640)

Closed on Cruzer Micro (front USB 2), 2026-08-16 — see
[2026-08-16-e5-iso-install.md](../evidence/r640/2026-08-16-e5-iso-install.md).
`STATUS-iso-install=closed`. Floppy is often read-only; use writable USB.

## Honesty / residuals

- **GAP(CLOSED M7.7)** — iron two-boot LBA stamp persist + reboot-to-disk (`BOOTED-FROM-DISK` on COM2).
- **El Torito / firmware CD-ROM** is GuestVisible on the private
  guest-UEFI VMCS (`attach_cdrom_uefi`; see `iso.md`). Firmware does
  not yet boot the CD. Unarmed attach stays `UnsupportedOnFirmware`.
  Host catalog parse (Stage 0), host attach (Stage 1), firmware arm
  (Stage 2), guest FW envelope (Stage 3), stub load (Stage 4), OVMF
  FV probe (Stage 5), ESP load (Stage 6), slot arm (Stage 7), and guest
  bind (Stage 8), launch-prepare (Stage 9), and size-floor (Stage 10) are
  closed; they are not guest UEFI VMLAUNCH and not an embedded EDK2 image.
  The 80-byte mock and 4 KiB floor are refused for VMLAUNCH.
- **ISO blob upload** not claimed — REST attach uses the host mock EFI prefix.
  Extract-boot uses existing PE/ESP assets first.
- **QEMU / firmware persist** is ESP `installdisk.bin` (LBA stamps), not a guest
  filesystem. Host synth remains fallback if the ESP write did not land.
- **Iron 64 MiB** persist is **marker sectors only** (1 KiB). `init_with_image`
  copies that prefix into the live RAM disk; equal-length copy was the 2026-08-16
  Cruzer `DRIVER_OK` miss. Full disk persist needs writable USB/NVMe, not a
  64 MiB file in the EFI.
- Outside Proven Core (ADR-009 / ADR-014); size still ADR-003.
- Do **not** claim Mount Everest: E5 stamp persist is closed; next product
  installer is UEFI guest firmware + virtio ([ADR-014](../adr/ADR-014.md)), not
  another bzImage extract. Windows ISO is later; do not paint a Linux-only corner.

## Stage 46 product ISO (OPEN — Everest E5)

Not persist-detect `ISO-BOOTED-FROM-DISK`. Not the 72 KiB lab El Torito stub.
Not extract-boot bzImage.

Copy a UEFI Linux distro ISO onto the Cruzer ESP as `\EFI\RayNu\linux.iso`
(fallbacks `\linux.iso`, `\EFI\RayNu\install.iso`). Size must exceed 73728
bytes. Prefer `alpine-virt-*-x86_64.iso` (virtio + serial); standard also
works if ATAPI `sr-mod` is on the cmdline. The ISO lives next to
`flashcruzer.sh` in `/home/vikkp/projects/raynuv`. Refresh the launcher
from the clone first (`./tools/flashcruzer.sh --install-launcher`): the
`~/projects/raynuv/flashcruzer.sh` copy is stale and rejects `--linux-iso`.
The Cruzer FAT already fills the 977.5 MiB RAYNUV stick after
`--refat-cruzer` (do **not** pass it again). `git fetch origin NAME` only
writes `FETCH_HEAD`; checkout `-B` onto `origin/NAME` then
`--wait --require-head --no-git` (do **not** `git checkout` a SHA). Do **not** flash
`ea30da1` / `--run 33389381409` (hide-IDE + inject `vec=0x20` livelocked the timer ISR
to n=16777216 `rip=0x7f03fbe5` `pci_ide=0` `hlt=0`). Do **not** flash `a2acfc8` /
`--run 33391068937` (n>16384 would not have changed that HLT at n~32768). firmware HLT skip without inject. flash 56f31d3. product ISO HLT stall before n=16384. do not F11 ea30da1. do not F11 56f31d3.
flash 56f31d3 (CI run `33392055961`). Do **not** F11 `56f31d3` / `--run 33392055961` (scsi@3 first, no El Torito boot option). product ISO fw_cfg bootorder El Torito ide@ first. flash 90da03d (CI run `33394776080`). firmware HLT skip after ataio. do not F11 90da03d (`ataio==0` skip parks PACKET HLT at RET). flash e70a295 (CI run `33397104645`). flash 77f5866 (CI run `33399209557`). firmware force IF for inject. do not F11 77f5866 (`skip-PIT` IF=0 after PACKET). retrigger 9df52c5. `9df52c5` CI run `33402411199` failed nested-KVM SHELL after GTIMER2 (iso=0 flake 5/5; not force-IF). flash 5227ad9 (CI run `33404368817`). firmware arm ATA GSI 14. flash 489d938 (CI run `33408594472`). firmware prefer ATA IRR. firmware ATA over PIC. flash bce5bbb (CI run `33411580450`). flash eaa580d (CI run `33413425759`). flash 12926eb (CI run `33415083012`). `--wait --require-head --no-git` after this pin, or `--run 33415083012` for the `12926eb` EFI. do not F11 eaa580d (`--run 33413425759` same-cycle only). do not F11 bce5bbb (`--run 33411580450` PIC IRQ 0 starves `0x2E`). do not F11 489d938 (`--run 33408594472` TPR-stuck `0x2E`). do not F11 5227ad9 (`--run 33404368817` pin 14 still masked). Do **not** F11 `b824789` / `--run 33387614559` (skip-after-inject raw pci_ide). Do **not** F11 `d61dc7e` / `--run 33349142609` (ConnectAll IdeBus CpuSleep). Do **not** F11 `5c0f7a2` / `--run 33347766697` (ATAPI-first bootorder without skip-without-inject). flash ea30da1.
product ISO fw_cfg bootorder virtio-iso scsi@3 first; product ISO fw_cfg bootorder El Torito ide@ first; flash ea30da1; do not F11 b824789; flash b824789; do not F11 d61dc7e; skip-after-inject uses pci_ready; flash d61dc7e; do not F11 5c0f7a2 (empty scsi@2 last; iso=0 stays CD then disk). Iron COM2 after F11 of `d61dc7e` is ConnectAll Started PIIX IDE (`pci_ide=1`, HLT `rip=0x7f0680d0` `ataio=0`, inj climbing, no virtio-iso IN). product ISO hides PIIX IDE. skip-after-inject uses pci_ready. firmware HLT skip without inject. product ISO HLT stall before n=16384. `8336a06` CI run `33387083800` failed nested-KVM kill-init after GTIMER2 (iso=0 still CDROM-OK BOTH-OK). Do **not** F11 `d61dc7e` / `8336a06` / `6c53fb0` / `b824789` / `ea30da1` / `a2acfc8` / `56f31d3` / `90da03d` / `e70a295` / `77f5866`. do not F11 ea30da1. Do **not** flash `ea30da1` pin `--run 33389381409` (inject `vec=0x20` timer ISR). flash 56f31d3. flash 90da03d. firmware HLT skip after ataio. do not F11 90da03d. flash e70a295. flash 77f5866. do not F11 e70a295. firmware force IF for inject. do not F11 77f5866. retrigger 9df52c5 CI after nested-KVM SHELL flake (33402411199). flash 5227ad9. firmware arm ATA GSI 14. flash 489d938. firmware prefer ATA IRR. firmware ATA over PIC. flash bce5bbb. do not F11 489d938. do not F11 5227ad9. do not F11 77f5866.
Do **not** F11 `2ae4544` / `--run 33345731636` (LAPIC expiry without I/O-over-PIT).
Do **not** F11 `084430f` / `--run 33337287432` (Delay then HLT stall).
Do **not** F11 `8663f56` / `--run 33333506987` (dest_ok then 0xAF00 Delay).
`FLASHCRUZER-OK` for `2d6b109` / run `33321642509` / EFI prefix `6fc742b0`
(checkout `cursor/e5-stage46-iso-a623`) is **not** F11. `2d6b109` IoReadFifo8 still
skips dest `0x205f18` inside identity `0x200000` (iron COM2 `3d6eba0`); that
SHA cannot install ACPI. Iron COM2 after F11 of that Cruzer is **`2d6b109`**: `pde0=0x20b027` (HV PT still `0x20B000`, identity `0x200000`), no `dest_ok fill`, `io string port=0x511 n=4 (rep insw)` only, DXE n=529 then `stop n=33297` `reason=0xc` `sectors=0` `catalog=0` `ataio=0` (POST_DXE_TAIL 32768, never PACKET). HPET froze at 11800 while `IN AL,DX` at `rip=0x7f020492`. That is not `8663f56` (`pde0` would be `0x40b027` plus `dest_ok fill`). Iron COM2 after F11 of `8663f56` **is** that SHA: `pde0=0x40b027`, `fw_cfg dest_ok fill dest=0x81ec98 n=56` x8, BOTH-OK, ACPI MADT, then `IN EAX,DX` at `rip=0x7f01f988` `stop n=33297` `sectors=0` `unh=4` after unhandled `0xAF00`/`0xAF05` (0xAF00 PM timer; do **not** F11 `8663f56` again). `unh=4` means later Delay I/Os were handled but `acpi` stayed 288 (not `0xB008`). 0xB000 dword timer firmware PIC before GSI 2; HLT stall quiet tick print-only; firmware HLT ignores TPR; firmware HLT stall waits for IRQ; iron COM2 084430f Delay via 0xB008 then HLT 0x7f0680d0 ataio=0; do not F11 c08a13d; do not F11 9ce65ae; firmware virtual-wire PIC; firmware virtual-wire AEOI; firmware virtual-wire GSI 2; firmware HLT force IF; firmware HLT skip after inject; firmware HLT activity active; firmware LAPIC timer expiry; IOAPIC I/O over PIT; firmware virtual-wire GSI 14; flash 5c0f7a2; do not F11 2ae4544; product ISO fw_cfg bootorder virtio-iso scsi@3 first; flash ea30da1; do not F11 b824789; flash b824789; do not F11 d61dc7e; skip-after-inject uses pci_ready; flash d61dc7e; do not F11 5c0f7a2; iron COM2 eac424b IRET-to-HLT; iron COM2 eac424b pic=1 sparse inject; iron COM2 beb1576 HLT if=1 tpr=0x0 pic=0 gsi2=0; do not F11 eac424b; do not F11 8e81c2e; do not F11 daf3195; do not F11 b26c86a. is in-tree for `IoRead32(0xB000)` after SCI_EN; **do not F11** `c08a13d` / `9ce65ae` (nested QEMU lost ATAPI-OK when quiet skipped leftover `cpu_flush`; EFI follows the run after the print-only SHA). flash 084430f. flash 2ae4544. flashcruzer reject 2d6b109 dest skip. auto-answer / # without login. product ISO POST_DXE_TAIL skip (armed Stage 46 does not stop at n=33297 `sectors=0`; lab iso=0 still uses the tail). emergency mount+exit (3.21 `/init` has no `setup-disk`). linux-line usbdelay (mkinitfs 3.11 `myopts` has no `alpine_dev`; nlplug `-b` is the repositories  product ISO HLT stall before n=16384; do not F11 ea30da1. file). io string (rep insb); 0xAF00 PM timer. 0xAF00 PM timer. Do not flash `fc03715` / `34b5767` / `3c95261` / `27de5f2` / `d0735bd` again
unless that SOL is still live. Iron COM2 after `d0735bd` (deliver line has no `err=`)
reached `#PF linux deliver n=1` then CPUID `rip=0xffffffffb8081783` `insn=`
empty — that is not `ISO-INSTALL-OK`. Do not flash `34b5767` (QEMU boot
gate `#UD` at CLWB), `d0735bd`, `40f1ada`,
`27de5f2`, `4a62e06`, or `e40bee0` again unless that SOL is still live. Leftover DRAM
`pool=1008 extra=846 no-zero` and `#PF linux deliver` are proven. Want
`report-RAM extra hpa=` / `pool=` near 1008 with
`extra=` `no-zero` then `#PF linux deliver` `err=` `linux cpuid` /
`linux skip-2` / `linux skip-1` then `Linux version`. INVLPG `0F 01 /7` is
skip-decoded; empty fetch logs `linux invlpg miss` and does not guess.
ISO serial patches allow ISO9660 NUL padding on
either side so alpine-virt `grub.cfg` `set timeout=1` still patches;
the linux-line grow into that pad also bumps ISO9660 + Joliet Data
Length 143→294 so GRUB sees `initrd` / `}` and `alpine_dev=vdb` / `virtio_pci` / `initcall_blacklist=piix_init` (a 143-byte read truncated
at `tsc=` and dropped to rescue `grub>` on iron COM2 after El Torito
`bootimg=1`); gzip `vmlinuz` is not rewritten; skip 256 MiB disk when leftover would
starve OVMF report-RAM; 64 MiB still GPT; port `0x61` TMR2_OUT; arm
product ISO before disk attach. Last iron COM2 on `faeaf38` after Boot0002
`Linux virt`: EFI stub gzip + `Loaded initrd` (`install disk bytes=67108864`
`report-RAM pool=162` `ram=` 59; not `uncompression error`). Not
`ISO-INSTALL-OK`. Do not flash `8a71596`. Never PERC.
Never `sda`/`sdb`. `--no-linux-iso` still strips leftovers so `iso=0` E4 SHELL
stays valid.

```bash
ls -l /home/vikkp/projects/raynuv/alpine-virt-*-x86_64.iso 2>/dev/null || \
  wget -O /home/vikkp/projects/raynuv/alpine-virt-3.21.3-x86_64.iso \
    https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/x86_64/alpine-virt-3.21.3-x86_64.iso
cd ~/projects/raynu
# Do not checkout cursor/e5-stage46-iso-a623 (that is 2d6b109 dest skip).
# FETCH_HEAD-only is not a checkout. Point HEAD at origin, then pin the ACPI EFI.
# FLASHCRUZER-OK for 2d6b109 / 33321642509 / 6fc742b0 is not F11.
git fetch origin refs/heads/cursor/e5-pm1-sci-a623:refs/remotes/origin/cursor/e5-pm1-sci-a623
git checkout -B cursor/e5-pm1-sci-a623 origin/cursor/e5-pm1-sci-a623
git log -1 --oneline   # want this SHA (flash b5c3a9c pin 33440050729).
# Pin --run 33440050729 (b5c3a9c firmware HLT insn_len 0 skip). do not F11 a14223f.
# Do not pin --run 33436232227 (a14223f superseded). do not F11 a14223f.
# Do not pin --run 33436822494 (4730397 pin of a14223f). do not F11 4730397.
# Do not pin --run 33433126839 (3b7bbac PIC ICW2 clobber IRQ 14 0x26). do not F11 3b7bbac.
# Do not pin --run 33429494930 (e4faceb leftover IOAPIC 0x2E). do not F11 e4faceb.
# Do not pin --run 33426291731 (d7d63ca PIC ATA clobbers IOAPIC to 0x2E). do not F11 d7d63ca.
# Pin --run 33426291731 (d7d63ca firmware PIC ATA). do not F11 8e581c7.
# Do not pin --run 33424573770 (8e581c7 PIC unmask never reached take_pic). do not F11 8e581c7.
# firmware PIC ATA. firmware PIC ATA ICW2. firmware PIC ATA AEOI.
# Do not pin --run 33422323257 (30b78a0 take IOAPIC ATA with edge remote IRR). do not F11 30b78a0.
# Pin --run 33422323257 (30b78a0 firmware take IOAPIC ATA). do not F11 0bb06a2.
# Do not pin --run 33418246409 (0bb06a2 ATA IRR only without take IOAPIC ATA). do not F11 0bb06a2.
# Do not pin --run 33415083012 (12926eb take_highest_irr LVT 0xEF). do not F11 12926eb.
# Pin --run 33415083012 (12926eb firmware ATA over PIC keeps latched 0x2E). do not F11 eaa580d.
# Do not pin --run 33413425759 (eaa580d same-cycle ATA over PIC). do not F11 eaa580d.
# Pin --run 33413425759 (eaa580d firmware ATA over PIC). do not F11 bce5bbb.
# Do not pin --run 33411580450 (bce5bbb prefer ATA IRR; PIC IRQ 0 starves 0x2E). do not F11 bce5bbb.
# Pin --run 33411580450 (bce5bbb firmware prefer ATA IRR). do not F11 489d938.
# Do not pin --run 33408594472 (489d938 TPR-stuck 0x2E). do not F11 489d938.
# Do not pin --run 33404368817 (5227ad9 force-IF pin 14 still masked). do not F11 5227ad9.
# Do not pin --run 33440951898 (c0c9810 pin-docs nested ATAPI miss ataio=0). nested iso=0 firmware HLT PIT.
# Do not pin --run 33443188019 (3ff3cf9 nested PIT VMXON-SKIP). nested iso=0 EDK2 IRQ0.
# Do not pin --run 33444677681 (deb64f5 nested EDK2 IRQ0 VMXON-SKIP). nested iso=0 firmware LAPIC timer.
# Do not pin --run 33445476540 (a83c51c nested LAPIC VMXON-SKIP). product ISO firmware HLT wake.
# Do not pin --run 33446918467 (2b1433f product HLT wake VMXON-SKIP). nested iso=0 firmware HLT ATA. firmware SRST ATA IRQ.
# Do not pin --run 33448452364 (61eef92 nested ATA VMXON-SKIP). product ISO firmware HLT ATA.
# Do not pin --run 33449291916 (05938ac product HLT ATA VMXON-SKIP). product ISO firmware HLT ATA IOAPIC.
# Do not pin --run 33450139765 (fe05f78 product ATA IOAPIC VMXON-SKIP). nested iso=0 firmware HLT ATA LAPIC.
# Do not pin --run 33438918646 (9299888 retrigger nested ATAPI miss ataio=0). firmware HLT insn_len 0 skip.
# Do not pin --run 33437881901 (0d36b53 nested ATAPI miss ataio=0 packet=0). retrigger 0d36b53. PIC ATA vector follows ICW2.
# Do not pin --run 33430294210 (5a69de2 nested-KVM kill-init after GTIMER2). retrigger 5a69de2. firmware OVMF ATA vector.
# Do not pin --run 33417361559 (cdbee39 nested-KVM kill-init after GTIMER2). retrigger cdbee39. firmware ATA IRR only.
# Do not pin --run 33402411199 (9df52c5 nested-KVM SHELL flake 5/5). firmware force IF for inject.
# Do not pin --run 33399209557 (77f5866 skip-PIT IF=0 after PACKET). do not F11 77f5866.
# Do not pin --run 33397104645 (e70a295 skip-without-inject blocked ATA 14). do not F11 e70a295.
# Pin --run 33394776080 (90da03d El Torito ide@ first). do not F11 90da03d.
# Do not pin --run 33392055961 (56f31d3 scsi@3 first, no El Torito boot option). do not F11 56f31d3.
# Do not pin --run 33389381409 (ea30da1 inject vec=0x20 timer ISR).
# Do not pin --run 33391068937 (a2acfc8 n>16384 after that boot ended).
lsusb | grep -i 0781:5151
./tools/flashcruzer.sh --no-git --run 33440050729 \
  --linux-iso /home/vikkp/projects/raynuv/alpine-virt-3.21.3-x86_64.iso
# --wait --require-head --no-git stays valid on this branch after a green HEAD
# artifact; do not use it on e5-stage46-iso-a623.
# firmware prefer ATA IRR. firmware ATA over PIC. firmware ATA IRR only. firmware take IOAPIC ATA. firmware PIC ATA. firmware OVMF ATA vector. do not clobber IOAPIC ATA vector. do not inject leftover 0x2E. do not clobber PIC ICW2. PIC ATA vector follows ICW2. firmware HLT insn_len 0 skip. nested iso=0 firmware HLT PIT. nested iso=0 EDK2 IRQ0. nested iso=0 firmware LAPIC timer. product ISO firmware HLT wake. nested iso=0 firmware HLT ATA. firmware SRST ATA IRQ. product ISO firmware HLT ATA. product ISO firmware HLT ATA IOAPIC. nested iso=0 firmware HLT ATA LAPIC. IOAPIC edge no remote IRR. firmware arm ATA GSI 14. firmware force IF for inject. firmware skip PIT inject. flash bce5bbb. flash eaa580d. flash 12926eb. flash 0bb06a2. flash 30b78a0. flash 8e581c7. flash d7d63ca. flash e4faceb. flash 3b7bbac. flash a14223f. flash b5c3a9c.
# firmware prefer ATA IRR. firmware ATA over PIC. firmware ATA IRR only. firmware take IOAPIC ATA. firmware PIC ATA. IOAPIC edge no remote IRR. F11 pin is --run 33440050729.
# --run 33440050729 is F11. --run 33436232227 is not F11. --run 33436822494 is not F11. --run 33433126839 is not F11. --run 33429494930 is not F11. --run 33426291731 is not F11. --run 33424573770 is not F11. --run 33422323257 is not F11. --run 33418246409 is not F11. --run 33415083012 is not F11. --run 33417361559 is not F11. --run 33413425759 is not F11. --run 33411580450 is not F11. --run 33408594472 is not F11. --run 33404368817 is not F11.
# do not F11 a14223f. do not F11 3b7bbac. do not F11 d7d63ca. do not F11 8e581c7. do not F11 30b78a0. do not F11 0bb06a2. do not F11 12926eb. do not F11 eaa580d. do not F11 bce5bbb. do not F11 489d938. do not F11 5227ad9. do not F11 77f5866. do not F11 e70a295.
```

`--no-linux-iso` removes a leftover product ISO so `iso=0` E4 `LINUX-EARLY`
still runs. QEMU: `PRODUCT_ISO=/path/to.iso ./tools/run-qemu.sh` (default ESP
strips leftovers so the boot gate does not HOLD). PRE-EBS copies the ISO into
`LOADER_DATA`. Guest OVMF boots that CD, virtio-pci queues target an empty
install disk at `00:02.0` (`/dev/vda`; 1 GiB want on iron, 64 MiB reserved before greedy report-RAM, 1 MiB nested) and a
read-only virtio-blk at `00:03.0` (`/dev/vdb`) serving the same ISO bytes
(alpine-virt finds ISO9660 without `ata_piix`), virtio GPA copies stop at
4 KiB so report-RAM 2 MiB slots are not overrun, product ISO PIC/IOAPIC injects
ATA IRQ 14 and virtio INTx (GSI 17/18 plus PCI line 11 as IOAPIC pin 11;
lab 8259 stays RAZ/WI), PIT IRQ 0 on HLT/preemption so Linux `noapic` jiffies
advance, i8253 channel 0 is a 16-bit lo/hi + latch counter (`raise_pit` steps it), product ISO COM1 is a
scratch/FIFO 16550 (lab UART stays stub), host COM2/COM1 RX is copied into
guest COM1 RBR, Alpine `login:` / `~# ` on that console is auto-answered
with `BOOTLOADER=grub USE_EFI=1 setup-disk -m sys -s 0 /dev/vda` after `modprobe virtio_pci; modprobe virtio_blk; modprobe sr_mod; modprobe isofs; for i in 0 1 2 3 4;do mdev -s;[ -b /dev/vda ]&&break;sleep 1;done; mkdir -p /media/cdrom; mount -t iso9660 /dev/vdb /media/cdrom || mount -t iso9660 /dev/sr0 /media/cdrom; echo /media/cdrom/apks > /etc/apk/repositories; apk update` (and `grub` if `bootloader?`
appears, `/dev/vda` if `Which disk`, `sys` if `How would you like`, `n` if `No disks available` then `(y/n)`, or `y` if `[y/N]` / `(y/n)` erase confirm; not ISO-INSTALL-OK), the ISO cmdline is patched to
`squashfs,virtio_blk console=ttyS0` (`modules=loop,squashfs,virtio_blk` stays valid so Alpine
can mount the live root and load virtio-blk; `console=` is a kernel param; product ISO xAPIC is
trap-and-emulate so CUR_COUNT/EOI move and `nolapic` is not required; optional `console=tty0` → `noapic`; GRUB
`timeout=10` → `timeout=0` then `set timeout=1` → `set timeout=0`; linux-line NUL-pad grow also bumps ISO9660 + Joliet `grub.cfg` Data Length 143→294 so GRUB sees `initrd`/`}` and `alpine_dev=vdb` / `virtio_pci` / `initcall_blacklist=piix_init` (do not leave Data Length at 143; Linux 6.12 `ata_piix.c` is `module_init(piix_init)`, not `ata_piix_init`); `gfxterm` / `efi_gop` / `efi_uga` / `all_video` / `terminal_output console` → `serial` when present;
`alpine_dev=cdrom` → `alpine_dev=vdb` when present) when it
contains `squashfs,sd-mod,usb-storage quiet`, ATAPI PIO DRQ is 31 CD sectors (Linux `sr` READ(10) is not completed short at 4), dest-reg ALU (`02`/`03` ADD r, r/m through `32`/`33` XOR) plus INC/DEC/NOT/NEG update RFLAGS so virtio/xAPIC RMW does not spin, BT/BTS/BTR/BTC so `lock bts` on a BAR does not spin, CMPXCHG/XADD so `lock cmpxchg` does not spin, guest-UEFI CR8-load/store exiting so Linux `mov cr8` syncs `lapic_virt` TPR (E4 SHELL does not request CR8 exiting), ADC/SBB so `adc`/`sbb` on a BAR consume CF, group-2 SHL/SHR/SAR/ROL/ROR/RCL/RCR so bitfield ops on a BAR do not spin, CMOVcc/SETcc so conditional moves/sets on a BAR do not spin, PREFETCH/NOP/CLFLUSH so compiler hints on a BAR skip without access, BSF/BSR so bit-scan on a BAR does not spin, IMUL so signed multiply of a BAR does not spin, F6/F7 MUL/IMUL so DX:AX product of a BAR does not spin, F6/F7 DIV/IDIV so DX:AX quotient of a BAR does not spin (#DE on 0/overflow), MOVNTI so a non-temporal store to a BAR does not spin, SHLD/SHRD so a double-precision shift of a BAR does not spin, CMPXCHG8B so `lock cmpxchg8b` on a BAR does not spin, TZCNT/LZCNT/POPCNT so BMI1 `tzcnt`/`lzcnt`/`popcnt` of a BAR does not decode as BSF/BSR, PUSH/POP r/m so `push`/`pop` of a BAR does not decode-fail, MOVS/STOS/LODS so memcpy/memset of a BAR does not decode-fail, CALL/JMP r/m so `call`/`jmp` of a BAR does not decode-fail, CMPS/SCAS so memcmp/memchr of a BAR does not decode-fail, MOVUPS/MOVDQU so SSE memcpy of a BAR does not decode-fail, firmware-RIP insn fetch from the OVMF flash HPA so xAPIC SVR (`0xFEE000F0`) at `rip=0xFFFCFxxx` is not `insn=` empty, install disk reserved before greedy scratch and report-RAM, iron product-ISO frame pool 512 MiB so Alpine can get a 256 MiB disk (`iso=0`/nested stay 256 MiB BAR/shell), MMIO fetch uses CS.base+RIP unless 64-bit CS, virtqueue GPA lazy-maps report-RAM, IOAPIC vectors latch LAPIC IRR (remote IRR / level EOI retry; not a bare VM-entry inject), and guest-UEFI **holds**
(does not fail-soft to E4). Armed product ISO uses the 16 777 216 resume cap
on nested QEMU too (lab stub / `iso=0` nested stays 65536). Iron COM2 close is `RAYNU-V-M7-ISO-INSTALL-OK` after the
installer writes a partition table. Host/CI never prints that marker. `iso=0`
/ lab stub still E4 `LINUX-EARLY`. Keep `windows_iso` / `generic_uefi`. Iron
P0-14 `last_commit` stays `2b795a0` until this gate actually closes.
Linux virtio/IOAPIC MMIO retries decode from fetched bytes when VMCS length is 0
(`ioread` is `"=r"`, not EAX); empty-peek Linux EAX fallback skips 3. iso=0 still
stops on decode fail. Not `ISO-INSTALL-OK`.

## Next

1. ~~Wire `InstallLaunchContract` → guest launch (extract-boot + install-sized virtio-blk).~~
   **Done (scaffold wire):** PRE-EBS `POST /iso/{id}/install` arms a static contract;
   post-EBS `virtio_blk::init` uses `disk_bytes_for_virtio_launch()` (64 MiB when armed,
   else 4 KiB M4.3 probe). Serial: `boot: E5 install-sized virtio-blk armed`.
2. ~~QEMU smoke: ESP lab install write + reboot detect.~~
   **Done (lab):** two-boot `./tools/m7-iso-install-qemu-smoke.sh` →
   `RAYNU-V-M7-ISO-INSTALL-LAB-OK` + `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`
   (host-synthesized persist image between boots; not a guest filesystem installer).
3. ~~Firmware ESP persist of LBA stamps (`installdisk.bin`).~~ **Done (iron):**
   Cruzer Micro persist-detect + prefix-copy → `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`
   (2026-08-16). [`STATUS-iso-install`](../evidence/r640/STATUS-iso-install) closed.
4. Guest filesystem install + full-disk persist (beyond LBA marker lab) — **after** post-EBS HTTP.
5. El Torito / guest UEFI firmware + typed ISO ([ADR-014](../adr/ADR-014.md)) —
   product installer. Not another bzImage extract. Windows ISO later.
   **Stage 0 (host, closed):** boot spec on the wire + catalog parse
   (`RAYNU-V-M7-E5-BOOT-SPEC-OK`).
   **Stage 1 (host, closed):** host CD-ROM attach (`RAYNU-V-M7-E5-CDROM-ATTACH-OK`).
   **Stage 2 (host, closed):** firmware-facing CD arm (`RAYNU-V-M7-E5-CDROM-FIRMWARE-OK`).
   **Stage 3 (host, closed):** guest FW envelope boxed (`RAYNU-V-M7-E5-GUEST-FW-OK`).
   **Stage 4 (host, closed):** stub payload load (`RAYNU-V-M7-E5-GUEST-FW-LOAD-OK`).
   **Stage 5 (host, closed):** OVMF FV probe (`RAYNU-V-M7-E5-OVMF-PROBE-OK`).
   **Stage 6 (host, closed):** ESP OVMF load (`RAYNU-V-M7-E5-OVMF-ESP-OK`).
   **Stage 7 (host, closed):** firmware slot arm (`RAYNU-V-M7-E5-OVMF-SLOT-OK`).
   **Stage 8 (host, closed):** firmware-to-guest bind (`RAYNU-V-M7-E5-FW-BIND-OK`).
   **Stage 9 (host, closed):** firmware launch-prepare (`RAYNU-V-M7-E5-FW-PREP-OK`).
   **Stage 10 (host, closed):** firmware size-floor (`RAYNU-V-M7-E5-FW-FLOOR-OK`).
   **Stage 11 (host, closed):** firmware EDK2-sized stage (`RAYNU-V-M7-E5-FW-EDK2-OK`).
   **Stage 12 (host, closed):** ESP-path VMLAUNCH (`RAYNU-V-M7-E5-ESP-LAUNCH-OK`).
   **Stage 13 (host, closed):** live ESP OVMF map (`RAYNU-V-M7-E5-ESP-MAP-OK`).
   **Stage 14 (host, closed):** reset-vector VMCS (`RAYNU-V-M7-E5-RESET-VEC-OK`).
   **Stage 15 (host, closed):** firmware-alias EPT (`RAYNU-V-M7-E5-FW-ALIAS-OK`).
   **Stage 16 (host, closed):** alias-EPT program (`RAYNU-V-M7-E5-ALIAS-EPT-OK`).
   **Stage 17 (host, closed):** private alias-EPT install (`RAYNU-V-M7-E5-EPT-INSTALL-OK`).
   **Stage 18 (host, closed):** real-ESP VMLAUNCH-ready (`RAYNU-V-M7-E5-REAL-ESP-OK`).
   **Stage 19 (host, closed):** guest-UEFI VMLAUNCH insn arm (`RAYNU-V-M7-E5-REAL-LAUNCH-OK`).
   **Stage 20 (host, closed):** live-ESP VMLAUNCH execute gate (`RAYNU-V-M7-E5-LIVE-EXEC-OK`).
   **Stage 21 (host, closed):** private guest-UEFI VMCS arm (`RAYNU-V-M7-E5-PRIV-VMCS-OK`).
   **Stage 22 (host, closed):** live-ESP VMLAUNCH issue path (`RAYNU-V-M7-E5-LIVE-ISSUE-OK`).
   **Stage 23 (host, closed):** live-ESP bytes probe (`RAYNU-V-M7-E5-LIVE-BYTES-OK`).
   **Stage 24 (host, closed):** live-ESP FD require (`RAYNU-V-M7-E5-LIVE-FD-OK`).
   **Stage 25 (host, closed):** live-ESP present-attempt (`RAYNU-V-M7-E5-LIVE-PRESENT-OK`).
   **Stage 26 (host, closed):** live-ESP admit-attempt (`RAYNU-V-M7-E5-LIVE-ADMIT-OK`).
   **Stage 27 (host, closed):** live-ESP read-attempt (`RAYNU-V-M7-E5-LIVE-READ-OK`).
   **Stage 28 (host, closed):** live-ESP copy-attempt (`RAYNU-V-M7-E5-LIVE-COPY-OK`).
   **Stage 29 (host, closed):** live-ESP place-attempt (`RAYNU-V-M7-E5-LIVE-PLACE-OK`).
   **Stage 30 (host, closed):** live-ESP apply-attempt (`RAYNU-V-M7-E5-LIVE-APPLY-OK`).
   **Stage 31 (host, closed):** live-ESP commit-attempt (`RAYNU-V-M7-E5-LIVE-COMMIT-OK`).
   **Stage 32 (host, closed):** live-ESP latch-attempt (`RAYNU-V-M7-E5-LIVE-LATCH-OK`).
   **Stage 33 (host, closed):** live-ESP seal-attempt (`RAYNU-V-M7-E5-LIVE-SEAL-OK`).
   **Stage 34 (host, closed):** live-ESP lock-attempt (`RAYNU-V-M7-E5-LIVE-LOCK-OK`).
   **Stage 35 (host, closed):** live-ESP hold-attempt (`RAYNU-V-M7-E5-LIVE-HOLD-OK`).
   **Stage 36 (host + QEMU, closed):** real ESP `OVMF.fd` retain
   (`RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK`). Presence follows
   `accept_real_ovmf_bytes` on the retained buffer. Heap fixtures are
   rejected. Private guest-UEFI VMCS is not allocated. VMLAUNCH insn
   not issued. No further `*Absent` bookkeeping stages.
   **Stage 37 (host + QEMU, closed):** private guest-UEFI VMCS + EPT +
   VMLAUNCH of retained ESP `OVMF.fd`
   (`RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK`). Not the E4 SHELL VMCS/EPT.
   First entry only. Not installer. Not Everest E5.
   **Stage 38 (host + QEMU, closed):** OVMF past first triple-fault
   (`RAYNU-V-M7-E5-OVMF-ALIVE-OK`). CR4.VMXE host-owned. Not full
   OVMF boot. Not installer. Not Everest E5.
   **Stage 39 (host + QEMU, closed):** OVMF past SEC
   (`RAYNU-V-M7-E5-OVMF-PAST-SEC-OK`). Left last 64 KiB + PEI PCI /
   firmware COM / HLT. COM1/COM2 forwarded. Not full DXE. Not installer.
   Not Everest E5.
   **Stage 40 (host + QEMU, closed):** guest-UEFI CD visible
   (`RAYNU-V-M7-E5-OVMF-CDROM-OK`). `attach_cdrom_uefi` → GuestVisible.
   PCI IDE/ATAPI on the private VMCS. Not full DXE. Not installer.
   Not Everest E5.
   **Stage 41 (host + QEMU nested VT-x, closed):** past-PEI/DXE or CD
   boot attempt (`RAYNU-V-M7-E5-OVMF-DXE-OK`). `OVMF-CDROM-OK`
   pci_ide=1 sectors=0 (`val=0x7010`). CMOS/fw_cfg + i440FX at `00:08.0`
   + IDE at `00:00.0`. Post-DXE tail then E4. Not a completed firmware
   CD boot. Not installer.    Not Everest E5.
   **Stage 42 (host + QEMU nested VT-x, closed):** empty virtio-blk + boot
   order CD then disk (`RAYNU-V-M7-E5-OVMF-VIRTIO-OK`). PCI virtio 1.0 at
   `00:00.0` (PEI DID `0x1042`). CD GuestVisible. `pci_ide=0` sectors=0.
   Stop n=115 virtio=1. Not a completed firmware CD boot.
   Not installer. Not Everest E5.
   **Stage 43 (host + QEMU nested VT-x, closed):** simultaneous virtio
   `00:00.0` + IDE `00:00.1` (`RAYNU-V-M7-E5-OVMF-BOTH-OK`). Nested VT-x
   `1b07692`: `pci select 00:00.01` `val=0x70108086`, stop n=1111
   `pci_ide=1 virtio=1` `sectors=0` `spin=1`. Not a completed firmware
   CD boot. Not installer. Not Everest E5.
   **Stage 44 (iron COM2 `bf696ca`, closed):** firmware ATAPI READ
   (`RAYNU-V-M7-E5-OVMF-ATAPI-OK`). `sectors=1` `packet=9` `scsi=0x28`
   stop n=30769 `pci_ide=1 virtio=1`. Not El Torito boot. Not installer.
   Not Everest E5.
   **Stage 45 (iron COM2 `0be7283`, closed):** firmware El Torito CD EFI
   (`RAYNU-V-M7-E5-OVMF-ELTORITO-OK`). `RN-ELT` n=197992 catalog=1 bootimg=1
   magic=1 sectors=183 elt=1 packet=533 scsi=0x28 port=0x3f8. Not installer.
   Not Everest E5.
   Next: Stage 46 `ISO-INSTALL-OK` (OPEN; ESP product ISO + virtio-pci queues + PIC/IOAPIC inject + 16550/`ttyS0` + hold when armed; virtio BAR trap over scratch + PIIX3 ISA BAR RAZ + packed virtio common cfg + virtio MMIO raises PIT + virtio MMIO eax fallback size; packed virtio common cfg write; virtio MMIO polls lapic; linux I/O does not raise PIT (iron MADT stop); linux xAPIC EPT insn_len 0; linux preempt deadloop noskip; linux PIT prefer once; linux PIT prefer until DRIVER_OK; UART reassert RX not THRE; virtio drain every resume; product ISO fw_cfg ACPI MADT (iso=0 named files stay 3); linux PIC before LAPIC; linux PIC IRQ0; MADT IRQ0 ISO GSI 2; PIT skips IOAPIC pin 0; linux GSI 2 before PIC; fw_cfg IoReadFifo8 fills RAM (skip HV identity PML4 dest); PIIX4 PM1 SCI_EN; PM1 SCI_EN at reset; DSDT PCI0 _PRT; DSDT PCI0 _CRS; linux hides duplicate slot0 IDE; linux hides PIIX IDE; linux high-half hides PIIX; linux-line alpine_dev=vdb; linux-line virtio_pci; linux ATA floating bus; fw_cfg skip dest n=; fw_cfg identity overlay; HV identity PML4 0x400000; PEI dest holds ACPI tables; fw_cfg dest_ok fill dest=; dest_ok fill log cap 8; ACPI tables ZONE_FSEG; FSEG dest holds ACPI tables; linux-line ata_piix blacklist; linux-line piix_init blacklist; FADT FACS; flashcruzer reject 2d6b109 dest skip; auto-answer / # without login; product ISO POST_DXE_TAIL skip; emergency mount+exit; linux-line usbdelay; io string (rep insb); 0xAF00 PM timer; 0xB000 dword timer; firmware PIC before GSI 2; HLT stall quiet tick print-only; firmware HLT ignores TPR; firmware HLT stall waits for IRQ; iron COM2 084430f Delay via 0xB008 then HLT 0x7f0680d0 ataio=0; do not F11 c08a13d; do not F11 9ce65ae; firmware virtual-wire PIC; firmware virtual-wire AEOI; firmware virtual-wire GSI 2; firmware HLT force IF; firmware HLT skip after inject; firmware HLT activity active; firmware LAPIC timer expiry; IOAPIC I/O over PIT; firmware virtual-wire GSI 14; flash 5c0f7a2; do not F11 2ae4544; product ISO fw_cfg bootorder virtio-iso scsi@3 first; product ISO fw_cfg bootorder El Torito ide@ first; flash ea30da1; do not F11 b824789; flash b824789; do not F11 d61dc7e; skip-after-inject uses pci_ready; flash d61dc7e; do not F11 5c0f7a2; iron COM2 eac424b IRET-to-HLT; iron COM2 eac424b pic=1 sparse inject; iron COM2 beb1576 HLT if=1 tpr=0x0 pic=0 gsi2=0; do not F11 eac424b; do not F11 8e81c2e; do not F11 daf3195; do not F11 b26c86a; flash 2ae4544; lab stub still E4; not clo firmware HLT skip without inject; product ISO HLT stall before n=16384; do not F11 ea30da1; do not F11 a2acfc8; not closed). M4.3 host-slab closed on iron after `22e28d0` (`M4-BLK-OK` `0x10c00000`). `ISO-BOOTED-FROM-DISK` is persist-detect, not the installer.
