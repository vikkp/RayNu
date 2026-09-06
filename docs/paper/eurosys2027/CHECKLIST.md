# EuroSys 2027 fall checklist

## Thursday 17 Sep 2026 AoE — title + abstract

- [ ] HotCRP account created (you)
- [ ] Authors + affiliation entered
- [ ] Conflicts clicked through (do not over-conflict)
- [ ] Topics: verification + virtualization
- [ ] Title + abstract pasted from `HOTCRP.md`
- [ ] AI disclosure pasted
- [ ] Not marked as a spring-cycle resubmit

PDF is **not** required on the 17th.

## Thursday 24 Sep 2026 AoE — full PDF

- [ ] `cd paper && make` → `main.pdf`
- [ ] Technical content ≤ 12 pages; references unlimited
- [ ] Ink inside 7×9in (HotCRP: text height must not exceed 9in)
- [ ] Pages numbered; two-column SIGPLAN; ≥10 pt / ≥12 pt leading
- [ ] Figures readable in grayscale (tables only in this draft)
- [ ] `make anonymity-check` clean; `pdftotext` grep empty
- [ ] Abstract in PDF matches HotCRP (or you update HotCRP)
- [ ] Claim still honest: two-axis ladder; L3 ghost / L2 live EPT / L1 iron;
  range path disjoint from ghost maps; nested ≠ iron
- [ ] Optional supplement uploaded (proofs list / artifact zip)
- [ ] Paper stands alone without the supplement

## After reviews (6 Jan 2027) — rebuttal by 8 Jan

- [ ] Paste from [`REBUTTAL.md`](REBUTTAL.md); ≤500 words; facts and reviewer questions only
- [ ] No new experiments in the HotCRP box
- [ ] If the R640 is free before 8 Jan, measure then and put numbers in the response; otherwise keep the camera-ready promise in the operator notes, not a new claim

## If accepted (notify 29 Jan 2027) — camera-ready 5 Mar 2027

- [ ] Deanonymize: author, ORCID, acks, system name policy per shepherd
- [ ] ACM e-rights; Open Access APC if not ACM Open (~$500 / $750 in 2027)
- [ ] One full registration
- [ ] Shepherd must approve; acceptance is tentative until then

## Do not

- [ ] Claim `ISO-INSTALL-OK` or “the hypervisor is proved”
- [ ] Submit the living `paper.html` / markdown as the PDF
- [ ] Use the public title or the name RayNu-V in the review PDF
- [ ] Concurrent-submit this archival PDF elsewhere
