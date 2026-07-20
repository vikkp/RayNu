# M5 Plan — Operationally Viable

**Status:** **open** — M5.0 closed on host; next is **M5.1** (CLI + REST).  
**Prior:** M5.0 closed on host (`RAYNU-V-M5-LIFE-OK`).  
**Parent roadmap:** [CLAUDE.md](../CLAUDE.md) (M5 row) · lived gates: [progress.md](progress.md)  
**Prior track:** [m4_plan.md](m4_plan.md) · EPT theorem: [adr/ADR-004.md](adr/ADR-004.md) · iDRAC: [adr/ADR-005.md](adr/ADR-005.md) · migrate: [adr/ADR-007.md](adr/ADR-007.md)

M4 delivered a usable multi-VM platform (4+ guests, blk/net/SMP probes, N-guest L3 + refine, large-page *spec*).  
**M5** makes that platform **operationally viable**: management plane, audit engine, SOX/ISO-style reports, Dell Tier‑1 health, and the ADR-004 proof debt deferred from M4.

---

## Strategy (accepted)

**Ops spine first; bolt proof onto stable surfaces; never block on Dell Tier‑2 or VMware.**

- Do **not** fold VMware import into M5 critical path (ADR-007 → **M5.5**, parallel).
- Do **not** block M5 on Dell Tier‑2 Redfish OEM schemas (ADR-005); Tier‑1 is enough to close.
- Do **not** defer all remaining EPT proof to M6 — large-page L3 + NUMA *spec* are M5 exit criteria (ADR-004).
- **M5.0–M5.2** make the mgmt plane real (lifecycle → API/CLI → embedded Web UI).
- **M5.3–M5.4** make [A] believable (audit ring + hash chain → SOX/ISO reports).
- **M5.5** (parallel workstream) is VMware migration — outside Proven Core.
- **M5.6** Dell Tier‑1 iDRAC / SMBIOS / ACPI topology (health + NUMA layout for ops).
- **M5.7–M5.9** bolt on proof debt: large-page L3, NUMA in ghost spec, allocator↔EPT / HW PTE correspondence.

```
M5.0 lifecycle → M5.1 API/CLI → M5.2 Web UI
       → M5.3 audit ring → M5.4 SOX/ISO reports
       → M5.6 Dell Tier-1 health (may start after M5.0)
       → M5.7 large-page L3 → M5.8 NUMA spec → M5.9 allocator/HW refine
→ M5 closed → M6
         ║
         ╚══ parallel M5.5 VMware migrate (ADR-007; not on critical path)
```

---

## Debt / open items inherited from M4

| Item | Why it waits | M5+ home |
|------|--------------|----------|
| Large-page L3 discharge | Spec only (M4.8) | **M5.7** |
| NUMA in ghost spec | Not in M4 scope | **M5.8** |
| Frame-allocator ↔ EPT L3 beyond `ConcreteEptMap` | Refine scoped in M4.9 | **M5.9** |
| HW PTE identity-builder correspondence | Exec EPT still L1-ish | **M5.9** / M6 |
| EPT violation exclusivity | Not in M4 scope | M5–**M6** |
| Live migration page transfer | ADR-004 M6 row | **M6** |
| Full mgmt plane / audit reports | Product spine | **M5.0–M5.4** |
| VMware / vCenter import | Outside Proven Core | **M5.5** (ADR-007) |
| Dell Tier‑2 OEM Redfish | Partnership / reverse-eng | Deferred (ADR-005); not M5 exit |
| HA / soak / external audit | Production bar | **M6** |

---

## Subgates

Each = branch `cursor/m5-N-…-a623`, marker `RAYNU-V-M5-*-OK`, Latitude and/or host gate, docs touch.

### Track A — Management plane (ops spine)

### M5.0 — VM lifecycle API — `RAYNU-V-M5-LIFE-OK`

**Status: closed** (host `./tools/m5-life-smoke.sh` → `RAYNU-V-M5-LIFE-OK`)

**Goal:** Create / start / stop / destroy guests through a durable lifecycle surface (`mgmt/`), not only hardcoded bring-up in `src/main.rs`. Stubs already exist (`mgmt::VmLifecycle`); this gate makes them real against the M4 multi-guest spine.

**Shipped / wiring:**

1. `VmTable` with `create` / `start` / `stop` / `destroy` (Defined → Running → Stopped → Destroyed).
2. Audit events `VmCreated` / `VmStarted` / `VmStopped` / `VmDestroyed` via `audit_log!`.
3. Host gate `mgmt/m5_life_gate.rs` + `tools/m5-life-smoke.sh` + CI `m5-life`.
4. Live VMLAUNCH path remains in `src/main.rs` / `vmx/launch.rs` — mgmt is the durable ops surface for M5.1+.

