# M8 Plan — operator product hardening (post-Everest)

**Status:** **OPEN** — M7 Mount Everest **CLOSED on iron** (`f72b4276` / `34552377351`, 2026-09-11).  
**Parent:** [ADR-018](adr/ADR-018.md) · Everest: [ADR-009](adr/ADR-009.md) · lived: [progress.md](progress.md) · HDA: [hda.md](hda.md)  
**Prior track:** [m7_plan.md](m7_plan.md) (closed). Cluster / elasticity is **M9**, not this plan.

M8 is the polish table that used to read as “Everest residual.” It is **not** a claim that the product loop is unfinished. COM2 already printed `HOST-NIC-HTTP-OK` → SPA Start of RayNu-F → `ISO-INSTALL-OK` → `DISK-BOOT-OK` → `login:`.

---

## Iron rollback (M8.0 known-good)

Flash this if an M8 persist prototype misbehaves. Do **not** treat a later tip as the Everest close.

| Field | Value |
|-------|-------|
| GitHub | https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed (Latest) |
| Tag | `v0.1.0-everest-closed` → `f72b4276d198b5e90147e9be1037d0d0b7213a28` |
| CI | `--run 34552377351` · COM2 `build: sha=f72b4276d198` |
| EFI SHA256 | `e74460ff0e248a06d2e4ab546684d1006edd25855f50c8dc27dc202facab9cbc` |
| In-tree kit | `releases/v0.1.0-everest-closed/` (PR #243, open) |
| Not this | 1.0/GA · TLS · leftover persist across HV reboot · F11 `34548550755` / `7f8dc0a9` |

Evidence: [`docs/evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md`](evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md). Cursor rule: [`.cursor/rules/iron-rollback.mdc`](../.cursor/rules/iron-rollback.mdc). Historical pre-native-NIC preserve (not this kit): `releases/v0.1.0-adr013-baseline`.

---

## Strategy

**Harden the single-host operator product before cluster features.**

- Do **not** reopen M7. Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`.
- Do **not** pull TLS / persist / console into Proven Core without a new ADR (default **no**).
- Do **not** start vMotion / DRS / hot-add on this path (→ **M9**).
- Build **in order**. A later gate may be designed in parallel; it does not close before its predecessor without rewriting this plan.

```
M8.0  Persist leftover-DRAM install disk across HV reboot
M8.1  TLS on coexist :8443
M8.2  Real operator auth (not bring-up token as product default)
M8.3  Guest console UI (web/VNC; serial already worked)
M8.4  ISO blob upload via SPA/REST
M8.5  UEFI catalog persist (SFS/NVMe write)
M8.6  Windows / multi-distro (ADR-014 later)

→ M9 sketch: vMotion-like · DRS-like · hot-add
```

---

## Close rule

Software gates: CI + host/QEMU smoke.  
Iron gates: real PowerEdge R640 COM2 (or documented HTTPS capture) — Latitude/QEMU insufficient when the claim is “survives a host reboot” or “browser TLS.”

HDA + `site/hda.html` stay fresh: update `docs/hda.md`, then `./tools/sync-hda-site.sh`.

---

## Gates

### M8.0 — Persist install disk across HV reboot

**Status: open** (next)

**Goal:** The virtio install disk that `setup-disk` wrote still exists after the **hypervisor** reboots — not only after a **guest** F7 reset (ADR-017 `reset_keep_disk`).

**Why first:** Every other polish is wasted if the guest filesystem dies when RayNu-V restarts. Everest E5 did not require this.

**Acceptance (draft):** Named marker on real R640 (to be minted when the path exists). Host/CI still never print `ISO-INSTALL-OK`.

**Not this gate:** TLS, VNC, Windows. If a persist prototype misbehaves, re-flash [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed) (see [Iron rollback](#iron-rollback-m80-known-good)).

---

### M8.1 — TLS on the mgmt listen

**Status: open** (after M8.0, or overlapping design only)

**Goal:** Coexist HTTP on BCM5720 (`10.99.99.x:8443`) becomes HTTPS. Plaintext remains a lab fallback. Size stays inside ADR-003.

**Acceptance (draft):** Browser or `curl --cacert` (or documented equivalent) on the operator LAN after `BOOT-OK`. PRE-EBS SNP does not count.

**Honesty:** ADR-009 already deferred TLS to close Everest. Closing M8.1 does not rewrite that history.

---

### M8.2 — Real operator auth

**Status: open**

**Goal:** Product default is not `raynu-v-bringup`. ESP `auth.token` (or successor) is the operator credential path.

**Depends on:** M8.1 is the natural pairing (TLS without auth is still a toy; auth without TLS leaks the token).

---

### M8.3 — Guest console UI

**Status: open**

**Goal:** Operator types in the guest from the SPA (web/VNC or equivalent). iDRAC `console com2` remains the evidence channel, not the only keyboard.

**Honesty:** Serial auto-answer already installed Alpine on iron. This gate is UX, not E5.

---

### M8.4 — ISO blob upload via SPA

**Status: open**

**Goal:** Operator can PUT/POST an ISO through the network UI. ESP-staged `linux.iso` stays valid.

---

### M8.5 — UEFI catalog persist

**Status: open**

**Goal:** Replace `UnsupportedOnFirmware` with a real SFS/NVMe write of `EFI/RAYNU/images/catalog.txt`. Host `std::fs` path already closed M7.2.

---

### M8.6 — Windows / multi-distro (ADR-014)

**Status: open** (later)

**Goal:** Same UEFI+virtio product boot; typed `windows_iso` / additional Linux. Not a RayNu-F rewrite. Not WHQL.

---

## M9 sketch (out of scope)

| Theme | Intent |
|-------|--------|
| vMotion-like | Live migrate running VM between hosts (builds on M6.3 proofs) |
| DRS-like | Placement / load-aware scheduling across hosts |
| Hot-add | CPU / RAM / disk add to running guest |

Do not pull M9 into M8 gate lists.

---

## First action

**M8.0.** Design persist of the leftover-DRAM virtio disk across a host reboot of RayNu-V (NVMe or ESP-backed). Keep ADR-004 exclusive ownership. Do not claim persist from nested QEMU alone. Keep [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed) as the flash rollback until a new named iron marker exists.
