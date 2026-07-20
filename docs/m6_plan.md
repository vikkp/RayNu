# M6 Plan — Production Ready

**Status:** **open** — M6.4 closed on Latitude; next **M6.5** PDF.  
**Prior:** M6.4 closed on Latitude (`RAYNU-V-M6-AUTH-OK`); M6.3–M6.0 closed; M5 closed.  
**Parent roadmap:** [CLAUDE.md](../CLAUDE.md) (M6 row) · lived gates: [progress.md](progress.md)  
**Prior track:** [m5_plan.md](m5_plan.md) · EPT theorem: [adr/ADR-004.md](adr/ADR-004.md) · toolchain: [adr/ADR-008.md](adr/ADR-008.md) · migrate: [adr/ADR-007.md](adr/ADR-007.md)

M5 delivered an **operationally viable** multi-VM platform: mgmt plane, audit + SOX/ISO reports, Dell Tier‑1, VMware inventory import, and the ADR-004 M5 proof row (large-page L3, NUMA *spec*, allocator↔EPT refine).  
**M6** makes that platform **production ready**: finish the headline EPT proof (violation + migration transfer), harden ops/security, prove stability under soak/faults, and clear external audit / spec review.

---

## Strategy (accepted)

**Close ADR-004 first on stable ghost surfaces; harden ops in parallel; production bar last; never block on Dell Tier‑2.**

- Do **not** claim ADR-004 complete until EPT-violation exclusivity **and** live-migration page transfer are discharged (or explicitly waived with ADR note).
- Do **not** block M6 closed on Dell Tier‑2 OEM Redfish (ADR-005) — slip-ok with docs.
- Do **not** fold inventory-import (`migrate/`, M5.5) into Proven Core without a new ADR (default **no**).
- **M6.0–M6.3** finish proof debt deferred from M5 (`ept_proof` GAP list).
- **M6.4–M6.6** harden the ops surface (auth, PDF reports, HA / security).
- **M6.7–M6.9** clear the production bar (fault injection, 72-hr soak, external audit + spec review).

```
Track A (proof):  M6.0 EPT-violation → M6.1 HW PTE → M6.2 NUMA-L3 → M6.3 migrate-xfer
Track B (ops):    start after M6.0 → M6.4 auth → M6.5 PDF → M6.6 HA/harden
Track C (prod):   start after M6.4 (or parallel late) → M6.7 fault → M6.8 soak → M6.9 external

→ M6 closed when critical markers green (below)
         ║
         ╚══ optional: Dell Tier-2 OEM, R640 hardware CI, Verus upgrade dry-run
```

---

## Debt / open items inherited from M5

| Item | Why it waits | M6 home |
|------|--------------|---------|
| EPT violation exclusivity | Not discharged in M5 | **M6.0** |
| HW PTE bit-decode / `ept_hw` correspondence | Scoped identity abs only (M5.9) | **M6.1** |
| NUMA affinity / exclusivity L3 | Spec only (M5.8) | **M6.2** |
| Live migration page transfer | ADR-004 M6 row | **M6.3** |
| REST auth stubbed | Size / scope (M5.1) | **M6.4** |
| PDF audit reports | JSON/CSV only (M5.4) | **M6.5** |
| HA / security harden | Production bar | **M6.6** |
| Fault injection suite | Pre-production (CLAUDE.md) | **M6.7** |
| 72-hr soak | Pre-production (CLAUDE.md) | **M6.8** |
| External `verus --verify` + spec review | Pre-production (CLAUDE.md) | **M6.9** |
| Bitmap allocator L3 polish | Coupling closed (M5.9); bit↔set open | Polish / slip-ok |
| Dell Tier‑2 OEM Redfish | Partnership / reverse-eng (ADR-005) | Optional / slip-ok |
| R640 hardware CI (not only QEMU) | Iron availability | Optional / with M6.8 |
| Verus pin upgrade dry-run | ADR-008 maintenance | Optional / with M6.9 |

---

## Subgates

Each = branch `cursor/m6-N-…-a623`, marker `RAYNU-V-M6-*-OK`, Latitude and/or host gate, docs touch.  
Do **not** claim a gate closed in docs/site until Latitude (or the documented host/iron path) is green.

### Track A — Proof closeout (ADR-004 M6 row)

### M6.0 — EPT-violation exclusivity — `RAYNU-V-M6-EPTVIO-OK`

