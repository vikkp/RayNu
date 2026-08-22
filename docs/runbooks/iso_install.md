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
- **El Torito / firmware CD-ROM** still `UnsupportedOnFirmware` (see `iso.md`).
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
   Guest UEFI VMLAUNCH remains open (real EDK2 only; mock and 4 KiB floor refused).
