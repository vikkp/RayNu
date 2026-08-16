# R640 E5 — persist write + persist-detect, virtio prefix miss (2026-08-16)

**Claim:** On writable Cruzer Micro (front USB 2), PRE-EBS SPA Install wrote
`EFI/RayNu/installdisk.bin` (1 KiB). The **next** boot auto-armed
`persist-detect` without curl. Virtio launched at **64 MiB** with
`E5 install disk preload (reboot detect)`, then the blk probe HLT’d without
`DRIVER_OK` because `init_with_image` required `image.len() == disk_bytes`
and therefore **zeroed** the 64 MiB disk instead of copying the 1 KiB prefix.

**Also still true:** E2 `RAYNU-V-R640-BOOT-OK` (boot 1 only), E3
`RAYNU-V-M7-UEFI-HTTP-OK` (boot 1 SPA window), E4 SPA create + 64 MiB arm.

**Not claimed:** `RAYNU-V-M7-ISO-INSTALL-OK` / `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`
/ Mount Everest.

## Media / path

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-08-16 |
| Host | PowerEdge R640, iDRAC SOL `console com2` |
| Boot method | Disk connected to front USB 2: Cruzer Micro |
| Path | SNP residual `10.99.99.133:8443` (Tcp4 SB still 0) |
| Auth | bring-up token (`raynu-v-bringup`) |
| Persist file | `EFI/RayNu/installdisk.bin` bytes=1024 |

## Boot 1 (write)

Full excerpt: [`logs/2026-08-16-e5-persist-write-com2.txt`](logs/2026-08-16-e5-persist-write-com2.txt)

```text
boot: SNP lease 10.99.99.133/24 router=10.99.99.1
boot: CURL NOW → http://10.99.99.133:8443/
boot: SNP TCP accept — client connected
RAYNU-V-AUDIT: AuthAllowed method_tag=2
RAYNU-V-AUDIT: VmCreated guest_id=1
boot: E5 persist wrote installdisk.bin bytes=1024
RAYNU-V-M7-UEFI-HTTP-OK
boot: M4.3 virtio-blk … bytes=67108864
boot: E5 install-sized virtio-blk armed (PRE-EBS contract)
RAYNU-V-M7-ISO-DISK-WRITTEN
RAYNU-V-M7-ISO-INSTALL-LAB-OK
RAYNU-V-M7-ISO-REBOOT-PENDING
RAYNU-V-R640-BOOT-OK
```

## Boot 2 (detect, then fail)

Same stick, **not** re-imaged; SPA window timed out (correct — do not re-Install).
Full excerpt: [`logs/2026-08-16-e5-persist-detect-blk-fail-com2.txt`](logs/2026-08-16-e5-persist-detect-blk-fail-com2.txt)

```text
boot: E5 persist-detect armed (installdisk.bin)
boot: WARN — mgmt HTTP accept timeout (continuing to EBS)
boot: M4.3 virtio-blk … bytes=67108864
boot: E5 install disk preload (reboot detect)
boot: M4.3 — launching virtio-blk probe guest
boot: ERROR — blk probe HLT without DRIVER_OK readback
boot: boot gate failed
```

No `RAYNU-V-M4-BLK-OK`, no `BOOTED-FROM-DISK`, no `R640-BOOT-OK` on this boot.

## Root cause (code)

`devices/virtio_blk.rs` `init_with_image` copied the persist image only when
`img.len() == disk_bytes`. Iron persist is **1024** bytes; live disk is
**67108864**. The `_` branch zeroed 64 MiB. Reboot-detect then verified LBA0/LBA1
stamps, found zeros, left `BLK_OK` false, guest HLT’d.

Fix: copy `min(image, disk)` at offset 0 and zero the rest. Serial:
`boot: E5 persist preload bytes=1024 prefix_into=67108864`.

## Honesty / residuals

- Persist **write** and **detect** on writable USB are proven.
- Reboot-to-disk **verify** is not, until the prefix-copy EFI is on the same stick
  (keep `installdisk.bin`; replace `EFI/BOOT/BOOTX64.EFI` only).
- Lab stamps ≠ guest filesystem installer. `STATUS-iso-install` stays **open**.
- Do not print iron `ISO-INSTALL-OK` from host/CI.

## HDA / Everest

- E5 iron progress: persist two-boot detect on COM2.
- Close marker still requires `BOOTED-FROM-DISK` (then evidence → `ISO-INSTALL-OK`).
- Mount Everest still open.
