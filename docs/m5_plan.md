# M5 Plan — Operationally Viable

**Status:** **open** — M5.0–M5.8 closed on Latitude; **M5.9** ALLOC-REFINE wired host/CI (Latitude pending).  
**Prior:** M5.8 closed on Latitude (`RAYNU-V-M5-NUMA-OK`; `51 verified, 0 errors`); M5.7 closed (`RAYNU-V-M5-LPAGE-VERIFY-OK`).  
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

**Status: closed** (Latitude `./tools/m5-life-smoke.sh` → `RAYNU-V-M5-LIFE-OK`)

**Goal:** Create / start / stop / destroy guests through a durable lifecycle surface (`mgmt/`), not only hardcoded bring-up in `src/main.rs`. Stubs already exist (`mgmt::VmLifecycle`); this gate makes them real against the M4 multi-guest spine.

**Shipped / wiring:**

1. `VmTable` with `create` / `start` / `stop` / `destroy` (Defined → Running → Stopped → Destroyed).
2. Audit events `VmCreated` / `VmStarted` / `VmStopped` / `VmDestroyed` via `audit_log!`.
3. Host gate `mgmt/m5_life_gate.rs` + `tools/m5-life-smoke.sh` + CI `m5-life`.
4. Live VMLAUNCH path remains in `src/main.rs` / `vmx/launch.rs` — mgmt is the durable ops surface for M5.1+.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-LIFE-OK`. Does not require Web UI or REST (M5.1 / M5.2).

**Files:** `mgmt/mod.rs`, `mgmt/m5_life_gate.rs`, `audit/integrity.rs`, `tools/m5-life-smoke.sh`, `.github/workflows/ci.yml`.

### M5.1 — Control plane: CLI + REST — `RAYNU-V-M5-API-OK`

**Status: closed** (Latitude `./tools/m5-api-smoke.sh` → `RAYNU-V-M5-API-OK`)

**Goal:** Operator can drive lifecycle over CLI and a minimal REST API (same ops as M5.0).

**Shipped / wiring:**

1. `mgmt/api.rs` — `parse_cli` / `dispatch_cli` for `create|start|stop|destroy|list`.
2. Same file — `dispatch_rest` routes (`GET/POST/DELETE /vms…`); **auth stubbed** (`GAP: REST auth stubbed → M6`).
3. `VmTable::list` for list/GET; host gate `mgmt/m5_api_gate.rs` + `tools/m5-api-smoke.sh` + CI `m5-api`.
4. No HTTP crate / no TCP stack (keeps ADR-003 size budget); host-testable request shapes only.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-API-OK`. Does not require Web UI (M5.2).

### M5.2 — Embedded Web UI — `RAYNU-V-M5-WEBUI-OK`

**Status: closed** (Latitude `./tools/m5-webui-smoke.sh` → `RAYNU-V-M5-WEBUI-OK`)

**Goal:** Embedded Web UI SPA (ADR-003 `.assets.webui`) drives the same lifecycle surface.

**Shipped / wiring:**

1. `assets/webui.html` — compact SPA (list / start / stop against M5.1 `/vms` routes).
2. `mgmt/webui.rs` — PE `.aswebui` embed + first-use `load_webui()` (identity decompress;
   `GAP: webui zstd → keep under ADR-003 budget`).
3. `dispatch_webui_action` → M5.1 REST; host gate `mgmt/m5_webui_gate.rs` +
   `tools/m5-webui-smoke.sh` + CI `m5-webui`.
4. `tools/check-pe-assets.sh` verifies `.aswebui` on the UEFI binary.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-WEBUI-OK`. Track A (mgmt spine) complete.

---

### Track B — Audit trail ([A])

### M5.3 — Audit ring + hash chain — `RAYNU-V-M5-AUDIT-OK`

**Status: closed** (Latitude `./tools/m5-audit-smoke.sh` → `RAYNU-V-M5-AUDIT-OK`)

**Goal:** Append-only audit ring with hash chaining; security-relevant actions from M5.0+ land in the ring. Builds on existing `audit/integrity` + `audit_log!` (L0).

**Shipped / wiring:**

1. `AuditRing` append + `verify_chain` + `tamper_hash_at`; public `boot_ring_verify`.
2. Mandatory categories on-chain: VMCS / EPT map+unmap / MSR block / lifecycle.
3. Call-site wiring: `vmx/vmcs` (`VmcsCreated`), `main` (`EptMapped`), `ept::unmap` firmware
   (`EptUnmapped`), `launch` MSR #GP (`MsrBlocked`), `mgmt` lifecycle.
4. Host gate `audit/m5_audit_gate.rs` + `tools/m5-audit-smoke.sh` + CI `m5-audit`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-AUDIT-OK`. Reports are M5.4.

