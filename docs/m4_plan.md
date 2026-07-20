# M4 Plan — Usable VM Platform

**Status:** **open** — M4.5 in progress (SMP → `RAYNU-V-M4-SMP-OK`).  
**Prior:** M4.4 closed on Latitude (`RAYNU-V-M4-NET-OK`).  
**Parent roadmap:** [CLAUDE.md](../CLAUDE.md) (M4 row) · lived gates: [progress.md](progress.md)  
**Prior track:** [m3_post_shell_plan.md](m3_post_shell_plan.md) · EPT theorem: [adr/ADR-004.md](adr/ADR-004.md)

M3 delivered one real Linux guest under tight EPT with scoped true L3 + refine.  
**M4** turns that into a **usable multi-VM platform**: 4+ VMs, SMP, storage, network, and EPT L3 for **N guests**.

---

## Strategy (accepted)

**Platform bring-up is the spine; bolt proof onto the first moment multi-guest is real.**

- Do **not** proof-first (N-guest L3 before a second VM exists).
- Do **not** defer all proof until after M4 (ADR-004 / R01: N-guest L3 is an M4 exit criterion).
- **M4.0–M4.2** make multi-guest real in exec (ownership asserts from day one).
- **M4.3–M4.5** make the platform believable (blk, net, SMP).
- **M4.6–M4.7** are a **mid-milestone freeze**: N-guest L3 verify before calling M4 closed.
- **M4.8–M4.9** add large-page *spec* + refine; large-page *proof* stays **M5** (ADR-004).

```
M4.0 2VM → M4.1 sched → M4.2 4VM
       → M4.3 blk → M4.4 net → M4.5 SMP
       → M4.6 N-guest spec → M4.7 N-guest L3 verify
       → M4.8 large-page spec → M4.9 N-guest refine
→ M4 closed → M5
```

---

## Debt / open items inherited from M3

| Item | Why it waits | M4+ home |
|------|--------------|----------|
| N-guest exclusivity in ghost + proof | Single-guest L3 only (M3.17/18) | M4.6–M4.7 |
| Large pages (2M/1G) in ghost model | Scoped 4K proof | M4.8 (spec); proof M5 |
| Frame-allocator ↔ EPT L3 coupling | Refine scoped to `ConcreteEptMap` | M4.9 / M5 |
| HW PTE identity builder correspondence | Exec EPT still L1-ish | M4+ / M5 |
| EPT violation exclusivity | Not in M3 scope | M5–M6 |
| Live migration page transfer | ADR-004 M6 row | M6 |
| IOAPIC / drop `noapic` | PIC path still stubbed | M4+ if needed for SMP I/O |
| Virtio / vSwitch / credit scheduler | Stubs only (`devices/`, `net/`, `sched/`) | M4.1–M4.5 |
| VMware import | Outside Proven Core | **M5.5** (ADR-007) — not M4 |

---

## Subgates

Each = branch `cursor/m4-N-…-a623`, marker `RAYNU-V-M4-*-OK`, Latitude and/or host gate, docs touch.

### Track A — Multi-VM core (spine)

### M4.0 — Second guest under EPT — `RAYNU-V-M4-2VM-OK`

**Status: closed** (Latitude `Boot gate PASSED (M0 → M4.0)`)

**Goal:** Two guests under distinct EPT ownership. G0 remains real Linux to SHELL; G1 is a second VMCS on a private 2 MiB EPT slab that latches SHELL via CPUID. No shared guest frames (G1 HPA unmapped from G0 EPT). Scheduling fairness is **M4.1**.

**Shipped / wiring:**

1. `build_single_2m_identity` + `clear_2m_identity_leaf` + `write_guest_shell_cpuid_page`.
2. `claim_precise_with_guest1_hole` — G0 precise window with G1 HPA punched out (`M4_GUEST1_ID=2`).
3. G1 slab HPA chosen in `[GUEST_RAM, PRECISE)` (above e820), **outside** the HV `FrameAllocator` pool (Latitude low-memory pool).
4. G1 uses a **second precise identity EPT** + host CR3 (G0 has the slab leaf cleared). Private-only EPT/slab-CR3 deferred — triple-faulted on Latitude.
5. Host TSS/GDT installed once and reused — VM-exit forces `GDTR.limit=FFFF` (cannot re-copy).
6. After G0 SHELL+APIC+NOIRQ: mask host LAPIC, `try_launch_second_guest` → G1 VMLAUNCH; dedicated G1 exit path (EOI leftover EXT_INT) → `RAYNU-V-M4-SHELL-G1` + `RAYNU-V-M4-2VM-OK`.
7. Host gate `memory/m4_2vm_gate.rs`; qemu pass line `M0 → M4.0`.

