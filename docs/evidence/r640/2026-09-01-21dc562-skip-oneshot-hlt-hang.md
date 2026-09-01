# Iron COM2 — `21dc562` skip-HLT after one-shot, same CpuSleep

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33559849096` (`21dc562`, sha256 `938f9928…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `HLT wait-for-irq rip=0x7f0680d0` | Same first wait as `e3cbfa5` |
| `HLT skip after PIT one-shot` | #229 skip path **did** fire |
| `HLT skip` ×4 | RIP+Active after the one `vec=0x20` |
| CR8 around inject | Timer ISR entered, then IRET |

SNP DHCP failed this boot (incidental). Guest path through dest_ok / ACPI MADT / BOTH-OK is unchanged.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After the skip lines, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>4.3M` (same HPET cadence as `e3cbfa5`)

Skip-HLT after one PIT is not the event BDS needs before ATA. HLT policy is exhausted (`b5c3a9c` / `24c5fa6` / `e3cbfa5` / this pin). Do not F11 this pin again. Next pin prints IDE `cmdwr` / `pcicmd` (ADR-015). Do not OR PCI command `0x0001`.

## Ladder

| Step | Result |
|------|--------|
| 3a wait-for-irq + one-shot | **DONE** (`e3cbfa5`) |
| skip HLT after one-shot | **DONE** (this COM2 line) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
