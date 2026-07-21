---
hda_version: 1
last_updated: 2026-07-20
last_commit: PENDING
last_commit_short: PENDING
updated_by: bootstrap
mount_everest_target: "Ship EFI on real R640 + network vSphere-like UI + deploy Linux ISO + production bar (M6.9)"
months_to_everest: 4.5
months_to_everest_prev: 4.5
velocity_commits_30d: 0
velocity_gates_30d: 0
overall_pct: 28
confidence: medium
baseline_date: 2026-07-20
baseline_months: 4.5
everest_eta_month: "2026-12"
---

# Honest Distance Assessment (HDA)

> **Living document.** Updated on every meaningful commit by Cursor (see `.cursor/rules/hda-update.mdc`).  
> **North star product loop (“Mount Everest”):**  
> Ship the EFI → boot on a **real PowerEdge R640** → **network-reachable** vSphere-like UI → **deploy a Linux ISO** → production bar (soak + external review).

Pillars: **[V]** verified core · **[Z]** single binary · **[D]** iDRAC-native · **[A]** audit-first.  
Authoritative gates: [`docs/progress.md`](progress.md) · plans: [`m6_plan.md`](m6_plan.md) · constitution: [`CLAUDE.md`](../CLAUDE.md).

---

## Scoreboard (read this first)

| Metric | Value | Δ vs previous HDA |
|--------|------:|-------------------|
| **Overall product readiness** | **28%** | — (baseline) |
| **Months to Mount Everest** | **4.5** | — (baseline) |
| **ETA month** | **2026-12** | — |
| **Confidence** | medium | iron + ISO path unproven |
| **Hypervisor core (VMX/EPT/Linux/multi-VM)** | ~78% | strong |
| **Ship EFI artifact** | ~85% | build/size exist; release kit thin |
| **Real R640 boot** | ~25% | Latitude/QEMU ≠ R640 |
| **vSphere-like UI (network)** | ~12% | demo SPA; no TCP/HTTP stack |
| **Deploy Linux ISO** | ~8% | bzImage/initrd only; no ISO/CD-ROM |
| **Production bar (M6.8–M6.9)** | ~70% | M6.8 soak closed; M6.9 open |

```
Months to Everest  ████████████░░░░░░░░  4.5 mo   (baseline 4.5)
Overall %          ██████░░░░░░░░░░░░░░  28%
```