**Acceptance (met):** Latitude `./tools/qemu-boot-test.sh` → `Boot gate PASSED (M0 → M4.0)` with prior M3.22 chain + `RAYNU-V-M4-2VM-OK`.

**Files:** `memory/ept_hw.rs`, `memory/ept.rs`, `memory/m4_2vm_gate.rs`, `memory/frame_allocator.rs`, `vmx/launch.rs`, `src/main.rs`, `tools/qemu-boot-test.sh`.

**Note:** Dual *real Linux* guests remains a stretch; G1 SHELL latch under distinct EPT proves the dual-VMCS / dual-ownership spine for M4.1+.

### M4.1 — Scheduler time-slices ≥2 VMs — `RAYNU-V-M4-SCHED-OK`

**Status: closed** (Latitude `Boot gate PASSED (M0 → M4.1)`)

**Goal:** Credit (or equivalent) scheduler runs ≥2 runnable VMs; both make forward progress under preemption / yield (not “boot VM0 then freeze”).

**Shipped / wiring:**

1. `CreditScheduler::consume_quantum` + `pick_next_fair` — alternate G0/G1.
2. Retain G0 `LaunchFrames` (`FIRST_GUEST`); after G1 SHELL/`RAYNU-V-M4-2VM-OK`, enter `SCHED_MODE`.
3. Host LAPIC one-shot preempt → EOI → consume → `VMPTRLD` other VMCS → VMRESUME.
4. Per-guest GPR banks; markers `RAYNU-V-M4-SLICE-G0` / `SLICE-G1` then `RAYNU-V-M4-SCHED-OK`.
5. Host gate `sched/m4_sched_gate.rs`; qemu pass line `M0 → M4.1`.

**Acceptance (met):** Latitude `./tools/qemu-boot-test.sh` → `Boot gate PASSED (M0 → M4.1)` with M4.0 chain + `RAYNU-V-M4-SCHED-OK`.

**Files:** `sched/scheduler.rs`, `sched/m4_sched_gate.rs`, `vmx/launch.rs`, `tools/qemu-boot-test.sh`.

### M4.2 — Scale gate: 4+ concurrent shells — `RAYNU-V-M4-NVM-OK`

**Status: closed** (Latitude `Boot gate PASSED (M0 → M4.2)`)

**Goal:** **4+** concurrent guests to shell under EPT — roadmap “4+ VMs” gate. MV: G0 real Linux SHELL + G1–G3 SHELL-CPUID guests (distinct EPT ownership), credit-scheduled.

**Shipped / wiring:**

1. `claim_precise_with_shell_holes` — G0 precise window with three HPA slabs punched out.
2. `set_shell_guest` slots 1–3; cascade VMLAUNCH after G0 SHELL; then `SCHED_MODE` across 4 slots.
3. Markers `SLICE-G0`…`G3` → `SCHED-OK` (G0+G1) → `RAYNU-V-M4-NVM-OK` (all four).
4. Host gate `sched/m4_nvm_gate.rs`; qemu pass line `M0 → M4.2`.

**Acceptance (met):** Latitude `./tools/qemu-boot-test.sh` → `Boot gate PASSED (M0 → M4.2)` with 2VM + SCHED + NVM markers.

**Files:** `memory/ept.rs`, `vmx/launch.rs`, `src/main.rs`, `sched/m4_nvm_gate.rs`, `tools/qemu-boot-test.sh`.

---

### Track B — Platform I/O

### M4.3 — Virtio-blk (guest disk) — `RAYNU-V-M4-BLK-OK`

**Status: closed** (Latitude `Boot gate PASSED (M0 → M4.3)`)

