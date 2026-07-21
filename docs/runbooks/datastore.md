# Runbook — Datastore / image library (M7.2)

**Marker:** `RAYNU-V-M7-STORE-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md)

## What this gate proves

M7.2 adds an in-binary **image library**: register / list / delete metadata for
ISOs, disks, and templates. Host tests persist a catalog under an **ESP-shaped**
path suitable for R640 USB / ESP layout:

```text
EFI/RAYNU/images/catalog.txt
```

Lines are `id|kind|size|name` (`kind`: 1=iso, 2=disk, 3=template).

## REST shapes (Bearer auth)

Same bring-up token as M6.4 / M7.1 (`Authorization: Bearer raynu-v-bringup`).

| Method | Path | Result |
|--------|------|--------|
| `GET` | `/images` | 200 listed count |
| `GET` | `/images/{id}` | 200 image record |
| `POST` | `/images/{id}` | 201 register (default kind=iso) |
| `POST` | `/images/{id}/iso\|disk\|template` | 201 register kind |
| `DELETE` | `/images/{id}` | 200 delete |

HTTP codec (`mgmt/http.rs`) routes `/images*` to `dispatch_store_rest`.

## Host smoke

```bash
./tools/m7-store-smoke.sh
```

Expect:

```text
RAYNU-V-M7-STORE-OK
==> M7.2 datastore smoke PASSED
```

## Honesty / residuals

- **UEFI persist** (`persist_catalog_uefi`) returns `UnsupportedOnFirmware`
  until SimpleFileSystem / NVMe write is wired. Host `std::fs` catalog proves
  the on-disk format.
- **Blob bytes** (actual ISO payload) are not staged here — M7.3 registers ISO
  content / CD-ROM path on top of this metadata library.
- Outside Proven Core (ADR-009); size still ADR-003.

## Next

M7.3 ISO deploy path (`RAYNU-V-M7-ISO-OK`).
