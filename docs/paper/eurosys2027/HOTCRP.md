# HotCRP paste fields — EuroSys 2027 fall

Portal: https://eurosys27-fall.hotcrp.com/

Paste these **tomorrow** after the account exists. Do not put author
identity into `paper/main.pdf`. Authors belong in the HotCRP form (visible
to chairs, not to reviewers).

---

## Title

```
Exclusive Guest-Physical Isolation in a Type-1 Hypervisor: A Machine-Checked Ghost Model and a Bare-Metal Runtime
```

This is **not** the public living-paper title
(“RayNu-V: A Formally Verified Bare-Metal Hypervisor”). Keep it that way.

Short title (if HotCRP asks):

```
Exclusive Guest-Physical Isolation
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
Type-1 hypervisors isolate guest memory with extended page tables, yet production stacks treat that isolation as a testing problem. This paper reports Isolon, a clean-slate Type-1 hypervisor that boots as a single UEFI binary on production two-socket Xeon servers and is designed so the security-critical path can be machine-checked.

Isolon confines formal proofs to a Proven Core. The headline property is exclusive ownership of host frames: every valid guest-physical to host-physical mapping is owned by exactly one guest and belongs to neither the hypervisor nor any other guest. We discharge that property in a host-only Verus ghost model through map, unmap, EPT-violation handling, and a page-transfer primitive—80 lemmas verified, 0 errors, with no admit on the exclusivity path—under a frozen SMT toolchain pin. Live executable EPT remains at spec-written maturity: a runtime ownership registry plus assertions, not a bisimulation of the EFI. Device emulation, the management plane, and guest firmware are outside the Proven Core and are not claimed as theorems.

On a real PowerEdge R640 we reproduce VMX bring-up, EPT identity mapping, an unmodified Linux shell, multi-VM probes, durable host-NIC HTTP, and guest-firmware CD-ROM boot. Nested VT-x completed a Linux distro install-to-disk; nested virtualization is not that server. We report those facts, name the ghost/executable gap, and stop there.
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
formal verification, Type-1 hypervisor, EPT, Verus, exclusive ownership
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
