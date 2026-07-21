# M7 Plan — Mount Everest (shippable single-host)

**Status:** **open** — M7.0 wired (host); Latitude smoke pending close. Next after close: **M7.1 HTTP**.  
**Prior:** M6 closed on Latitude (`RAYNU-V-M6-EXT-OK`; `80 verified, 0 errors`).  
**Parent roadmap:** [CLAUDE.md](../CLAUDE.md) (M7 row) · ADR: [adr/ADR-009.md](adr/ADR-009.md) · HDA: [hda.md](hda.md) · lived: [progress.md](progress.md)  
**Prior track:** [m6_plan.md](m6_plan.md)

**Mount Everest (product loop):**  
Ship EFI → boot via **iDRAC virtual media** on **real R640** → **network Web UI** → **deploy Linux ISO / install guest**.

M6 closed the production-ready *bar* (proof + ops harden + soak + external audit) on Latitude/QEMU.  
**M7** delivers the shippable **single-host** operator product. Cluster features (vMotion-like, DRS-like, hot-add) are **M8** — not M7 blockers.

---

## Strategy (accepted)

**Build software readiness before iron; close M7 only after real R640 boot; single-host first.**

- Do **not** claim M7 closed without `RAYNU-V-R640-BOOT-OK` (or equiv.) on real PowerEdge R640.
- Do **not** block M7 on Dell Tier‑2 OEM Redfish (ADR-005) — slip-ok.
- Do **not** pull HTTP/datastore/ISO into Proven Core without a new ADR (default **no**).
- Do **not** start product vMotion / DRS / hot-add on the M7 critical path (→ M8).
- Pre-iron order: **Ship kit → TLS/HTTP → datastore/ISO** (R640 racked ~1 month after plan open).

```
Track Ship:   M7.0 release kit + USB/iDRAC runbook
Track Net:    M7.1 TLS/HTTP serves SPA + REST on host NIC
Track Store:  M7.2 datastore (images + ISOs)
Track ISO:    M7.3 ISO register + CD-ROM or extract-boot + virtio disk
Track UI:     M7.4 create-VM + media attach + basic console/log
Track Iron:   M7.5 real R640 boot (hard gate for M7 closed)

→ M7 closed when SHIP + HTTP + STORE + ISO + UI + R640-BOOT green
         ║
         ╚══ M8 sketch: vMotion-like · DRS-like · hot-add
```

---

## Close rule

Do **not** claim a gate closed in docs/site until:

- Software gates: CI + documented host/QEMU smoke green (same culture as M6).
- **`RAYNU-V-R640-BOOT-OK`:** real PowerEdge R640 evidence only (Latitude/QEMU insufficient).

HDA + `site/hda.html` must stay fresh: update `docs/hda.md`, then `./tools/sync-hda-site.sh`.

---

## Gates

### M7.0 — EFI release kit — `RAYNU-V-M7-SHIP-OK`

**Status: wired** (host gate; Latitude `./tools/m7-ship-smoke.sh` pending close)

**Goal:** Ops-trustable ship artifact — not just `cargo build`.

**Deliverables:**

1. Versioned `r640-hypervisor.efi` packaging (tag or version stamp) + SHA256.
2. Size gate in release path (`tools/check-size.sh` / CI).
3. One-page USB + iDRAC virtual media runbook (`docs/runbooks/`).
4. Host gate + smoke → `RAYNU-V-M7-SHIP-OK`.
5. `GAP(CLOSED M7.0): EFI release kit`.

**Shipped (host):**

1. `tools/package-release.sh` → `dist/raynu-v-<version>/` + `.tar.gz` + SHA256 sidecars.
2. `mgmt/ship.rs` + `mgmt/m7_ship_gate.rs` + `tools/m7-ship-smoke.sh` + CI `m7-ship` (+ package step on `build-uefi`).
3. Runbook [`docs/runbooks/usb_idrac.md`](runbooks/usb_idrac.md).
4. `GAP(CLOSED M7.0): EFI release kit`.

**Acceptance:** Latitude smoke + marker. Does **not** require R640.

---

### M7.1 — Network TLS/HTTP mgmt plane — `RAYNU-V-M7-HTTP-OK`

**Status: open**

**Goal:** Browser on operator LAN reaches SPA + REST (not in-process dispatch only).

**Deliverables:**

