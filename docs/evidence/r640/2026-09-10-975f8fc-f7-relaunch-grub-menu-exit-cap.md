# Iron COM2 — `975f8fc` F7 relaunch booted the installed disk's GRUB; exit-count cap fired inside the 2 s menu

- **Date:** 2026-09-10
- **Operator:** vikkp @ raynuvsrv1 (iDRAC `console com2`)
- **Platform:** Dell PowerEdge R640 (real iron)
- **Boot:** one-time F11 LogiLink UDisk (front USB 2)
- **EFI:** CI `--run 34474850361` (`975f8fc`; COM2 `build: sha=975f8fc2d756`)
- **Media:** Stage 46 product ISO `alpine-extended` (~994 MiB) from ESP; leftover-DRAM virtio install disk 1 GiB
- **Firmware:** RayNu-F (ADR-016)
- **Claims:** F7 relaunch on iron (`src=kbc` → relaunch → GPT ESP → GRUB from `vda`); `RAYNU-V-M7-ISO-INSTALL-OK` printed again (second iron install)
- **Does not claim:** `RAYNU-V-RAYNU-F-DISK-BOOT-OK` on iron · second Linux from disk · Everest E5 whole loop

## What moved

- `boot: guest-UEFI host stack top=0x142d000 pages=32 guard=0x140c000` — the
  `59ac070` fix is live.
- Install re-ran clean: apk 25 packages in ~0.5 s, `setup-disk -m sys -s 0
  /dev/vda`, `Installation finished. No error reported.`,
  `RAYNU-V-M7-ISO-INSTALL-OK`, `Installation is complete. Please reboot.`
- `reboot` → `[65.77] reboot: Restarting system` → **`RayNu-F guest reset
  requested src=kbc n=1`** (now visible: TX flush before share-off) →
  **`RayNu-F relaunch after reset (F7)`** — no `launch failed`.
- **`RayNu-F GPT ESP lba=2048 sectors=98304 part=1`** · `disk whole-disk
  path` · `found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk)` ·
  `VMLAUNCH entry=0xb23000 … image=DISK-BOOTX64`.
- The installed GRUB 2.12 ran on RayNu-F: `MEM-OK`, `BLOCKIO-OK`
  (`blk_rd=59`), `CONOUT-OK`, read `grub.cfg`, drew the menu
  `*Alpine Linux v3.21, with Linux lts` / `UEFI Firmware Settings`,
  `The highlighted entry will be executed automatically in 2s.` The menu
  was mirrored on COM1 because Alpine's grub.cfg adds
  `serial`/`terminal_input serial console` when `KERNELOPTS` has
  `console=ttyS0`.
- Harmless GRUB lines: `error: no suitable video mode found.` (gfxterm, no
  GOP) and `error: unable to determine partition UUID of boot device.`
  (GRUB `bli` module → our `SetVariable` returns `EFI_UNSUPPORTED`,
  `svc=RuntimeServices id=0x308 status=0x8000000000000003`).

## What did not

```
boot: RayNu-F stop exit-cap exits=1048577 svc=492778 svc_err=4 conout_ok=1 blk_rd=59 blk_wr=0 allocs=6 free_pages=62553 (F2b; not ISO-INSTALL-OK)
boot: guest-UEFI restore host xcr0=0x1 osxsave=0 reason=0x1e rip=0xffffffffad085991
boot: Stage 46 product ISO hold (not ISO-INSTALL-OK); not E4 SHELL
```

The countdown never reached 0. The RayNu-F firmware-phase runaway guard was
a fixed exit **count** (`RAYNU_F_EXIT_CAP = 1_048_576`), reset at relaunch.

## Root cause (GRUB source + the counters, not a guess)

`grub-core/normal/menu.c` `run_menu` loops `grub_getkey_noblock()` with no
`grub_cpu_idle`. With `terminal_input serial console` every iteration is one
`ReadKeyStroke` (RayNu-F service call — an I/O exit) plus one serial LSR
`inb 0x3fd` (an I/O exit): two exits per iteration. `svc=492778` ≈ half of
`exits=1048577` matches. On the R640 that loop runs at roughly 2 exits/µs,
so 1 M exits is well under a second — less than GRUB's own 2 s timeout. On
nested KVM each exit costs 10–50× more, so the same loop never approached
the cap and reboot-to-disk completed (`fe4785a`). GRUB's clock is not the
issue: no ACPI (pmtimer skipped), PIT channel 2 reads `0xff` on port `0x61`
so `grub_pit_wait` bails, and `grub_tsc_calibrate_from_efi` measures our
TSC-honest `Stall(1000)`.