**Goal:** Guest root or data disk via Virtio-blk (or documented equivalent); guest can read/write without host COM1 crutches.

**Accepted MV (this gate):** virtio-mmio over an EPT hole + bare-metal probe guest. On `DRIVER_OK` the host write+readbacks an in-memory disk image (FrameAllocator-backed, not guest-exclusive) and latches `RAYNU-V-M4-BLK-OK`. Full Linux root-on-virtio is later polish — not required to close M4.3.

**Acceptance (met):**

1. `devices/virtio_blk.rs` MMIO config/status + EPT violation path (`apply_virtio_mov`).
2. Latitude: `RAYNU-V-M4-BLK-OK` after DRIVER_OK write/readback (post NVM-OK probe guest).
3. Disk frames from FrameAllocator pool (host-owned; not guest-exclusive slabs).

**Files:** `devices/virtio_blk.rs`, `devices/m4_blk_gate.rs`, `memory/ept_hw.rs`, `vmx/launch.rs`, `vmx/mmio_decode.rs`, `src/main.rs`, `tools/qemu-boot-test.sh`.

### M4.4 — Virtio-net + minimal vSwitch — `RAYNU-V-M4-NET-OK`

**Status: closed** (Latitude `Boot gate PASSED (M0 → M4.4)`)

**Goal:** Guest↔guest or guest↔host networking via Virtio-net + L2 learning switch (`net/` stub → real).

**Accepted MV (this gate):** two virtio-mmio net BARs + host L2 learning `VSwitch`. A bare-metal probe guest handshakes both ports to `DRIVER_OK`; the host injects an Ethernet frame port0→port1 into allocator-backed RX buffers and latches `RAYNU-V-M4-NET-OK`. Full Linux virtio-net / TAP is later polish.

**Acceptance (met):**

1. `devices/virtio_net.rs` + `net::VSwitch` forward/learn; MMIO exit path.
2. Latitude: `RAYNU-V-M4-NET-OK` after dual-port exchange (post BLK-OK probe).
3. Packet buffers from FrameAllocator pool (host-owned; not guest-exclusive slabs).

**Files:** `net/mod.rs`, `devices/virtio_net.rs`, `devices/m4_net_gate.rs`, `memory/ept_hw.rs`, `vmx/launch.rs`, `vmx/mmio_decode.rs`, `src/main.rs`, `tools/qemu-boot-test.sh`.

### M4.5 — SMP guest (2+ vCPUs) — `RAYNU-V-M4-SMP-OK`

**Status: in progress** ← current

**Goal:** One guest with **2+ vCPUs** reaches shell (AP bring-up under virtual APIC).

**Accepted MV (this gate):** bare-metal **BSP + AP** under two VMCS, **same guest id**, **shared EPT** (G0 EPTP). After NET-OK the host VMLAUNCHes the BSP; when BSP stores a ready flag and HLTs, the host performs a **documented AP wake** (VMLAUNCH of the AP VMCS — INIT-SIPI equivalent for this gate). When both ready flags are seen, latch `RAYNU-V-M4-SMP-OK`. Full Linux `CONFIG_SMP` / ICR Wait-for-SIPI / IOAPIC is deferred (slip-ok vs proof track).

**Acceptance sketch:**

1. Second VMCS/vCPU for same guest id; shared EPT; INIT-SIPI or documented AP wake. ✅
2. May retain `noapic` only if SMP still works; prefer progress toward IOAPIC if blocked. (Linux UP retained; probe does not need IOAPIC.)
3. Marker `RAYNU-V-M4-SMP-OK`.

**Likely files:** `sched/smp_probe.rs`, `vmx/launch.rs`, `memory/ept_hw.rs`, `src/main.rs`, `tools/qemu-boot-test.sh`.

---

### Track C — Proof (ADR-004 M4 row; bolt-on after multi-guest is real)

May start once **M4.0** (preferably **M4.2**) is green. Must complete before **M4 closed**.

### M4.6 — N-guest exclusivity in ghost model — `RAYNU-V-M4-NGUEST-SPEC-OK`

**Status: open** (host-first)

