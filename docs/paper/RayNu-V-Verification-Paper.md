# RayNu-V: A Formally Verified Bare-Metal Hypervisor
### Optimized for Dell PowerEdge R640 / R650 / R660

**Living Draft** — Version `v0.1-skeleton`  
**Last updated:** 2026-07-21  
**Corresponding hypervisor commit:** *TBD*  
**Proof toolchain:** Verus (pinned) + Kani (pinned) — see ADR-008  
**Governing ADR:** ADR-010

> This document is a living verification paper.  
> Sections are filled only with evidence from real runs, Verus/Kani outputs, and milestone gates.  
> Maturity claims never exceed the actual state of the Proven Core.  
> When the work reaches sufficient L3 coverage and external review, a frozen snapshot of this page becomes the formal conference submission.

---

## Abstract

*[To be written at M3/M4 checkpoint. Placeholder only.]*

RayNu-V is a clean-slate Type-1 bare-metal hypervisor written in Rust and optimized for Dell PowerEdge servers. Its security-critical path (VMX, EPT, physical frame allocator, and related control planes) is designed for formal verification with Verus and Kani. This paper reports the progressive verification of the Proven Core, culminating in a machine-checked proof of the EPT Isolation Theorem: every valid guest-physical to host-physical mapping is exclusively owned by a single guest and belongs to neither the hypervisor nor any other guest.

**Keywords:** formal verification, Type-1 hypervisor, EPT, Verus, Kani, bare-metal, Dell PowerEdge

---

## 1. Introduction

*[Static until early content is ready.]*

Motivation, problem statement, the four pillars ([V], [Z], [D], [A]), and the decision to treat formal verification as an architectural north star rather than a post-hoc exercise.

**Evidence block template (fill only with real data):**

```
**Evidence (YYYY-MM-DD, commit <hash>, run <id>)**
- Maturity: Lx
- Artifact: <serial log / Verus transcript / audit-ring hash / gate checklist>
- Observation: <one factual sentence>
```

---

## 2. Background and Threat Model

- Existing Type-1 hypervisors and their verification status (or lack thereof).
- Why memory isolation is the headline property.
- Threat model: malicious or compromised guest, buggy device emulation outside the Proven Core, operator error.
- Assumptions that the proofs rely on (and that are therefore out of scope).

---

## 3. System Architecture

### 3.1 Proven Core Boundary (ADR-002)

Only the following modules receive Verus specifications and progressive proofs. Everything else is verified by Rust’s type system, testing, and code review only.

| Module                     | Est. LOC | Criticality Reason                                      |
|----------------------------|----------|---------------------------------------------------------|
| VMX lifecycle              | ~800     | Incorrect VMXON/VMXOFF leaves CPU in undefined state    |
| VMCS management            | ~1,500   | Host-state corruption = guest owns the host             |
| EPT engine                 | ~2,000   | The memory isolation mechanism                          |
| Physical frame allocator   | ~1,500   | Double-alloc / use-after-free = silent corruption       |
| vCPU state management      | ~1,000   | Incomplete save/restore leaks host state                |
| Interrupt injection        | ~800     | Wrong injection can escalate guest privilege            |
| Hypercall interface        | ~500     | Only intentional guest→host channel                     |
| MSR / CPUID / CR firewalls | ~1,200   | Unfiltered MSR writes can subvert host security         |
| Audit log integrity        | ~600     | Tampered audit log collapses the [A] pillar             |
| IPI confinement            | ~500     | Unconfined IPIs enable cross-VM interference            |

**Hard limit:** 15,000 LOC including ~1,600 LOC of proof scaffolding.

### 3.2 Single-Binary Strategy (ADR-003)

One `.efi` binary. All assets (kernel, initrd, Web UI, schemas) embedded and zstd-compressed. Target size 15 MB, hard limit 20 MB. Non-critical assets are lazy-decompressed.

### 3.3 Hardware Focus

Dell PowerEdge R640 / R650 / R660. Tier-1 iDRAC/Redfish integration is in scope; Tier-2 (PERC deep health, predictive failure) is best-effort and requires partnership.

---

## 4. Formal Verification Approach

### 4.1 Toolchain (ADR-001, ADR-008)

- **Verus** — primary tool for functional correctness (SMT).
- **Kani** — bounded model checking of every `unsafe` block.
- Fallback chain: Verus → Kani → runtime assertions + fuzzing.  
  The architecture is always designed for Level 3 even when tooling is not yet ready.

### 4.2 Verification Maturity Model (ADR-006)

| Level | Name               | Meaning                                              |
|-------|--------------------|------------------------------------------------------|
| L0    | Documented         | Invariants written as comments                       |
| L1    | Runtime-enforced   | `assert!` / `debug_assert!` + Kani on unsafe         |
| L2    | Spec-written       | Verus `.spec.rs` with ghost state and contracts      |
| L3    | Proof-complete     | `cargo verus --verify` succeeds                      |

