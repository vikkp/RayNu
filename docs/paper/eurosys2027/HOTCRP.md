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
Isolon is a clean-slate Type-1 hypervisor that boots as one UEFI binary on a Dell PowerEdge R640. On that server it reaches unmodified Linux 6.12 and, on the same boot, virtio-blk, virtio-net, dual-vCPU SMP, and four-VM probes, then serves host-NIC HTTP.

A machine-checked isolation theorem next to that binary is a labelling problem: the proved artifact and the mapping path the guests actually use need not be the same object. We propose a two-axis ladder—proof maturity (L0–L3) against artifact (ghost model, executable module, binary on the target)—and apply it to Isolon and to seven prior systems. Isolon's L3 result is guest-exclusivity in a host-only Verus crate. Linux's 512 MiB identity window is a 16-slot range registry with no ghost correspondence; the proved 4K table is the demand-fill and self-test path. Hypervisor non-access is an assumption. Virtio rings are guest frames the emulator touches. Nested VT-x completed a distro install-to-disk; that host is not the R640.
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
