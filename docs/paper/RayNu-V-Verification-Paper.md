# RayNu-V: A Formally Verified Bare-Metal Hypervisor
### Optimized for Dell PowerEdge R640 / R650 / R660

**Author:** Vikash Pandey  
**ORCID:** [https://orcid.org/0009-0001-2160-6357](https://orcid.org/0009-0001-2160-6357)  
**Affiliation:** RayNu Technologies  

**Living Draft** — Version `v0.2.1-r640-boot-confirm`  
**Last updated:** 2026-08-15  
**Corresponding hypervisor commit:** `d7cc603` (kit `v0.1.0-xsavesfix`; evidence close + confirming retest on later docs commits)  
**Proof toolchain:** Verus (pinned) + Kani (pinned) — see ADR-008  
**Governing ADR:** ADR-010  
**Iron evidence:** [`docs/evidence/r640/logs/`](../evidence/r640/logs/) · claim [`STATUS=closed`](../evidence/r640/STATUS)

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
| M2        | Spec written + runtime asserts + Kani       | L1–L2 (Latitude) | Gate markers + Kani CI (`docs/progress.md`) |
| M3        | 4K-page, single-guest proof attempt         | L2 runtime + L3 ghost refine (scoped) | M3.17–M3.21; iron COM2 through Linux SHELL |
| M4        | Extended to N guests (4K)                   | L3 N-guest verify (Latitude); iron M4 probes | M4.6–M4.9; iron `M4-NVM-OK` … `M4-SMP-OK` |
| M5        | Large-page support attempted                | L3 large-page + NUMA affinity (Latitude) | M5.7–M5.9 / M6.2 gates |
| M6        | Full proof incl. live migration + external review | L3 through migrate-xfer + EXT | `80 verified, 0 errors`; `RAYNU-V-M6-EXT-OK` |

Iron COM2 logs prove the **runtime path** on PowerEdge R640. They do not, by
themselves, raise Verus maturity beyond what Latitude/CI already closed.

---

## 6. Progressive Evaluation — Milestone Log

*This section is filled only with contemporaneous evidence from real runs.*

### 6.1 Milestone 0 — “It Boots”

**Status:** Closed on Latitude/QEMU; **closed on real PowerEdge R640** (2026-08-15).  
**Target gate:** Boots on R640 (or QEMU+OVMF), serial console works, Verus CI pipeline green.

**Evidence (2026-08-15, commit `d7cc603`, run `r640-xsavesfix-com2`)**
- Maturity level claimed: L1 (runtime / gate)
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt`](../evidence/r640/logs/2026-08-15-xsavesfix-com2.txt)
- Observation: iDRAC Virtual Floppy boot printed `RAYNU-V-M0-BOOT-OK` on COM2 (COM1+COM2 mirror); EFI SHA256 `c3a688d0…ba28d611`.
- Confirming retest (same EFI SHA): [`docs/evidence/r640/logs/2026-08-15-confirm-rebuild-com2.txt`](../evidence/r640/logs/2026-08-15-confirm-rebuild-com2.txt).

**Evidence (2026-08-15, kit `v0.1.0-keepconfix`, run `r640-keepconfix-com2`)**
- Maturity level claimed: L1 (runtime residual)
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-keepconfix-com2.txt`](../evidence/r640/logs/2026-08-15-keepconfix-com2.txt)
- Observation: Same M0 path green before XSAVES residual; archived for reproducibility of the pre-close failure mode.

### 6.2 Milestone 1 — “VMX Works”

**Status:** Closed on Latitude; **reproduced on R640 iron**.  
**Target gate:** VMLAUNCH / VMEXIT cycle, VMCS host/guest state configured.

**Evidence (2026-08-15, commit `d7cc603`, run `r640-xsavesfix-com2`)**
- Maturity level claimed: L1 (runtime / gate)
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt`](../evidence/r640/logs/2026-08-15-xsavesfix-com2.txt)
- Observation: `RAYNU-V-M1-VMXON-OK`, `RAYNU-V-M1-VMEXIT-OK`; VMCS ctls include `secondary=0x0010100a` (EPT|RDTSCP|INVPCID|XSAVES).

### 6.3 Milestone 2 — “Guest Executes Real Code”

**Status:** Closed on Latitude; **reproduced on R640 iron**.  
**Target gate:** Guest code runs under EPT; timer/IRQ inject path.

**Evidence (2026-08-15, commit `d7cc603`, run `r640-xsavesfix-com2`)**
- Maturity level claimed: L1 (runtime / gate); ADR-004 ownership selftest at boot
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt`](../evidence/r640/logs/2026-08-15-xsavesfix-com2.txt)
- Observation: `RAYNU-V-M2-EPT-OK` … `RAYNU-V-M2-TIMER-OK` after precise EPT `[0,512MiB)` with guest CR3 in-window and distinct `HOST_CR3`.

### 6.4 Milestone 3 — “Linux Boots”

**Status:** Closed on Latitude; **closed on R640 iron** (`RAYNU-V-M3-SHELL-OK`).  
**Target gate:** Unmodified Linux 6.x reaches a shell (init SHELL hypercall).

