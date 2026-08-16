# Runbook — ISO install-to-disk (E5 / M7.7)

**Scaffold marker (host/CI):** `RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK`  
**Iron / QEMU close marker:** `RAYNU-V-M7-ISO-INSTALL-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md) · HDA E5  
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

`REBOOT-PENDING` means the phase machine advanced to “would reboot from disk” —
it does **not** execute a second boot (disk backing is still RAM; no persistence yet).

```bash
./tools/m7-iso-install-qemu-smoke.sh
```

Iron / REST still use the **64 MiB** default via `POST /iso/{id}/install`.

### Full product path

1. `POST /iso/{id}/deploy` then `POST /iso/{id}/install` (or install alone / ESP lab).
2. Guest boots via **extract-boot** (`load_bzimage_guest` + staged bzImage/initrd).
3. Host/guest writes a marker (or filesystem) to the virtio-blk install disk.
4. Hypervisor records DiskWritten → RebootPending.
5. Second boot from the install disk → `RAYNU-V-M7-ISO-INSTALL-OK` on serial.

### Iron (R640)

Mirror E2/E3:

1. Build kit → `releases/v0.1.0-iso-install-*` + SHA256.
2. `make-boot-media` → iDRAC Virtual Floppy.
3. COM2 capture (`console com2`).
4. Mac curl: deploy/install REST during PRE-EBS window if needed.
5. Fill [TEMPLATE-iso-install.md](../evidence/r640/TEMPLATE-iso-install.md).
6. Set `docs/evidence/r640/STATUS-iso-install` to `STATUS=closed` only with real proof.

## Honesty / residuals

- **GAP(OPEN M7.7)** until install-to-disk + reboot-to-disk proven.
- **El Torito / CD-ROM** still `UnsupportedOnFirmware` (see `iso.md`).
- **ISO blob upload / parse** not claimed — extract-boot uses existing PE/ESP assets first.
- **Wire into `vmx/launch.rs`** is the next engineering step after this scaffold.
- Outside Proven Core (ADR-009); size still ADR-003.
- Do **not** claim Mount Everest closed until E4 + E5 are both green.

## Next

1. ~~Wire `InstallLaunchContract` → guest launch (extract-boot + install-sized virtio-blk).~~
   **Done (scaffold wire):** PRE-EBS `POST /iso/{id}/install` arms a static contract;
   post-EBS `virtio_blk::init` uses `disk_bytes_for_virtio_launch()` (64 MiB when armed,
   else 4 KiB M4.3 probe). Serial: `boot: E5 install-sized virtio-blk armed`.
2. ~~QEMU smoke: PRE-EBS curl install → confirm install-sized disk serial + `M4-BLK-OK`.~~
   **Done (lab):** ESP `isoinstall.txt` / `ISO_INSTALL_LAB=1` → 1 MiB disk +
   `RAYNU-V-M7-ISO-INSTALL-LAB-OK` via `./tools/m7-iso-install-qemu-smoke.sh`.
3. Guest filesystem install + reboot-from-disk path (lab is host LBA1 marker +
   `REBOOT-PENDING` honesty latch; no second boot yet).
4. Iron kit + evidence close → `RAYNU-V-M7-ISO-INSTALL-OK`.
