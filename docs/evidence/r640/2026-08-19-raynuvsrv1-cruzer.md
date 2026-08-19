# R640 — raynuvsrv1 sees the Cruzer in front USB 2

**Date:** 2026-08-19  
**Host:** Ubuntu Server `raynuvsrv1` (`vikkp@raynuvsrv1`) on the PowerEdge R640 PERC  
**Claim:** The SanDisk Cruzer Micro (`0781:5151`) is visible as a writable
whole-disk FAT volume `LABEL=RAYNUV`. Operators can replace
`EFI/BOOT/BOOTX64.EFI` in place. This does **not** close E3b and does **not**
print `RAYNU-V-M7-HOST-NIC-HTTP-OK`.

**Runbook:** [`docs/runbooks/r640_cruzer_flash.md`](../../runbooks/r640_cruzer_flash.md)  
**Script:** `tools/flash-cruzer-esp.sh`

## Census (operator paste)

`lsusb` includes:

```
Bus 001 Device 002: ID 0781:5151 SanDisk Corp. Cruzer Micro Flash Drive
```

`lsblk` (names as of this boot):

| NAME | MODEL | SIZE | TRAN | Role |
|------|--------|------|------|------|
| sda | PERC H740P Mini | 200G | | Windows leftovers — **do not write** |
| sdb | PERC H740P Mini | 3.1T | | Ubuntu (`/boot/efi`, `/boot`, LVM `/`) — **do not write** |
| sdc | Cruzer Micro serial `200524441218e7503e33` | 977.5M | usb | Lab stick `LABEL=RAYNUV` |
| sdd | Virtual Floppy | 0B | usb | iDRAC — ignore |
| sr0 | Virtual CD | 1024M | usb | iDRAC — ignore |

`blkid`:

```
/dev/sdc: LABEL_FATBOOT="RAYNUV" LABEL="RAYNUV" UUID="5616-5E4B" BLOCK_SIZE="512" TYPE="vfat"
```

Whole-disk FAT (no `sdc1`). Operator mounted `/dev/sdc` at `/mnt/usb` and
created `test-file` (writable). Prefer `/dev/disk/by-label/RAYNUV` going
forward because `sdc` is not stable.

## Next

Flash the current inherit-PHY EFI (PR #162) with
`tools/flash-cruzer-esp.sh`, then one-time F11 boot the Cruzer. Keep Ubuntu
as the default PERC boot.