**Acceptance (met):** Host smoke + gate → `RAYNU-V-M5-LIFE-OK`. Does not require Web UI or REST (M5.1 / M5.2).

**Files:** `mgmt/mod.rs`, `mgmt/m5_life_gate.rs`, `audit/integrity.rs`, `tools/m5-life-smoke.sh`, `.github/workflows/ci.yml`.

### M5.1 — Control plane: CLI + REST — `RAYNU-V-M5-API-OK`

**Status: open** ← next

**Goal:** Operator can drive lifecycle over CLI and a minimal REST API (same ops as M5.0).

**Acceptance sketch:**

1. CLI subcommands (or documented equivalent) for create/start/stop/destroy/list.
2. REST endpoints covering the same verbs; auth may be stubbed with documented GAP → M6.
3. Host/CI smoke → `RAYNU-V-M5-API-OK`.
4. Binary size stays under ADR-003 budget (lazy assets OK).

**Likely files:** `mgmt/`, `tools/`, host gate, docs.

### M5.2 — Embedded Web UI — `RAYNU-V-M5-WEBUI-OK`

**Status: open**

**Goal:** Embedded Web UI SPA (ADR-003 `.assets.webui`) drives the same lifecycle surface.

**Acceptance sketch:**

1. Lazy-decompressed Web UI asset present; first-use load path works.
2. UI can list VMs and invoke start/stop (against M5.1 API).
3. Marker `RAYNU-V-M5-WEBUI-OK`; size budget respected.
4. Polish UX is not required to close — functional ops surface is.

**Likely files:** `mgmt/`, asset pack, host gate.

---

### Track B — Audit trail ([A])

### M5.3 — Audit ring + hash chain — `RAYNU-V-M5-AUDIT-OK`

**Status: open**

**Goal:** Append-only audit ring with hash chaining; security-relevant actions from M5.0+ land in the ring. Builds on existing `audit/integrity` + `audit_log!` (L0).

**Acceptance sketch:**

1. `audit/` ring buffer + hash chain; tamper-evident verify path (host gate).
2. Mandatory events for VMCS/EPT/lifecycle at least (existing `audit_log!` categories).
3. Marker `RAYNU-V-M5-AUDIT-OK`.
4. Report templates are M5.4 — this gate is integrity of the log itself.

**Likely files:** `audit/`, host gate, CI smoke.

### M5.4 — SOX / ISO-style reports — `RAYNU-V-M5-REPORT-OK`

**Status: open** (**M5 product exit criterion** with M5.0–M5.3)

**Goal:** Generate auditor-facing reports (JSON/CSV minimum; PDF optional) from the audit ring using embedded schemas (ADR-003 `.assets.schemas`).

**Acceptance sketch:**

1. At least one SOX-style and one ISO-style (or documented equivalent) report template.
2. Report generation is deterministic given a frozen ring snapshot.
3. Marker `RAYNU-V-M5-REPORT-OK`.
4. External auditor sign-off is **M6** — this gate is generator + templates.

**Likely files:** `audit/`, report assets, host gate.

---

### Track C — Dell hardware ops ([D]; Tier‑1 only)

### M5.6 — Dell Tier‑1 health + topology — `RAYNU-V-M5-IDRAC-OK`

**Status: open** (may start after M5.0; parallel with Track B)

**Goal:** iDRAC / Redfish Tier‑1 health (thermal, fan, PSU) + SMBIOS/ACPI topology visible to ops (ADR-005). Builds on `idrac/` stubs (`IdracTier::Tier1`).

**Acceptance sketch:**

1. Redfish client reads thermal/fan/PSU (or documented QEMU/mock path for CI).
2. SMBIOS DIMM / ACPI MADT (+ SRAT/SLIT if available) surfaces NUMA/socket layout to mgmt.
3. Marker `RAYNU-V-M5-IDRAC-OK`.
4. Tier‑2 (PERC OEM, predictive failure) explicitly out of scope — documented GAP.

**Likely files:** `idrac/`, `mgmt/`, host gate; Latitude when hardware available.

**Numbering note:** **M5.5** is reserved for the VMware workstream (below), so Dell health is **M5.6**.

---

### Track D — Proof (ADR-004 M5 row; bolt-on)

May start once M4.8/M4.9 are green (already closed). Must complete before **M5 closed**.

### M5.7 — Large-page L3 discharge — `RAYNU-V-M5-LPAGE-VERIFY-OK`

**Status: open** (host-first; **M5 proof exit criterion**)

**Goal:** Green `cargo verus verify -p ept_model` for large-page (2M/1G) map/unmap exclusivity — **no `admit`**. Closes `GAP: Large-page L3 discharge`.

