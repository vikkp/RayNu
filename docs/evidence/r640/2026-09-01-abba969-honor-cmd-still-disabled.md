# Iron COM2 — `abba969` honor COMMAND, still disabled

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33567464001` (`abba969`, sha256 `80c79a6f…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `Stage 46 IDE pci cmdwr honor` | This SHA stored COMMAND as written |
| `cmdwr=6 pcicmd=0x0 wr=0x0` | Readback matches last write (was `pcicmd=0x1` on `184ee61`) |

Honor COMMAND is proved. Nested `cmdwr=0` is still not the iron picture. Firmware's last write is disable, and disable now sticks.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>5.5M`

EnableAttributes did **not** write `0x5` after the last `0`. HLT policy is still exhausted. `cmd=0x00` is last ATA 0x1F7, not PCI COMMAND.

## Next SHA

Print the six COMMAND write values (`wr=` per write + HLT `seq=`). Do not OR `0x0001`. Do not F11 this pin again. Still not `ISO-INSTALL-OK`.

## Ladder

| Step | Result |
|------|--------|
| 3a wait-for-irq + one-shot + skip-HLT | **DONE** |
| iron `cmdwr` printed | **DONE** (`184ee61`) |
| honor COMMAND as written | **DONE** (`pcicmd=` equals last `wr=`) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
