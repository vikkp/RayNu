# EuroSys 2027 fall — operator kit

## Compiled PDF (open this)

The GitHub PR diff does **not** preview binaries. The files are on branch
`cursor/eurosys-2027-fall-b7a8`, **not** on `main`.

- **Anonymous 11-page SIGPLAN PDF:** [`Isolon-eurosys2027-anon.pdf`](Isolon-eurosys2027-anon.pdf)
  ([GitHub](https://github.com/vikkp/raynu/blob/cursor/eurosys-2027-fall-b7a8/docs/paper/eurosys2027/Isolon-eurosys2027-anon.pdf)
  · [raw download](https://github.com/vikkp/raynu/raw/cursor/eurosys-2027-fall-b7a8/docs/paper/eurosys2027/Isolon-eurosys2027-anon.pdf))
- Same bytes: [`paper/main.pdf`](paper/main.pdf)
- Optional supplement: [`supplement/supplement.pdf`](supplement/supplement.pdf)

You create the HotCRP account. Everything else for the **17 Sep title/abstract**
deadline and a first **anonymous 12-page PDF** for **24 Sep** is in this
directory.

Portal: [https://eurosys27-fall.hotcrp.com/](https://eurosys27-fall.hotcrp.com/)
CFP: [https://2027.eurosys.org/cfp.html](https://2027.eurosys.org/cfp.html)
All deadlines **AoE**. HotCRP may show Friday 18 Sep 07:59 EDT for the
abstract; that is still Thursday 17 Sep AoE.

## What this is

| File | You do |
|------|--------|
| [`Isolon-eurosys2027-anon.pdf`](Isolon-eurosys2027-anon.pdf) | **Open/download the anonymous conference PDF** |
| [`HOTCRP.md`](HOTCRP.md) | Paste title, abstract, topics, keywords, conflicts, AI disclosure into HotCRP |
| [`paper/main.tex`](paper/main.tex) | Anonymous SIGPLAN draft (system name **Isolon**). Iterate before 24 Sep |
| [`paper/paper.bib`](paper/paper.bib) | Real related-work BibTeX |
| [`paper/Makefile`](paper/Makefile) | `make` → `main.pdf` + anonymity grep |
| [`ANONYMIZATION.md`](ANONYMIZATION.md) | What must not appear in the PDF |
| [`ARTIFACT.md`](ARTIFACT.md) | Optional anonymized `ept_model` zip |
| [`CHECKLIST.md`](CHECKLIST.md) | 17 Sep vs 24 Sep vs camera-ready |
| [`REBUTTAL.md`](REBUTTAL.md) | Operator notes for 6–8 Jan; do not paste into the PDF |
| [`supplement/README.md`](supplement/README.md) | Optional proofs file (reviewers may ignore) |

## Hard rules (do not skip)

- Living public page remains **RayNu-V** / `v0.4.0-living`. This PDF uses a
  **different title** and **Isolon**. That is the CFP rule for public
  non-archival work.
- PDF is **anonymous**. No author name, ORCID, GitHub, `raynuv.com`,
  `RAYNU-V-*` markers.
- Do **not** claim the EFI is proved. L3 is host-only `ept_model` (ghost).
  Live EPT is L2. The Linux 512 MiB window is the range registry (no ghost
  correspondence). Iron ISO install via Isolon-F is closed on the R640 and
  is outside the Proven Core; host/CI still must not print the iron-only
  install-complete marker. Nested rehearsal ≠ iron column.
- **PDF version:** v10 (Everest iron install loop reflected; ladder cells unchanged).
- This is **not** ADR-010 `v1.0-preprint`. It is a conference rewrite.
- Spring 2026 cycle already closed; this is not a spring reject, so the
  “no immediate resubmit” rule does not apply.
- At most **three** papers per author this cycle. Submit this one.

## Build the PDF

```bash
cd docs/paper/eurosys2027/paper
make
pdfinfo main.pdf   # Pages must be ≤12 of technical content; refs extra
```

Needs TeX Live with `acmart` (`texlive-publishers`).

## What you still owe after the account exists

1. Register, enter **conflicts** (chairs if any personal COI; likely none).
2. Paste `HOTCRP.md` fields on/before **17 Sep AoE**.
3. Upload `main.pdf` (+ optional supplement) by **24 Sep AoE**.
4. ORCID `0009-0001-2160-6357` is for **camera-ready**, not the review PDF.
5. If accepted: ACM Open likely **does not** cover RayNu Technologies →
   subsidized 2027 APC ~$500 member / $750 non-member.

Do not register or submit from this agent. The account is yours.
