# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `24c5fa6` / `--run 33555104832` again (PIT livelock after wait-for-irq).
- Do not F11 `b5c3a9c` / `--run 33440050729` again (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.

---

## Ladder

| # | Status | Proof | This COM2 (`24c5fa6`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33555104832` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | `fw_cfg dest_ok` + ACPI MADT + **`HLT wait-for-irq`** | `dest=0x81ec98` · MADT · `pic=1 gsi2=1` · inject `0x20` |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | PIT livelock; `ataio=0`; never reached Linux |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md`](evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md) — dest_ok + skip-HLT hang
- [`docs/evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md`](evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md) — wait-for-irq proved, stacked PIT livelock

---

## Current wall

OVMF leaves CpuSleep when PIT is injected, then livelocks in APIC/CR8 (`rip=0x7f03f641` / `0x7f03fbe5` / `0x7f0697a9`). `ataio=0` `sectors=0` `cmd=0x00` `pin14=0`. CD is visible. ATA never started. That is **STAGE46_WALL**, now after a successful wait-for-irq.

This #229 HEAD: dest_ok pin plus wait-for-irq plus **one-shot PIT** (first `0x20` wakes HLT; further `0x20` dropped until `ataio>0`). Do not F11 `24c5fa6` / `33555104832`. Do not F11 `b5c3a9c` / `33440050729`.

Next proof: COM2 `Stage 46 PIT one-shot` then `ataio>0` or `sectors>0` / El Torito. Not `cmdwr` OR `0x0001`. Not a new fw_cfg dest SHA (**FWCFG_PRODUCT_STANCE**). Still not `ISO-INSTALL-OK`.
