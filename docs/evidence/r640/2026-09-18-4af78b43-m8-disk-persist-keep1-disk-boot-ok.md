# Iron COM2 — `4af78b43` M8.0 A2 CLOSED on evidence: Force Off keep=1 → SPA DISK-BOOT → same UUID

- **Date:** 2026-09-18
- **Operator:** vikkp @ raynuvsrv1 (iDRAC `console com2`)
- **Platform:** Dell PowerEdge R640 (real iron)
- **Boot:** F11 existing LogiLink UDisk (front USB 2) — **not** a new flash
- **EFI:** `build: sha=4af78b4303fd` (skip GPT array CRC + `xhci diskprime`)
- **LUN:** Toshiba `0480:a004` ~298 GiB on xHCI p11; guest virtio capped at **8 GiB**
- **Claims:** **A2 persist across HV reboot CLOSED on evidence.** After Force Off of RayNu-V (not guest F7): peek `keep=1` `installed=1` → coexist HTTP → SPA Start → `xhci diskprime` → `GPT ESP lba=2048` `BOOTX64.EFI bytes=139264` `image=DISK-BOOTX64` → GNU GRUB 2.12 → `RAYNU-V-RAYNU-F-DISK-BOOT-OK` → second Linux `root=UUID=348005a9-25b7-4027-bc95-af9aef244c3a` on `[vda] 8.00 GiB` → `EXT4-fs (vda2): mounted filesystem 348005a9-…` → `login: root`. Same UUID as the guest8g F7 install on `b661808c`.
- **Does not claim:** minted `RAYNU-V-M8-DISK-PERSIST-OK` on COM2 (const-only; never serial-wired). Whole-Toshiba virtio. TLS. PERC. Nested QEMU. Host/CI never print persist-OK or `ISO-INSTALL-OK`.

## Why this is Force Off persist (not guest F7)

Guest F7 on `b661808c` already reached `DISK-BOOT-OK` + `login: root` `UUID=348005a9-…`. That is ADR-017 leftover-in-guest. This COM2 is a **hypervisor** reboot: peek `keep=1` before SPA, then the installed GRUB from USB LUN, same UUID.

Prior A2 SPA on `b661808c` launched `image=ISO-BOOTX64` (724992, El Torito lba=125) then `vda` I/O error. Skip-CRC + `xhci diskprime` is why this boot preferred `DISK-BOOTX64`.

## The loop, in COM2 order

1. `Booting from Disk connected to front USB 2: UDisk` · `build: sha=4af78b4303fd`.
2. Toshiba `usb I/O ready bytes=320072933376` · leftover skip · peek `efi=EFI PART gpt=1 fit=1 gpt_err=0 usb_err=0 guest=8589934592 bootx64=1 ext4=1 installed=1` · virtio `bytes=8589934592 keep=1 (durable LUN usb)`.
3. Coexist `HOST-NIC-HTTP-OK` `10.99.99.146:8443` · SPA create/start `guest_id=1`.
4. `E4 SPA start — RayNu-F product ISO` · `xhci diskprime` · `GPT ESP lba=2048` · `BOOTX64.EFI bytes=139264` · `image=DISK-BOOTX64`.
5. GNU GRUB 2.12 `*Alpine Linux v3.21, with Linux lts` · `RAYNU-V-RAYNU-F-DISK-BOOT-OK`.
6. Linux `root=UUID=348005a9-25b7-4027-bc95-af9aef244c3a` · `[vda] 8.00 GiB` `vda1 vda2` · `EXT4-fs (vda2): mounted filesystem 348005a9-…` · `login: root`.
7. Spurious `RAYNU-V-M7-ISO-INSTALL-OK` on journal recovery `wr=1` — **not** a new install.
8. Serial auto-answer `setup-disk` then `No disks found` — did **not** wipe.

## Residuals (why not 100%)

- Minted `RAYNU-V-M8-DISK-PERSIST-OK` never serial-printed.
- Guest virtio is the 8 GiB slice, not the whole Toshiba.
- Auto-answer still fires `setup-disk` after keep=1 disk-boot login.
- TLS / console / PERC remain M8.1+.

## Operator

Sit at `localhost:~#`. Do **not** `setup-disk`. Do not Force Off. Do not flash Toshiba `/dev/sdc`. Optional: `cat /proc/cmdline` to confirm `UUID=348005a9-…`.
