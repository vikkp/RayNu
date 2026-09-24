# 2026-09-24 — `e5cca2e0` boot 2: installed GRUB, then `grub>`, `blk_rd=33`

EFI `e5cca2e0d7a8`. Same stick as the soak, with `EFI/RayNu/usbsoak.txt`
removed and `raynuf.txt` left in place. Lease `10.99.99.151/24`. No
`USB soak requested`.

## What printed

Toshiba `0480:a004` enumerated. Peek `efi=EFI PART gpt=1 fit=1 installed=1
guest=8589934592`. Virtio attached `bytes=8589934592 keep=1`,
`disk_last_lba=16777215`. `diskprime lba1=pin warm=1` ran (INQUIRY + READ
of LBA 0, which the pin already holds). RayNu-F staged `BOOTX64.EFI`
`bytes=139264` from ESP LBA 2048 (`sectors=98304`) and printed
`image=DISK-BOOTX64`. Then `diskprime past pin`, GRUB 2.12, and the normal
prompt `grub>`.

```
RayNu-F stop wall-cap exits=301551616 wall_ms=180005 svc=301551566 svc_err=1 conout_ok=1 blk_rd=33 blk_wr=0 allocs=6 free_pages=62553
Stage 46 product ISO hold
Stage 46 hold alive t_s=60
```

`blk_rd` counts one successful `ReadBlocks` call. 33 is the same count as
`d60431ee`, the boot where the RAM pin served LBA 0–33 and GRUB never
completed a sector past it. `blk_wr=0`. `svc_err=1` is one protocol status
after the opening sequence. The hold does not boot Alpine.

## What it means

The soak on this EFI already proved single-sector USB reads, including
LBA 2048, across 120 s of idle. This boot proved the product path can
stage the installed GRUB on the 8 GiB slice. The function named “past pin”
then USB-read LBA 0 again, inside GRUB’s first read outside the pin.
GRUB stopped at `grub>`.

A 1 GiB disk (`[vda] 2097152`) did not appear. The ISO was not staged.
The Toshiba was not written.

## Not this

Not `login:`. Not `RAYNU-V-M8-DISK-PERSIST-OK`. Do not F11 `e5cca2e0`
again expecting the menu. Force Off the hold if it is still up.
