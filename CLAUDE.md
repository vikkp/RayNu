# CLAUDE.md — RayNu-V Hypervisor
# Derived from RayNu-V Production Roadmap v1.2 (June 2026)
# This file governs ALL code generation, review, and architectural decisions.

## Identity

RayNu-V is a clean-slate Type-1 bare-metal hypervisor built from scratch in Rust,
optimized first for Dell PowerEdge R640 / R650 / R660 servers. It is the world's
first formally verified bare-metal hypervisor.

It does NOT compete on feature count. It competes on trust, simplicity, and provability.

---

## The Four Pillars

Every line of code, every design decision, every PR must advance at least one pillar.
If it doesn't serve a pillar, it doesn't ship.

| Tag  | Pillar                          | Priority            |
|------|---------------------------------|---------------------|
| [V]  | Formally Verified Core          | LONG-TERM NORTH STAR |
| [Z]  | Zero-Config Single Binary       | NEAR-TERM BET       |
| [D]  | Dell iDRAC-Native               | NEAR-TERM BET       |
| [A]  | Audit-First / FinOps-Native     | MEDIUM-TERM BET     |

### Pillar Priorities — How They Interact

- **[V] is the architectural north star.** It shapes HOW we write code today — minimal
  unsafe, explicit state machines, documented invariants, clean ownership — even when
  proofs are months away. We never write code that structurally prevents future verification.
- **[Z], [D], [A] are the near-to-medium term product bets.** They deliver demonstrable
  value early and form the commercial identity. They ship first.
- When pillars conflict, resolve in this order: safety ([V] architecture) > correctness
  ([A] audit trail) > simplicity ([Z] single binary) > hardware depth ([D] iDRAC).

---

## Repository Structure

```
boot/           — Early boot, firmware handoff, CPU bring-up
vmx/            — VT-x / VMX setup, VMCS management, entry/exit
memory/         — Physical allocators, page tables, EPT, address translation
devices/        — Emulated and passthrough device logic
sched/          — vCPU scheduling and runstate management
net/            — Virtual switch, packet filtering, SR-IOV
audit/          — Audit ring buffer, hash chain, report generation
mgmt/           — CLI, REST API, Web UI, VM lifecycle
migrate/        — VMware migration engine (vCenter, VMDK, OVF)
idrac/          — iDRAC Redfish client, hardware health integration
arch/           — x86 / R640-specific helpers
docs/           — Architecture decisions, subsystem notes
docs/adr/       — Architecture Decision Records (ADR-001 through ADR-008+)
tools/          — Build, debug, test, and verification scripts
```

---

## Proven Core Boundary

The Proven Core is the set of modules that receive formal Verus specifications and,
progressively, formal proofs. **Everything else** uses Rust's type system + testing only.

### INSIDE the Proven Core (~12,000 LOC budget: ~10,400 code + ~1,600 scaffolding)

