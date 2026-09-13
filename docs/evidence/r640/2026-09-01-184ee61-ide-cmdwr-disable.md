# Iron COM2 — `184ee61` IDE cmdwr=6, last write disable

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33562028442` (`184ee61`, sha256 `43a38ac8…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `Stage 46 IDE pci cmdwr` | First firmware write to IDE PCI COMMAND printed |
| `cmdwr=6 pcicmd=0x1 wr=0x0` | Six COMMAND writes; last written value was `0x0000` |
| `pcicmd=0x1` | Stored value is `wr \| 0x0001` — disable never stuck on readback |

Nested `STAGE46_WALL` (`cmdwr=0`) is **not** the iron picture. EnableAttributes-class writes ran. The last write is disable.

SNP lease `10.99.99.122` this boot (incidental). Guest path through dest_ok / ACPI MADT / BOTH-OK / one-shot / skip-HLT is unchanged.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>1.1M`

`cmd=0x00` on HLT is last ATA 0x1F7, not PCI COMMAND.

## Why the next SHA honors COMMAND

OR `0x0001` makes write-0 read back as `0x1`. PciBus disable-before-BAR / EnableAttributes can stall and never reach IDENTIFY. ADR-015: honor COMMAND as written. Do not OR `0x0001`.

Do not F11 this pin again. Next proof: `pcicmd=` matches last `wr=` (expect `0` if last write stays 0, or `0x5` if EnableAttributes re-enables) then `ataio>0`.

## Ladder

| Step | Result |
|------|--------|
| 3a wait-for-irq + one-shot + skip-HLT | **DONE** (`21dc562`) |
| iron `cmdwr` / `pcicmd` printed | **DONE** (`cmdwr=6` `wr=0x0` `pcicmd=0x1`) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