### M5.4 — SOX / ISO-style reports — `RAYNU-V-M5-REPORT-OK`

**Status: closed** (Latitude `./tools/m5-report-smoke.sh` → `RAYNU-V-M5-REPORT-OK`)

**Goal:** Generate auditor-facing reports (JSON/CSV minimum; PDF optional) from the audit ring using embedded schemas (ADR-003 `.assets.schemas`).

**Shipped / wiring:**

1. Schemas: `assets/schemas/sox_access_control.json` + `iso_event_inventory.json` (PE `.aschema`).
2. `RingSnapshot::from_ring` + deterministic `render_report` (JSON/CSV); PDF = `GAP: PDF report → M6`.
3. Host gate `audit/m5_report_gate.rs` + `tools/m5-report-smoke.sh` + CI `m5-report`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-REPORT-OK`. Track B (audit) complete. External auditor sign-off remains **M6**.

---

### Track C — Dell hardware ops ([D]; Tier‑1 only)

### M5.6 — Dell Tier‑1 health + topology — `RAYNU-V-M5-IDRAC-OK`

**Status: closed** (Latitude `./tools/m5-idrac-smoke.sh` → `RAYNU-V-M5-IDRAC-OK`)

**Goal:** iDRAC / Redfish Tier‑1 health (thermal, fan, PSU) + SMBIOS/ACPI topology visible to ops (ADR-005). Builds on `idrac/` stubs (`IdracTier::Tier1`).

**Shipped / wiring:**

1. Mock Redfish Tier‑1 thermal/fan/PSU (`assets/idrac/mock_redfish.json`) via `read_tier1_health`.
2. SMBIOS DIMM + ACPI MADT/SRAT/SLIT topology (`assets/idrac/mock_topology.txt`) → `TopologySnapshot`.
3. Host gate `idrac/m5_idrac_gate.rs` + `tools/m5-idrac-smoke.sh` + CI `m5-idrac`.
4. Live BMC HTTP and Tier‑2 OEM remain documented GAPs (`GAP: live Redfish BMC → polish`, `GAP: Dell Tier-2 OEM`).

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-IDRAC-OK`.

**Numbering note:** **M5.5** is reserved for the VMware workstream (below), so Dell health is **M5.6**.

---

### Track D — Proof (ADR-004 M5 row; bolt-on)

May start once M4.8/M4.9 are green (already closed). Must complete before **M5 closed**.

### M5.7 — Large-page L3 discharge — `RAYNU-V-M5-LPAGE-VERIFY-OK`

**Status: closed** (Latitude `./tools/verus-lpage-verify-smoke.sh` → `RAYNU-V-M5-LPAGE-VERIFY-OK`; `47 verified, 0 errors`)
(**M5 proof exit criterion** — met)

**Goal:** Green `cargo verus verify -p ept_model` for large-page (2M/1G) map/unmap exclusivity — **no `admit`**. Closes `GAP: Large-page L3 discharge`.

**Shipped / wiring:**

1. `ghost_large_map` / `ghost_large_unmap` + `theorem_large_page_map_unmap_exclusive`,
   `lemma_2m_map_unmap_exclusive`, `lemma_1g_map_unmap_exclusive`,
   `lemma_two_guests_large_map_distinct_spans_exclusive` in `ept_model` (no `admit`).
