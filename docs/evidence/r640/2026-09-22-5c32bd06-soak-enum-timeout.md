# 2026-09-22 — `5c32bd06` soak flag seen, Address Device timeout, RAM install

EFI `5c32bd066ebe` (CI run `35774213760`, Cruzer SHA256 prefix `f72483c4`).
ESP had empty `EFI/RayNu/usbsoak.txt` and `raynuf.txt`. Lease `10.99.99.150/24`.

## What printed

`boot: USB soak requested` then xHCI reset. CCS on p10, p11, p14 (`PORTSC=0x000206e1` at scan: connected, PED=0, PLS=Polling).

`p11first` then Address Device on p11: `sc=0x00000e03 speed=3` (HS, U0, PED=1) `cmd=3 cmpl=0xff timeout`. Abort `crr=0`. `addrretry p11`. Same timeout again.

p14 `vid=0x1604 did=0x10c0 class=0x09` hub skip.

p10 Address Device: same `sc=0x00000e03 cmd=3 cmpl=0xff timeout`.

```
usb I/O fail err=3 bar=0x92b00000 portsc=0x0000000a00000e03 cmpl=0x00000000000303ff bot=? scsi=?
leftover install disk hpa=0x140a00000 bytes=1073741824
```

`err=3` is Enum. Packed `portsc` is port **10** + `0xe03`. Packed `cmpl=0x303ff` is speed=HS, cmd=Address Device, completion=software timeout. BOT never started (`bot=?`). No `usb I/O ready`, no `USBSOAK`, no `xhci timeout` dump. The soak only runs after I/O is ready.

## What the guest was

No durable LUN, so `EFI PART` was never seen and the ISO skip did not arm. `image=ISO-BOOTX64` (724992 bytes, El Torito lba=125). `vda` was **2097152 sectors = 1 GiB** leftover DRAM (`keep=0`), not the 8 GiB Toshiba slice. Auto-answer ran `setup-disk -m sys -s 0 /dev/vda` on that RAM disk. `RAYNU-V-M7-ISO-INSTALL-OK` printed. Guest reboot (F7, not Force Off) found GPT ESP lba=2048, `DISK-BOOTX64` 139264 bytes, `root=UUID=4c27e121-4eda-4ddf-a07c-bc5cf8f0b942`, and `localhost login:`.

That login dies on Force Off. The Toshiba was never opened this boot (enumeration never reached a sector read).

## Not this

Not `RAYNU-V-USBSOAK-DONE`. Not `RAYNU-V-M8-DISK-PERSIST-OK`. Not A4s. Do not F11 `5c32bd06` again expecting the soak: the same Address Device timeout launches the RAM installer again.
