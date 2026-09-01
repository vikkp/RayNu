# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `e3cbfa5` / `--run 33558261624` again (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` again (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` again (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.

---

## Ladder

| # | Status | Proof | This COM2 (`e3cbfa5`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33558261624` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | dest_ok + ACPI MADT + wait-for-irq + **one-shot PIT** | `PIT one-shot` · one `vec=0x20` · no APIC storm |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | IRET to HLT `0x7f0680d0`; cap `n=16777216`; `ataio=0` `catalog=0` |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md`](evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md) — dest_ok + skip-HLT hang
- [`docs/evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md`](evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md) — wait-for-irq, stacked PIT livelock
- [`docs/evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md) — one-shot stops storm; HLT hang

---

## Current wall

One PIT is delivered; firmware IRET's back to CpuSleep. Wait-for-irq after one-shot deadlocks because further `0x20` is skipped. `ataio=0` `sectors=0` `cmd=0x00`. CD is visible. That is **STAGE46_WALL**.

This #229 pin: `21dc562` / CI `--run 33559849096` (49/49 green). After the first PIT, **skip HLT** (no more PIT). Do not F11 `e3cbfa5` / `33558261624`. Do not F11 `24c5fa6` / `33555104832`. Do not F11 `b5c3a9c`.

Next proof: COM2 `HLT skip after PIT one-shot` then `ataio>0` or `sectors>0` / El Torito. Not `cmdwr` OR `0x0001`. Not a new fw_cfg dest SHA. Still not `ISO-INSTALL-OK`.