**Status: closed** (Latitude `./tools/verus-eptvio-smoke.sh` → `RAYNU-V-M6-EPTVIO-OK`; `65 verified, 0 errors`)

**Goal:** Show that the EPT-violation / miss-handling path preserves exclusive ownership (ADR-004: must hold across violation handling). Ghost lemmas in `ept_model` + live `EptMap` / exit path asserts; no `admit` on theorems in scope.

**Shipped / wiring:**

1. `EptViolationDisposition` { `EmulateNoMap`, `Reject`, `ClaimMap` } + `violation_enabled` /
   `apply_violation` in `ept_model` (no `admit`).
2. Discharged: `theorem_ept_violation_preserves_exclusive`,
   `lemma_violation_noop_preserves_exclusive`, `lemma_violation_claim_preserves_exclusive`,
   `lemma_ept_violation_emulate_then_claim`.
3. Runtime hook `apply_violation_disposition` / `prop_violation_preserves_exclusive` in `ept.rs`.
4. `GAP(CLOSED M6.0)` in `ept_proof.rs`; `TODO(M6.0 CLOSED)` in `ept_spec.rs`.
5. Host gate `memory/m6_eptvio_gate.rs` + `tools/verus-eptvio-smoke.sh` + CI `verus-eptvio`.
6. Live MMIO path remains EmulateNoMap; unexpected GPA Reject; ClaimMap is demand-fill.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M6-EPTVIO-OK`.

### M6.1 — HW PTE bit-decode correspondence — `RAYNU-V-M6-HWPTE-OK`

**Status: closed** (Latitude `./tools/verus-hwpte-smoke.sh` → `RAYNU-V-M6-HWPTE-OK`; `72 verified, 0 errors`)

**Goal:** Deepen M5.9’s scoped `identity_leaf_ok` into correspondence between `ept_hw` identity-builder leaves and the ghost/concrete ownership view (bit-level or leaf-level abs as far as feasible).

**Acceptance sketch:**

1. No `admit` on theorems in scope; marker `RAYNU-V-M6-HWPTE-OK`.
2. Remaining decode / walk gaps listed explicitly → polish or ADR waiver.
3. Live precise identity path keeps runtime asserts.

**Delivered (host-first):**

1. Ghost: `ept_leaf_large_enc` / `ept_rwe_present` / `ept_large_bit` / `ept_hpa_from_pte` /
   `hw_2m_identity_leaf_ok` / `lemma_ept_leaf_large_decode` /
   `theorem_hw_2m_leaf_refines_identity` / `lemma_hw_2m_leaf_at_two_mib`.
2. Runtime: public `ept_leaf_large` / decode helpers + `prop_hw_pte_identity_correspondence` in `ept_hw.rs`.
3. Host gate `memory/m6_hwpte_gate.rs` + `tools/verus-hwpte-smoke.sh` + CI `verus-hwpte`.
4. `GAP(CLOSED M6.1): Hardware EPT PTE bit-decode`; full multi-level walk → polish GAP.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M6-HWPTE-OK`.

### M6.2 — NUMA affinity L3 — `RAYNU-V-M6-NUMA-L3-OK`

**Status: closed** (Latitude `./tools/verus-numa-l3-smoke.sh` → `RAYNU-V-M6-NUMA-L3-OK`; `77 verified, 0 errors`)

**Goal:** Discharge NUMA affinity / exclusivity beyond M5.8’s ghost *spec* (`guest_frames_on_node` under map/unmap, or documented L2 ceiling with ADR note).

**Acceptance sketch:**

1. Attempt L3 for affinity posts; if verify cost is high, document GAP + ADR waiver and still ship a stronger host/runtime gate.
2. Marker `RAYNU-V-M6-NUMA-L3-OK`.
3. Ties to `memory/numa.rs` + `idrac` SRAT/SLIT mock (already present).

**Delivered (host-first):**

1. Ghost: `lemma_numa_map_establishes_affinity` / `lemma_numa_unmap_preserves_affinity` /
   `theorem_numa_map_unmap_affinity` / `lemma_mock_numa_map_unmap_affinity`.
