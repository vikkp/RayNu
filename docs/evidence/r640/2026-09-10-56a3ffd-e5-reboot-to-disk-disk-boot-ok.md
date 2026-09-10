# Iron COM2 — `56a3ffd` E5 reboot-to-disk on the real R640: `RAYNU-V-RAYNU-F-DISK-BOOT-OK`, second Linux from `vda`, `login:`

- **Date:** 2026-09-10
- **Operator:** vikkp @ raynuvsrv1 (iDRAC `console com2`)
- **Platform:** Dell PowerEdge R640 (real iron)
- **Boot:** one-time F11 LogiLink UDisk (front USB 2)
- **EFI:** CI `--run 34480107961` (`56a3ffd`; COM2 `build: sha=56a3ffda8406`)
- **Media:** Stage 46 product ISO `alpine-extended` (~994 MiB) from ESP; leftover-DRAM virtio install disk 1 GiB (`vda`, `hpa=0x140800000`)
- **Firmware:** RayNu-F (ADR-016; own EFI system table + boot services, no OVMF)
- **Claims:** **E5 Phase A whole loop on iron** — Alpine 3.21 ISO → UEFI installer under RayNu-F → `setup-disk -m sys` to virtio-blk `vda` → `RAYNU-V-M7-ISO-INSTALL-OK` → guest `reboot` → F7 relaunch → installed GRUB 2.12 → **`RAYNU-V-RAYNU-F-DISK-BOOT-OK`** → second `Linux version 6.12.13-0-lts` with `root=UUID=9af18543-…` → ext4 `/dev/vda2` mounted → OpenRC → `login:` → `cat /proc/cmdline` from the installed system
- **Does not claim:** Everest closed (Phase B — SPA/REST registers the ISO, creates the VM, attaches media and starts *this* disk — is still the SHELL stub `2b795a0`); TLS; guest console UI; persistence of the leftover-DRAM disk across a **host** reboot; multi-distro

## The loop, in COM2 order

1. `build: sha=56a3ffda8406` (second COM2 line) · `guest-UEFI host stack top=0x142d000 pages=32 guard=0x140c000`.
2. First boot from the ISO (RayNu-F direct, OVMF leg bypassed at first exit): `image=ISO-BOOTX64` → `RAYNU-V-RAYNU-F-EBS-OK` → `Linux version 6.12.13-0-lts` (`modules=loop,squashfs,virtio_pci,virtio_blk`) → `Mounting boot media: ok.` → `Installing packages to root filesystem` 25 packages in ~0.5 s → `login:` → auto-answer `setup-disk -m sys -s 0 /dev/vda`.
3. `Installation finished. No error reported.` → **`RAYNU-V-M7-ISO-INSTALL-OK`** (iron-only firmware path) → `initramfs: creating /boot/initramfs-lts` → `Generating grub configuration file … done` → `Installation is complete. Please reboot.`
4. `reboot` → `[65.37] reboot: Restarting system` → **`RayNu-F guest reset requested src=kbc n=1`** → **`RayNu-F relaunch after reset (F7)`**.
5. **`RayNu-F GPT ESP lba=2048 sectors=98304 part=1`** · `disk whole-disk path` · `found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk)` · `disk bootloader staged base=0xb22000 entry=0xb23000` · `VMLAUNCH entry=0xb23000 … image=DISK-BOOTX64`.
6. Installed GRUB 2.12 on RayNu-F: `MEM-OK` · `BLOCKIO-OK` · `CONOUT-OK` · menu `*Alpine Linux v3.21, with Linux lts` / `UEFI Firmware Settings` · **`executed automatically in 2s` → `1s` → `0s`** (the countdown survived — the `975f8fc` exit-cap is gone; the wall cap never fired) · `Booting 'Alpine Linux v3.21, with Linux lts'` · `Loading Linux lts ...` · `Loading initial ramdisk ...`.
7. `RayNu-F StartImage entry=0x2b09515` → `RAYNU-V-RAYNU-F-START-IMAGE-OK` → **`RAYNU-V-RAYNU-F-DISK-BOOT-OK`** → `EFI stub: Loaded initrd from LINUX_EFI_INITRD_MEDIA_GUID device path` → **`RAYNU-V-RAYNU-F-EBS-OK`**.
8. **Second kernel from the disk:**

