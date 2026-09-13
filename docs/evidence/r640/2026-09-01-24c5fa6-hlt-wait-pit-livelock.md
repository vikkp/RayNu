# Iron COM2 — `24c5fa6` HLT wait-for-irq, then PIT livelock

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33555104832` (`24c5fa6` / same wait-for-PIT as `ee82483`, sha256 `937a2f6e…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288 (alpine-virt)
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved (ladder 3a)

| Line | Meaning |
|------|---------|
| `fw_cfg dest_ok fill dest=0x81ec98 n=56` | Dest skip still gone |
| `product ISO fw_cfg ACPI MADT` | HV still offers MADT |
| `OVMF-VMLAUNCH` … `BOTH-OK` | Guest-UEFI reached both PCI functions |
| `HLT if=1 tpr=0x0 pic=1 gsi2=1 pin14=0 nien=0 cmd=0x00` | Virtual-wire armed (unlike `b5c3a9c` `pic=0 gsi2=0`) |
| `HLT wait-for-irq rip=0x7f0680d0` | CpuSleep **waited** instead of skip-after-inject |
| `Stage 46 inject vec=0x20` | PIT woke firmware; left `0x7f0680d0` |

## What did not (ladder 3b)

No `Linux version`, no `efi:` / `ACPI=`, no `Freeing initrd`, no `ISO-INSTALL-OK`.

After the PIT burst:

- `ataio=0` `sectors=0` `cmd=0x00` `pin14=0` through `n>2.5M`
- Tick cycle: `reason=0x1f` `rip=0x7f03f641` (RDMSR `0x1b`) → `reason=0xa` `rip=0x7f03fbe5` (CPUID) → `reason=0x1c` `rip=0x7f0697a9` / `0x7f0696a6` (CR8/CR4)
- Same class as `ea30da1` (unbounded PIT after HLT)

Wait-for-irq **proved**. Stacked `vec=0x20` before first ATA **livelocks**. Skip-after-inject / skip-PIT stay after PACKET. Do not F11 this pin again.

## Ladder

| Step | Result |
|------|--------|
| 3a wait-for-irq | **DONE** on this COM2 |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`) |

Next proof is still `ataio>0` or `sectors>0` / El Torito after **one** PIT wake. Not PCI command `0x0001`. Not a new fw_cfg dest SHA. Not IdeBus on #231.