2. Runtime: `prop_numa_affinity_l3` in `memory/numa.rs` (mock SRAT/SLIT affinity policy).
3. Host gate `memory/m6_numa_gate.rs` + `tools/verus-numa-l3-smoke.sh` + CI `verus-numa-l3`.
4. `GAP(CLOSED M6.2): NUMA affinity / exclusivity L3`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M6-NUMA-L3-OK`.

### M6.3 — Live migration page transfer — `RAYNU-V-M6-MIGRATE-XFER-OK`

**Status: closed** (Latitude `./tools/verus-migrate-xfer-smoke.sh` → `RAYNU-V-M6-MIGRATE-XFER-OK`; `80 verified, 0 errors`)

**Goal:** ADR-004 M6 row — exclusive ownership preserved across **live migration page transfer** (handoff of HPA frames between guests / hosts under the ghost model). Distinct from M5.5 inventory import (`RAYNU-V-M5-MIGRATE-OK`).

**Acceptance sketch:**

1. Ghost transfer step(s) + discharged exclusivity theorem(s); runtime hook or mock transfer path.
2. Marker `RAYNU-V-M6-MIGRATE-XFER-OK`.
3. Full cross-host live migrate product may remain outside Proven Core; ownership handoff is the proof gate.

**Delivered (host-first):**

1. Ghost: `PageTransferStep` / `transfer_enabled` / `apply_transfer` /
   `lemma_transfer_preserves_exclusive` / `theorem_page_transfer_preserves_exclusive` /
   `lemma_mock_page_transfer_exclusive`.
2. Runtime: `transfer_page` / `prop_page_transfer_preserves_exclusive` in `ept.rs`.
3. Host gate `memory/m6_migrate_gate.rs` + `tools/verus-migrate-xfer-smoke.sh` + CI `verus-migrate-xfer`.
4. `GAP(CLOSED M6.3): Live migration page transfer`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M6-MIGRATE-XFER-OK`.

---

### Track B — Ops harden

### M6.4 — REST auth — `RAYNU-V-M6-AUTH-OK`

**Status: closed** (Latitude `./tools/m6-auth-smoke.sh` → `RAYNU-V-M6-AUTH-OK`)

**Goal:** Replace M5.1 auth stub (`GAP: REST auth stubbed → M6`) with a real, host-testable auth gate for the control-plane REST shapes (still no heavy HTTP stack if ADR-003 size budget forbids it).

**Acceptance sketch:**

1. Reject unauthenticated / unauthorized lifecycle ops; allow authorized ones.
2. Marker `RAYNU-V-M6-AUTH-OK`; audit events on deny/allow as appropriate.
3. Token/secret source documented (bring-up mock OK).

**Delivered (host-first):**

