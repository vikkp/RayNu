# 2026-10-29 — `bc50d59aa77a`: the Overview shows the build COM2 already printed (ADR-021 A3 lived)

**EFI:** pull-request build of `9ee60cb3` (PR #419). `tools/build.sh` stamps `GITHUB_SHA`, and a pull-request run's `GITHUB_SHA` is the merge of the branch onto `main`, not the branch tip. That merge is `bc50d59aa77a9b2d9ba12e84af44dbbb5f3dd8c9`. CI pull-request run `40876033671`, green. The build log prints `sha=bc50d59aa77a run=40876033671`.
**Flash:** Lexar, label `RAYNUV`, front USB 2, F11 one-shot. Ubuntu 26.04 standing on UBUNTU0.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell. No guest was started for this check.

## COM2

```
build: sha=bc50d59aa77a run=40876033671
```

`build: sha=` is still the first token. The second copy of the banner, on the firmware console, shows the same two fields. The `?` in that copy are the em dash and the middle dot; the COM2 line is the one that counts.

## Overview

Under the Host / RayNu-V / Guest / Install media tiles, before the page title:

```
build bc50d59aa77a · CI 40876033671
```

`40876033671` is a link to `https://github.com/vikkp/RayNu/actions/runs/40876033671`. The page, COM2, and that Actions run name the same SHA and the same run. The Host tile was green (`Server is answering`). No guest was running. **A3 lived.**

## Not claimed

No new `RAYNU-V-*-OK` marker. B20 still planned: the page still does not say why a guest stopped. Wave 2 stays open on B20. Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