2. `GAP(CLOSED M5.7)` in `ept_proof.rs`; `TODO(M5.7 CLOSED)` in `ept_spec.rs`.
3. Host gate `memory/m5_lpage_verify_gate.rs` + `tools/verus-lpage-verify-smoke.sh` + CI `verus-lpage-verify`.
4. Live path may keep 4K registry + HW large leaves; full HW correspondence is M5.9 / M6.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-LPAGE-VERIFY-OK`.

### M5.8 — NUMA in ghost spec — `RAYNU-V-M5-NUMA-OK`

**Status: closed** (Latitude `./tools/verus-numa-smoke.sh` → `RAYNU-V-M5-NUMA-OK`; `51 verified, 0 errors`)

**Goal:** NUMA topology in the **ghost spec** (ADR-004 M5 row). Proof attempt may stay L2 if needed; document GAPs.

**Shipped / wiring:**

1. `GhostNumaTopology` + `numa_well_formed` / `slit_symmetric` / `numa_map_enabled` /
   `guest_frames_on_node` / `mock_bringup_numa` in `ept_model` (no `admit`).
2. Discharged: `lemma_mock_bringup_numa_facts`, `lemma_slit_local_is_10`,
   `lemma_numa_map_ok_exclusive` (map under NUMA affinity preserves exclusivity).
3. Runtime hook `memory/numa.rs`: `HostNumaTopology` ← `idrac::TopologySnapshot` /
   `mock_topology.txt` (SRAT/SLIT) with synthetic frame affinity matching the ghost mock.
4. `GAP(CLOSED M5.8)` in `ept_proof.rs`; `TODO(M5.8 CLOSED)` in `ept_spec.rs`;
   open `GAP: NUMA affinity / exclusivity L3 (M6)`.
5. Host gate `memory/m5_numa_gate.rs` + `tools/verus-numa-smoke.sh` + CI `verus-numa`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-NUMA-OK`.

### M5.9 — Allocator↔EPT + HW PTE correspondence — `RAYNU-V-M5-ALLOC-REFINE-OK`

**Status: open** (host/CI wired; Latitude smoke pending)

**Goal:** Deepen refine: frame-allocator coupling and/or HW PTE identity-builder correspondence under `abs` / `refines` (close M4.9 deferred GAPs as far as feasible).

**Shipped / wiring:**

1. `GhostFramePool` + `alloc_ept_refines` / `alloc_map_enabled` in `ept_model` (no `admit`).
2. Discharged: `theorem_alloc_map_unmap_refines`, `lemma_alloc_map_ok_refines`,
   `lemma_alloc_unmap_ok_refines`, `lemma_allocate_preserves_pool`.
3. Scoped precise-identity GPA==HPA: `PRECISE_IDENTITY_FRAMES` / `identity_leaf_ok` /
   `lemma_identity_leaf_gpa_eq_hpa` (matches `ept_hw::PRECISE_BYTES`).
4. `GAP(CLOSED M5.9)` for allocator coupling + identity abs; open
   `GAP: Hardware EPT PTE bit-decode / EPT-violation (M6)`.
5. Host gate `memory/m5_alloc_refine_gate.rs` + `tools/verus-alloc-refine-smoke.sh` + CI `verus-alloc-refine`.

**Acceptance:** Latitude `./tools/verus-alloc-refine-smoke.sh` → `RAYNU-V-M5-ALLOC-REFINE-OK` (then close docs/site).

---

### Parallel workstream (not on M5 critical path)

### M5.5 — VMware / vCenter import — `RAYNU-V-M5-MIGRATE-OK`

**Status: closed** (Latitude `./tools/m5-migrate-smoke.sh` → `RAYNU-V-M5-MIGRATE-OK`)
(ADR-007; **outside Proven Core**; not required for M5 closed)

**Goal:** Migrate **10+** VMs from vCenter in one command (`migrate/`).

**Shipped / wiring:**

1. One-command `migrate_one_command` over a documented OVF/VMDK inventory
   (`assets/migrate/sample_inventory.txt`, ≥12 guests).
2. Audit events `MigrateStarted` / `MigrateCompleted` / `MigrateFailed`.
3. Guests land in `VmTable` as `Defined` (`MGMT_GUEST_CAP` raised to 16).
4. Host gate `migrate/m5_migrate_gate.rs` + `tools/m5-migrate-smoke.sh` + CI `m5-migrate`.
5. Live vCenter SOAP/REST client remains `GAP: live vCenter API → polish`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M5-MIGRATE-OK`.

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

Optional / slip-ok with docs: `RAYNU-V-M5-IDRAC-OK`, `RAYNU-V-M5-NUMA-OK`, `RAYNU-V-M5-ALLOC-REFINE-OK`.

---

## First action

**M5.0–M5.8 closed** on Latitude. **M5.9** wired host/CI (`RAYNU-V-M5-ALLOC-REFINE-OK`); Latitude smoke pending — then M5 close (or ADR waiver for remaining M6 GAPs).
