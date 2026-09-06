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
Attaching a machine-checked isolation theorem to a Type-1 hypervisor that ships as a UEFI binary creates a labelling problem the verification literature usually hides: the proved artifact and the binary that leaves the compiler are not the same object. This paper reports Isolon, a clean-slate Type-1 hypervisor written in Rust, and the discipline we used to keep that gap from becoming a false claim.

The instrument is a four-level maturity ladder (documented invariants, runtime-enforced, spec-written, proof-complete). We argue that verification papers should state which column a claim lives in. What Isolon actually discharges is exclusivity among guests in a host-only Verus ghost model: two finite maps forming a bijection, preserved under map, unmap, EPT-violation handling, and a page-transfer primitive. Hypervisor separation is a modelling assumption, not a theorem about hypervisor page tables. The property is about EPT mappings, not about hypervisor access: virtio emulation reads and writes guest-owned virtqueue frames without a second EPT owner. There is no device passthrough; guest DMA is emulation-mediated. The live engine is a 512-slot 4K table plus a 16-slot range registry; Linux bring-up covers a 512 MiB identity window with range claims and 2M hardware leaves, not 131,072 ghost 4K keys. SMT found no mapping bug in the executable; we say so.

On a PowerEdge R640 the EFI reaches an unmodified Linux 6.12 shell and, on the same boot, virtio-blk, virtio-net, dual-vCPU SMP, and four-VM probes, then serves host-NIC HTTP. Nested VT-x completed a distro install-to-disk; that nested host is not the R640. Five lessons generalize: pin the prover, do not host SMT on UEFI, do not poke third-party firmware, firmware TCP is not a management plane, and nested is not iron.
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
