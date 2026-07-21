---
hda_version: 1
last_updated: 2026-07-21
last_commit: e2129e05a6311b81e860a3cd729d052fe967ff21
last_commit_short: e2129e0
updated_by: cursor
mount_everest_target: "Ship EFI on real R640 + network vSphere-like UI + deploy Linux ISO (M7 Mount Everest)"
months_to_everest: 3.25
months_to_everest_prev: 3.75
velocity_commits_30d: 324
velocity_gates_30d: 12
overall_pct: 45
confidence: medium
baseline_date: 2026-07-20
baseline_months: 4.5
everest_eta_month: "2026-11"
summit_core_pct: 78
summit_efi_pct: 95
summit_r640_pct: 25
summit_ui_pct: 40
summit_iso_pct: 15
summit_prod_pct: 100
---

# Honest Distance Assessment (HDA)

> **Living document.** Updated on every meaningful commit by Cursor (see `.cursor/rules/hda-update.mdc`).  
> **North star product loop (“Mount Everest”):**  
> Ship the EFI → boot on a **real PowerEdge R640** → **network-reachable** vSphere-like UI → **deploy a Linux ISO** (M7 / ADR-009). Production bar (M6.8–M6.9) is already closed on Latitude.

Pillars: **[V]** verified core · **[Z]** single binary · **[D]** iDRAC-native · **[A]** audit-first.  
Authoritative gates: [`docs/progress.md`](progress.md) · plan: [`m7_plan.md`](m7_plan.md) · ADR: [`adr/ADR-009.md`](adr/ADR-009.md) · constitution: [`CLAUDE.md`](../CLAUDE.md).

---

## Scoreboard (read this first)

| Metric | Value | Δ vs previous HDA |
|--------|------:|-------------------|
| **Overall product readiness** | **45%** | +3 (M7.1 HTTP closed on Latitude) |
| **Months to Mount Everest** | **3.25** | −0.5 (HTTP host path closed) |
| **ETA month** | **2026-11** | — |
| **Confidence** | medium | iron + ISO path unproven; R640 ~1 month |
| **Hypervisor core (VMX/EPT/Linux/multi-VM)** | ~78% | strong |
| **Ship EFI artifact** | ~95% | M7.0 closed; Secure Boot still open |
| **Real R640 boot** | ~25% | Latitude/QEMU ≠ R640 |
| **vSphere-like UI (network)** | ~40% | M7.1 closed (host TCP); UEFI NIC + TLS residual |
| **Deploy Linux ISO** | ~15% | M7.2 image library wired (host catalog); ISO boot still open |
| **Production bar (M6.8–M6.9)** | **100%** | soak + EXT closed on Latitude |

