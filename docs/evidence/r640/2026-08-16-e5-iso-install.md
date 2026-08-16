# R640 E5 — iron reboot-to-disk (LBA stamp persist) 2026-08-16

**Claim (M7.7 / E5 stamp contract):** Writable Cruzer Micro two-boot:
PRE-EBS SPA Install persisted `installdisk.bin` (1 KiB); next boot
`persist-detect` + prefix-copy into 64 MiB virtio → `DRIVER_OK` verified
stamps → firmware printed `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`.
E2 still green (`RAYNU-V-R640-BOOT-OK`).

**Documented equivalent of** `RAYNU-V-M7-ISO-INSTALL-OK` (firmware prints
`BOOTED-FROM-DISK` on detect; host/CI never prints the iron OK marker).

**Not claimed:** Mount Everest; guest filesystem installer; El Torito / distro ISO
blob; firmware Tcp4 listen; post-EBS durable mgmt HTTP (next residual).

## Closing tip

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-08-16 |
| Platform | PowerEdge R640 |
| Boot method | Front USB 2: Cruzer Micro (same stick, EFI replaced only) |
| EFI SHA256 (Mac) | `2d931dcdada32cbd0e23f542513d78de427f087d2c6869a6b910ae6cf4281eed` |
| Branch / tip | `cursor/e5-persist-esp-a623` prefix-copy (`2875103`) |
| Lease | `10.99.99.133:8443` (SNP residual; Tcp4 SB still 0) |
| Install disk | 67108864 bytes (from `installsize.txt`) |
| Persist file | `EFI/RayNu/installdisk.bin` 1024 bytes |
| Extract-boot | PE `.askern` / `.asinit` |

## Checklist

- [x] Deploy / install REST (SPA Install, boot 1) issued launch contract
- [x] Guest extract-boot progressed (Linux SHELL)
- [x] Virtio-blk write proof (boot 1 `DISK-WRITTEN` / LBA stamps)
- [x] Reboot-to-disk / second boot from persist file
- [x] Serial shows `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` (documented equivalent)

## Boot 1 (write) — prior paste

[`logs/2026-08-16-e5-persist-write-com2.txt`](logs/2026-08-16-e5-persist-write-com2.txt)

`persist wrote installdisk.bin bytes=1024` → `bytes=67108864` → `REBOOT-PENDING`

## Boot 2 (detect) — this close

Full excerpt: [`logs/2026-08-16-e5-booted-from-disk-com2.txt`](logs/2026-08-16-e5-booted-from-disk-com2.txt)

Prefix-miss residual (same stick, pre-fix EFI):
[`2026-08-16-e5-persist-detect-blk-fail.md`](2026-08-16-e5-persist-detect-blk-fail.md)

```text
boot: E5 persist-detect armed (installdisk.bin)
boot: WARN — mgmt HTTP accept timeout (continuing to EBS)
boot: E5 persist preload bytes=1024 prefix_into=67108864
boot: M4.3 virtio-blk … bytes=67108864
boot: E5 install disk preload (reboot detect)
RAYNU-V-M4-BLK-OK
RAYNU-V-M7-ISO-BOOTED-FROM-DISK
RAYNU-V-M4-NET-OK
RAYNU-V-M4-SMP-OK
RAYNU-V-R640-BOOT-OK
```

## Honesty / residuals

- Persist is **LBA0+LBA1 stamps** (1 KiB), not a guest root filesystem and not a
  distro installer. Live 64 MiB virtio is still RAM; durability is the ESP file.
- SPA window timed out on boot 2 (correct — do not re-Install).
- El Torito / ISO blob upload / guest console / TLS remain **after** post-EBS listen.
- Mount Everest stays **open** (post-EBS mgmt HTTP first; then E4 polish + real distro installer).

## Close

`docs/evidence/r640/STATUS-iso-install` → `STATUS=closed`  
`GAP(CLOSED M7.7)` in `mgmt/iso_install.rs`.