1. Minimal TLS (or HTTP) listener in-binary (size-boxed; ADR-003) serving embedded Web UI + REST.
2. Auth beyond bring-up toy token (reuse/extend M6.4 patterns).
3. QEMU/lab proof of reachability from a second host or user-net forward.
4. Host gate + smoke → `RAYNU-V-M7-HTTP-OK`.
5. `GAP(CLOSED M7.1): Network HTTPS/HTTP mgmt`.

**Acceptance:** Documented browser-reachable path. Prefer TLS; HTTP allowed for lab MVP with ADR note.

---

### M7.2 — Datastore / image library — `RAYNU-V-M7-STORE-OK`

**Status: open**

**Goal:** Somewhere to put ISOs, disks, templates (ESP/NVMe-backed).

**Deliverables:**

1. Datastore abstraction (register/list/delete images).
2. Persistence on ESP or NVMe path suitable for R640.
3. API shapes for UI.
4. Host gate + smoke → `RAYNU-V-M7-STORE-OK`.
5. `GAP(CLOSED M7.2): Datastore`.

---

### M7.3 — ISO deploy path — `RAYNU-V-M7-ISO-OK`

**Status: open**

**Goal:** Operator registers a distro ISO → VM can boot installer (CD-ROM **or** documented kernel-extract) with virtio-blk install target.

**Deliverables:**

1. ISO register into datastore.
2. CD-ROM emulation **or** extract-boot MVP (documented).
3. Empty/persistent virtio-blk for install-to-disk.
4. Host/QEMU smoke → `RAYNU-V-M7-ISO-OK`.
5. `GAP(CLOSED M7.3): Linux ISO deploy path`.

**Acceptance:** One supported distro path on QEMU before iron; R640 validation after M7.5.

---

### M7.4 — Ops Web UI MVP — `RAYNU-V-M7-UI-OK`

**Status: open**

**Goal:** vSphere-*like* enough for single-host install — not full parity.

**Deliverables:**

1. Create-VM (CPU/RAM/disk/NIC/ISO) over network UI.
2. Attach media; start/stop; basic console/serial log.
3. Surfaces datastore + ISO from M7.2/M7.3.
4. Host gate + smoke → `RAYNU-V-M7-UI-OK`.
5. `GAP(CLOSED M7.4): Network create-VM + ISO UI`.

---

### M7.5 — Real R640 boot — `RAYNU-V-R640-BOOT-OK`

**Status: open** (iron-bound; hardware ~1 month after plan open)

**Goal:** First light on real PowerEdge R640 via USB or iDRAC vMedia.

**Deliverables:**

1. Boot `r640-hypervisor.efi` on R640; COM1/iDRAC serial works.
2. VMX + EPT + Linux shell path observed on iron (or documented residual with follow-up).
3. Runbook evidence + marker `RAYNU-V-R640-BOOT-OK`.
4. `GAP(CLOSED M7.5): Real R640 boot`.

**Acceptance:** **Real R640 only.** Latitude/QEMU cannot close this gate.

---

## Milestone acceptance

**Critical for M7 closed:**

```text
RAYNU-V-M7-SHIP-OK
RAYNU-V-M7-HTTP-OK
RAYNU-V-M7-STORE-OK
RAYNU-V-M7-ISO-OK
RAYNU-V-M7-UI-OK
RAYNU-V-R640-BOOT-OK
==> Mount Everest single-host product loop PASSED
```

**Optional / follow-on (not required for M7 closed):** live Tier‑1 Redfish health, R640 soak (P0-10), multi-distro ISO matrix, Secure Boot signing.

**M7 closed ⇒** operator can iDRAC-boot RayNu-V on R640 and install a Linux guest from the network Web UI on that host.

---

## M8 sketch (out of scope for this plan)

| Theme | Intent |
|-------|--------|
| vMotion-like | Live migrate running VM between hosts (product ops; builds on M6.3 proofs) |
| DRS-like | Placement / load-aware scheduling across hosts |
| Hot-add | CPU / RAM / disk add to running guest |

Do not pull M8 into M7 gate lists.

---

## First action

**M7.0 wired** (host gate). Close on Latitude via `./tools/m7-ship-smoke.sh` →
`RAYNU-V-M7-SHIP-OK`, then start **M7.1** TLS/HTTP under ADR-003.