```
Months to Everest  ███████░░░░░░░░░░░░░  3.25 mo  (was 3.75)
Overall %          █████████░░░░░░░░░░░  45%
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

**Out of Everest / M7 scope (→ M8 or later):** vMotion-like live migrate, DRS-like placement, hot-add, full vSphere parity, Dell Tier-2 PERC OEM, multi-site DR, Windows guest WHQL.

---

## Four-summit breakdown

### Summit A — Ship the EFI
**Status: NEAR · ~95% · ~0.0–0.25 months residual (Secure Boot optional)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `cargo build` → `.efi` | DONE | `tools/build.sh`, UEFI target |
| Size budget | DONE | `tools/check-size.sh` (15/20 MB) |
| PE assets kernel/initrd/webui | DONE | M3.22 / M5.2 |
| CI build | DONE | `.github/workflows/ci.yml` |
| Release tarball + SHA256 | DONE | `tools/package-release.sh` → `dist/` (M7.0 Latitude) |
| Secure Boot signing | MISSING | optional; not required for M7.0 |
| One-page USB/iDRAC runbook | DONE | `docs/runbooks/usb_idrac.md` (M7.0) |

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
**Status: MEDIUM · ~40% · ~1.0–2.0 months residual**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Embedded SPA list/start/stop | DONE | `assets/webui.html`, M5.2 |
| In-process REST shapes + auth token | DONE | `mgmt/api.rs` M5.1/M6.4 |
| HTTP/1.1 codec + Bearer wire | DONE | `mgmt/http.rs` (M7.1 Latitude) |
| Host TCP proof (loopback) | DONE | `mgmt/http_listen.rs` (M7.1 Latitude) |
| **UEFI NIC HTTP listen** | STUB | `UnsupportedOnFirmware` until Tcp4/SNP |
| TLS | DEFERRED | plaintext lab HTTP (ADR-009) |
| Datastore / image library UI | MISSING | — |
| Create-VM wizard | MISSING | demo create id only |
| Guest console | MISSING | — |
| Networking/storage ops UI | MISSING | probes only |
| Audit/tasks pane | PARTIAL | ring exists; UI thin |

### Summit D — Deploy Linux ISO
**Status: FAR · ~15% · ~1.0–2.5 months residual**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| bzImage + initrd boot | DONE | real tiny Linux → shell |
| Image library (register/list/delete) | WIRED | `mgmt/datastore.rs` (M7.2; Latitude pending) |
| Host ESP-shaped catalog | WIRED | `EFI/RAYNU/images/catalog.txt` (host `std::fs`) |
| UEFI catalog persist | STUB | `UnsupportedOnFirmware` until SFS/NVMe write |
| ISO parse / El Torito / EFI boot img | MISSING | M7.3 |
| CD-ROM or virtio media attach | MISSING | M7.3 |
| Persistent install disk workflow | MISSING | virtio-blk probe only |
| Upload ISO via API/UI | PARTIAL | REST `/images` shapes; blob upload → M7.3 |
| Multi-distro matrix | MISSING | — |

---

## Rolling month timeline (Mount Everest)

Months are **calendar months from `baseline_date`**, adjusted by velocity.  
When work finishes early, **pull rows upward** (shrink residual). When blocked, **push ETA**.  

| Month | Calendar | Planned focus | Exit criteria | Status |
|-------|----------|---------------|---------------|--------|
| M+0 | 2026-07 | **M7.0–M7.1 closed**; M7.2 datastore wired | M7.2 Latitude close | **DONE (M7.1)** / M7.2 wired |
| M+1 | 2026-08 | Ship kit done; HTTP + datastore; **R640 first light** (~1 mo) | M7.0–M7.2; M7.5 if iron ready | PLANNED |
| M+2 | 2026-09 | HTTPS mgmt + ISO path on QEMU | M7.1–M7.3 | PLANNED |
| M+3 | 2026-10 | Create-VM UI + install-to-disk MVP | M7.3–M7.4 | PLANNED |
| M+4 | 2026-11 | R640 validation complete; M7 closed | **M7.0–M7.5 all green** | **ETA** |
| M+5 | 2026-12 | Buffer / M8 sketch start | — | BUFFER |

### Timeline burn-down

```
2026-07 ████████  HDA + M6 closed (Latitude)
2026-08 ████░░░░  R640 boot
2026-09 ████░░░░  Network UI
2026-10 ████░░░░  ISO deploy MVP
2026-11 ████░░░░  Everest (E1–E6)   ← months_to_everest ≈ 4.0
2026-12 ░░░░░░░░  buffer
```

**Pull-forward rule:** If E2 closes in August and HTTPS lands early September, set `months_to_everest` down and move Everest row earlier. Document why in [Changelog](#hda-changelog).

---

## Everest workstream backlog (P0)

Ordered for critical path (parallelize B with D design):

| ID | Workstream | Summit | Est. residual (mo) | Depends on | Repo touchpoints |
|----|------------|--------|-------------------|------------|------------------|
| P0-1 | **M7.0** Release kit: tag, SHA256, size gate, USB/iDRAC runbook | A | **DONE** | — | `tools/package-release.sh`, runbook |
| P0-2 | **M7.5** R640 boot gate (real iron; ~1 month) | B | 0.75 | P0-1 helpful | `boot/`, runbooks |
| P0-3 | Live Tier-1 Redfish (read-only health) | B | 0.5 | P0-2 | `idrac/` — after first boot |
| P0-4 | **M7.1** Minimal HTTP server (serve SPA + REST) | C | 0.25 | size budget | **DONE host path**; UEFI listen + TLS residual |
| P0-5 | **M7.2** Datastore on ESP/NVMe (images + ISOs) | C+D | 0.5 | P0-4 | `mgmt/datastore` wired; UEFI persist stub |
| P0-6 | **M7.3** ISO register + CD-ROM or kernel-extract boot | D | 1.0 | P0-5 | `devices/`, `guest/` |
| P0-7 | **M7.4** Create-VM API/UI (CPU/RAM/disk/ISO) | C+D | 0.75 | P0-5, P0-6 | `mgmt/`, `assets/webui.html` |
| P0-8 | Install-to-disk + reboot-to-disk path | D | 0.5 | P0-6, P0-7 | `guest/`, `devices/virtio_blk` |
| P0-9 | M6.9 external audit + spec review | E6 | **DONE** | proofs green | `docs/`, `ept_model/`, `mgmt/ext` |
| P0-10 | R640 soak / hardware confidence | E2 | 0.5 | P0-2 | `tools/`, `mgmt/soak` — post M7.5 |
| P0-11 | **M8 sketch** vMotion-like / DRS-like / hot-add | — | — | M7 closed | deferred — not M7 critical path |

---

## What is already strong (do not rebuild)

- Type-1 UEFI → VMX → EPT → **real Linux shell** (M3 chain)
- ≥4 guests, credit scheduler, SMP probe, virtio-blk/net probes (M4)
- ADR-004 exclusivity proofs through violation + migrate transfer (M6.0–M6.3 area)
- Audit ring + SOX/ISO/PDF; lifecycle CLI/REST shapes; VMware inventory import
- Single-binary discipline, gate markers, frozen Verus/Kani pins
- **M6 closed** on Latitude — soak + external audit/spec review (`RAYNU-V-M6-EXT-OK`; `80 verified, 0 errors`)
- **M7.0–M7.1 closed** on Latitude — **M7.2 datastore wired** (`RAYNU-V-M7-STORE-OK`); close on Latitude; then ISO; R640 ~1 mo

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
| Commit | M7.2 datastore wired (+ HDA tip @ M7.1 merge) |
| Summary | Image library register/list/delete + ESP catalog host path; Latitude pending |
| Everest impact | iso~8%→15%; months unchanged until Latitude close |
| Gates touched | `RAYNU-V-M7-STORE-OK` wired (not closed) |
| Months Δ | 3.25 → 3.25 |

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
| 2026-07-21 | m7-2-store | 3.25 | 45 | M7.2 datastore wired (catalog+REST); tip last_commit→M7.1 merge; Latitude pending |
| 2026-07-21 | m7-1-close | 3.25 | 45 | M7.1 HTTP closed on Latitude; next M7.2 datastore; UEFI listen residual |
| 2026-07-21 | m7-1-http | 3.75 | 42 | M7.1 HTTP wired (codec+host TCP+Bearer); UEFI listen stub; Latitude pending |
| 2026-07-21 | m7-0-close | 3.75 | 42 | M7.0 SHIP closed on Latitude (`raynu-v-0.1.0`); P0-1 DONE; next M7.1 |
| 2026-07-21 | m7-0-ship | 4.0 | 39 | M7.0 release kit wired (SHA256+tarball+runbook); Latitude pending; efi~90% |
| 2026-07-21 | m7-gov | 4.0 | 39 | ADR-009 + M7 plan accepted; next = M7.0 ship kit; M8 = vMotion/DRS/hot-add |
| 2026-07-21 | site-hda | 4.0 | 39 | Public `site/hda.html` + `sync-hda-site.sh` (numbers unchanged) |
| 2026-07-21 | 8f091fd | 4.0 | 39 | M6.9 EXT + E6 DONE on Latitude (`80 verified, 0 errors`); P0-9 closed; ETA→2026-11 |
| 2026-07-20 | bootstrap | 4.5 | 28 | Initial HDA; Everest = EFI+R640+UI+ISO+M6.9 |

---

## Operator quick view

```
Mount Everest:  Ship EFI → R640 → UI → Linux ISO  (M7)
Now:           M7.2 datastore wired (host); Latitude close pending
Months left:   3.25  (ETA ~ 2026-11)
Next move:     Close M7.2 on Latitude → M7.3 ISO  (R640 ~1 mo)
Do not claim:  M7 closed without real R640; no vMotion/DRS until M8
```

---

## Maintenance

- **Owner:** whoever merges to `main` (Cursor agent updates HDA in the same change or immediate follow-up).  
- **Public site:** [`site/hda.html`](../site/hda.html) ← synced via [`./tools/sync-hda-site.sh`](../tools/sync-hda-site.sh) → [`site/hda.json`](../site/hda.json).  
- **Rule file:** [`.cursor/rules/hda-update.mdc`](../.cursor/rules/hda-update.mdc)  
- **Prompt card:** [`docs/hda-cursor-prompt.md`](hda-cursor-prompt.md)  
- **Do not** edit scoreboard numbers without updating frontmatter + changelog **and** re-running `./tools/sync-hda-site.sh`.  
- **Do not** mark E2 DONE without real R640 evidence in `progress.md` or runbook artifact.