**How the month number moves:** faster closed Everest-path work → `months_to_everest` shrinks and `everest_eta_month` pulls closer. Stalls / new scope → it slips. See [Velocity model](#velocity-model).

---

## Mount Everest — definition of done

All must be true (no hand-waving):

| # | Criterion | Done when | Pillar |
|---|-----------|-----------|--------|
| E1 | **Ship EFI** | Versioned `r640-hypervisor.efi` + checksums; `tools/check-size.sh` green; USB/iDRAC media runbook | [Z] |
| E2 | **R640 boot** | Marker `RAYNU-V-R640-BOOT-OK` (or equiv.) on **real PowerEdge R640**; serial via iDRAC; VMX+EPT+Linux shell | [D][Z] |
| E3 | **Network UI** | Browser on operator LAN reaches HTTPS UI; list/create/start/stop; not host-only dispatch | [Z][A] |
| E4 | **vSphere-like MVP** | Datastore/images, create-VM (CPU/RAM/disk/NIC), attach ISO or boot media, basic console/log, auth beyond bring-up toy | [Z][A] |
| E5 | **Linux ISO deploy** | Operator registers a distro ISO → VM boots installer (or documented extract path) → installs to virtio-blk → reboot to disk | [Z] |
| E6 | **Production bar** | M6.8 soak + M6.9 external audit/spec review closed per `progress.md` | [V][A] |

**Out of Everest scope (track separately):** full vSphere parity, Dell Tier-2 PERC OEM, multi-site DR, Windows guest WHQL.

---

## Four-summit breakdown

### Summit A — Ship the EFI
**Status: NEAR · ~85% · ~0.25–0.5 months residual**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `cargo build` → `.efi` | DONE | `tools/build.sh`, UEFI target |
| Size budget | DONE | `tools/check-size.sh` (15/20 MB) |
| PE assets kernel/initrd/webui | DONE | M3.22 / M5.2 |
| CI build | DONE | `.github/workflows/ci.yml` |
| Release tarball + SHA256 | MISSING | no release kit |
| Secure Boot signing | MISSING | roadmap; not closed |
| One-page USB/iDRAC runbook | PARTIAL | docs fragments |

### Summit B — Load on real R640
**Status: MEDIUM · ~25% · ~0.5–1.5 months residual (iron-bound)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| UEFI+VMX+EPT+Linux shell | DONE on Latitude/QEMU | `progress.md` M0–M3 |
| Real **R640** boot gate | MISSING | Latitude ≠ R640 |
| Live iDRAC Redfish | MISSING | `GAP: live Redfish BMC → polish` |
| R640 topology from real SRAT/SMBIOS | MOCK | `idrac/` mock text |
| Hardware CI on R640 | MISSING | optional in M6 plan |

### Summit C — vSphere-like UI
**Status: FAR · ~12% · ~1.5–3 months residual**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Embedded SPA list/start/stop | DONE | `assets/webui.html`, M5.2 |
| In-process REST shapes + auth token | DONE | `mgmt/api.rs` M5.1/M6.4 |
| **TCP/TLS HTTP server in HV** | MISSING | explicit: no TCP/HTTP crate |
| Datastore / image library UI | MISSING | — |
| Create-VM wizard | MISSING | demo create id only |
| Guest console | MISSING | — |
| Networking/storage ops UI | MISSING | probes only |
| Audit/tasks pane | PARTIAL | ring exists; UI thin |

### Summit D — Deploy Linux ISO
**Status: FAR · ~8% · ~1.0–2.5 months residual**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| bzImage + initrd boot | DONE | real tiny Linux → shell |
| ISO parse / El Torito / EFI boot img | MISSING | no ISO/cdrom code |
| CD-ROM or virtio media attach | MISSING | — |
| Persistent install disk workflow | MISSING | virtio-blk probe only |
| Upload ISO via API/UI | MISSING | needs datastore |
| Multi-distro matrix | MISSING | — |

---

## Rolling month timeline (Mount Everest)

Months are **calendar months from `baseline_date`**, adjusted by velocity.  
When work finishes early, **pull rows upward** (shrink residual). When blocked, **push ETA**.  

| Month | Calendar | Planned focus | Exit criteria | Status |
|-------|----------|---------------|---------------|--------|
| M+0 | 2026-07 | HDA bootstrap; M6.9 prep; EFI release kit sketch | HDA live; next gate clear | **IN PROGRESS** |
| M+1 | 2026-08 | **R640 first light** + EFI ship kit | E1 mostly; E2 serial/VMX on R640 | PLANNED |
| M+2 | 2026-09 | Datastore + HTTPS mgmt plane | E3 browser reaches UI | PLANNED |
| M+3 | 2026-10 | ISO/CD-ROM or extract path + virtio disk install | E5 MVP one distro | PLANNED |
| M+4 | 2026-11 | UI wizard polish + console/log; harden | E4 MVP | PLANNED |
| M+5 | 2026-12 | M6.9 external + soak on R640; Everest declaration | **E1–E6 all green** | **ETA (baseline)** |

### Timeline burn-down

```
2026-07 ████░░░░  HDA + core freeze
2026-08 ████░░░░  R640 boot
2026-09 ████░░░░  Network UI
2026-10 ████░░░░  ISO deploy MVP
2026-11 ████░░░░  vSphere-like MVP
2026-12 ████░░░░  Everest (E1–E6)   ← months_to_everest ≈ 4.5 from baseline
```

**Pull-forward rule:** If E2 closes in August and HTTPS lands early September, set `months_to_everest` down and move Everest row earlier (e.g. 2026-11). Document why in [Changelog](#hda-changelog).

---

## Everest workstream backlog (P0)

Ordered for critical path (parallelize B with D design):

| ID | Workstream | Summit | Est. residual (mo) | Depends on | Repo touchpoints |
|----|------------|--------|-------------------|------------|------------------|
| P0-1 | Release kit: tag, SHA256, size gate in release CI | A | 0.25 | — | `tools/`, `.github/` |
| P0-2 | R640 boot gate + runbook (USB/iDRAC vMedia) | B | 0.75 | P0-1 helpful | `boot/`, `docs/runbooks/` |
| P0-3 | Live Tier-1 Redfish (read-only health) | B | 0.5 | P0-2 | `idrac/` |
| P0-4 | Minimal TLS HTTP server (serve SPA + REST) | C | 1.0 | size budget | `mgmt/`, maybe `net/` |
| P0-5 | Datastore on ESP/NVMe (images + ISOs) | C+D | 0.75 | P0-4 | new `datastore/` or `mgmt/` |
| P0-6 | ISO register + CD-ROM or kernel-extract boot | D | 1.0 | P0-5 | `devices/`, `guest/` |
| P0-7 | Create-VM API/UI (CPU/RAM/disk/ISO) | C+D | 0.75 | P0-5, P0-6 | `mgmt/`, `assets/webui.html` |
| P0-8 | Install-to-disk + reboot-to-disk path | D | 0.5 | P0-6, P0-7 | `guest/`, `devices/virtio_blk` |
| P0-9 | M6.9 external audit + spec review | E6 | 0.5 | proofs green | `docs/`, `ept_model/` |
| P0-10 | R640 soak / hardware confidence | E2+E6 | 0.5 | P0-2 | `tools/`, `mgmt/soak` |

---

## What is already strong (do not rebuild)

- Type-1 UEFI → VMX → EPT → **real Linux shell** (M3 chain)
- ≥4 guests, credit scheduler, SMP probe, virtio-blk/net probes (M4)
- ADR-004 exclusivity proofs through violation + migrate transfer (M6.0–M6.3 area)
- Audit ring + SOX/ISO/PDF; lifecycle CLI/REST shapes; VMware inventory import
- Single-binary discipline, gate markers, frozen Verus/Kani pins
- M6.8 soak gate closed (per `progress.md`); **next numbered gate: M6.9**

---

## Velocity model

Used every HDA update to move **months_to_everest**.

### Inputs (compute from git + progress.md)

1. `gates_closed_since_last_hda` — new `RAYNU-V-*-OK` rows or Everest criteria flipped DONE  
2. `everest_loc_or_modules` — new code under P0 touchpoints (datastore, http, iso, cdrom, r640 runbook)  
3. `days_since_last_hda`  
4. `blockers_active` — iron wait, partner wait, proof stuck  

### Update formula (heuristic — apply with judgment)

```
progress_delta_pct =
    +8  per Everest criterion E1–E6 newly DONE
    +3  per P0 workstream moved to DONE
    +1  per major related gate (R640, HTTPS, ISO) partial→significant
    -2  per new HARD blocker opened
    -1  per 14 days with zero Everest-path commits

overall_pct = clamp(prev + progress_delta_pct, 0, 100)

# Residual months: start from sum of unfinished P0 residuals,
# then apply velocity factor.
base_residual = sum(est residual months of open P0-*)
velocity_factor =
    0.7  if ≥2 Everest-path PRs merged in last 14 days
    1.0  normal
    1.3  if blocked on iron/external > 14 days
    1.5  if no Everest-path commits in 21 days

months_to_everest = round(base_residual * velocity_factor, 0.25)
everest_eta_month = today + months_to_everest  (first of month or YYYY-MM)
```

**Always** set `months_to_everest_prev` to the previous value before changing.  
**Never** reduce months without citing concrete DONE evidence in the changelog.

---

## This-commit delta

| Field | Value |
|-------|-------|
| Commit | _bootstrap_ |
| Summary | Initial HDA from distance assessment (2026-07-20) |
| Everest impact | Baseline only — no product % burn-down yet |
| Gates touched | none (docs only) |
| Months Δ | 4.5 → 4.5 |

---

## Blockers & risks (Everest-relevant)

| ID | Blocker / risk | Severity | Mitigations |
|----|----------------|----------|-------------|
| H1 | No real R640 validation yet | HIGH | Schedule iron week; USB + iDRAC vMedia |
| H2 | No in-HV HTTP/TLS stack | HIGH | Size-boxed stack or documented split helper (prefer in-binary for [Z]) |
| H3 | No ISO/CD-ROM path | HIGH | MVP: extract boot files from ISO **or** virtio-cdrom |
| H4 | UI is demo-grade | MED | Grow SPA only after HTTPS + datastore |
| H5 | Latitude success hides server firmware gaps | MED | Explicit R640 gate; don’t claim E2 early |
| H6 | Single-dev velocity (R10) | MED | Everest P0 only; defer Tier-2 / full parity |
| H7 | Binary size if HTTP+ISO+UI grow | MED | ADR-003 checks; lazy assets; zstd webui GAP |

---

## HDA changelog

| Date | Commit | Months | Overall % | Note |
|------|--------|-------:|----------:|------|
| 2026-07-20 | bootstrap | 4.5 | 28 | Initial HDA; Everest = EFI+R640+UI+ISO+M6.9 |

---

## Operator quick view

```
Mount Everest:  Ship EFI → R640 → UI → Linux ISO → prod bar
Now:           Core hypervisor strong; product loop incomplete
Months left:   4.5  (ETA ~ 2026-12)
Next move:     E1 release kit + E2 R640 boot  ||  design P0-4/P0-5/P0-6
Do not claim:  "vSphere alternative" or "ISO deploy" until E3–E5 green
```

---

## Maintenance

- **Owner:** whoever merges to `main` (Cursor agent updates HDA in the same change or immediate follow-up).  
- **Rule file:** [`.cursor/rules/hda-update.mdc`](../.cursor/rules/hda-update.mdc)  
- **Prompt card:** [`docs/hda-cursor-prompt.md`](hda-cursor-prompt.md)  
- **Do not** edit scoreboard numbers without updating frontmatter + changelog.  
- **Do not** mark E2 DONE without real R640 evidence in `progress.md` or runbook artifact.
