# Runbook — 72-hour soak (M6.8)

**Marker:** `RAYNU-V-M6-SOAK-OK`  
**Smoke:** `./tools/m6-soak-smoke.sh`

## Story

M6.8 encodes a **72-hour soak** for memory leak posture, scheduler fairness, and
exit-rate stability (CLAUDE.md).

| Mode | What runs | Where |
|------|-----------|--------|
| **Host / CI gate** | Accelerated simulation: 72 ticks (= hours) through `run_soak_simulation` | `cargo test` / `m6-soak-smoke.sh` |
| **Iron companion** | Optional wall-clock 72-hr on Latitude/R640 using the same thresholds | Documented here; not required for CI green |

The gate closes when the **same threshold logic** passes for a completed 72-hour
simulated run. Prefer R640 for a wall-clock companion when available; QEMU nested
is acceptable for interim host confirmation.

## Thresholds

| Metric | Rule |
|--------|------|
| Duration | `SOAK_TARGET_HOURS == 72` completed |
| Memory leak | After warmup (2h), live allocated frames must not grow; end live == 0 |
| Scheduler fairness | Each of 2 vCPUs ≥ 40% of slices (`CreditScheduler::pick_next_fair`) |
| Exit-rate stability | Per-hour exits within ±5 of baseline 100 |

## Artifact

`SoakMetrics::artifact_line` retains hours, live/peak alloc, slice counts,
exit min/max, and pass flag. Audit emits `SoakStarted` / `SoakCompleted` /
`SoakFailed`.

## Limits

Simulation does not replace a production burn-in on customer iron. External
audit / spec review is **M6.9**.
