# Runbook — Boot media maker (USB / iDRAC)

**Marker (host smoke):** `RAYNU-V-M7-BOOT-MEDIA-OK`  
**Tools:**
- `./tools/make-boot-media.sh` — verify kit → FAT `.img` (+ `.iso` if `xorriso` present)
- `./tools/make-boot-usb.sh` — write `.img` to a physical USB (destructive)
- `./tools/media-maker.sh` — interactive wrapper
- Smoke: `./tools/make-boot-media-smoke.sh`

**Why this exists:** Section 2–3 of the printable field guide
([`r640_field_guide.md`](r640_field_guide.md)) used to be manual FAT copy
steps. The media maker does that packing for you so iDRAC Virtual Media (or a
USB stick) gets `\EFI\BOOT\BOOTX64.EFI` without hand-formatting.

## Dependencies

| OS | Packages |
|----|----------|
| Ubuntu/Debian | `dosfstools` `mtools` (`xorriso` optional for ISO) |
| macOS (Homebrew) | `dosfstools` `mtools` (`xorriso` optional) |

```bash
# Debian/Ubuntu
sudo apt install dosfstools mtools xorriso

# macOS
brew install dosfstools mtools xorriso
```

A **browser page alone cannot format USB sticks**. This is intentional: the
helper is a local script (optionally wrapped later in a tiny native UI).

## Quick path (recommended for racked R640)

```bash
./tools/package-release.sh          # or use an existing dist/raynu-v-* kit
./tools/make-boot-media.sh          # or: ./tools/media-maker.sh
```

Outputs (example for version `0.1.0`):

```text
dist/raynu-v-0.1.0-boot-media/
  raynu-v-0.1.0-uefi-boot.img
  raynu-v-0.1.0-uefi-boot.img.sha256
  raynu-v-0.1.0-uefi-boot.iso          # if xorriso installed
  raynu-v-0.1.0-uefi-boot.iso.sha256
  MEDIA.txt
```

### iDRAC Virtual Media (preferred)

1. Open iDRAC → **Virtual Console** → **Virtual Media**.
2. Map **`*-uefi-boot.img` as a virtual USB** stick (best match for
   `\EFI\BOOT\BOOTX64.EFI`).
3. Or map **`*-uefi-boot.iso` as a virtual CD** if your iDRAC prefers ISO.
4. Set next boot to that virtual device; keep serial open; reboot.
5. Unmap media after the attempt.

### Physical USB

```bash
# List disks carefully first (macOS: diskutil list / Linux: lsblk)
./tools/make-boot-usb.sh \
  --img dist/raynu-v-0.1.0-boot-media/raynu-v-0.1.0-uefi-boot.img \
  --disk /dev/diskN
```

You must re-type the disk path to confirm. `YES=1` skips the prompt (CI /
automation only).

## What the FAT image contains

| Path | Role |
|------|------|
| `\EFI\BOOT\BOOTX64.EFI` | UEFI default boot file (copy of `r640-hypervisor.efi`) |
| `\r640-hypervisor.efi` | Same binary, original name (convenience) |
| `\VERSION` | From the release kit when present |
| `\r640-hypervisor.efi.sha256` | Sidecar when present |

## Limits

- Does **not** build the hypervisor for you unless you run `package-release`
  first (or interactive option 2).
- Does **not** close `RAYNU-V-R640-BOOT-OK` — that still needs real R640 serial
  evidence ([`r640_boot.md`](r640_boot.md)).
- Host smoke uses a tiny fixture EFI to prove layout only — not a bootable HV.
