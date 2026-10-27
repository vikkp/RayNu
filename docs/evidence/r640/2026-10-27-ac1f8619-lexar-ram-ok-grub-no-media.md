# 2026-10-27 — `ac1f8619`: report-RAM fix lived; GRUB had no disk media

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-10-27 |
| Operator | vikkp |
| Host (service tag / hostname) | D0324Y2 / R640 lab |
| Boot method | USB (F11 one-shot, front USB 2, Lexar 116.1G `USB Flash Drive`) |
| EFI path on media | `\EFI\BOOT\BOOTX64.EFI` |
| `r640-hypervisor.efi` SHA256 | `2f555b36a0b2861490fa2c7fd6bef80ddb8f837f62b6905b7bf56d366f999450` |
| EFI source | `ac1f8619` (`cursor/m8-9-close-50bb`, PR #414) · CI `--run 40424711990` · COM2 `build: sha=ac1f8619269f` |
| ESP contents | `EFI/BOOT/BOOTX64.EFI`, `EFI/RayNu/OVMF.fd`, `EFI/RayNu/installdisk.bin`; `find -iname '*.iso'` empty |
| Serial channel | iDRAC SOL `console com2` |

## What this boot was for

Second F11 of the wave 1 D3 thread on the bare Lexar. The previous boot
(`250dc961`, [2026-10-26-250dc961-lexar-no-iso-grub-abort.md](2026-10-26-250dc961-lexar-no-iso-grub-abort.md))
ran GRUB out of memory because report-RAM keyed off an ESP ISO. This EFI
arms report-RAM on the PERC image latch too.

## What lived

Every memory line the fix promised printed, with no ISO on the stick:

```text
boot: Stage 46 no product ISO on ESP — lab El Torito stub
boot: frame pool product-ISO iron [1MiB,512MiB); Stage 46 hold (not E4 BAR/shell)
boot: frame pool phys=0x1000000 pages=126976
boot: perc image latched — HTTPS will wait for SPA boot or reinstall (not PERC-BOOT-OK)
boot: conventional above PRECISE pages=16441135
boot: report-RAM extra hpa=0x140a00000 bytes=2147483648 (Stage 46; not ISO-INSTALL-OK)
RAYNU-V-M1-EBS-OK
RAYNU-V-AUDIT: GuestNicSaved off=19327352832 mode=2
RAYNU-V-AUDIT: PercSpaChoice choice=4
boot: guest-UEFI report-RAM pool=1008 extra=814 no-zero
boot: guest-UEFI report-RAM EPT pre-map n=1008
boot: RayNu-F high RAM base=0x2000000 bytes=2113929216 tail=1845493760 premapped=2113929216 (F6-prep; not ISO-INSTALL-OK)
```

Both PHYs `bmsr=796d` again. SNP lease and standing SPA on
`10.99.99.146:8443`. GRUB staged, `RAYNU-V-RAYNU-F-MEM-OK`, AllocatePool
and GetMemoryMap all `status=0x0`; no OOM.

## What stopped

```text
boot: guest-UEFI CD GuestVisible iso=1 bytes=73728
boot: RayNu-F BlockIo cd_bytes=0 cd_last_lba=0 disk_bytes=0 disk_last_lba=0 (F4; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\ubuntu\grubx64.efi bytes=2828168 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F grubcfg body=search.fs_uuid 6ded2137-de53-4881-93df-8bbdeb2b20ec root  set prefix=($root)'/grub' configfile $prefix/grub.cfg
boot: RayNu-F blk fail st=0x800000000000000c bytes=512 lba=0 (not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-CONOUT-OK
GNU GRUB  version 2.14
   Minimal BASH-like line editing is supported. ...
grub>
```

`0xc` is `EFI_NO_MEDIA`. There is no `boot: Stage 46 virtio-blk install
disk bytes=… tail_off=…` line anywhere on this COM2: the 10 GiB tail was
never attached, so the disk BlockIo published zero media. GRUB's stub
`search.fs_uuid` could not read the disk, `$root` stayed unset, and
`configfile` dropped to the shell.

## Diagnosis

`attach_product_iso_install_disk` returns before doing anything unless
`ide_cdrom::product_iso_window_armed()`, and that window was armed only by
presenting a window-sized ISO. The same predicate is the master switch for
the whole product guest path (about sixty call sites: interrupt injection
`INJECT_SKIP_UNARMED`, PIT raising, product UART, Linux earlycon share,
virtio drain, the resume cap). Every lived Ubuntu-tail boot had an ISO on
the UDisk by accident.

Fix on `cursor/m8-9-close-50bb`: `product_iso_window_armed()` is now
`cd_len > GUEST_CD_ISO_CAP || perc_image_boot_latched()`. Host and nested
never latch, so the lab and nested `iso=0` paths are unchanged. The COM2
line after `CD GuestVisible` says `product guest from the PERC image (no
ESP ISO; lab stub CD)` when the CD is only the stub.

Expected on the next F11, after `PercSpaChoice choice=4`:
`boot: guest-UEFI product guest from the PERC image …`, `boot: Stage 46
virtio-blk install disk bytes=10737418240 keep=0 tail_off=19327352832
(free tail; …)`, `RayNu-F BlockIo … disk_bytes=10737418240
disk_last_lba=20971519`, GRUB into the Ubuntu kernel, then the first
`wire=` line.

## What this is

- Report-RAM without an ESP ISO is lived on iron (`pool=1008`, `pre-map
  n=1008`, `high RAM … premapped=2113929216`).
- A second product bug named: the install-disk attach and the product guest
  path depended on the USB stick.

## What this is not

- Not D3. No guest frame left `:39`. No `wire=` line.
- Not `RAYNU-V-M7-ISO-INSTALL-OK`, not `PERC-LUN-OK`. No disk formatted.
