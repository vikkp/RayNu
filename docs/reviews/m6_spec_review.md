# M6 Spec Review (R09) — Are we proving the right things?

**Gate:** M6.9 → `RAYNU-V-M6-EXT-OK`  
**Question (R09):** Do Proven Core specs state the security property buyers and
auditors care about, or a convenient proxy?

## Scope reviewed

| Surface | Claim | Verdict |
|---------|--------|---------|
| ADR-004 exclusivity | A host frame is owned by at most one guest; map/unmap/violation/transfer preserve exclusivity | **In scope** — machine-checked in `ept_model` through M6.3 |
| HW PTE encode (M6.1) | Leaf encoding refines identity mapping intent | **In scope** — correspondence lemmas |
| NUMA affinity (M6.2) | Map/unmap respect node affinity ghost | **In scope** — affinity theorems |
| Page transfer (M6.3) | Live migration transfer preserves exclusivity | **In scope** — `PageTransferStep` |
| Ops Track B/C (auth, PDF, HA, fault, soak) | Host-testable gates, **outside** Proven Core | **Correctly outside** — not claimed as Verus theorems |

## What we deliberately do **not** claim

1. Full x86 ISA / VMX correctness (out of Proven Core budget).
2. Network/crypto isolation beyond EPT frame exclusivity.
3. That JSON/PDF audit reports constitute an external auditor *sign-off* (M6.5
   emits artifacts; this M6.9 note + findings register is the review surface).
4. Wall-clock 72-hr iron soak as a Verus theorem (M6.8 is a metrics harness).

## R09 conclusion

The Proven Core headline — **exclusive ownership of guest frames under
map/unmap/EPT-violation/page-transfer** — matches the product trust claim
(“memory isolation isn’t tested; it’s proved”). Ops gates are labeled outside
Proven Core and do not inflate the proof surface.

**Reviewer posture for M6.9:** bring-up self-review against ADR-004 + lived
markers in `docs/progress.md`. Independent third-party re-run of
`verus --verify` under ADR-008 pin is the auditor path
(`docs/runbooks/external_audit.md`).

**Status:** Accepted for M6.9 close (no R09 blockers).
