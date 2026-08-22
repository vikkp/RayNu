# Runbook — ISO deploy path (M7.3)

**Marker:** `RAYNU-V-M7-ISO-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md) · product ISO: [ADR-014](../adr/ADR-014.md)  
**Prior:** [datastore.md](datastore.md) (M7.2)

## What this gate proves

M7.3 closes a **documented kernel-extract** deploy path on top of the image library:

1. **Register ISO** metadata into `ImageTable` (`ImageKind::Iso`).
2. **Bind extract-boot** to the existing bzImage/initrd staging +
   `guest::load_bzimage_guest` path (PE `.askern` / ESP `BZIMAGE` + `INITRD`).
3. **Empty virtio-blk** install target (`devices/virtio_blk.rs` capacity surface).

## REST shapes (Bearer auth)

| Method | Path | Result |
|--------|------|--------|
| `POST` | `/iso/{id}/deploy` | 201 — register ISO if needed + bind extract-boot + default install disk |
| `GET` | `/iso/deploy` | 200 — listed count `1` when plan ready |
| `POST` | `/iso/{id}/attach` or `/iso/{id}/attach/{type}` | 201 — host El Torito CD-ROM attach (mock EFI prefix until blob upload) |
| `GET` | `/iso/attach` | 200 — listed count of host-attached CD-ROMs |
| `POST` | `/iso/{id}/firmware` | 201 — firmware-facing CD arm (requires host attach first) |
| `GET` | `/iso/firmware` | 200 — listed count of FirmwareArmed records |
| `POST` | `/fw/box` | 201 — box the embedded guest UEFI firmware envelope (not OVMF) |
| `GET` | `/fw` | 200 — listed count of boxed guest firmware envelopes (0/1) |

Token: `Authorization: Bearer raynu-v-bringup` (same as M6.4 / M7.1 / M7.2).

## Host smoke

```bash
./tools/m7-iso-smoke.sh
```

Expect:

```text
RAYNU-V-M7-ISO-OK
==> M7.3 ISO deploy smoke PASSED
```

## Honesty / residuals

- **MVP is kernel-extract**, not full installer media emulation. Lab/M7.3 only.
- Product ISO install is **UEFI-first + typed** ([ADR-014](../adr/ADR-014.md)):
  `linux_iso` | `windows_iso` | `generic_uefi`. Do not hard-wire SPA install to
  bzImage jump. Windows install is later; the type exists now.
- **`attach_cdrom_uefi`** returns `UnsupportedOnFirmware` — live guest firmware
  CD is still deferred (M7.3 honesty). Host catalog **parse** (`parse_el_torito`)
  is Stage 0. Host **attach** (`attach_cdrom_host`) is Stage 1. Firmware **arm**
  (`attach_cdrom_firmware` / `FirmwareArmed`) is Stage 2
  (`RAYNU-V-M7-E5-CDROM-FIRMWARE-OK`). Guest firmware **envelope**
  (`box_guest_firmware` / `.asguefw`) is Stage 3
  (`RAYNU-V-M7-E5-GUEST-FW-OK`). Envelope box is not guest UEFI VMLAUNCH
  and not OVMF.
- **ISO blob upload** (raw bytes into ESP) is not claimed; metadata register is.
- Outside Proven Core (ADR-009 / ADR-014); size still ADR-003.

## Next

M7.4 Ops Web UI MVP (`RAYNU-V-M7-UI-OK`) surfaces create-VM + media attach.

E5 / M7.7 install-to-disk: see [iso_install.md](iso_install.md)
(`RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK`; iron `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` closed 2026-08-16).

E5 Stage 0 (host, closed): typed boot spec on REST/SPA + El Torito catalog parse.
`POST /vms/{id}/spec/{cpu}/{ram}/{disk}/{iso}/{linux_iso|windows_iso|generic_uefi}`.
`iso=0` stays E4 SHELL.

E5 Stage 1 (host, closed): `POST /iso/{id}/attach` arms host CD-ROM from El Torito.

E5 Stage 2 (host, closed): `POST /iso/{id}/firmware` arms FirmwareArmed after
host attach + boot-image sector validate. Not OVMF and not VMLAUNCH.

E5 Stage 3 (host, closed): `POST /fw/box` boxes the ADR-003 guest firmware
envelope (`.asguefw` header-only placeholder). Not OVMF and not VMLAUNCH.