```
[    0.000000] Linux version 6.12.13-0-lts (buildozer@build-3-21-x86_64) …
[    0.000000] Command line: BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=9af18543-bbaf-475e-925a-60ab53d49847 ro modules=sd-mod,usb-storage,ext4 console=ttyS0 earlycon=uart8250,io,0x3f8 lpj=4194304 no_timer_check tsc=reliable clocksource=tsc idle=poll initcall_blacklist=piix_init efi=noruntime rootfstype=ext4
[    0.000000] efi: EFI v2.10 by RayNu-F
…
[    1.601013] virtio_blk virtio0: [vda] 2097152 512-byte logical blocks (1.07 GB/1.00 GiB)
[    1.604194]  vda: vda1 vda2
[    3.095798] EXT4-fs (vda2): orphan cleanup on readonly fs
[    3.096813] EXT4-fs (vda2): mounted filesystem 9af18543-bbaf-475e-925a-60ab53d49847 ro with ordered data mode. Quota mode: none.
[    3.099045] Mounting root: ok.
   OpenRC 0.55.1 is starting up Linux 6.12.13-0-lts (x86_64)
 * Checking local filesystems  ...
/dev/vda2: clean, 6377/62336 files, 52818/249344 blocks
fsck.fat 4.2 (2021-01-31)
/dev/vda1: 5 files, 548/96760 clusters
 * Remounting root filesystem read/write ... [ ok ]
Welcome to Alpine Linux 3.21
Kernel 6.12.13-0-lts on an x86_64 (/dev/ttyS0)
localhost login: root
localhost:~# cat /proc/cmdline; mount | head -3
BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=9af18543-bbaf-475e-925a-60ab53d49847 ro modules=sd-mod,usb-storage,ext4 console=ttyS0 … rootfstype=ext4
```

The installed system's `/proc/cmdline` carries `root=UUID=9af18543-…` — the
GRUB config written by `setup-disk` on the first boot — and `fsck` ran on both
`vda2` (ext4 root) and `vda1` (FAT ESP). The shell at the end is the installed
Alpine, not the ISO's live overlay (`modules=sd-mod,usb-storage,ext4`, no
`squashfs`, no `apks` repository step).

## What changed between `975f8fc` and this pin

Only the RayNu-F firmware-phase runaway guard. `975f8fc` stopped at
`stop exit-cap exits=1048577 svc=492778` inside GRUB's menu poll loop
(~2 exits/µs on the R640; a fixed exit count is not a time bound). `56a3ffd`
bounds the loader phase by **wall time** (`RAYNU_F_WALL_CAP_S = 180` s from
`rdtsc` at launch/relaunch, sampled every 4096 exits) and keeps the exit count
only as a u32-wrap guard (`1 << 30`). No `RayNu-F stop` line appears anywhere
in this log.

## Harmless lines seen (same as `975f8fc`)

- GRUB `error: no suitable video mode found.` (gfxterm, no GOP).
- GRUB `error: unable to determine partition UUID of boot device.` (`bli` module → our `SetVariable` returns `EFI_UNSUPPORTED`, `svc=RuntimeServices id=0x308 status=0x8000000000000003`).
- Kernel NMI selftest `local IPI:TIMEOUT` / `BUG: 1 unexpected failures` (single vCPU, no self-IPI delivery under `no_timer_check`; present on every boot since the first iron Linux).
- First-boot `/dev/vdb1: Can't open blockdev` (Alpine init probing the ISO's first partition before `vdb2`).

## Residual after this close

- **Phase B (operator product / E4):** the SPA/REST create-VM + attach-media path on iron still launches the SHELL CPUID stub (`2b795a0`). Everest is closed when the SPA starts *this* installed disk.
- The leftover-DRAM install disk does not survive a **host** reboot of RayNu-V (persistence to ESP/NVMe is post-Everest).
- Diagnostic COM2 noise (`guest-UEFI virtio MMIO` heartbeats, idle `virtio stall dump`) is now cosmetic and should be demoted.
- TLS (deferred, ADR-009), guest console UI, live Redfish — unchanged.

## Close claim

`RAYNU-V-RAYNU-F-DISK-BOOT-OK` and a second Linux booted from `vda`
(`root=UUID=`, ext4 `vda2`, `login:`) are **proven on the real R640** with
EFI `56a3ffd` (run `34480107961`). Together with `59ac070` / `975f8fc` /
this run's `RAYNU-V-M7-ISO-INSTALL-OK`, Everest **E5** (Linux ISO deploy:
ISO → UEFI installer → virtio-blk → reboot to disk) is closed on iron.
Host/CI never prints `RAYNU-V-M7-ISO-INSTALL-OK`.
