# 2026-09-24 — `c4a41a17`: installed GRUB, then `grub>`

EFI `c4a41a174f81` (`cursor/m8-unpin-read-8366`). `usbsoak.txt` stayed off.
`raynuf.txt` stayed. Lease `10.99.99.149/24`. No `USB soak requested`. No
`diskprime past pin`.

## What printed

Toshiba `0480:a004` enumerated. Peek `efi=EFI PART gpt=1 fit=1 installed=1
guest=8589934592`. Virtio attached the 8 GiB slice (`disk_last_lba=16777215`).
`diskprime lba1=pin warm=1` ran twice (INQUIRY + READ of LBA 0, which the pin
already holds). RayNu-F found the ESP at LBA 2048 (`sectors=98304`) and printed:

```
boot: Stage 46 disk past pin lba=2048 n=512 ok (not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk; not ISO-INSTALL-OK)
image=DISK-BOOTX64
```

Then GRUB 2.12 and the normal prompt `grub>`. The operator Force Off at the
prompt. There is no wall-cap line and no `blk_rd` for this boot.

## What it means

The one-shot line is the firmware reading the ESP boot sector (the FAT BPB)
while it stages `BOOTX64.EFI`. That read is past the GPT pin, so it consumed
the only COM2 line this EFI prints for an unpinned read. GRUB’s own
`BlockIo.ReadBlocks` calls came after that line. `ok` plus a parsed BPB plus
a 139264-byte file means the firmware FAT walk of the installed ESP worked.
It does not say whether `\EFI\BOOT\grub.cfg` or `\EFI\alpine\grub.cfg` is on
that volume, and it does not say which sector GRUB asked for after launch.

A 1 GiB disk (`[vda] 2097152`) did not appear. The ISO was not staged.
The Toshiba was not written.

## Not this

Not `login:`. Not `RAYNU-V-M8-DISK-PERSIST-OK`. Do not F11 `c4a41a17`
again expecting the menu. Force Off the prompt if it is still up.