1. `auth_allows` requires `BRINGUP_AUTH_TOKEN` (`raynu-v-bringup`); missing/wrong → 401.
2. Audit `AuthAllowed` / `AuthDenied` on REST dispatch.
3. Host gate `mgmt/m6_auth_gate.rs` + `tools/m6-auth-smoke.sh` + CI `m6-auth`.
4. `GAP(CLOSED M6.4): REST auth stubbed → M6`; Web UI / M5.1 REST callers use mock token.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M6-AUTH-OK`.

### M6.5 — PDF audit reports — `RAYNU-V-M6-PDF-OK`

**Status: open** (host-first)

**Goal:** Close `GAP: PDF report → M6` — emit a deterministic PDF (or PDF-shaped artifact) from the same `RingSnapshot` used for JSON/CSV (M5.4).

**Acceptance sketch:**

1. Report path produces PDF bytes/file from ring snapshot; hash/stable layout for CI.
2. Marker `RAYNU-V-M6-PDF-OK`.
3. External auditor *sign-off* remains **M6.9** (this gate is the artifact, not the audit).

**Likely files:** `audit/report.rs`, gate + smoke + CI.

### M6.6 — HA / security harden — `RAYNU-V-M6-HA-OK`

**Status: open** (host-first; iron optional)

**Goal:** Production hardening bar from CLAUDE.md M6 row: HA posture and security hardening that operators can exercise (restart/failover story, privilege boundaries, safe defaults). Scope must stay concrete enough for a smoke/gate — not a vague “secure everything.”

**Acceptance sketch:**

1. Documented HA story with a host-testable failover or restart path (bring-up mock OK).
2. Security harden checklist encoded as gate checks (e.g. auth required, audit on privileged ops, no debug stubs in release path).
3. Marker `RAYNU-V-M6-HA-OK`.

**Likely files:** `mgmt/`, `audit/`, runbooks under `docs/`, gate + smoke.

---

### Track C — Production bar

### M6.7 — Fault injection — `RAYNU-V-M6-FAULT-OK`

**Status: open** (host / QEMU)

**Goal:** CLAUDE.md pre-production: fault injection (kill vCPUs, corrupt pages, drop IRQs, network partition) with expected recovery / fail-closed behavior and audit trail.

**Acceptance sketch:**

1. Scripted fault suite with pass/fail criteria; markers or log assertions.
2. Marker `RAYNU-V-M6-FAULT-OK`.
3. Does not require 72-hr duration (that is M6.8).

**Likely files:** `tools/` fault smoke, CI job (may be nightly / manual on Latitude).

### M6.8 — 72-hr soak — `RAYNU-V-M6-SOAK-OK`

**Status: open** (Latitude / iron preferred)

**Goal:** 72-hour soak: memory leaks, scheduler fairness, exit-rate stability (CLAUDE.md). Prefer R640 when available; QEMU nested acceptable for an interim host gate if iron is blocked — document which.

**Acceptance sketch:**

1. Soak harness + metrics thresholds; artifact log retained.
2. Marker `RAYNU-V-M6-SOAK-OK` only after a completed 72-hr run meets thresholds.
3. Optional companion: R640 hardware CI (not only QEMU).

**Likely files:** `tools/` soak harness, docs for thresholds, CI or Latitude runbook.

### M6.9 — External audit + spec review — `RAYNU-V-M6-EXT-OK`

**Status: open** (process + toolchain)

**Goal:** External security audit (auditor runs `verus --verify` under ADR-008 pin) **and** external spec review (“are we proving the right things?” — R09). Includes proof-maintenance dry run (upgrade Verus, re-verify, measure breakage) as a documented sub-check.

**Acceptance sketch:**

1. Frozen pin still green for auditor; findings tracked; critical findings closed or waived by ADR.
2. Spec-review note filed under `docs/` (or ADR amendment).
3. Marker `RAYNU-V-M6-EXT-OK`.

**Likely files:** `verus-version.toml`, `docs/` review note, runbook; may not be a pure CI smoke.

---

### Optional / slip-ok (document if deferred)

| Item | Marker (if pursued) | Notes |
|------|---------------------|-------|
| Dell Tier‑2 OEM Redfish | `RAYNU-V-M6-IDRAC2-OK` | ADR-005; partnership / reverse-eng |
| R640 hardware CI | (fold into M6.8 or separate) | Real iron, not only QEMU |
| Verus upgrade dry-run | (fold into M6.9) | ADR-008 maintenance |
| Bitmap allocator L3 | — | Polish after M5.9 coupling |

---

## Milestone acceptance (target)

**Critical for M6 closed:**

```text
RAYNU-V-M6-EPTVIO-OK
RAYNU-V-M6-HWPTE-OK
RAYNU-V-M6-MIGRATE-XFER-OK
RAYNU-V-M6-AUTH-OK
RAYNU-V-M6-HA-OK
RAYNU-V-M6-FAULT-OK
RAYNU-V-M6-SOAK-OK
RAYNU-V-M6-EXT-OK
==> ADR-004 M6 proof row + production bar PASSED
```

**Should be closed or explicitly waived with ADR note:**  
`RAYNU-V-M6-NUMA-L3-OK`, `RAYNU-V-M6-PDF-OK`.

**Optional / slip-ok with docs:** Dell Tier‑2, dedicated R640 CI job, bitmap L3 polish.

**M6 closed ⇒ product is production-ready under CLAUDE.md M6 row** (HA, hardened, soaked, externally reviewed). Further releases are maintenance / feature tracks outside this numbered plan unless a new milestone is opened.

---

## Execution order

```
Track A (proof):  M6.0 → M6.1 → M6.2 → M6.3
Track B (ops):    start after M6.0 → M6.4 → M6.5 → M6.6
Track C (prod):   start after M6.4 (prefer after M6.6) → M6.7 → M6.8 → M6.9

M6 closed when: EPTVIO + HWPTE + MIGRATE-XFER + AUTH + HA + FAULT + SOAK + EXT green,
                and NUMA-L3 + PDF closed or ADR-waived.
```

---

## First action

**M6.4 closed** on Latitude (`RAYNU-V-M6-AUTH-OK`). Next: **M6.5** (`RAYNU-V-M6-PDF-OK`) — PDF audit reports.
