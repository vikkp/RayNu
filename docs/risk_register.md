# RayNu-V Risk Register

Derived from Production Roadmap v1.2. Severity: HIGH / MEDIUM-HIGH / MEDIUM / LOW.

| ID | Risk | Severity | Pillar | Key Mitigation |
|----|------|----------|--------|----------------|
| R01 | EPT bugs → silent memory corruption / host exposure | HIGH | [V] | Spec L2 in M2; proof L3 by M4; fuzz + runtime asserts (ADR-004) |
| R02 | VMCS host-state corruption → guest owns host | HIGH | [V] | VMCS in Proven Core; L2 specs in M1; Kani on unsafe |
| R03 | Interrupt virtualization gaps stall M2 | HIGH | [V] | Explicit state machines; long M2 buffer; L1 asserts early |
| R04 | Linux boot exposes emulation holes (M3) | HIGH | — | Outside-core device stubs; iterative QEMU+R640 bring-up |
| R05 | Live migration exposes guest memory | HIGH | [V][A] | Migration page transfer in EPT theorem; zero-on-free; audit events |
| R06 | Binary exceeds 20 MB hard size limit | MEDIUM | [Z] | Lazy asset decompress; `tools/check-size.sh`; split-mode fallback (ADR-003) |
| R07 | Dell Tier 2 (PERC/OEM Redfish) blocks a milestone | MEDIUM | [D] | Never gate on Tier 2; Tier 1 sufficient to ship (ADR-005) |
| R08 | Proof effort exceeds estimates | MEDIUM-HIGH | [V] | Ship at L1/L2 if blocked; AI-assisted proofs; maturity model (ADR-006) |
| R09 | Specs prove the wrong property | HIGH | [V] | External spec review; fuzz Proven Core; ADR-004 formal statement |
| R10 | Single-developer velocity limits delivery | HIGH | all | Near-term [Z][D][A] bets ship value; [V] shapes architecture without blocking |
| R11 | VMware migration complexity spills into core | MEDIUM | [A][Z] | Dedicated M5.5 workstream; outside Proven Core (ADR-007) |
| R12 | Toolchain / Verus nightly drift breaks proofs | MEDIUM | [V] | Pin versions; nightly regression job; quarterly upgrade budget (ADR-008) |
| R13 | Audit log tampering undermines [A] pillar | HIGH | [A][V] | Audit integrity in Proven Core; hash chain; mandatory `audit_log!` |
| R14 | Proof maintenance burden post-delivery | MEDIUM | [V] | Pin toolchain; nightly CI; ~1 week/quarter maintenance (ADR-008) |

## Hotspots

- **M2** — EPT + interrupt virtualization (R01, R03). Primary schedule risk.
- **M3** — Real Linux kernels (R04). Secondary schedule risk.
- **EPT proof** — Spec M2, partial L3 M4, full incl. migration M6 (R01, R05, R08, R09).

## Process

1. New risks get an ID, severity, pillar tag, and mitigation owner in this file.
2. HIGH risks must reference an ADR or milestone gate when mitigated by architecture.
3. Closed risks stay listed with status `mitigated` / `accepted` (add a Status column when first risk closes).