**Acceptance sketch:**

1. Theorems for large-page span exclusivity; marker `RAYNU-V-M5-LPAGE-VERIFY-OK`.
2. CI hard-fail job (same pattern as M4.7).
3. Live path may keep 4K registry + HW large leaves; full HW correspondence is M5.9 / M6.

**Likely files:** `ept_model/`, `memory/ept_proof.rs`, verify smoke, CI, ADR-004 / ADR-006.

### M5.8 — NUMA in ghost spec — `RAYNU-V-M5-NUMA-OK`

**Status: open** (host-first)

**Goal:** NUMA topology in the **ghost spec** (ADR-004 M5 row). Proof attempt may stay L2 if needed; document GAPs.

**Acceptance sketch:**

1. Spec + runtime hooks tying SRAT/SLIT (or bring-up mock) into ghost NUMA domains.
2. Marker `RAYNU-V-M5-NUMA-OK`.
3. Full NUMA exclusivity L3 may remain GAP → M6 if verify cost is high — document explicitly.

**Likely files:** `ept_model/`, `memory/ept_spec.rs`, host gate.

### M5.9 — Allocator↔EPT + HW PTE correspondence — `RAYNU-V-M5-ALLOC-REFINE-OK`

**Status: open** (host-first)

**Goal:** Deepen refine: frame-allocator coupling and/or HW PTE identity-builder correspondence under `abs` / `refines` (close M4.9 deferred GAPs as far as feasible).

**Acceptance sketch:**

1. No `admit` on theorems in scope; marker `RAYNU-V-M5-ALLOC-REFINE-OK`.
2. Remaining HW PTE / EPT-violation gaps explicitly listed → M6.
3. Live multi-VM path keeps runtime asserts.

**Likely files:** `ept_model/`, `memory/frame_allocator*.rs`, `memory/ept_hw.rs`, refine smoke + gate.

---

### Parallel workstream (not on M5 critical path)

### M5.5 — VMware / vCenter import — `RAYNU-V-M5-MIGRATE-OK`

**Status: open** (ADR-007; **outside Proven Core**)

**Goal:** Migrate **10+** VMs from vCenter in one command (`migrate/`).

**Acceptance sketch:**

1. One-command import path (VMDK/OVF as documented).
2. Audit events for migrate start/complete/fail ([A]).
3. Marker `RAYNU-V-M5-MIGRATE-OK`.
4. Must not pull new modules into Proven Core without a separate ADR.
5. May close after or during late M5; **M5 closed does not require M5.5**.

**Likely files:** `migrate/`, assets, ops docs.

---

## Out of scope for M5

| Item | Milestone |
|------|-----------|
| Live migration + full EPT proof incl. transfer | **M6** |
| HA / security harden / 72-hr soak | **M6** |
| External security audit (auditor runs `verus --verify`) | **M6** |
| Dell Tier‑2 OEM Redfish / predictive failure | Deferred (ADR-005) |
| Folding migrate into Proven Core | Needs new ADR — default **no** |

---

## Execution order

```
Track A (mgmt):   M5.0 → M5.1 → M5.2
Track B (audit):  start after M5.0 → M5.3 → M5.4
Track C (Dell):   start after M5.0 → M5.6 (Tier-1 only)
Track D (proof):  start anytime post-M4.9 → M5.7 → M5.8 → M5.9

Parallel:         M5.5 VMware migrate (ADR-007; slip-ok)

M5 closed when: LIFE + API + AUDIT + REPORT green,
                and LPAGE-VERIFY green (ADR-004 M5 row).
                WEBUI + IDRAC + NUMA + ALLOC-REFINE should be closed
                or explicitly waived with ADR note.
                MIGRATE (M5.5) is optional for M5 closed.
```

**M5 closed ⇒ next is M6 (production ready).**

---

## Milestone acceptance (target)

```text
RAYNU-V-M5-LIFE-OK
RAYNU-V-M5-API-OK
RAYNU-V-M5-AUDIT-OK
RAYNU-V-M5-REPORT-OK
RAYNU-V-M5-LPAGE-VERIFY-OK
==> Mgmt + audit smoke PASSED
==> Host large-page L3-verify smoke PASSED
```

Optional / slip-ok with docs: `RAYNU-V-M5-WEBUI-OK`, `RAYNU-V-M5-IDRAC-OK`, `RAYNU-V-M5-NUMA-OK`, `RAYNU-V-M5-ALLOC-REFINE-OK`, `RAYNU-V-M5-MIGRATE-OK`.

---

## First action

Draft accepted. **M5.0 closed** on host (`RAYNU-V-M5-LIFE-OK`). Next: **M5.1** (`RAYNU-V-M5-API-OK`) — CLI + REST over the lifecycle surface.
