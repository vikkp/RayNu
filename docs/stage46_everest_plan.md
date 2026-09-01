# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `21dc562` / `--run 33559849096` again (skip-HLT then same CpuSleep).
- Do not F11 `e3cbfa5` / `--run 33558261624` again (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` again (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` again (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.

---

## Ladder

| # | Status | Proof | This COM2 (`21dc562`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33559849096` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | dest_ok + ACPI MADT + wait-for-irq + one-shot + **skip-HLT** | `HLT skip after PIT one-shot` |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | same `rip=0x7f0680d0` `ataio=0` |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md`](evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md) — dest_ok + skip-HLT hang
- [`docs/evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md`](evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md) — wait-for-irq, stacked PIT livelock
- [`docs/evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md) — one-shot stops storm; HLT hang
- [`docs/evidence/r640/2026-09-01-21dc562-skip-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-21dc562-skip-oneshot-hlt-hang.md) — skip fired; same hang

---

## Current wall

HLT policy is exhausted. Skip after the one PIT printed, then firmware IRET'd back to CpuSleep. `ataio=0` `sectors=0` `cmd=0x00`. CD is visible. That is **STAGE46_WALL**. Iron still has no `cmdwr=` / `pcicmd=` line.

This #229 HEAD: print IDE PCI `cmdwr` / `pcicmd` / `wr=` on HLT stall and stop. Do not F11 `21dc562` / `33559849096`. Do not OR PCI command `0x0001`.

Next proof: COM2 `cmdwr=` (0 or >0) on the HLT/stop line. Not a new fw_cfg dest SHA. Still not `ISO-INSTALL-OK`.