**Goal:** Extend `ept_model` ghost map/unmap to **N guests**; L2→L3 *attempt* with explicit gaps documented.

**Acceptance sketch:**

1. Close `TODO(M4): N guests` in `memory/ept_spec.rs` / `ept_proof.rs` GAP list (spec side).
2. Host smoke + gate → `RAYNU-V-M4-NGUEST-SPEC-OK`.
3. Does not yet claim ADR-006 L3 for N guests (that is M4.7).

**Likely files:** `ept_model/`, `memory/ept_proof.rs`, `memory/ept_spec.rs`, tools smoke + gate.

### M4.7 — True L3 N-guest verify — `RAYNU-V-M4-NGUEST-VERIFY-OK`

**Status: open** (host-first; **M4 exit criterion**)

**Goal:** Green `cargo verus verify -p ept_model` for N-guest map/unmap exclusivity — **no `admit`**.

**Acceptance sketch:**

1. Theorem(s) for ≥2 guests; marker `RAYNU-V-M4-NGUEST-VERIFY-OK`.
2. CI hard-fail job (same pattern as M3.17).
3. Live multi-VM path keeps runtime asserts; full ghost↔exec refine is M4.9.

**Likely files:** `ept_model/`, verify smoke, CI, ADR-004 / ADR-006 notes.

### M4.8 — Large-page (2M/1G) in ghost spec — `RAYNU-V-M4-LPAGE-OK`

**Status: open** (host-first)

**Goal:** Large pages in the **ghost spec** (ADR-004: may stay L2). Proof attempt deferred to **M5**.

**Acceptance sketch:**

1. Spec + Kani/runtime hooks as appropriate; marker `RAYNU-V-M4-LPAGE-OK`.
2. Document GAP for L3 large-page discharge → M5.

**Likely files:** `ept_model/`, `memory/ept_spec.rs`, `memory/ept_proof.rs`.

### M4.9 — N-guest ghost↔exec refine — `RAYNU-V-M4-REFINE-OK`

**Status: open** (host-first)

**Goal:** Refine multi-guest exec registry / allocator coupling under `abs` / `refines` (extend M3.18 pattern).

**Acceptance sketch:**

1. No `admit` on refine theorems in scope; marker `RAYNU-V-M4-REFINE-OK`.
2. HW PTE identity correspondence may remain GAP → M5.

**Likely files:** `ept_model/`, `memory/ept.rs`, refine smoke + gate.

---

## Out of scope for M4

| Item | Milestone |
|------|-----------|
| Full mgmt plane, audit/SOX reports | **M5** |
| Large-page **proof** + NUMA in spec | **M5** (ADR-004) |
| VMware / vCenter import | **M5.5** (ADR-007) |
| Live migration + full EPT proof incl. transfer | **M6** |
| 72-hr soak / external audit | **M6** |

---

## Execution order

```
Track A (spine):  M4.0 → M4.1 → M4.2
Track B (I/O):    M4.3 → M4.4 → M4.5 (SMP may slip)
Track C (proof):  start after M4.0/M4.2 → M4.6 → M4.7 → M4.8 → M4.9

M4 closed when: NVM + BLK + NET green, and NGUEST-VERIFY green.
                SMP + LPAGE + REFINE should be closed or explicitly waived with ADR note.
```

**M4 closed ⇒ next is M5 (operationally viable).**

---

## Milestone acceptance (target)

```text
RAYNU-V-M4-2VM-OK
RAYNU-V-M4-SCHED-OK
RAYNU-V-M4-NVM-OK
RAYNU-V-M4-BLK-OK
RAYNU-V-M4-NET-OK
RAYNU-V-M4-NGUEST-VERIFY-OK
==> Boot gate PASSED (M0 → M4.x; multi-VM)
==> Host N-guest L3-verify smoke PASSED
```

Optional / slip-ok with docs: `RAYNU-V-M4-SMP-OK`, `RAYNU-V-M4-LPAGE-OK`, `RAYNU-V-M4-REFINE-OK`.

---

## First action

**M4.5 in progress** on branch `cursor/m4-5-smp-a623` (`RAYNU-V-M4-SMP-OK`). Do not start M4.6 until Latitude is green (or explicitly slip SMP and start Track C).
