# Nested F7 reboot-to-disk (`fe4785a`) — not ISO-INSTALL-OK

- **Host:** `raynuvsrv1` nested QEMU (KVM nested VT-x)
- **Commit:** `fe4785a` (`feat(raynu-f): F7 HANDLE_DISK whole-disk Vendor path (GRUB hd0)`)
- **Harness:** `TIMEOUT_SECS=1800 ./tools/e5-product-iso-qemu-serial.sh`
- **QEMU:** exit 124 (1800s timeout; expected)
- **Honesty:** Nested QEMU ≠ R640. Do **not** print `RAYNU-V-M7-ISO-INSTALL-OK`. Iron E5 stays open (Cruzer 977.5 MiB cannot hold alpine-extended).

## What ran

1. Alpine-extended ISO under RayNu-F; leftover virtio-blk `bytes=536870912`.
2. `setup-disk -m sys -s 0 /dev/vda` → `Installation is complete. Please reboot.`
3. F7 relaunch: GPT ESP `lba=2048 sectors=98304 part=1`, `disk whole-disk path`, `image=DISK-BOOTX64`.
4. `RAYNU-V-RAYNU-F-DISK-BOOT-OK` then EFI stub initrd + `RAYNU-V-RAYNU-F-EBS-OK`.
5. Second `Linux version 6.12.13-0-lts` with `root=UUID=698a922a-3a7d-45ea-9da3-2b11403af82a` (not modloop).
6. OpenRC: `/dev/vda2: clean` (ext4), `fsck.fat` `/dev/vda1`.
7. Second `localhost:~# cat /proc/cmdline; mount | head -3` showed the UUID root.

Harness printed: `nested reboot-to-disk reached a second Linux boot (not ISO-INSTALL-OK)`.

Prior nested `3492ebc` stopped at GRUB rescue `disk `,gpt2' not found` (Media/HardDrive node on whole-disk `HANDLE_DISK`).
