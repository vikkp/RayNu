# HDA — Cursor prompt card

Copy any block below into Cursor chat. The standing rule is also in [`.cursor/rules/hda-update.mdc`](../.cursor/rules/hda-update.mdc) (`alwaysApply: true`).

Living doc: [`docs/hda.md`](hda.md).

---

## 1) Default — run on every commit (paste when committing)

```text
Before you finish this commit/push:

1. Read docs/hda.md and docs/progress.md.
2. Diff this change against Mount Everest:
   Ship EFI → real R640 boot → network vSphere-like UI → deploy Linux ISO → M6.9 prod bar.
3. Update docs/hda.md in this same commit:
   - YAML frontmatter (last_updated, last_commit, months_to_everest_prev → months_to_everest, overall_pct, everest_eta_month)
   - Scoreboard + ASCII bars
   - Four-summit status if anything moved
   - Rolling month timeline Status / burn-down if ETA moved
   - This-commit delta
   - HDA changelog row
4. Apply the velocity model in docs/hda.md. Only shrink months with concrete DONE evidence.
5. Honesty locks:
   - Latitude/QEMU ≠ R640
   - In-process REST ≠ network UI
   - bzImage/initrd ≠ ISO deploy
   - Demo SPA ≠ vSphere-like
6. Commit message footer line:
   HDA: months X→Y · overall A%→B% · ETA YYYY-MM

Follow CLAUDE.md. Do not expand Proven Core for UI/ISO/HTTP without ADR.
```

---

## 2) Standalone refresh (no code change)

```text
Update the Honest Distance Assessment now.

Read docs/hda.md, docs/progress.md, git log -20, and summarize Everest distance.
Recalculate months_to_everest and overall_pct using the velocity model in docs/hda.md.
Rewrite scoreboard, timeline, this-commit delta, changelog.
Be honest: do not mark R640 / network UI / ISO done without evidence.
Commit as: docs(hda): refresh distance assessment
```

---

## 3) After closing a gate

```text
We just closed gate <MARKER>. Update docs/progress.md if needed, then update docs/hda.md:
- Map the gate to Everest criteria E1–E6 and P0 backlog
- Move months_to_everest closer only if this reduces residual P0 work
- Add changelog row with evidence
- Keep structure stable
```

---

## 4) “How far to Everest?” one-liner

```text
Open docs/hda.md scoreboard only. Reply with: overall %, months_to_everest, ETA month, next P0, top blocker. Then offer to full-refresh HDA.
```

---

## 5) Force pull-forward (we moved fast)

```text
We shipped faster than the timeline. Re-estimate docs/hda.md residuals for open P0 items using actual commits/gates since baseline_date. Shrink months_to_everest and pull everest_eta_month earlier. Cite evidence in changelog. Do not invent closed R640/ISO/UI work.
```

---

## 6) Force slip (blocked)

```text
We are blocked on <reason>. Update docs/hda.md blockers, raise months_to_everest with velocity_factor ≥ 1.3, push everest_eta_month, note slip in changelog.
```

---

## Optional: Composer always-on instruction

Add to Cursor **User Rules** or project instructions:

```text
RayNu-V: On every commit to this repo, update docs/hda.md per .cursor/rules/hda-update.mdc. Mount Everest = EFI ship + real R640 + network UI + Linux ISO deploy + M6.9. Keep the month timeline honest; faster Everest-path work pulls ETA closer.
```
