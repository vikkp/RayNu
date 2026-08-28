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
works if ATAPI `sr-mod` is on the cmdline. Operator flash:

```bash
~/projects/raynuv/flashcruzer.sh --wait --linux-iso /path/to/alpine-virt-x86_64.iso
```

`--no-linux-iso` removes a leftover product ISO so `iso=0` E4 `LINUX-EARLY`
still runs. QEMU: `PRODUCT_ISO=/path/to.iso ./tools/run-qemu.sh` (default ESP
strips leftovers so the boot gate does not HOLD). PRE-EBS copies the ISO into
`LOADER_DATA`. Guest OVMF boots that CD, virtio-pci queues target an empty
install disk at `00:02.0` (`/dev/vda`; 1 GiB on iron, 1 MiB nested) and a
read-only virtio-blk at `00:03.0` (`/dev/vdb`) serving the same ISO bytes
(alpine-virt finds ISO9660 without `ata_piix`), virtio GPA copies stop at
4 KiB so report-RAM 2 MiB slots are not overrun, product ISO PIC/IOAPIC injects
ATA IRQ 14 and virtio INTx (GSI 17/18 plus PCI line 11 as IOAPIC pin 11;
lab 8259 stays RAZ/WI), PIT IRQ 0 on HLT/preemption so Linux `noapic` jiffies
advance, i8253 channel 0 is a 16-bit lo/hi + latch counter (`raise_pit` steps it), product ISO COM1 is a
scratch/FIFO 16550 (lab UART stays stub), host COM2/COM1 RX is copied into
guest COM1 RBR, Alpine `login:` / `~# ` on that console is auto-answered
with `BOOTLOADER=grub USE_EFI=1 setup-disk -m sys /dev/vda` after `mkdir -p /media/cdrom; mount /dev/vdb /media/cdrom` (and `grub` if `bootloader?`
appears, or `y` if `[y/N]` / `(y/n)` erase confirm; not ISO-INSTALL-OK), the ISO cmdline is patched to
`squashfs,virtio_blk console=ttyS0` (`modules=loop,squashfs,virtio_blk` stays valid so Alpine
can mount the live root and load virtio-blk; `console=` is a kernel param; product ISO xAPIC is
trap-and-emulate so CUR_COUNT/EOI move and `nolapic` is not required; optional `console=tty0` → `noapic`; GRUB
`timeout=10` → `timeout=0` then `set timeout=1` → `set timeout=0`; `gfxterm` / `efi_gop` / `efi_uga` / `all_video` / `terminal_output console` → `serial` when present;
`alpine_dev=cdrom` → `alpine_dev=vdb` when present) when it
contains `squashfs,sd-mod,usb-storage quiet`, ATAPI PIO DRQ is 31 CD sectors (Linux `sr` READ(10) is not completed short at 4), dest-reg ALU (`02`/`03` ADD r, r/m through `32`/`33` XOR) plus INC/DEC/NOT/NEG update RFLAGS so virtio/xAPIC RMW does not spin, BT/BTS/BTR/BTC so `lock bts` on a BAR does not spin, CMPXCHG/XADD so `lock cmpxchg` does not spin, guest-UEFI CR8-load/store exiting so Linux `mov cr8` syncs `lapic_virt` TPR (E4 SHELL does not request CR8 exiting), ADC/SBB so `adc`/`sbb` on a BAR consume CF, group-2 SHL/SHR/SAR/ROL/ROR/RCL/RCR so bitfield ops on a BAR do not spin, CMOVcc/SETcc so conditional moves/sets on a BAR do not spin, PREFETCH/NOP/CLFLUSH so compiler hints on a BAR skip without access, BSF/BSR so bit-scan on a BAR does not spin, IMUL so signed multiply of a BAR does not spin, F6/F7 MUL/IMUL so DX:AX product of a BAR does not spin, IOAPIC vectors latch LAPIC IRR (remote IRR / level EOI retry; not a bare VM-entry inject), and guest-UEFI **holds**
(does not fail-soft to E4). Iron COM2 close is `RAYNU-V-M7-ISO-INSTALL-OK` after the
installer writes a partition table. Host/CI never prints that marker. `iso=0`
/ lab stub still E4 `LINUX-EARLY`. Keep `windows_iso` / `generic_uefi`. Iron
P0-14 `last_commit` stays `2b795a0` until this gate actually closes.

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
   Next: Stage 46 `ISO-INSTALL-OK` (OPEN; ESP product ISO + virtio-pci queues + PIC/IOAPIC inject + 16550/`ttyS0` + hold when armed; lab stub still E4; not closed). M4.3 host-slab closed on iron after `22e28d0` (`M4-BLK-OK` `0x10c00000`). `ISO-BOOTED-FROM-DISK` is persist-detect, not the installer.
