# 2026-09-23 — `e5cca2e0` USB soak finished: 239/239, Toshiba still installed

EFI `e5cca2e0d7a8` (`xhci trb order event cycle-first, write cycle-last`).
ESP had empty `EFI/RayNu/usbsoak.txt` and `raynuf.txt`. Lease `10.99.99.147/24`.
No guest ran. Boot ended in `RAYNU-V-USBSOAK-DONE` and the halt line.

## What printed

`xhci nop` with no `cmd timeout` after it. Port 11 enumerated as the Toshiba
`vid=0x0480 did=0xa004`, BOT `ep_out=2 ep_in=1 cfg=1`, then:

```
durable LUN usb I/O ready bytes=320072933376 lba=512
durable LUN peek usb efi=EFI PART gpt=1 fit=1 gpt_err=0 usb_err=0 lba=512 guest=8589934592 bootx64=1 ext4=1 installed=1
USBSOAK start reads=239 idle_s=1020
USBSOAK gap_s=0   n=200 ok=200 fail=0 max_ms=12 first_fail=none
USBSOAK gap_s=5   n=24  ok=24  fail=0 max_ms=0  first_fail=none
USBSOAK gap_s=30  n=10  ok=10  fail=0 max_ms=0  first_fail=none
USBSOAK gap_s=120 n=5   ok=5   fail=0 max_ms=0  first_fail=none
USBSOAK total ok=239 fail=0
RAYNU-V-USBSOAK-DONE
USBSOAK halt — Force Off when done reading COM2
```

`bytes=320072933376` is the whole Toshiba (~298 GiB). `guest=8589934592` is the
8 GiB virtio slice. `installed=1` with `bootx64=1` and `ext4=1` means the
`928d6224` Alpine is still on that slice. The two mashed words (`duboot`,
`bootboot`) are the COM1/COM2 mirror printing the ready line and a
`usb rw wait spins=100000000` heartbeat on top of each other; the fields are intact.

## What it means

The cycle-first event poll lived. Enumeration, the first sector read, and
239 further reads with idle gaps of 0 / 5 / 30 / 120 seconds all completed.
The failure shape since `928d6224` (a BOT command after idle returning
`cmpl=0xff`) did not recur on this bench. `max_ms=0` on the later gaps is a
sub-millisecond read, not a skipped one: `ok` equals `n` on every line.

This is the USB bench. It is not `RAYNU-V-M8-DISK-PERSIST-OK` and not an
installed `login:`. No guest ran, so the product path (GRUB off the Toshiba
ESP, `[vda] 16777216`, `login:`) is the next boot, on this same EFI, with
`usbsoak.txt` removed.

## Not this

Not a score change. Not A4s. Do not reflash. Do not F11 `1fa231df` or
`5c32bd06`. Boot 2 is `e5cca2e0` without the soak flag.
