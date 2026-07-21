# Runbook — ISO deploy path (M7.3)

**Marker:** `RAYNU-V-M7-ISO-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md)  
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

- **MVP is kernel-extract**, not full installer media emulation.
- **`attach_cdrom_uefi`** returns `UnsupportedOnFirmware` — El Torito / CD-ROM
  attach is deferred.
- **ISO blob upload** (raw bytes into ESP) is not claimed; metadata register is.
- Outside Proven Core (ADR-009); size still ADR-003.

## Next

M7.4 Ops Web UI MVP (`RAYNU-V-M7-UI-OK`) surfaces create-VM + media attach.
