# Iron COM2 — `f72b4276` Phase B closed on the real R640: coexist HTTP-OK → SPA Start of RayNu-F ISO → `ISO-INSTALL-OK` → `DISK-BOOT-OK`

- **Date:** 2026-09-11
- **Operator:** vikkp @ raynuvsrv1 (iDRAC `console com2`)
- **Platform:** Dell PowerEdge R640 (real iron)
- **Boot:** one-time F11 LogiLink UDisk (front USB 2)
- **EFI:** CI `--run 34552377351` (`f72b4276`; COM2 `build: sha=f72b4276d198`)
- **Media:** Stage 46 product ISO `alpine-extended` (~994 MiB) from ESP; leftover-DRAM virtio install disk 1 GiB (`vda`, `hpa=0x140800000`)
- **Firmware:** RayNu-F (ADR-016). **No** ESP `raynuf.txt` / no `--raynu-f` — this is the operator SPA path, not Phase A auto-launch.
- **Claims:** **P0-63 / Phase B closed on iron.** Coexist native BCM5720 HTTP on `10.99.99.145:8443` → SPA/REST create-VM + start of a typed product ISO → RayNu-F boots `\EFI\BOOT\BOOTX64.EFI` from the ISO → Alpine 3.21 live → `setup-disk -m sys` to virtio-blk `vda` → **`RAYNU-V-M7-ISO-INSTALL-OK`** → guest `reboot` → F7 relaunch → installed GRUB 2.12 → **`RAYNU-V-RAYNU-F-DISK-BOOT-OK`** → second Linux `root=UUID=814a97a0-d3be-4a64-8611-b3dbd9859201` from ext4 `vda2` → `login:` → `cat /proc/cmdline` from the **installed** system.
- **Does not claim:** TLS (deferred ADR-009); leftover-DRAM disk surviving a **host** reboot of RayNu-V; guest VNC console UI; multi-distro; 100% product polish. Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`.

## Why this pin (TSC Instant)

Iron `7f8dc0a9` / `34548550755` printed SKIP-OVMF-OK + E4-CONTINUE-OK + `HOST-NIC coexist listening on 10.99.99.146:8443`, then Mac `curl: (7)` in ~1 s. Tight coexist idle called `tick` at CPU speed while `MILLIS += 10` per call, so smoltcp ARP/TCP timers expired. `f72b4276` drives coexist Instant from TSC (`coexist_millis_from_tsc`). This COM2 is that pin working.

## The loop, in COM2 order

1. `build: sha=f72b4276d198` · PRE-EBS SNP `10.99.99.145:8443` 45 s accept timeout (expected; operator did not curl PRE-EBS; firmware SNP is dead after EBS).
2. `RAYNU-V-M7-PHASE-B-SKIP-OVMF-OK` · `skip OVMF — product ISO on CD; wait for SPA Start`.
3. `RAYNU-V-M7-PHASE-B-E4-CONTINUE-OK` · `Stage 46 hold skipped — Phase B coexist idle, no G0 bzImage`.
4. `HOST-NIC BCM5720 reuse` · `HOST-NIC coexist listening on 10.99.99.145:8443` · lease **`.145`** this boot (was `.146` on `7f8dc0a9`). LOM MAC `b0:26:28:5c:5a:38`.
5. RX `to=us` IPv4 + ARP · TX ARP/IP · `HOST-NIC TCP accept` · `HOST-NIC HTTP exchange ok` · **`RAYNU-V-M7-HOST-NIC-HTTP-OK`** (GET, then spec, then start).
6. `RAYNU-V-AUDIT: AuthAllowed method_tag=2` · `VmCreated guest_id=4` · `VmStarted guest_id=4`.
7. **`boot: E4 SPA start — RayNu-F product ISO (Phase B; not SHELL; not ISO-INSTALL-OK)`**.
8. ISO `image=ISO-BOOTX64` · `RAYNU-V-RAYNU-F-START-IMAGE-OK` · `RAYNU-V-RAYNU-F-EBS-OK` · `Booting 'Linux lts'` · Alpine 3.21 live (`modules=loop,squashfs,virtio_pci,virtio_blk`) → `login:` → auto `setup-disk -m sys -s 0 /dev/vda`.
9. **`RAYNU-V-M7-ISO-INSTALL-OK`** (iron-only firmware path) · `Installation finished. No error reported.` · GRUB config · `Installation is complete. Please reboot.`
10. `reboot` → `[66.55] reboot: Restarting system` → `RayNu-F guest reset requested src=kbc n=1` → `RayNu-F relaunch after reset (F7)`.
11. `RayNu-F GPT ESP lba=2048 sectors=98304 part=1` · `image=DISK-BOOTX64` · GRUB 2.12 countdown `2s → 1s → 0s` · `Booting 'Alpine Linux v3.21, with Linux lts'`.
12. `RAYNU-V-RAYNU-F-START-IMAGE-OK` → **`RAYNU-V-RAYNU-F-DISK-BOOT-OK`** → `RAYNU-V-RAYNU-F-EBS-OK`.
13. Second kernel from the disk:

```
[    0.000000] Linux version 6.12.13-0-lts …
[    0.000000] Command line: BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=814a97a0-d3be-4a64-8611-b3dbd9859201 ro modules=sd-mod,usb-storage,ext4 console=ttyS0 … rootfstype=ext4
[    0.000000] efi: EFI v2.10 by RayNu-F
…
[    1.603872] virtio_blk virtio0: [vda] 2097152 512-byte logical blocks (1.07 GB/1.00 GiB)
[    1.606999]  vda: vda1 vda2
[    3.118674] EXT4-fs (vda2): mounted filesystem 814a97a0-d3be-4a64-8611-b3dbd9859201 ro with ordered data mode.
[    3.120997] Mounting root: ok.
Welcome to Alpine Linux 3.21
localhost login: root
localhost:~# cat /proc/cmdline; mount | head -3
BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=814a97a0-d3be-4a64-8611-b3dbd9859201 ro modules=sd-mod,usb-storage,ext4 … rootfstype=ext4
```

The installed system's `/proc/cmdline` carries `root=UUID=814a97a0-…` — the
GRUB config written by `setup-disk` on the first boot — and `fsck` ran on both
`vda2` (ext4 root) and `vda1` (FAT ESP). This is the same E5 loop as
`56a3ffd`, started from the **SPA**, not `--raynu-f`.

## Harmless / expected lines

- Dual-UART `?` vs `—` on COM1+COM2 mirror (not a regression).
- PRE-EBS SNP accept timeout (operator window unused).
- GRUB `error: no suitable video mode found.` / `unable to determine partition UUID of boot device.`
- Kernel NMI selftest `local IPI:TIMEOUT`.
- First-boot `/dev/vdb1: Can't open blockdev`.
- OpenRC `clock skew detected` (RTC epoch `2026-08-16T12:00:00`).
- Diagnostic `guest-UEFI virtio MMIO` / `virtio stall dump` after login (cosmetic).

## Residual after this close

- Leftover-DRAM install disk does not survive a **host** reboot of RayNu-V.
- TLS deferred (ADR-009 plaintext HTTP). Guest console UI thin. ISO blob upload / UEFI catalog persist / multi-distro are post-Everest polish.
- Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`.

## Close claim

`RAYNU-V-M7-HOST-NIC-HTTP-OK` on Phase B coexist idle, SPA Start of RayNu-F
(no `raynuf.txt`), iron `RAYNU-V-M7-ISO-INSTALL-OK`, and
`RAYNU-V-RAYNU-F-DISK-BOOT-OK` with a second Linux from `vda` (`root=UUID=`,
ext4 `vda2`, `login:`) are **proven on the real R640** with EFI `f72b4276`
(run `34552377351`). Mount Everest product loop (EFI → R640 → network UI →
Linux ISO via SPA) is closed on iron. Do not F11 `34548550755` / `7f8dc0a9`
or earlier Phase B fails; this pin is the Phase B reference.
