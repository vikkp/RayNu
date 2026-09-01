# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `b5c3a9c` / `--run 33440050729` again (same HLT).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.

---

## Ladder

| # | Status | Proof | This COM2 (`b5c3a9c`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33440050729` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | `fw_cfg dest_ok` + `product ISO fw_cfg ACPI MADT` | `dest=0x81ec98` · MADT line printed |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | Never reached Linux |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron note: [`docs/evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md`](evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md).

---

## Current wall

OVMF is in BDS **CpuSleep**: `ataio=0` `sectors=0` `cmd=0x00` `pin14=0`, HLT `rip=0x7f0680d0`. CD is visible. ATA never started. That is **STAGE46_WALL**, now on real COM2.

This #229 HEAD: dest_ok pin `b5c3a9c` plus firmware HLT **wait-for-PIT** before first ATA (virtual-wire + inject `0x20`, no skip-after-inject while `ataio==0`).

Next proof: COM2 `HLT wait-for-irq` then `ataio>0` or `sectors>0` / El Torito. Not `cmdwr` OR `0x0001`. Not a new fw_cfg dest SHA (**FWCFG_PRODUCT_STANCE**). Still not `ISO-INSTALL-OK`.
