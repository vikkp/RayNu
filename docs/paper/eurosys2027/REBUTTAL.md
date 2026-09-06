# Rebuttal prep — EuroSys 2027 fall, Isolon (#394)

Do **not** put this file in the anonymous PDF. Paste only after reviews
arrive (6 Jan 2027), by 8 Jan. HotCRP budget: **≤500 words**, facts and
reviewer questions only. No new experiments in the HotCRP box.

If the R640 is free **before** 8 Jan, measure then and put the numbers in
the response. Measured numbers beat any promise. If it is not free, keep
the camera-ready commitment below and do not invent cycles.

---

## Objection 1 — “The proved artifact is trivial.”

**Do not defend the bijection.** The paper already concedes it was not
hard. The answer is the labelling instrument plus the gap report, with
the model as the object those are about.

**Paste (≈170 words):**

The exclusivity crate is a ghost bijection; we do not claim it was a
hard proof. Two modelling choices in §4.8 are the systems content. The
miss-handler trichotomy (unmap / identity-miss / ClaimMap) is a real
bug class: treating every EPT violation as a map-or-fail would either
invent mappings or reject legal identity holes. The span-versus-loop
encoding is an SMT design decision: a 1G identity as 262,144 4K steps
does not discharge, while a span predicate does. Those are the reasons
the crate is in the paper.

The contribution is not “we proved a bijection.” It is a two-axis
labelling instrument (maturity × artifact) plus a gap report on one
Type-1 that boots on iron. Table 7 is the evaluation of that instrument:
seL4’s C-to-binary trajectory, SeKVM’s abstract versus overlay, and
Isolon’s three cells that a one-axis “L3 isolation” label would fuse.
The model is the object those labels are about. We do not upgrade the
ghost crate to a theorem about the EFI.

---

## Objection 2 — “No performance evaluation.”

Threats-to-validity already names the missing numbers (no VM-exit cycle
comparison, no `verus verify` wall-clock). Promise camera-ready work
that is cheap and checkable. Do not invent rdtsc or RSS.

**Paste (≈130 words):**

We agree the paper has no VM-exit cycle comparison and no prover timing.
Section 9 states why: the R640 was not re-instrumented this cycle, and
the 80/0 CI run predates this submission and was not instrumented for
timing. Those are absences, not hidden numbers.

If the same PowerEdge R640 is free, we will put `rdtsc` around CPUID,
ClaimMap, and HLT exits, and a `verus verify` wall-clock/RSS run against
the pinned Verus zip, into the camera-ready. That is a one-box, one-pin
measurement, not a new evaluation question. If the box is available
before this rebuttal deadline, those numbers belong here instead.

---

## Combined paste (if both appear; trim to ≤500)

Use the two blocks above. Cut the last sentence of objection 1 if over
budget. Do not add range-L3, ISO-INSTALL-OK, or nested-as-iron.

---

## Do not

- Claim the bijection was hard.
- Invent cycle counts, Verus RSS, or COM2 occupancy dumps.
- Print `RAYNU-V-*` or the public system name in the HotCRP box.
- Treat nested VT-x numbers as R640 numbers.
- Promise range-as-span L3 unless it has actually verified.