A count is not a time bound.

## Fix pin (next green artifact of `cursor/pit-during-apk-b7a8`)

| Change | Where | COM2 tell |
|---|---|---|
| RayNu-F wall cap — loader phase bounded by `RAYNU_F_WALL_CAP_S = 180` s from `rdtsc` at launch/relaunch, sampled every 4096 exits | `vmx/guest_uefi.rs` `raynu_f_vmexit` | `stop wall-cap … wall_ms=` only if it trips |
| exit count kept only as a u32-wrap guard (`1 << 30`) | `vmx/guest_uefi.rs` | `stop exit-cap` should never print |
| `raynu_f_stop` summary prints `wall_ms=` | `vmx/guest_uefi.rs` | every stop line |

Expected on the next flash after the menu: ~2 s, GRUB `linux`/`initrd`
`ReadBlocks` bursts, `RAYNU-V-RAYNU-F-EBS-OK`, `RAYNU-V-RAYNU-F-DISK-BOOT-OK`,
second `Linux version 6.12.13-0-lts` with `root=UUID=`, `login:`.

## Serial excerpt (abridged, operator paste)

```text
build: sha=975f8fc2d756
boot: guest-UEFI host stack top=0x142d000 pages=32 guard=0x140c000 (guest-UEFI host stack guard; not ISO-INSTALL-OK)
...
RAYNU-V-M7-ISO-INSTALL-OK
...
Installation finished. No error reported.
...
Installation is complete. Please reboot.
localhost:~# reboot
[   65.770698] reboot: Restarting system
boot: RayNu-F guest reset requested src=kbc n=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F relaunch after reset (F7; not ISO-INSTALL-OK)
boot: RayNu-F BlockIo cd_bytes=1042284544 cd_last_lba=508927 disk_bytes=1073741824 disk_last_lba=2097151 (F4; not ISO-INSTALL-OK)
boot: RayNu-F GPT ESP lba=2048 sectors=98304 part=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F disk whole-disk path (F7; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F disk bootloader staged base=0xb22000 entry=0xb23000 size=139264 relocs=1851 (F7; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH entry=0xb23000 system_table=0x801000 relocs=2 image=DISK-BOOTX64 (F2b/F5; not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-MEM-OK
...
RAYNU-V-RAYNU-F-BLOCKIO-OK
...
RAYNU-V-RAYNU-F-CONOUT-OK
error: no suitable video mode found.
boot: RayNu-F svc=RuntimeServices id=0x308 status=0x8000000000000003 a1=0x11e64d40 a2=0x11e652e0 a3=0x6 a4=0x14 (ADR-016; not ISO-INSTALL-OK)
error: unable to determine partition UUID of boot device.
GNU GRUB  version 2.12
      Use the ? and ? keys to select which entry is highlighted.
      Press enter to boot the selected OS, `e' to edit the commands before booting or `c' for a command-line.
   *Alpine Linux v3.21, with Linux lts
    UEFI Firmware Settings
   The highlighted entry will be executed automatically in 2s.
boot: RayNu-F stop exit-cap exits=1048577 svc=492778 svc_err=4 conout_ok=1 blk_rd=59 blk_wr=0 allocs=6 free_pages=62553 (F2b; not ISO-INSTALL-OK)
boot: guest-UEFI restore host xcr0=0x1 osxsave=0 reason=0x1e rip=0xffffffffad085991
boot: Stage 46 product ISO hold (not ISO-INSTALL-OK); not E4 SHELL
```

## Close claim

Nothing new closes. E5 install-to-disk on iron stays closed (second
consecutive iron install, `59ac070` and `975f8fc`). E5 reboot-to-disk on
iron is now one guard away: the relaunch, the GPT ESP, and the installed
GRUB all ran on the R640; only the firmware-phase exit counter ended it. Do
not F11 `34474850361` again expecting `RAYNU-V-RAYNU-F-DISK-BOOT-OK`.

Host/CI never prints `RAYNU-V-M7-ISO-INSTALL-OK`.
