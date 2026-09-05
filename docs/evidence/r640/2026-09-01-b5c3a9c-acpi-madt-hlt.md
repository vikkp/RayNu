# Iron COM2 — `b5c3a9c` dest_ok + ACPI MADT, then IdeBus HLT

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33440050729` (`b5c3a9c`, sha256 `d6664bb3…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `fw_cfg dest_ok fill dest=0x81ec98 n=56` | Dest skip at HV identity `0x200000` is gone (that is why `2d6b109` was refused) |
| `product ISO fw_cfg ACPI MADT` | HV offered MADT via fw_cfg |
| `OVMF-VMLAUNCH/ALIVE/PAST-SEC/DXE/CDROM/VIRTIO/BOTH-OK` | Guest-UEFI reached both PCI functions |
| `CD GuestVisible iso=1 bytes=66060288` | Product ISO is attached |

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `Freeing initrd`, no `ISO-INSTALL-OK`.

After `BOTH-OK`:

- `pci_ide=1` **`sectors=0`** **`ataio=0`**
- `HLT if=1 … cmd=0x00 … pin14=0` then `HLT skip-after-inject`
- Tick storm `reason=0xc` at `rip=0x7f0680d0` (CpuSleep) through `n=1835008`

Firmware sees the CD, never issues ATA, parks in HLT. Same stop as **STAGE46_WALL** (`ataio=0`, command empty). This dump has no `cmdwr=` / `pcicmd=` (that EFI does not print them).

## Ladder

Step 3 is **half**: HV `ACPI MADT` yes; Linux `ACPI=` no, because Linux never started. Next proof is El Torito / ATA (`ataio>0` or `sectors>0`), not another dest/`rep insb` SHA. Do not F11 this pin again. Do not flash `8024439`.
