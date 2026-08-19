# Runbook — In-place Cruzer flash from raynuvsrv1

**Host marker:** `RAYNU-V-CRUZER-FLASH-OK` (script on Ubuntu; **not** an iron HTTP gate)  
**Script:** `./tools/flash-cruzer-esp.sh`  
**Self-test:** `./tools/flash-cruzer-esp.sh --self-test` → `RAYNU-V-CRUZER-FLASH-SELFTEST-OK`  
**Evidence:** [`docs/evidence/r640/2026-08-19-raynuvsrv1-cruzer.md`](../evidence/r640/2026-08-19-raynuvsrv1-cruzer.md)

## Story

Ubuntu Server **raynuvsrv1** is installed on the R640 PERC (`sdb`). The SanDisk
Cruzer Micro stays in **front USB 2** (`lsusb` `0781:5151`). Operators flash
`EFI/BOOT/BOOTX64.EFI` from Ubuntu so Vignesh does not walk the stick to a Mac.

This is **replace-only**. It is **not** `./tools/make-boot-usb.sh` (that `dd`s
and erases the disk).

Do **not** claim `RAYNU-V-M7-HOST-NIC-HTTP-OK` from a successful copy.

## How Ubuntu sees the stick (2026-08-19)

| What | Value |
|------|--------|
| Hostname / user | `vikkp@raynuvsrv1` |
| `lsusb` | `Bus 001 Device 002: ID 0781:5151 SanDisk Corp. Cruzer Micro Flash Drive` |
| Block device that day | `/dev/sdc` (name **will** change — do not hardcode) |
| Model / serial | `Cruzer Micro` / `200524441218e7503e33` |
| Size | 977.48 MiB (`1024966656` bytes) |
| Filesystem | whole-disk FAT (`LABEL=RAYNUV`, UUID `5616-5E4B`) — no partition node |
| Mount | `sudo mount -t vfat /dev/disk/by-label/RAYNUV /mnt/usb` |

**Never write these:**

| Device | What it is |
|--------|------------|
| `/dev/sda` | PERC H740P Mini ~200G (Windows leftovers) |
| `/dev/sdb` | PERC H740P Mini ~3.1T (**Ubuntu**: `/boot/efi`, `/boot`, LVM `/`) |
| `/dev/sdd` | iDRAC Virtual Floppy |
| `/dev/sr0` | iDRAC Virtual CD |

Identify by **label + USB + Cruzer + size**, never by `sdc`.

## Flash (preferred)

On the Mac (or any box that has the verified EFI):

```
scp r640-hypervisor.efi vikkp@raynuvsrv1:~/
```

On **raynuvsrv1** (clone of this repo, or copy the script over):

```
sha256sum ~/r640-hypervisor.efi
sudo ./tools/flash-cruzer-esp.sh --efi ~/r640-hypervisor.efi --sha256 <hex>
```

Expect `RAYNU-V-CRUZER-FLASH-OK`. The script:

- resolves `/dev/disk/by-label/RAYNUV`
- refuses PERC / vMedia / >4 GiB / missing `EFI/RayNu/installdisk.bin`
- copies only `EFI/BOOT/BOOTX64.EFI`
- checks destination SHA256 and that `installdisk.bin` size did not change
- leaves `auth.token` alone if present

If `/mnt/usb` is already mounted (operator session), the script reuses that
mount and does not unmount it.

## Boot order

Keep BIOS boot order **Ubuntu on PERC first**. For a RayNu-V run: iDRAC
virtual console **before** reboot, then **one-time** F11 → Cruzer USB.
If USB is first in the permanent order, the box will skip Ubuntu and you lose
this flash path until you override boot.

## Manual equivalent (if the script is not on disk yet)

```
sudo mkdir -p /mnt/usb
sudo mount -t vfat /dev/disk/by-label/RAYNUV /mnt/usb
ls -l /mnt/usb/EFI/BOOT/BOOTX64.EFI /mnt/usb/EFI/RayNu/installdisk.bin
sudo cp ~/r640-hypervisor.efi /mnt/usb/EFI/BOOT/BOOTX64.EFI
sync
sha256sum ~/r640-hypervisor.efi /mnt/usb/EFI/BOOT/BOOTX64.EFI
sudo umount /mnt/usb
```

Still do **not** use `/dev/sdc` in a script. `test-file` on the volume (if
present) is harmless; delete when convenient.

## Limits

- Ubuntu is **down** while RayNu-V owns the machine. Flash, then F11 USB;
  boot PERC again to flash the next EFI.
- iDRAC Virtual Floppy/CD may still appear as `sdd`/`sr0` — ignore them.
- `make-boot-usb.sh` remains the **destructive** re-image path. Do not run it
  against the Cruzer from Ubuntu unless you intend to wipe `installdisk.bin`.
