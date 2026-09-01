# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `184ee61` / `--run 33562028442` again (`cmdwr=6` last `wr=0x0` stored `0x1`).
- Do not F11 `21dc562` / `--run 33559849096` again (skip-HLT then same CpuSleep).
- Do not F11 `e3cbfa5` / `--run 33558261624` again (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` again (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` again (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.
- Do not OR PCI command `0x0001`.

---

## Ladder

| # | Status | Proof | This COM2 (`184ee61`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33562028442` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | dest_ok + ACPI MADT + wait-for-irq + one-shot + skip-HLT | unchanged |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | `cmdwr=6 wr=0x0 pcicmd=0x1` then same `rip=0x7f0680d0` `ataio=0` |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md`](evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md) — dest_ok + skip-HLT hang
- [`docs/evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md`](evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md) — wait-for-irq, stacked PIT livelock
- [`docs/evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md) — one-shot stops storm; HLT hang
- [`docs/evidence/r640/2026-09-01-21dc562-skip-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-21dc562-skip-oneshot-hlt-hang.md) — skip fired; same hang
- [`docs/evidence/r640/2026-09-01-184ee61-ide-cmdwr-disable.md`](evidence/r640/2026-09-01-184ee61-ide-cmdwr-disable.md) — `cmdwr=6` last `wr=0x0` stored `0x1`

---

## Current wall

Iron wrote IDE PCI COMMAND (`cmdwr=6`). Last write is disable (`wr=0x0`). Stored `pcicmd=0x1` because #229 ORed `0x0001`. Nested `cmdwr=0` is not this dump. HLT policy is exhausted. `ataio=0` `sectors=0` `cmd=0x00` (ATA 0x1F7). That is **STAGE46_WALL** plus the COMMAND-OR confuse.

This #229 HEAD: honor IDE PCI COMMAND as written. Do not OR `0x0001`. Do not F11 `184ee61` / `33562028442`.

Next proof: HLT/stop `pcicmd=` equals last `wr=` (expect `0` if last write stays 0, or `0x5` if EnableAttributes re-enables) then `ataio>0`. Still not `ISO-INSTALL-OK`.
