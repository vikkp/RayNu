# RayNu-V: A Formally Verified Bare-Metal Hypervisor
### Optimized for Dell PowerEdge R640 / R650 / R660

**Author:** Vikash Pandey  
**ORCID:** [https://orcid.org/0009-0001-2160-6357](https://orcid.org/0009-0001-2160-6357)  
**Affiliation:** RayNu Technologies  

**Living Draft** — Version `v0.4.0-living`  
**Last updated:** 2026-09-06  
**Corresponding hypervisor commit (iron P0-14):** `2b795a0`  
**Proof toolchain:** Verus `0.2026.07.12.0b42f4c` (pinned) + Kani `0.67.0` — see ADR-008  
**Governing ADR:** ADR-010  
**Iron evidence:** [`docs/evidence/r640/`](../evidence/r640/) · claim [`STATUS=closed`](../evidence/r640/STATUS) (E2)

> This document is a living verification paper (ADR-010).  
> Sections are filled only with evidence from real runs, Verus/Kani outputs, and milestone gates.  
> Maturity claims never exceed the actual state of the Proven Core.  
> This version **completes the living draft** against closed M6 L3 + R640 iron through El Torito.  
> It is **not** `v1.0-preprint`. Everest E5 (iron Linux ISO install-to-disk) remains open. Nested QEMU is not PowerEdge.

---

## Abstract

Type-1 hypervisors are trusted to isolate guest memory, yet production
hypervisors almost uniformly treat that isolation as a testing problem.
RayNu-V is a clean-slate Type-1 hypervisor, written in Rust, that boots as a
single UEFI binary on Dell PowerEdge R640-class servers and is designed so the
security-critical path can be machine-checked.

The Proven Core (ADR-002) is the only surface that receives Verus specifications
and proofs. Its headline property is the EPT Isolation Theorem (ADR-004):
every valid guest-physical to host-physical mapping is exclusively owned by one
guest and belongs to neither the hypervisor nor any other guest. On
Latitude/QEMU, the ghost model `ept_model` discharges that property through
map, unmap, EPT-violation handling, and live-migration page transfer —
**80 verified, 0 errors** — and the M6.9 auditor path closed
(`RAYNU-V-M6-EXT-OK`). Live executable EPT remains **L2** (runtime registry
+ asserts). Device emulation, the management plane, and guest firmware are
outside the Proven Core and are not claimed as theorems.

On a real PowerEdge R640, COM2 archives close M0→Linux SHELL→M4 probes
(`RAYNU-V-R640-BOOT-OK`), PRE-EBS and post-EBS host-NIC HTTP, E4 SPA
VMLAUNCH of a SHELL stub, and firmware ATAPI + El Torito CD EFI. Nested
RayNu-F completed an alpine-extended install and reboot-to-disk. That nested
loop is not iron E5. This paper reports those facts and stops there.

**Keywords:** formal verification, Type-1 hypervisor, EPT, Verus, Kani, bare-metal, Dell PowerEdge

---

## 1. Introduction

A hypervisor’s job is to make one computer look like several, without letting
those several computers look at each other. The mechanism that actually
enforces that on modern Intel servers is Extended Page Tables. If EPT is
wrong, two guests share a frame, or a guest sees hypervisor memory, and no
amount of API polish repairs it.

Commercial Type-1 stacks (ESXi, Hyper-V, Xen, KVM) are large, mature, and
tested. They are not accompanied by a machine-checked isolation theorem for
the frames they hand to guests. Research systems that *are* proved
(seL4, CertiKOS, Komodo) target microkernels, verified kernels, or enclaves —
not a single-binary UEFI hypervisor aimed at the PowerEdge fleet already in
the rack.

RayNu-V takes four architectural bets, in priority order when they conflict:

| Tag | Pillar | Role |
|-----|--------|------|
| **[V]** | Formally verified core | North star. Shape code so proofs remain possible. |
| **[Z]** | Zero-config single binary | One `r640-hypervisor.efi`. No host OS. |
| **[D]** | Dell iDRAC-native | Hardware we own and boot: R640 / R650 / R660. |
| **[A]** | Audit-first | Tamper-evident ring; SOX/ISO-shaped reports. |

Formal verification here is not a post-hoc whitepaper. ADR-001 picked Verus
(SMT) plus Kani (bounded `unsafe`). ADR-006 defined L0→L3 so the project can
ship at L1/L2 when L3 is still in flight. ADR-010 required this page to exist
from day one and to refuse narrative that runs ahead of logs.

This living draft is complete against the evidence we have: M6 production bar
on Latitude, iron through El Torito, nested RayNu-F as a product path. A
frozen arXiv/conference snapshot (`v1.0-preprint`) waits on iron
`RAYNU-V-M7-ISO-INSTALL-OK` and a third-party re-run of the auditor path
that is not a bring-up self-review.

**Evidence (2026-07-21, gate M6.9, run `m6-ext-smoke`)**
- Maturity: L3 (`ept_model`) + L0 gate docs
- Artifact: [`docs/reviews/m6_spec_review.md`](../reviews/m6_spec_review.md), [`docs/findings/m6_external.md`](../findings/m6_external.md)
- Observation: R09 review accepted; findings register **0 critical / 0 high**; marker `RAYNU-V-M6-EXT-OK`; `80 verified, 0 errors`.

---

## 2. Background and Threat Model

### 2.1 Why memory isolation is the headline

A Type-1 hypervisor is the TCB for every guest it runs. CPUID and MSR
firewalls, interrupt injection, and the hypercall door all matter. None of
them matter if EPT can map the same host frame to two guests, or to a guest
and the hypervisor, without anyone noticing. ADR-004 therefore names
**exclusive ownership of host frames** as the property we prove, not “the
guest seemed isolated in tests.”

### 2.2 Threat model

We assume:

- A **malicious or compromised guest** (ring-0 Linux, firmware, or installer)
  that may issue any architected VM exit: EPT violation, CPUID, MSR, I/O,
  hypercall, HLT, exception.
- **Buggy device emulation** outside the Proven Core (virtio, IDE/ATAPI,
  UART). Bugs there can DoS a guest or confuse an installer; they must not
  become a second EPT mapping of a Proven Core frame.
- **Operator error** (wrong ISO, wrong NIC, exhausted Cruzer). Audit events
  record what happened; they do not enlarge the theorem.

We do **not** assume a correct third-party guest firmware. Stage 46 taught
that forcing OVMF’s internal WaitForEvent state is a self-inflicted fault
(ADR-016). Product ISO boot now uses RayNu-F, which we author. RayNu-F is
**outside** the Proven Core.

### 2.3 Assumptions out of scope for the theorems

- The Intel SDM’s VMX/EPT hardware behaves as documented.
- The frozen Verus/SMT stack (ADR-008 pin) is trusted for the ghost proofs.
- Host physical memory given to the allocator is the memory the machine has.
- DMA from devices we do not model (untrusted NICs, USB) is a future
  confinement problem, not a discharged lemma.
- Nested VT-x on a lab server is not a proof about a PowerEdge R640.

---

## 3. System Architecture

### 3.1 Proven Core Boundary (ADR-002)

Only the following modules receive Verus specifications and progressive proofs.
Everything else is Rust’s type system, testing, and review.

| Module | Est. LOC | Criticality Reason |
|--------|----------|-------------------|
| VMX lifecycle | ~800 | Incorrect VMXON/VMXOFF leaves CPU in undefined state |
| VMCS management | ~1,500 | Host-state corruption = guest owns the host |
| EPT engine | ~2,000 | The memory isolation mechanism |
| Physical frame allocator | ~1,500 | Double-alloc / use-after-free = silent corruption |
| vCPU state management | ~1,000 | Incomplete save/restore leaks host state |
| Interrupt injection | ~800 | Wrong injection can escalate guest privilege |
| Hypercall interface | ~500 | Only intentional guest→host channel |
| MSR / CPUID / CR firewalls | ~1,200 | Unfiltered MSR writes can subvert host security |
| Audit log integrity | ~600 | Tampered audit log collapses the [A] pillar |
| IPI confinement | ~500 | Unconfined IPIs enable cross-VM interference |

**Hard limit:** 15,000 LOC including ~1,600 LOC of proof scaffolding. Promotion
into the Proven Core requires an ADR.

**Lived split (2026-09):** the machine-checked exclusivity theorems live in
host-only crate `ept_model` (not linked into the `.efi`). The executable EPT
ownership registry in `memory/ept.rs` is **L2**. Device emulation, HTTP, SPA,
ISO, RayNu-F, iDRAC, and migration are outside.

### 3.2 Single-Binary Strategy (ADR-003)

One `.efi` binary. Kernel, initrd, Web UI, and schemas are PE/COFF sections,
zstd-compressed, lazy-decompressed. Target 15 MB, hard limit 20 MB.

### 3.3 Hardware Focus (ADR-005)

Dell PowerEdge R640 / R650 / R660. Tier-1 (COM2 via iDRAC SOL, SMBIOS/ACPI,
BCM5720 LOM) is in scope. Tier-2 PERC OEM is not an M7 blocker.

---

## 4. Formal Verification Approach

### 4.1 Toolchain (ADR-001, ADR-008)

- **Verus** — SMT proofs of ghost state. Frozen pin
  `release/0.2026.07.12.0b42f4c` (`verus-version.toml`). CI never uses `latest`.
- **Kani** — bounded model checking of `unsafe` (CI pin `0.67.0`,
  `RAYNU-V-M3-KANI-OK`).
- Fallback: Verus → Kani → `assert!` + fuzz. Architecture is always aimed at L3.

### 4.2 Verification Maturity Model (ADR-006)

| Level | Name | Meaning |
|-------|------|---------|
| L0 | Documented | Invariants in comments |
| L1 | Runtime-enforced | `assert!` / `debug_assert!` + Kani on unsafe |
| L2 | Spec-written | Verus `.spec.rs` with ghost state and contracts |
| L3 | Proof-complete | `cargo verus --verify` succeeds |

Runtime assertions are kept at L3.

### 4.3 File convention

Proven Core modules use four files (`module.rs`, `_spec.rs`, `_proof.rs`,
`_test.rs`). The EPT exclusivity theorems of record live in
`ept_model/src/lib.rs` (`verus! { ... }`), verified with
`cargo verus verify -p ept_model`.

---

## 5. The EPT Isolation Theorem (ADR-004)

**Formal statement:**

> For every valid EPT mapping from a guest-physical address to a host-physical frame, that frame is exclusively owned by the mapping guest and belongs to neither the hypervisor nor any other guest.

“Exclusively owned” = exactly one guest holds a mapping to that frame at any
moment. “Belongs to neither” = the frame is absent from the hypervisor’s page
tables and from every other guest’s EPT.

The theorem must hold across map, unmap, EPT-violation handling, and
live-migration page transfer.

### 5.1 Proofs of record (Latitude / CI)

Named lemmas in `ept_model` (host-only; not in the EFI):

| Lemma | Gate | Claim |
|-------|------|--------|
| `theorem_single_guest_4k_map_unmap_exclusive` | M3.17 | 4K, one guest |
| `theorem_n_guest_4k_map_unmap_exclusive` | M4.7 | 4K, N guests |
| `theorem_concrete_single_guest_4k_refine` / `theorem_concrete_n_guest_4k_refine` | M3.18 / M4.9 | ghost↔exec refine (scoped) |
| `theorem_large_page_map_unmap_exclusive` | M5.7 | 2M/1G |
| `theorem_numa_map_unmap_affinity` | M6.2 | NUMA affinity ghost |
| `theorem_hw_2m_leaf_refines_identity` | M6.1 | HW PTE encode |
| `theorem_ept_violation_preserves_exclusive` | M6.0 | violation disposition |
| `theorem_page_transfer_preserves_exclusive` | M6.3 | `PageTransferStep` |

**Evidence (2026-07-21, M6.3–M6.9, Latitude + CI)**
- Maturity: **L3** (ghost model)
- Artifact: `ept_model` verify transcript; [`docs/reviews/m6_spec_review.md`](../reviews/m6_spec_review.md)
- Observation: `80 verified, 0 errors`; no `admit` on the exclusivity path;
  auditor smoke `./tools/m6-ext-smoke.sh` → `RAYNU-V-M6-EXT-OK`.

### 5.2 What iron logs do and do not raise

Iron COM2 logs prove the **runtime path** (VMXON, EPT identity, Linux
SHELL, M4 probes) on a Xeon Silver 4110. They do not, by themselves, raise
Verus maturity. Latitude L3 and R640 L1 are both real; they are not the
same claim.

### 5.3 Proof progression (living)

| Milestone | Target | Actual Maturity | Evidence |
|-----------|--------|-----------------|----------|
| M2 | Spec + asserts + Kani | L1–L2 live EPT; Kani CI | `docs/progress.md`; `RAYNU-V-M3-KANI-OK` |
| M3 | 4K single-guest proof | L3 ghost; iron SHELL | M3.17–M3.21; COM2 `M3-SHELL-OK` |
| M4 | N guests (4K) | L3 N-guest (Latitude); iron M4 probes | M4.6–M4.9; iron `M4-SMP-OK` |
| M5 | Large pages | L3 large-page + NUMA spec (Latitude) | M5.7–M5.9 |
| M6 | Violation + migrate-xfer + external review | L3 + EXT | `80 verified, 0 errors`; `M6-EXT-OK` |

---

## 6. Progressive Evaluation — Milestone Log

*Filled only with contemporaneous evidence from real runs.*

### 6.1 Milestone 0 — “It Boots”

**Status:** Closed on Latitude/QEMU; **closed on real PowerEdge R640** (2026-08-15).  
**Target:** Boots on R640, serial works, Verus CI green.

**Evidence (2026-08-15, commit `d7cc603`, run `r640-xsavesfix-com2`)**
- Maturity: L1 (runtime / gate)
- Artifact: [`docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt`](../evidence/r640/logs/2026-08-15-xsavesfix-com2.txt); confirming rebuild [`…-confirm-rebuild-com2.txt`](../evidence/r640/logs/2026-08-15-confirm-rebuild-com2.txt)
- Observation: iDRAC Virtual Floppy printed `RAYNU-V-M0-BOOT-OK` on COM2; EFI SHA256 `c3a688d0…ba28d611`. The confirming rebuild reproduced the M0→SHELL→M4 chain. The literal string `RAYNU-V-R640-BOOT-OK` is the marker-build archive below, not these two transcripts.

**Evidence (2026-08-15, build `r640-boot-ok-marker`, run `r640-boot-ok-marker-com2`)**
- Maturity: L1
- Artifact: [`…-boot-ok-marker-com2.txt`](../evidence/r640/logs/2026-08-15-boot-ok-marker-com2.txt) · [screenshot](../evidence/r640/logs/2026-08-15-boot-ok-marker-com2.png)
- Observation: After M0→SHELL→M4→`VMXOFF`, COM2 printed the literal string `RAYNU-V-R640-BOOT-OK`.

**Evidence (2026-08-15, kit `v0.1.0-keepconfix`)**
- Maturity: L1 (negative / residual)
- Artifact: [`…-keepconfix-com2.txt`](../evidence/r640/logs/2026-08-15-keepconfix-com2.txt)
- Observation: Without secondary Enable XSAVES, `Run /init` hit TASK stack guard on Xeon Silver 4110; archived as the pre-close failure mode.

### 6.2 Milestone 1 — “VMX Works”

**Status:** Closed on Latitude; **reproduced on R640 iron**.

**Evidence (2026-08-15, `d7cc603`, `r640-xsavesfix-com2`)**
- Maturity: L1
- Artifact: same xsavesfix COM2 log
- Observation: `RAYNU-V-M1-VMXON-OK`, `RAYNU-V-M1-VMEXIT-OK`; VMCS `secondary=0x0010100a` (EPT|RDTSCP|INVPCID|XSAVES).

### 6.3 Milestone 2 — “Guest Executes Real Code”

**Status:** Closed on Latitude; **reproduced on R640 iron**.

**Evidence (2026-08-15, `d7cc603`, `r640-xsavesfix-com2`)**
- Maturity: L1 (runtime); ADR-004 ownership selftest at boot
- Artifact: same log
- Observation: `RAYNU-V-M2-EPT-OK` … `RAYNU-V-M2-TIMER-OK` after precise EPT `[0,512MiB)` with guest CR3 in-window and distinct `HOST_CR3`.

### 6.4 Milestone 3 — “Linux Boots”

**Status:** Closed on Latitude; **closed on R640** (`RAYNU-V-M3-SHELL-OK`).

**Evidence (2026-08-15, `d7cc603`, `r640-xsavesfix-com2`)**
- Maturity: L1
- Artifact: same log
- Observation: Linux 6.12.40 earlyprintk on COM2; `RAYNU-V-M3-LINUX-EARLY-OK` → `Run /init` → `RAYNU-V-M3-SHELL-OK` / `RAYNU-V-M3-NOIRQ-OK`.

### 6.5 Milestone 4 — “Usable VM Platform”

**Status:** Closed on Latitude (M4.0–M4.9 L3 N-guest); **M4.0–M4.5 probes on R640 iron**.

**Evidence (2026-08-15, `d7cc603`, `r640-xsavesfix-com2`)**
- Maturity: L1 (iron runtime); L3 N-guest is Latitude/CI
- Artifact: same log
- Observation: `RAYNU-V-M4-SHELL-G1`, `M4-2VM-OK`, `M4-SCHED-OK`, `M4-NVM-OK`, `M4-BLK-OK`, `M4-NET-OK`, `M4-SMP-OK`, then `VMXOFF ok`.

Iron close claim (M7.5 / HDA E2): `RAYNU-V-R640-BOOT-OK` —
[`2026-08-15-r640-first-light.md`](../evidence/r640/2026-08-15-r640-first-light.md).

### 6.6 Milestones 5, 5.5, and 6 (Latitude / CI)

**Status:** M5–M6.9 **closed on Latitude**. Iron is not claimed for soak, HA, or
the Verus count.

**Evidence (2026-07-21, M6.3, `ept_model`)**
- Maturity: L3
- Artifact: verify of `theorem_page_transfer_preserves_exclusive`
- Observation: `RAYNU-V-M6-MIGRATE-XFER-OK`; `80 verified, 0 errors`.

**Evidence (2026-07-21, M6.8 soak)**
- Maturity: L0 harness (not a theorem)
- Artifact: `./tools/m6-soak-smoke.sh` → `RAYNU-V-M6-SOAK-OK`
- Observation: 72-hour *metrics* gate on Latitude; not an R640 soak.

**Evidence (2026-07-21, M6.9)**
- Maturity: L3 proofs + L0 review docs
- Artifact: [`m6_spec_review.md`](../reviews/m6_spec_review.md), [`m6_external.md`](../findings/m6_external.md), [`m6_proof_maintenance.md`](../reviews/m6_proof_maintenance.md), [`verus-version.toml`](../../verus-version.toml)
- Observation: `RAYNU-V-M6-EXT-OK`. Bring-up self-review against ADR-004; independent third-party `verus --verify` remains the auditor re-run described in [`docs/runbooks/external_audit.md`](../runbooks/external_audit.md).

### 6.7 Milestone 7 — Mount Everest iron track (product, not Proven Core)

M7 is the operator product loop (ADR-009). HTTP, SPA, ISO, and RayNu-F are
**outside** the Proven Core. Evidence here is L1 runtime / gate, never L3.

**Status:** last iron ISO gate is Stage 45 El Torito (2026-08-27). Nested
alpine-extended sys-install is complete (`Please reboot` + F7
`RAYNU-V-RAYNU-F-DISK-BOOT-OK`, 2026-09-05). Residual: iron installer +
TLS/console. Nested ≠ Everest. Host/CI never print
`RAYNU-V-M7-ISO-INSTALL-OK`.

**Evidence (2026-08-16, M7.6, R640 PRE-EBS HTTP)**
- Maturity: L1
- Artifact: [`2026-08-16-uefi-http-ok.md`](../evidence/r640/2026-08-16-uefi-http-ok.md)
- Observation: `RAYNU-V-M7-UEFI-HTTP-OK` on `10.99.99.127:8443` (SNP residual; Virtual Floppy has no Tcp4).

**Evidence (2026-08-16, M7.7 stamps)**
- Maturity: L1 (persist-detect)
- Artifact: [`logs/2026-08-16-e5-booted-from-disk-com2.txt`](../evidence/r640/logs/2026-08-16-e5-booted-from-disk-com2.txt)
- Observation: `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` on Cruzer. LBA stamps, **not** a guest rootfs.

**Evidence (2026-08-20, E3b + Phase F)**
- Maturity: L1
- Artifact: [`2026-08-20-e3b-host-nic-http-ok.md`](../evidence/r640/2026-08-20-e3b-host-nic-http-ok.md), [`2026-08-20-phase-f-coexist-ok.md`](../evidence/r640/2026-08-20-phase-f-coexist-ok.md)
- Observation: Host-owned BCM5720 `:38` served SPA+REST after `BOOT-OK` (`RAYNU-V-M7-HOST-NIC-HTTP-OK`); Phase F repeated the listen with VMX on.

**Evidence (2026-08-21, P0-14, EFI `2b795a0`)**
- Maturity: L1
- Artifact: [`2026-08-21-e4-spa-shadow-reentry-ok.md`](../evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md)
- Observation: `RAYNU-V-M7-E4-SPA-LAUNCH-OK` — private-EPT SHELL guest from SPA start + shadow re-entry. **Not a distro.**

**Evidence (2026-08-22 → 2026-08-23, nested VT-x, ADR-014 Stages 36–42)**
- Maturity: L1 (nested / host). **Not iron.**
- Artifact: [`docs/progress.md`](../progress.md) P0-51…P0-57
- Observation: Retained ESP OVMF.fd VMLAUNCHed on a private VMCS (`OVMF-VMLAUNCH-OK`), kept past the first triple-fault (host owns CR4.VMXE), driven past SEC, given a GuestVisible CD, PEI/DXE platform attempt, and an empty virtio-blk with CD-then-disk boot order. Nested ≠ iron. Not El Torito. Not an installer.

**Evidence (2026-08-23 → 2026-08-27, nested VT-x Stage 43, [PR #217](https://github.com/vikkp/RayNu/pull/217))**
- Maturity: L1 (nested)
- Artifact: PR #217 gate markers
- Observation: `OVMF-BOTH-OK` — firmware-simultaneous virtio-blk + IDE enumeration. Not El Torito. Not an installer.

**Evidence (2026-08-27, Stage 44, EFI `bf696ca`)**
- Maturity: L1
- Artifact: iron COM2 via [PR #226](https://github.com/vikkp/RayNu/pull/226) / P0-59
- Observation: `RAYNU-V-M7-E5-OVMF-ATAPI-OK` — ATAPI READ(10) `scsi=0x28`, `sectors=1`, twice. Not El Torito. Not an installer.

**Evidence (2026-08-27, Stage 45, EFI `0be7283`)**
- Maturity: L1
- Artifact: iron COM2 marker `RAYNU-V-M7-E5-OVMF-ELTORITO-OK` (catalog=1, bootimg=1, `sectors=183`); lived table on the E5 close path ([PR #229](https://github.com/vikkp/RayNu/pull/229))
- Observation: Guest firmware booted CD EFI (El Torito) on real R640. **Not an installer. Not Everest E5.**

**Evidence (2026-09-05, Stage 46 nested RayNu-F, `fe4785a`)**
- Maturity: L1 (nested VT-x). **Not iron. Not L3.**
- Artifact: [PR #229](https://github.com/vikkp/RayNu/pull/229); ADR-016 / ADR-017
- Observation: After OVMF WaitForEvent forcing produced `#PF cr2=0xffffffffffffffb8` (`9474ab6`) and `CpuDeadLoop` (`4e16b59`), RayNu-F became the guest firmware. Nested alpine-extended `setup-disk -m sys` completed; F7 reboot printed `RAYNU-V-RAYNU-F-DISK-BOOT-OK`. Cruzer is too small for alpine-extended. Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`.

---

## 7. Related Work

**seL4** (Klein et al., SOSP 2009) is the landmark machine-checked
microkernel: functional correctness and, later, integrity/information-flow.
It is not a Type-1 hypervisor for unmodified Linux on PowerEdge, and it is
not a single UEFI binary. RayNu-V borrows the *discipline* (small trusted
core, explicit invariants, proofs against a written theorem) and refuses
seL4’s scope (we do not prove the scheduler or device model).

**CertiKOS** (Gu et al., OSDI 2016) builds a certified concurrent OS kernel
with contextual refinement. **Komodo** (Ferraiuolo et al., SOSP 2017)
verifies a software enclave monitor on ARM TrustZone. **Hyperkernel**
(Nelson et al., SOSP 2017) pushes a hypervisor toward push-button
verification with a simplified interface. These systems show that isolation
*can* be proved; they do not ship as `BOOTX64.EFI` on iDRAC virtual media.

Production hypervisors — ESXi, Hyper-V, Xen, KVM/QEMU — isolate guests with
EPT/NPT and years of testing. We do not claim a larger feature set. We claim
a *smaller* trusted core whose exclusivity theorem is machine-checked in
`ept_model`, and we publish the gaps (live EPT L2, iron installer open)
instead of implying the theorem covers the SPA.

NOVA (Steinberg & Kauer, EuroSys 2010) and related microhypervisor designs
similarly argue for a small TCB. RayNu-V’s TCB cut is ADR-002’s Proven Core
list, not a second kernel in the guest.

---

## 8. Limitations and Open Proofs

**Proof surface**

- Live executable EPT (`memory/ept.rs`) is **L2**. L3 is the host-only ghost
  model. Ghost↔exec refine is scoped (M3.18 / M4.9), not a full bisimulation
  of the EFI.
- VMX lifecycle, VMCS, hypercalls, MSR firewalls, audit integrity, and IPI
  confinement are inside the Proven Core *budget* and are **not** at L3
  beside `ept_model`. Claiming “the hypervisor is proved” would be R09.
- Large-page and NUMA L3 are Latitude gates (M5.7 / M6.2). Iron stopped at
  M4.5 probes for those surfaces.
- `PageTransferStep` is a ghost migration primitive, not vCenter vMotion.

**Product / iron**

- Device emulation, SPA, HTTP, ISO, RayNu-F, iDRAC, and OVF import are
  outside the Proven Core (ADR-002, ADR-007, ADR-016).
- Firmware SNP/Tcp4 after ExitBootServices is a platform limit (ADR-013);
  durable HTTP is the host-owned BCM5720.
- `ISO-BOOTED-FROM-DISK` is persist-detect, not a rootfs.
- Stage 45 El Torito is CD EFI, not Alpine `setup-disk`.
- Nested alpine-extended sys-install is complete (`Please reboot` +
  `RAYNU-V-RAYNU-F-DISK-BOOT-OK`). Nested ≠ `RAYNU-V-M7-ISO-INSTALL-OK`.
- Iron installer + TLS/console remain Everest residuals. Live Redfish and
  R640 soak are not closed.
- Verus upgrades can break proofs (ADR-008); quarterly maintenance is budgeted.

Any future `v1.0-preprint` freeze will reprint this section with the modules
still at L2 and the iron gates still open.

---

## 9. Conclusion

RayNu-V now has what ADR-010 asked the living paper to wait for before
*writing* the argument, not before *freezing* it.

On the proof side: the EPT Isolation Theorem is stated tightly (ADR-004),
implemented as a ghost model, and discharged through map, unmap, violation,
and page transfer under a frozen Verus pin — `80 verified, 0 errors` — with
an M6.9 spec review that asked R09 out loud and did not find a proxy theorem.
That is a verified *core model*. It is not a verified `.efi`.

On the iron side: a PowerEdge R640 printed `RAYNU-V-R640-BOOT-OK`, served
HTTP on the operator LAN, launched a SHELL guest from the SPA, and let
guest firmware read a CD and boot El Torito. Nested RayNu-F completed
alpine-extended sys-install (`Please reboot`) and reboot-to-disk. Nested
is not iron. The remaining sentence of Mount Everest is the same installer
on metal, on a stick that fits alpine-extended, plus TLS/console, without
claiming a marker we have not printed.

Memory isolation isn’t tested. The ghost exclusivity theorems are proved.
The product loop is not done. This page stays a living audit artifact until
those three sentences can be one.

---

## Appendix A — Evidence Format

Every claim in Sections 5–6 uses:

```
**Evidence (YYYY-MM-DD, commit <git-hash>, run <id>)**
- Maturity level claimed: Lx
- Artifact type: serial log | Verus transcript | Kani report | audit-ring hash | gate checklist
- Link or embedded excerpt: …
- One-sentence factual observation: …
```

**Generation of runtime evidence (ADR-011):**  
Place `paperverbose.txt` on the ESP. The EFI emits an L1 Evidence Bundle on
serial. Verus/Kani transcripts remain host-side. EFI-produced maturity never
exceeds L1.

## Appendix B — Proven Core LOC Snapshot (2026-09-06)

Measured with `wc -l` on this tree. Not an ADR-002 budget re-baseline.

| Path | Lines | Role |
|------|------:|------|
| `ept_model/src/lib.rs` | 2,366 | L3 ghost + proofs of record (host-only crate) |
| `memory/ept.rs` | 705 | Live EPT engine (L2) |
| `memory/ept_spec.rs` + `ept_proof.rs` | 357 | Spec/proof notes beside live engine |
| `memory/frame_allocator.rs` + `_spec.rs` | 362 | Allocator + ghost allocated-set |

The ten-module ADR-002 budget (~10,400 code + ~1,600 scaffolding) is a
**cap**, not a claim that every listed module is at L3. `ept_model` is the
proof-of-record crate; it is deliberately not linked into
`r640-hypervisor.efi`.

## Appendix C — Version History of this Living Document

| Version | Date | Hypervisor commit | Notes |
|---------|------|-------------------|--------|
| v0.4.0-living | 2026-09-06 | iron P0-14 `2b795a0` | Complete living draft: abstract/intro/threat/related/conclusion; M6 L3 + iron through El Torito; nested F7 as product. **Not v1.0-preprint.** |
| v0.3.0-e3b-e4-iron | 2026-08-23 | `2b795a0` | Site render: E3b/E4/ATAPI; placeholders left in §1, §2, §7, §9 |
| v0.2.2-r640-boot-marker | 2026-08-15 | `r640-boot-ok-marker` | Literal `RAYNU-V-R640-BOOT-OK` on COM2 |
| v0.2.1-r640-boot-confirm | 2026-08-15 | same EFI SHA | Confirming COM2 rebuild |
| v0.2.0-r640-boot | 2026-08-15 | `d7cc603` | Iron COM2 archives + §6 M0–M4 |
| v0.1.2 | 2026-07-24 | — | ADR-011 evidence mode flag |
| v0.1.1 | 2026-07-21 | — | Author ORCID |
| v0.1-skeleton | 2026-07-21 | — | Initial skeleton |

---

*End of living draft. This page is an audit artifact of RayNu-V development.*