**Evidence (2026-08-15, commit `d7cc603`, run `r640-xsavesfix-com2`)**
- Maturity level claimed: L1 (runtime / gate)
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt`](../evidence/r640/logs/2026-08-15-xsavesfix-com2.txt)
- Observation: Linux 6.12.40 earlyprintk on COM2; `RAYNU-V-M3-LINUX-EARLY-OK` → `APIC-OK` → `Run /init` → `RAYNU-V-M3-SHELL-OK` / `RAYNU-V-M3-NOIRQ-OK`.

**Evidence (2026-08-15, kit `v0.1.0-keepconfix`, run `r640-keepconfix-com2`)**
- Maturity level claimed: L1 (negative / residual)
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-keepconfix-com2.txt`](../evidence/r640/logs/2026-08-15-keepconfix-com2.txt)
- Observation: Without secondary Enable XSAVES, `Run /init` hit TASK stack guard / panic in interrupt (compacted XSAVE on Xeon Silver 4110); fixed by `v0.1.0-xsavesfix`.

### 6.5 Milestone 4 — “Usable VM Platform”

**Status:** Closed on Latitude (M4.0–M4.9); **M4.0–M4.5 probe chain reproduced on R640 iron**.  
**Target gate:** ≥4 concurrent guests under credit scheduler; virtio-blk/net + SMP probes. (Full multi-Linux install is Everest E5 residual.)

**Evidence (2026-08-15, commit `d7cc603`, run `r640-xsavesfix-com2`)**
- Maturity level claimed: L1 (runtime / gate)
- Artifact type: serial log
- Link: [`docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt`](../evidence/r640/logs/2026-08-15-xsavesfix-com2.txt)
- Observation: After G0 Linux SHELL: `RAYNU-V-M4-SHELL-G1`, `RAYNU-V-M4-2VM-OK`, `RAYNU-V-M4-SCHED-OK`, `RAYNU-V-M4-NVM-OK`, `RAYNU-V-M4-BLK-OK`, `RAYNU-V-M4-NET-OK`, `RAYNU-V-M4-SMP-OK`, then `VMXOFF ok`.

**Iron close claim (M7.5 / HDA E2):** `RAYNU-V-R640-BOOT-OK` — see
[`docs/evidence/r640/2026-08-15-r640-first-light.md`](../evidence/r640/2026-08-15-r640-first-light.md)
and confirming COM2 archive
[`logs/2026-08-15-confirm-rebuild-com2.txt`](../evidence/r640/logs/2026-08-15-confirm-rebuild-com2.txt).

### 6.6 Milestone 5 / 5.5 / 6

**Status (Latitude/CI):** M5–M6.9 closed on Latitude (`RAYNU-V-M6-EXT-OK`;
`80 verified, 0 errors`). Iron reproduction of M5–M6 product surfaces (network
UI, ISO deploy, soak) is **not** claimed here — see HDA E3–E5.

*(Additional iron evidence blocks added only when those gates pass on R640.)*

---

## 7. Related Work

*[To be filled later with precise citations.]*

Existing formally verified or heavily verified hypervisors / kernels (seL4, CertiKOS, Komodo, Firecracker verification efforts, etc.). Differences in scope, threat model, and hardware target.

---

## 8. Limitations and Open Proofs

- Large-page EPT and NUMA-aware allocation: Latitude closed L3 large-page + NUMA affinity gates (M5.7 / M6.2); iron product path not re-proven beyond M4.5 probes.
- Live-migration **page-transfer** L3 is closed on Latitude (M6.3); live vCenter product migrate remains polish.
- Device emulation, scheduler algorithms, and the VMware migration engine are deliberately outside the Proven Core and are not claimed to be formally verified.
- Verus / Rust toolchain evolution can break existing proofs (see ADR-008); a quarterly maintenance budget is allocated.
- **Mount Everest residual (not paper L3 claims):** network-reachable UI (E3–E4), Linux ISO install-to-disk (E5), live Redfish, R640 soak.

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

**Generation of runtime evidence (ADR-011):**  
Place an empty file named `paperverbose.txt` (or `/EFI/RayNu/paperverbose.txt`) on the EFI System Partition / USB / iDRAC virtual media used to boot the single `.efi` binary. The hypervisor detects the flag at early boot, raises audit verbosity, runs deterministic self-tests appropriate to the current milestone, and emits a structured Evidence Bundle on the serial console (and optionally a small dump file). Formal Verus/Kani transcripts remain offline host-side artifacts and are attached separately. Maturity claims produced by the EFI never exceed L1 (runtime-enforced).

## Appendix B — Proven Core LOC Budget Snapshot

*(Updated only when the boundary or the measured LOC changes. Requires ADR if the boundary itself changes.)*

## Appendix C — Version History of this Living Document

| Version          | Date       | Hypervisor commit | Notes                          |
|------------------|------------|-------------------|--------------------------------|
| v0.2.1-r640-boot-confirm | 2026-08-15 | same EFI SHA | Confirming COM2 rebuild archive + Stories/site publish |
| v0.2.0-r640-boot | 2026-08-15 | `d7cc603` (xsavesfix) | Iron COM2 archives + §6 M0–M4 evidence; `RAYNU-V-R640-BOOT-OK` |
| v0.1.2           | 2026-07-24 | —                 | Documented ADR-011 evidence mode flag |
| v0.1.1           | 2026-07-21 | —                 | Added author ORCID             |
| v0.1-skeleton    | 2026-07-21 | —                 | Initial skeleton (this file)   |

---

*End of living draft. This page is an audit artifact of RayNu-V development.*
