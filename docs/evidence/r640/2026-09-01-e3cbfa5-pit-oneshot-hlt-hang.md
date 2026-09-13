# Iron COM2 — `e3cbfa5` PIT one-shot, then HLT hang

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33558261624` (`e3cbfa5`, sha256 `8a2a359e…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `HLT wait-for-irq rip=0x7f0680d0` | Same wait as `24c5fa6` (`pic=1 gsi2=1`) |
| `Stage 46 PIT one-shot` | First `0x20` latched; stacked PIT did **not** start |
| `Stage 46 inject vec=0x20` | **One** inject only (not the `24c5fa6` burst) |
| CR8 around inject | Timer ISR entered, then IRET |

APIC/CR8 livelock (`rip=0x7f03f641` / `0x7f03fbe5` / `0x7f0697a9`) did **not** return.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After the one-shot, ticks stay on CpuSleep until the 2²⁴ exit cap:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` `hlt=0`
- stop `n=16777216` `catalog=0` `bootimg=0` `readlba=0` `elt=0` `packet=0` `scsi=0x0` `ata=0x0` `acpi=3964`
- `Stage 46 product ISO hold (not ISO-INSTALL-OK); not E4 SHELL`

One PIT is not the event BDS is waiting for. Wait-for-irq after one-shot is a deadlock (further `0x20` is skipped). Do not F11 this pin again. Next pin is `21dc562` / `--run 33559849096` (skip HLT after the one-shot).

## Ladder

| Step | Result |
|------|--------|
| 3a wait-for-irq | **DONE** (`24c5fa6`) |
| one-shot stops PIT storm | **DONE** (this COM2) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, HLT hang) |