| Module                    | LOC    | Why Inside (Criticality Reason)                                                |
|---------------------------|--------|--------------------------------------------------------------------------------|
| VMX lifecycle             | ~800   | Incorrect VMXON/VMXOFF = CPU in undefined VMX mode                             |
| VMCS management           | ~1,500 | Host-state corruption = guest owns the host (#1 escape vector)                 |
| EPT engine                | ~2,000 | THE memory isolation mechanism; single bug = silent host memory exposure        |
| Physical frame allocator  | ~1,500 | Double-alloc = two guests sharing a frame; UAF = guest accessing freed HV mem  |
| vCPU state management     | ~1,000 | Incomplete save/restore leaks host pointers to guest                           |
| Interrupt injection       | ~800   | Wrong injection = guest privilege escalation                                   |
| Hypercall interface       | ~500   | Only intentional guest→host channel; unvalidated = arbitrary host mem access   |
| MSR/CPUID/CR firewalls    | ~1,200 | Unfiltered MSR writes = guest subverts host security features                  |
| Audit log integrity       | ~600   | Tampered audit log = entire [A] pillar collapses                               |
| IPI confinement           | ~500   | Unconfined IPIs = cross-VM interference or host disruption                     |

**Hard limit: 15,000 LOC.** The ~3,000 LOC buffer is for proof scaffolding growth or
at most one small module promotion (requires ADR).

### OUTSIDE the Proven Core (do NOT formally verify)

- Device emulation (serial, RTC, virtio-blk, virtio-net, virtio-console)
- Virtual switch (L2 learning, VLAN, packet filtering)
- Management plane (CLI, REST API, Web UI, TOML config)
- VMware migration engine (vCenter reader, VMDK converter, config translator)
- Scheduler algorithms (credit-based, affinity, PLE)
- iDRAC integration (Redfish client, thermal, PERC, DIMM)
- Linux boot protocol (zero page, ACPI tables, kernel loader)
- Audit report generator (SOX/ISO templates, PDF/JSON/CSV)

### Scope Creep Rules

1. No module enters the Proven Core without a formal ADR justifying inclusion.
2. Default answer is always "outside."
3. Promotion requires LOC budget update + timeline impact assessment.
4. Demotion requires an ADR explaining why verification was abandoned.

---

## Formal Verification Maturity Model

Not all modules reach L3 at the same time. This is expected. The architecture is
always designed for L3 even when tooling isn't there yet.

| Level | Name                  | What It Means                                                        |
|-------|-----------------------|----------------------------------------------------------------------|
| L0    | Documented Invariants | Invariants in code comments + doc comments only                      |
| L1    | Runtime-Enforced      | assert!() / debug_assert!() at entry/exit. Kani for unsafe blocks.   |
| L2    | Spec-Written          | Verus .spec.rs files with ghost state, pre/post conditions.          |
| L3    | Proof-Complete        | cargo verus --verify passes. Spec satisfied for all inputs.          |

**Key rule:** L1 with extensive fuzz testing is far safer than unverified code.
Runtime assertions are KEPT even at L3 (defense-in-depth).

**Fallback chain:** Verus → Kani → runtime assertions + fuzzing. Never blocked.

### EPT Isolation Theorem — The Headline Proof

**ADR-004 formal statement:**
> For every valid EPT mapping from a guest-physical address to a host-physical frame,
> that frame is exclusively owned by the mapping guest and belongs to neither the
> hypervisor nor any other guest.

"Exclusively owned" = exactly one guest maps that frame at any point in time.
"Belongs to neither" = frame not in HV page tables and not in any other guest's EPT.
Must hold across: map, unmap, EPT violation handling, and live migration page transfer.

**Proof progression:**
- M2: Spec written (L2). Runtime assertions enforce. Kani bounded checks.
- M3: Proof attempted for 4K-page, single guest. Gaps documented.
- M4: Extended to N guests. Large page added to spec (may stay L2).
- M5: Large page proof attempted. NUMA added to spec.
- M6: Full proof including live migration. External spec review.

---

## File Conventions

### Proven Core Modules — Four-File Convention

```
module/
├── module.rs           # Executable Rust code
├── module_spec.rs      # Verus specifications (ghost state, pre/post conditions)
├── module_proof.rs     # Verus proofs (may be partial — gaps documented with TODO)
└── module_test.rs      # Kani bounded checks + fuzz targets + unit/integration tests
```

### All Other Modules — Standard Convention

```
module/
├── module.rs           # Executable Rust code
└── module_test.rs      # Unit tests + integration tests + fuzz targets
```

---

## Coding Standards

### Language & Safety

- **Language:** Rust (edition 2021). C only if Rust proves too slow for early boot stubs.
- **unsafe blocks:** Kept absolutely minimal. Every unsafe block MUST have:
  - `// SAFETY:` comment explaining why it's sound
  - `// KANI-TARGET` tag for future bounded verification
  - A corresponding entry in the module's _test.rs for Kani checking
- **No hidden global state.** All mutable globals require a spinlock + justification comment.
- **Explicit state machines.** Use enum-based states, not boolean flags.
- **Clean ownership.** Every allocation has a documented owner. Every reference has a clear lifetime.
- **Defensive code.** Early assertions, explicit error paths. Panic > silent corruption.

### Invariant Documentation

Every function in the Proven Core MUST document its invariants:

```rust
/// Allocate a single physical frame.
///
/// INVARIANTS:
///   - Returned frame was NOT previously allocated
///   - After return, frame IS in the allocated set
///   - No other frame's allocation status changed
///
/// VERIFICATION: L2 (spec written) — see frame_allocator_spec.rs
/// FALLBACK: L1 (runtime assert at entry + exit)
pub fn allocate_frame(&mut self) -> Option<PhysFrame> {
    // ...
}
```

### Comments & Documentation

- All low-level hardware interactions get detailed comments citing the relevant
  Intel SDM / Dell iDRAC / PCIe spec section.
- Every VM exit reason handler documents: what triggered it, what we emulate, what we inject.
- Design decisions reference the relevant ADR (e.g., "Per ADR-004, this mapping must satisfy...").

### Audit Logging

Every security-relevant action MUST emit an audit event:

```rust
audit_log!(AuditEvent::VmcsCreated { timestamp, vcpu_id, vmcs_phys_addr });
audit_log!(AuditEvent::EptMapped { timestamp, guest_id, gpa, hpa, permissions });
audit_log!(AuditEvent::MsrBlocked { timestamp, vcpu_id, msr_index, access_type });
```

Categories of mandatory audit events:
- VMX lifecycle (enable, disable, launch, exit, resume)
- Memory operations (frame alloc/free, EPT map/unmap, EPT violation)
- VM lifecycle (create, start, stop, delete, snapshot, migrate)
- Admin actions (login, config change, quota change)
- Hardware events (thermal alert, PSU change, DIMM error)
- Security events (blocked MSR, blocked I/O port, #GP injection)

---

## Single-Binary Architecture [Z]

Everything compiles to one `r640-hypervisor.efi`. No OS, no packages, no config files.

### Binary Layout (PE/COFF sections)

```
.text / .rodata / .data / .bss  — Compiled Rust (HV core + all code)
.assets.kernel                   — zstd-compressed Linux kernel (~8 MB)
.assets.initrd                   — zstd-compressed initrd (~4 MB)
.assets.webui                    — zstd-compressed Web UI SPA (~1 MB)
.assets.schemas                  — zstd-compressed audit schemas (~100 KB)
.assets.vmconfigs                — Default VM config templates (TOML)
```

### Size Budget

- **Target:** 15 MB total
- **Hard limit:** 20 MB (triggers size audit if exceeded)
- **Rule:** Non-critical assets (Web UI, migration engine, report templates) are
  lazy-decompressed at first use. They do NOT affect boot time.
- **Fallback:** Split-mode deployment (.efi core + /assets/ on ESP) for constrained envs.

---

## Dell R640 Hardware Focus [D]

### Tier 1 — Low Risk (Self-Sufficient)

- COM1 serial via iDRAC virtual console
- Redfish API: thermal, fan, PSU status
- SMBIOS: DIMM topology, NUMA layout
- ACPI MADT: socket/core topology
- ACPI SRAT/SLIT: NUMA distance tables
- Intel X710 NIC awareness (onboard)
- NVMe direct passthrough

### Tier 2 — High Risk (Requires Dell Partnership or Reverse-Engineering)

- PERC H740P RAID health via Dell-specific Redfish extensions
- DIMM SPD detailed data beyond standard SMBIOS
- Predictive failure alerts (Dell-specific OEM Redfish schemas)
- Auto-throttle VMs on hardware degradation signals

**Rule:** Never block a milestone on Tier 2. Tier 1 is always sufficient to ship.

---

## Architecture Decision Records (ADRs)

All ADRs live in `docs/adr/`. Format: numbered, dated, context/decision/rationale/consequences.

| ADR   | Title                                  | Key Decision                                                   |
|-------|----------------------------------------|----------------------------------------------------------------|
| 001   | Why Verus + Kani                       | Dual-tool: Verus for correctness, Kani for unsafe blocks       |
| 002   | Proven Core Boundary                   | Only security-critical modules verified; scope changes need ADR |
| 003   | Single-Binary Asset Strategy           | PE/COFF sections + zstd + lazy load; 15 MB target, 20 MB limit |
| 004   | EPT Isolation Theorem Definition       | Tightened: "exclusively owned... belongs to neither"           |
| 005   | iDRAC Integration Tiers                | Tier 1 (low-risk, self-sufficient) vs Tier 2 (Dell partnership) |
| 006   | Verification Maturity Model            | L0→L1→L2→L3; ship at L1/L2 if tooling blocks L3               |
| 007   | VMware Migration as Dedicated Workstream | Own milestone (M5.5); outside Proven Core                     |
| 008   | Proof Maintenance & Toolchain Pinning  | Pin versions; nightly regression; ~1 week/quarter maintenance  |

**Rule:** Any new ADR is added here AND to `docs/adr/ADR-NNN.md`.

---

## Verification Toolchain

### Tools

- **Verus** (Microsoft Research) — SMT-based formal verification for Rust. Primary tool.
- **Kani** (Amazon) — Bounded model checking for unsafe Rust. Secondary tool.
- **AFL / libfuzzer** — Fuzz testing for Proven Core modules.

### CI Pipeline

```
cargo build --release --target x86_64-unknown-uefi    # Must produce one .efi
cargo test                                              # Unit + integration tests
cargo kani                                              # Bounded checks for unsafe
cargo verus --verify                                    # Formal proofs (Proven Core)
```

### Toolchain Pinning (ADR-008)

- Verus version and Rust nightly are pinned in `rust-toolchain.toml` and `verus-version.toml`.
- Proofs re-verified on every commit against pinned versions.
- Nightly CI job runs proofs against latest Verus to detect upcoming breakage.
- Toolchain upgrades are discrete engineering tasks with their own time budget.
- Budget ~1 week/quarter for proof maintenance after initial L3 delivery.

---

## Implementation Priorities (Milestone Order)

| #    | Milestone                       | Weeks  | Key Gate                                              |
|------|---------------------------------|--------|-------------------------------------------------------|
| M0   | It Boots                        | 1–3    | Boots on R640, serial works, Verus CI passes           |
| M1   | VMX Works                       | 4–10   | VMLAUNCH/VMEXIT, VMCS specs (L2), first L3 proofs     |
| M2   | Guest Executes Real Code        | 11–24  | EPT spec (L2), allocator proof (L3), guest runs code   |
| M3   | Linux Boots                     | 25–35  | Unmodified Linux 6.x to shell, verification checkpoint |
| M4   | Usable VM Platform              | 36–51  | 4+ VMs, SMP, storage, network, EPT L3 for N guests    |
| M5   | Operationally Viable            | 52–67  | Full mgmt plane, audit engine, SOX/ISO reports         |
| M5.5 | VMware Migration Workstream     | 60–70  | 10+ VMs migrated from vCenter in one command           |
| M6   | Production Ready                | 68–100 | HA, security hardened, 72-hr soak, external audit      |

### Current progress (lived, not aspirational)

**Through M3.14 closed** — Latitude `Boot gate PASSED (M0 → M3.13)`; host `RAYNU-V-M3-L3-OK` (Verus L3 attempt; EptMap still L2). Lived: [docs/progress.md](docs/progress.md). Post-shell: [docs/m3_post_shell_plan.md](docs/m3_post_shell_plan.md).

### Risk Hotspots

- **M2 is the #1 risk.** EPT + interrupt virtualization is where hypervisors stall. Plan 14–18 weeks.
- **M3 is the #2 risk.** Real kernels expose every emulation gap. Plan 11–14 weeks.
- **EPT proof is the #1 verification risk.** Spec in M2, partial proof in M4, full proof in M6.

---

## When Making Changes

### Pre-Change Checklist

1. **Identify pillar(s).** Which of [V], [Z], [D], [A] does this change serve?
   If none, reconsider whether the change belongs in RayNu-V.
2. **Identify subsystem(s).** Which directory/module is affected?
3. **Check the Proven Core boundary.** Is the affected module inside or outside?
   - Inside: four-file convention applies. Update spec + proof + test.
   - Outside: standard Rust + tests.
4. **Check relevant ADR.** Does an existing ADR govern this area? Follow it.
5. **Check verification maturity.** What level is this module at (L0/L1/L2/L3)?
   - Your change must not regress the maturity level.
   - If adding new functionality to a Proven Core module, add corresponding
     invariant documentation (L0), runtime assertions (L1), and spec updates (L2).
6. **Check audit impact.** Does this change introduce a security-relevant action?
   If yes, add an audit_log!() call.
7. **Check binary size.** If adding an embedded asset, update the size budget.

### Post-Change Checklist

1. All existing tests pass.
2. `cargo verus --verify` still passes (if Proven Core module changed).
3. `cargo kani` still passes (if unsafe blocks changed).
4. No new unsafe blocks without `// SAFETY:` + `// KANI-TARGET`.
5. No new global mutable state without spinlock + justification.
6. Documentation updated if architecture changed.
7. ADR written if a new architectural decision was made.
8. Bootability preserved on R640 (or QEMU+OVMF for CI).

---

## Testing Strategy

### Per-Module (Continuous)

- Unit tests for all public functions
- Kani bounded checks for all unsafe blocks in Proven Core
- Fuzz targets for all Proven Core modules that process variable-length input

### Per-Milestone (Gate)

- Integration test: boot-to-halt (M0), VMLAUNCH/VMEXIT cycle (M1),
  guest code execution through M2.5 (`./tools/qemu-boot-test.sh`),
  L2 host gate M2.6 (`cargo test` → `RAYNU-V-M2-L2-OK`),
  L3-attempt host gate M3.14 (`cargo test` → `RAYNU-V-M3-L3-OK`; Verus unpinned),
  Linux shell (M3), multi-VM (M4)
- Verification checkpoint: which modules are at L0/L1/L2/L3
  (see [docs/progress.md](docs/progress.md))
- Audit trail validation: all expected events present and correctly sequenced
- Binary size check: within 15 MB target

### Pre-Production (M6)

- 72-hour soak test (memory leaks, scheduler fairness, exit rate stability)
- Fault injection (kill vCPUs, corrupt pages, drop IRQs, network partition)
- External security audit (auditor runs `verus --verify`)
- External spec review (independent: "are we proving the right things?")
- Proof maintenance dry run (upgrade Verus, re-verify, measure breakage)
- R640 hardware CI (real iron, not just QEMU)

---

## Build & Run

### Build

```bash
# Install nightly Rust + UEFI target
rustup toolchain install nightly
rustup default nightly
rustup component add rust-src

# Build the single .efi binary
cargo build --release --target x86_64-unknown-uefi

# Output: target/x86_64-unknown-uefi/release/r640-hypervisor.efi
```

### Test in QEMU (before touching real R640)

```bash
mkdir -p esp/EFI/BOOT
cp target/x86_64-unknown-uefi/release/r640-hypervisor.efi esp/EFI/BOOT/BOOTX64.EFI
qemu-system-x86_64 \
    -bios /usr/share/OVMF/OVMF_CODE.fd \
    -drive format=raw,file=fat:rw:esp \
    -serial stdio \
    -m 512M \
    -enable-kvm \
    -cpu host
```

### Deploy to R640

```bash
# Copy .efi to USB stick formatted as FAT32 EFI System Partition
# Or: upload via iDRAC virtual media
# Boot from USB / virtual media
# RayNu-V boots. VMs run. That's it.
```

---

## What This Project Is NOT

- NOT a Linux distribution with KVM bolted on.
- NOT a port of Xen, bhyve, or any existing hypervisor.
- NOT a general-purpose OS. RayNu-V has no userspace, no shell, no package manager.
- NOT a research prototype. This is production-targeted from M0.
- NOT feature-competitive with ESXi on day one. We ship fewer features, but every
  feature we ship is auditable, metered, and (for the Proven Core) mathematically verified.

---

## Risk Awareness

| ID   | Risk                                           | Severity    | Key Mitigation                                      |
|------|-------------------------------------------------|-------------|------------------------------------------------------|
| R01  | EPT bugs → silent memory corruption             | HIGH        | Spec L2 in M2, proof L3 by M4, fuzz testing         |
| R05  | Live migration exposes guest memory              | HIGH        | Verus proof target; runtime assertions; memory zero  |
| R09  | Specs are wrong (prove the wrong thing)          | HIGH        | External spec review + fuzz Proven Core              |
| R10  | Single developer velocity                        | HIGH        | Near-term bets ship value early; V is north star     |
| R08  | Proof effort exceeds estimates                   | MEDIUM-HIGH | AI-assisted proofs; maturity model allows L1/L2 ship |
| R14  | Proof maintenance burden post-delivery           | MEDIUM      | Pin toolchain; nightly regression; ~1 wk/quarter     |

Full risk register: see `docs/risk_register.md` (14 risks total).

---

## Identity Statement

> **RayNu-V** — A commercially targeted, single-binary Type-1 hypervisor
> designed for formal verification from day one.
>
> One binary. Dell PowerEdge first (hardware we own and test on).
> Built toward VMware import, SOX-ready audit trails, and a machine-checked
> Proven Core — not just test results.
>
> North star: Memory isolation isn't tested. It's proved.
> (Roadmap tense — proofs are earned incrementally; see ADR-006.)