Runtime assertions are retained even at L3 (defense-in-depth).

### 4.3 File Convention for Proven Core Modules

```
module/
├── module.rs           # executable code
├── module_spec.rs      # Verus specifications
├── module_proof.rs     # Verus proofs (gaps marked TODO)
└── module_test.rs      # Kani + unit + fuzz
```

---

## 5. The EPT Isolation Theorem (ADR-004)

**Formal statement:**

> For every valid EPT mapping from a guest-physical address to a host-physical frame, that frame is exclusively owned by the mapping guest and belongs to neither the hypervisor nor any other guest.

“Exclusively owned” = exactly one guest holds a mapping to that frame at any moment.  
“Belongs to neither” = the frame is absent from the hypervisor’s page tables and from every other guest’s EPT.

The theorem must hold across map, unmap, EPT-violation handling, and (later) live-migration page transfer.

### 5.1 Proof Progression (living)

| Milestone | Target                                      | Actual Maturity | Evidence |
|-----------|---------------------------------------------|-----------------|----------|
| M2        | Spec written + runtime asserts + Kani       | *TBD*           | *TBD*    |
| M3        | 4K-page, single-guest proof attempt         | *TBD*           | *TBD*    |
| M4        | Extended to N guests (4K)                   | *TBD*           | *TBD*    |
| M5        | Large-page support attempted                | *TBD*           | *TBD*    |
| M6        | Full proof incl. live migration + external review | *TBD*     | *TBD*    |

---

## 6. Progressive Evaluation — Milestone Log

*This section is filled only with contemporaneous evidence from real runs.*

### 6.1 Milestone 0 — “It Boots”

**Status:** *Not yet started*  
**Target gate:** Boots on R640 (or QEMU+OVMF), serial console works, Verus CI pipeline green.

**Evidence blocks will appear here.**

### 6.2 Milestone 1 — “VMX Works”

**Status:** *Not yet started*  
**Target gate:** VMLAUNCH / VMEXIT cycle, VMCS host/guest state configured, first L3 proofs on VMCS legality.

**Evidence blocks will appear here.**

### 6.3 Milestone 2 — “Guest Executes Real Code”

**Status:** *Not yet started*  
**Target gate:** EPT spec (L2), physical allocator at L3, guest code runs under EPT.

**Evidence blocks will appear here.**  
*(This is the highest-risk technical milestone — EPT + interrupt virtualization.)*

### 6.4 Milestone 3 — “Linux Boots”

**Status:** *Not yet started*  
**Target gate:** Unmodified Linux 6.x reaches a shell. Verification checkpoint report published.

**Evidence blocks will appear here.**

### 6.5 Milestone 4 — “Usable VM Platform”

**Status:** *Not yet started*  
**Target gate:** ≥4 Linux VMs with SMP, storage, networking. EPT Isolation Theorem at L3 for 4K pages across N guests.

**Evidence blocks will appear here.**

### 6.6 Milestone 5 / 5.5 / 6

*(Sections reserved; content added only when the corresponding gates are passed.)*

---

## 7. Related Work

*[To be filled later with precise citations.]*

Existing formally verified or heavily verified hypervisors / kernels (seL4, CertiKOS, Komodo, Firecracker verification efforts, etc.). Differences in scope, threat model, and hardware target.

---

## 8. Limitations and Open Proofs

- Large-page EPT and NUMA-aware allocation may remain at L2 for the initial public release.
- Live-migration path is a late L3 target (M6).
- Device emulation, scheduler algorithms, and the VMware migration engine are deliberately outside the Proven Core and are not claimed to be formally verified.
- Verus / Rust toolchain evolution can break existing proofs (see ADR-008); a quarterly maintenance budget is allocated.

Any L2-only modules at the time of a public snapshot will be listed here with an explicit statement of the residual risk and the runtime-enforcement measures that remain in place.

---

## 9. Conclusion

*[Written only when the work has reached a stable, externally reviewable state.]*

---

## Appendix A — Evidence Format

Every claim in Sections 5–6 must be accompanied by an evidence block of the form:

```
**Evidence (YYYY-MM-DD, commit <git-hash>, run <id>)**
- Maturity level claimed: Lx
- Artifact type: serial log | Verus transcript | Kani report | audit-ring hash | gate checklist
- Link or embedded excerpt: …
- One-sentence factual observation: …
```

No claim advances without such a block.

## Appendix B — Proven Core LOC Budget Snapshot

*(Updated only when the boundary or the measured LOC changes. Requires ADR if the boundary itself changes.)*

## Appendix C — Version History of this Living Document

| Version          | Date       | Hypervisor commit | Notes                          |
|------------------|------------|-------------------|--------------------------------|
| v0.1-skeleton    | 2026-07-21 | —                 | Initial skeleton (this file)   |

---

*End of living draft. This page is an audit artifact of RayNu-V development.*
