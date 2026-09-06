# HotCRP paste fields — EuroSys 2027 fall

Portal: https://eurosys27-fall.hotcrp.com/

Paste these **tomorrow** after the account exists. Do not put author
identity into `paper/main.pdf`. Authors belong in the HotCRP form (visible
to chairs, not to reviewers).

---

## Title

```
Isolon: Keeping Isolation Claims Honest in a Bare-Metal Type-1 Hypervisor
```

This is **not** the public living-paper title
(“RayNu-V: A Formally Verified Bare-Metal Hypervisor”). Keep it that way.

Short title (if HotCRP asks):

```
Keeping Isolation Claims Honest
```

---

## Authors (HotCRP form only — not the PDF)

| Field | Value |
|-------|--------|
| Name | Vikash Pandey |
| Affiliation | RayNu Technologies |
| Email | (the address you use to register) |
| ORCID | `0009-0001-2160-6357` (required at camera-ready; enter now if the form allows) |
| Corresponding | you |

Per-author limit: ≤3 papers this cycle. This is the one.

---

## Abstract (same as `paper/main.tex`)

```
Isolon is a clean-slate Type-1 hypervisor that boots as one UEFI binary on a Dell PowerEdge R640, where it runs unmodified Linux 6.12 with virtio-blk, virtio-net, dual-vCPU SMP, and three stub guests on the same boot, then serves HTTP from a host-owned NIC.

Attaching a machine-checked isolation theorem to that binary is a labelling problem before it is a proof problem: the artifact that is proved and the mapping path the guests actually use need not be the same object. Isolon's proved artifact is guest-exclusivity in a host-only Verus ghost model. Its production guest-physical coverage at Linux boot is a 16-slot range registry the ghost model never sees. The two are disjoint, not approximate, and no one-axis "verified" label makes that visible.

We propose a two-axis ladder: proof maturity (L0–L3) against artifact (ghost model, executable module, binary on the target). Applied to eight systems, it names the axis the seL4 team crossed over four years from C to ARM binary, separates a SeKVM abstract that reads as a proved hypervisor from a Coq overlay on a KVM module, and would have split an earlier draft of this paper that put a ghost-model theorem beside a bare-metal boot. Six lessons generalized while shipping the binary; the sharpest is that a ghost model written against working code inherits that code's blind spots, so derive it from the threat model's domain instead.
```

---

## Topics (CFP list)

Check at least:

1. **Analysis, testing, and verification of systems**
2. **Virtualization and virtualized systems**

Optional secondaries (only if the form allows multi-select):

3. Systems security and privacy
4. Operating systems

Do not check AI/ML, databases, quantum, etc.

---

## Keywords

```
formal verification, Type-1 hypervisor, EPT, Verus, maturity ladder, experience
```

---

## Conflicts

PC chairs (enter as conflicted **only if you have a real COI** with them;
otherwise register and leave them unconflicted):

- Pramod Bhatotia — TU Munich
- Lydia Y. Chen — University of Neuchâtel
- Andreas Haeberlen — University of Pennsylvania
- Contact: pc-chairs-2027@eurosys.org

Likely for this submission: **no institutional conflict** (RayNu Technologies
is not ACM Open / not those universities). Still click through the HotCRP
conflict UI so chairs see you looked. Do not conflict the whole PC “to be
safe.”

---

## AI-tool disclosure (CFP / ACM authorship)

Paste into the disclosure field (or “comments to chairs” if that is the
only box):

```
This submission’s LaTeX draft, bibliography, and HotCRP title/abstract
fields were prepared with assistance from a coding agent (Cursor). Every
technical claim was checked against contemporaneous Verus transcripts and
serial logs by the author. The author is responsible for correctness. No
AI system is an author.
```

---

## Checkboxes you will see

- Original unpublished material: **yes** (living site page is non-archival).
- Not under review elsewhere as an archival paper: **yes**.
- Public TR / site exists: **yes**, with a **different title and system name**
  (Isolon vs RayNu-V). Do not attach the public HTML as the PDF.
- Not a EuroSys 2026 spring reject: **correct** (never submitted that cycle).
- Need a workshop-delta upload: **no**.
- Supplementary proofs: optional; see `supplement/README.md`.

---

## After 17 Sep

The abstract deadline is registration + title/abstract. You can still
edit the PDF until **Thu 24 Sep 2026 AoE**. Iterate `paper/main.tex` in
this directory; keep the HotCRP abstract in sync if you change the claim.
